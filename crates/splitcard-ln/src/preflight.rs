//! Checking every member can be paid before a payout, reporting all failures.
//!
//! [`preflight_amounts`] only fetches each pay request and checks the amount
//! against `minSendable`/`maxSendable`; it requests no invoices. Use it when
//! invoices are fetched later, just before each is paid, since some providers'
//! invoices expire within a minute. [`preflight`] also requests an invoice per
//! member.

use std::future::Future;

use futures::stream::{self, StreamExt};
use thiserror::Error;
use url::Url;

use crate::{
    address::{AddressError, parse_address},
    client::{ClientError, FetchedInvoice, LnClient},
    invoice::CheckedInvoice,
    lnurl::{PayRequest, Settlement, check_amount, sats_to_msat},
};

/// Default number of members resolved at once.
pub const DEFAULT_CONCURRENCY: usize = 5;

/// Why one member's invoice could not be obtained.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemberError {
    /// The member's Lightning Address is invalid.
    #[error(transparent)]
    Address(#[from] AddressError),
    /// Fetching or checking the member's invoice failed.
    #[error(transparent)]
    Client(#[from] ClientError),
}

/// Something that resolves Lightning Addresses to pay requests and checked
/// invoices. [`LnClient`] is the real implementation; tests use fakes.
pub trait InvoiceSource {
    /// Parses `address` and fetches its LNURL-pay request.
    fn pay_request_for(
        &self,
        address: &str,
    ) -> impl Future<Output = Result<PayRequest, MemberError>> + Send;

    /// Parses `address`, fetches its pay request and returns a checked invoice
    /// for exactly `amount_sats`, with its LUD-21 verify URL if offered.
    fn fresh_invoice(
        &self,
        address: &str,
        amount_sats: u64,
        now_unix: u64,
    ) -> impl Future<Output = Result<FetchedInvoice, MemberError>> + Send;

    /// Asks a LUD-21 verify URL whether its invoice was paid.
    fn check_settlement(
        &self,
        verify_url: &Url,
    ) -> impl Future<Output = Result<Settlement, ClientError>> + Send;

    /// Like [`InvoiceSource::fresh_invoice`], without the verify URL.
    fn invoice_for(
        &self,
        address: &str,
        amount_sats: u64,
        now_unix: u64,
    ) -> impl Future<Output = Result<CheckedInvoice, MemberError>> + Send {
        let fetched = self.fresh_invoice(address, amount_sats, now_unix);
        async move { fetched.await.map(|fetched| fetched.invoice) }
    }
}

impl InvoiceSource for LnClient {
    async fn pay_request_for(&self, address: &str) -> Result<PayRequest, MemberError> {
        let address = parse_address(address)?;
        Ok(self.fetch_pay_request(&address).await?)
    }

    async fn fresh_invoice(
        &self,
        address: &str,
        amount_sats: u64,
        now_unix: u64,
    ) -> Result<FetchedInvoice, MemberError> {
        let pay_request = InvoiceSource::pay_request_for(self, address).await?;
        Ok(self
            .request_invoice_with_verify(&pay_request, amount_sats, now_unix)
            .await?)
    }

    async fn check_settlement(&self, verify_url: &Url) -> Result<Settlement, ClientError> {
        LnClient::check_settlement(self, verify_url).await
    }
}

/// Fetches the pay request for `address` and checks that `amount_sats` lies
/// within its `minSendable`/`maxSendable`. No invoice is requested.
///
/// # Errors
///
/// Returns the address, transport or LNURL failure, or
/// [`LnurlError::AmountBelowMinimum`](crate::lnurl::LnurlError::AmountBelowMinimum) /
/// [`LnurlError::AmountAboveMaximum`](crate::lnurl::LnurlError::AmountAboveMaximum).
pub async fn check_payable<S: InvoiceSource>(
    source: &S,
    address: &str,
    amount_sats: u64,
) -> Result<PayRequest, MemberError> {
    let pay_request = source.pay_request_for(address).await?;
    let amount_msat = sats_to_msat(amount_sats).map_err(ClientError::from)?;
    check_amount(&pay_request, amount_msat).map_err(ClientError::from)?;
    Ok(pay_request)
}

/// One member's [`check_payable`] result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmountCheck<I> {
    /// The caller's identifier for the member.
    pub id: I,
    /// The Lightning Address as given.
    pub address: String,
    /// The amount checked, in sats.
    pub amount_sats: u64,
    /// The pay request whose limits the amount fits, or why it does not.
    pub outcome: Result<PayRequest, MemberError>,
}

/// Runs [`check_payable`] for every `(id, address, amount_sats)` member, at
/// most [`DEFAULT_CONCURRENCY`] at a time, returning results in input order.
/// Every member is checked, so all failures are reported.
pub async fn preflight_amounts<S: InvoiceSource, I: Clone>(
    source: &S,
    members: &[(I, String, u64)],
) -> Vec<AmountCheck<I>> {
    // Build the futures first: mapping inside the stream would make the
    // result's `Send`-ness higher-ranked, which servers cannot prove.
    let checks: Vec<_> = members
        .iter()
        .map(|(id, address, amount_sats)| async move {
            AmountCheck {
                id: id.clone(),
                address: address.clone(),
                amount_sats: *amount_sats,
                outcome: check_payable(source, address, *amount_sats).await,
            }
        })
        .collect();
    stream::iter(checks)
        .buffered(DEFAULT_CONCURRENCY)
        .collect()
        .await
}

/// One member's pre-flight result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberResult<I> {
    /// The caller's identifier for the member.
    pub id: I,
    /// The Lightning Address as given.
    pub address: String,
    /// The amount requested, in sats.
    pub amount_sats: u64,
    /// The checked invoice, or why there is none.
    pub outcome: Result<CheckedInvoice, MemberError>,
}

/// Every member's result, in input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport<I> {
    /// One result per member, in the order the members were given.
    pub members: Vec<MemberResult<I>>,
}

impl<I> PreflightReport<I> {
    /// True only when every member has a checked invoice.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.members.iter().all(|member| member.outcome.is_ok())
    }

    /// The members whose invoice could not be obtained, in input order.
    pub fn failures(&self) -> impl Iterator<Item = &MemberResult<I>> {
        self.members.iter().filter(|member| member.outcome.is_err())
    }
}

/// Resolves an invoice for every `(id, address, amount_sats)` member, at most
/// [`DEFAULT_CONCURRENCY`] at a time. Every member is attempted, so the report
/// lists all failures, not just the first.
pub async fn preflight<S: InvoiceSource, I: Clone>(
    client: &S,
    members: &[(I, String, u64)],
    now_unix: u64,
) -> PreflightReport<I> {
    preflight_with_concurrency(client, members, now_unix, DEFAULT_CONCURRENCY).await
}

/// Like [`preflight`], resolving at most `concurrency` members at a time
/// (treated as 1 if 0).
pub async fn preflight_with_concurrency<S: InvoiceSource, I: Clone>(
    client: &S,
    members: &[(I, String, u64)],
    now_unix: u64,
    concurrency: usize,
) -> PreflightReport<I> {
    let members = stream::iter(members)
        .map(|(id, address, amount_sats)| async move {
            MemberResult {
                id: id.clone(),
                address: address.clone(),
                amount_sats: *amount_sats,
                outcome: client.invoice_for(address, *amount_sats, now_unix).await,
            }
        })
        // `buffered` keeps input order while running up to `concurrency` at once.
        .buffered(concurrency.max(1))
        .collect()
        .await;
    PreflightReport { members }
}
