//! Pre-flight ordering, failure reporting and concurrency, using a fake
//! [`InvoiceSource`] so no network is involved.

mod common;

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{NOW, Spec, invoice};
use splitcard_ln::{
    address::{AddressError, parse_address},
    client::{ClientError, FetchedInvoice},
    invoice::{Network, check_invoice},
    lnurl::{LnurlError, PayRequest, Settlement},
    preflight::{
        DEFAULT_CONCURRENCY, InvoiceSource, MemberError, check_payable, preflight,
        preflight_amounts, preflight_with_concurrency,
    },
};
use url::Url;

/// Answers from a fixed table after a short delay, tracking peak concurrency
/// and how many invoices were requested.
#[derive(Default)]
struct FakeSource {
    in_flight: AtomicUsize,
    peak: AtomicUsize,
    invoices: AtomicUsize,
}

impl FakeSource {
    async fn lookup(&self, address: &str) -> Result<PayRequest, MemberError> {
        let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(current, Ordering::SeqCst);
        // Later members finish first, so ordering must come from the report.
        let delay = if address.starts_with('a') { 30 } else { 5 };
        tokio::time::sleep(Duration::from_millis(delay)).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);

        parse_address(address)?;
        if address.ends_with("@offline.example") {
            return Err(MemberError::Client(ClientError::HttpStatus { status: 503 }));
        }
        // Strike-like limits: at least 100 sats.
        Ok(PayRequest {
            callback: Url::parse("https://pay.example/cb").expect("url"),
            min_sendable_msat: 100_000,
            max_sendable_msat: 1_000_000_000,
            metadata: common::METADATA.to_owned(),
        })
    }
}

impl InvoiceSource for FakeSource {
    async fn pay_request_for(&self, address: &str) -> Result<PayRequest, MemberError> {
        self.lookup(address).await
    }

    async fn fresh_invoice(
        &self,
        address: &str,
        amount_sats: u64,
        now_unix: u64,
    ) -> Result<FetchedInvoice, MemberError> {
        self.lookup(address).await?;
        self.invoices.fetch_add(1, Ordering::SeqCst);
        let amount_msat = amount_sats * 1_000;
        let bolt11 = invoice(&Spec::new(amount_msat));
        let invoice = check_invoice(
            &bolt11,
            amount_msat,
            common::METADATA,
            Network::Mainnet,
            now_unix,
        )
        .map_err(ClientError::from)?;
        Ok(FetchedInvoice {
            invoice,
            verify_url: None,
        })
    }

    fn check_settlement(
        &self,
        _verify_url: &Url,
    ) -> impl Future<Output = Result<Settlement, ClientError>> + Send {
        std::future::ready(Err(ClientError::HttpStatus { status: 404 }))
    }
}

fn member(id: &str, address: &str, amount_sats: u64) -> (String, String, u64) {
    (id.to_owned(), address.to_owned(), amount_sats)
}

#[tokio::test]
async fn one_failure_gives_two_invoices_and_one_error_in_order() {
    let members = [
        member("ada", "ada@example.com", 36_001),
        member("bola", "bola@offline.example", 35_000),
        member("chidi", "chidi@example.net", 29_000),
    ];
    let report = preflight(&FakeSource::default(), &members, NOW).await;

    assert!(!report.is_ready());
    let ids: Vec<&str> = report.members.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["ada", "bola", "chidi"]);
    assert_eq!(
        report.members[0].outcome.as_ref().map(|i| i.amount_msat),
        Ok(36_001_000)
    );
    assert_eq!(
        report.members[1].outcome,
        Err(MemberError::Client(ClientError::HttpStatus { status: 503 }))
    );
    assert_eq!(
        report.members[2].outcome.as_ref().map(|i| i.amount_msat),
        Ok(29_000_000)
    );
    let failed: Vec<&str> = report.failures().map(|m| m.id.as_str()).collect();
    assert_eq!(failed, ["bola"]);
}

#[tokio::test]
async fn every_failure_is_reported_not_just_the_first() {
    let members = [
        member("1", "bad address", 1),
        member("2", "ok@example.com", 1),
        member("3", "down@offline.example", 1),
        member("4", "UPPER@Example.COM", 1),
    ];
    let report = preflight(&FakeSource::default(), &members, NOW).await;
    assert!(!report.is_ready());
    assert_eq!(
        report.members[0].outcome,
        Err(MemberError::Address(AddressError::MissingAt))
    );
    assert!(report.members[1].outcome.is_ok());
    assert!(report.members[2].outcome.is_err());
    assert!(report.members[3].outcome.is_ok());
    assert_eq!(report.failures().count(), 2);
}

#[tokio::test]
async fn all_successes_are_ready() {
    let members = [
        member("a", "a@example.com", 10),
        member("b", "b@example.com", 20),
    ];
    let report = preflight(&FakeSource::default(), &members, NOW).await;
    assert!(report.is_ready());
    assert_eq!(report.failures().count(), 0);
}

#[tokio::test]
async fn concurrency_is_limited() {
    let members: Vec<_> = (0..20)
        .map(|n| member(&n.to_string(), &format!("a{n}@example.com"), 1))
        .collect();

    let source = FakeSource::default();
    let report = preflight(&source, &members, NOW).await;
    assert_eq!(report.members.len(), 20);
    assert_eq!(source.peak.load(Ordering::SeqCst), DEFAULT_CONCURRENCY);

    let source = FakeSource::default();
    preflight_with_concurrency(&source, &members, NOW, 2).await;
    assert_eq!(source.peak.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn amount_preflight_checks_limits_without_requesting_invoices() {
    let members = [
        member("ada", "ada@example.com", 36_001),
        member("bola", "bola@offline.example", 35_000),
        member("chidi", "chidi@example.net", 99),
    ];
    let source = FakeSource::default();
    let checks = preflight_amounts(&source, &members).await;

    let ids: Vec<&str> = checks.iter().map(|check| check.id.as_str()).collect();
    assert_eq!(ids, ["ada", "bola", "chidi"]);
    assert!(checks[0].outcome.is_ok());
    assert_eq!(
        checks[1].outcome,
        Err(MemberError::Client(ClientError::HttpStatus { status: 503 }))
    );
    assert_eq!(
        checks[2].outcome,
        Err(MemberError::Client(ClientError::Lnurl(
            LnurlError::AmountBelowMinimum {
                amount_msat: 99_000,
                min_sendable_msat: 100_000,
            }
        )))
    );
    assert_eq!(source.invoices.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn amount_preflight_is_limited_and_checks_the_maximum() {
    let members: Vec<_> = (0..20)
        .map(|n| member(&n.to_string(), &format!("a{n}@example.com"), 100))
        .collect();
    let source = FakeSource::default();
    assert!(
        preflight_amounts(&source, &members)
            .await
            .iter()
            .all(|check| check.outcome.is_ok())
    );
    assert_eq!(source.peak.load(Ordering::SeqCst), DEFAULT_CONCURRENCY);

    assert_eq!(
        check_payable(&source, "big@example.com", 1_000_001).await,
        Err(MemberError::Client(ClientError::Lnurl(
            LnurlError::AmountAboveMaximum {
                amount_msat: 1_000_001_000,
                max_sendable_msat: 1_000_000_000,
            }
        )))
    );
}

#[tokio::test]
async fn invoice_for_drops_the_verify_url() {
    let source = FakeSource::default();
    let invoice = source
        .invoice_for("ada@example.com", 21_000, NOW)
        .await
        .expect("invoice");
    assert_eq!(invoice.amount_msat, 21_000_000);
}
