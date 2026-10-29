//! `LnClient` against a wiremock server (`allow_insecure_hosts = true`), plus
//! the network protections with the default config.

mod common;

use std::time::Duration;

use common::{METADATA, NOW, Spec, invoice, invoice_body, pay_request_body};
use splitcard_ln::{
    client::{ClientConfig, ClientError, LnClient},
    invoice::InvoiceError,
    lnurl::{LnurlError, PayRequest},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn insecure_client() -> LnClient {
    client(ClientConfig {
        allow_insecure_hosts: true,
        ..ClientConfig::default()
    })
}

fn client(config: ClientConfig) -> LnClient {
    LnClient::new(config).unwrap_or_else(|error| panic!("client must build: {error}"))
}

fn url(text: &str) -> Url {
    Url::parse(text).unwrap_or_else(|error| panic!("test URL must parse: {error}"))
}

fn well_known(server: &MockServer) -> Url {
    url(&format!("{}/.well-known/lnurlp/alice", server.uri()))
}

async fn mount_pay_request(server: &MockServer) {
    let callback = format!("{}/cb?user=alice", server.uri());
    Mock::given(method("GET"))
        .and(path("/.well-known/lnurlp/alice"))
        .respond_with(ResponseTemplate::new(200).set_body_string(pay_request_body(
            &callback,
            1_000,
            100_000_000,
        )))
        .mount(server)
        .await;
}

async fn mount_callback(server: &MockServer, amount_msat: u64, body: String) {
    Mock::given(method("GET"))
        .and(path("/cb"))
        .and(query_param("user", "alice"))
        .and(query_param("amount", amount_msat.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn happy_path_returns_a_checked_invoice() {
    let server = MockServer::start().await;
    mount_pay_request(&server).await;
    let bolt11 = invoice(&Spec::new(21_000_000));
    mount_callback(&server, 21_000_000, invoice_body(&bolt11)).await;

    let client = insecure_client();
    let pay_request = client
        .fetch_pay_request_url(well_known(&server))
        .await
        .unwrap_or_else(|error| panic!("pay request: {error}"));
    assert_eq!(pay_request.metadata, METADATA);
    let checked = client
        .request_invoice(&pay_request, 21_000, NOW)
        .await
        .unwrap_or_else(|error| panic!("invoice: {error}"));
    assert_eq!(checked.bolt11, bolt11);
    assert_eq!(checked.amount_msat, 21_000_000);
}

#[tokio::test]
async fn invoice_for_the_wrong_amount_is_rejected() {
    let server = MockServer::start().await;
    mount_pay_request(&server).await;
    mount_callback(
        &server,
        21_000_000,
        invoice_body(&invoice(&Spec::new(20_000_000))),
    )
    .await;

    let client = insecure_client();
    let pay_request = client
        .fetch_pay_request_url(well_known(&server))
        .await
        .unwrap_or_else(|error| panic!("pay request: {error}"));
    assert_eq!(
        client.request_invoice(&pay_request, 21_000, NOW).await,
        Err(ClientError::Invoice(InvoiceError::WrongAmount {
            expected_msat: 21_000_000,
            found_msat: 20_000_000
        }))
    );
}

#[tokio::test]
async fn out_of_range_amount_never_reaches_the_callback() {
    let server = MockServer::start().await;
    mount_pay_request(&server).await;
    Mock::given(path("/cb"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;

    let client = insecure_client();
    let pay_request = client
        .fetch_pay_request_url(well_known(&server))
        .await
        .unwrap_or_else(|error| panic!("pay request: {error}"));
    assert!(matches!(
        client.request_invoice(&pay_request, 100_001, NOW).await,
        Err(ClientError::Lnurl(LnurlError::AmountAboveMaximum { .. }))
    ));
    assert!(matches!(
        client.request_invoice(&pay_request, u64::MAX, NOW).await,
        Err(ClientError::Lnurl(LnurlError::AmountOverflow { .. }))
    ));
}

#[tokio::test]
async fn lnurl_error_from_the_callback_is_surfaced() {
    let server = MockServer::start().await;
    mount_pay_request(&server).await;
    Mock::given(path("/cb"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"status":"ERROR","reason":"wallet offline"}"#),
        )
        .mount(&server)
        .await;

    let client = insecure_client();
    let pay_request = client
        .fetch_pay_request_url(well_known(&server))
        .await
        .unwrap_or_else(|error| panic!("pay request: {error}"));
    assert_eq!(
        client.request_invoice(&pay_request, 21_000, NOW).await,
        Err(ClientError::Lnurl(LnurlError::Service {
            reason: "wallet offline".to_owned()
        }))
    );
}

#[tokio::test]
async fn slow_server_times_out() {
    let server = MockServer::start().await;
    Mock::given(path("/.well-known/lnurlp/alice"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pay_request_body("http://x.example/cb", 1, 2))
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let client = client(ClientConfig {
        timeout: Duration::from_millis(200),
        allow_insecure_hosts: true,
        ..ClientConfig::default()
    });
    assert_eq!(
        client.fetch_pay_request_url(well_known(&server)).await,
        Err(ClientError::Timeout)
    );
}

#[tokio::test]
async fn oversized_response_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(path("/.well-known/lnurlp/alice"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(64 * 1024 + 1)))
        .mount(&server)
        .await;
    assert_eq!(
        insecure_client()
            .fetch_pay_request_url(well_known(&server))
            .await,
        Err(ClientError::ResponseTooLarge { limit: 64 * 1024 })
    );
}

/// A chunked response has no Content-Length, so only the limit applied while
/// reading can stop it. The server sends far more than the limit.
#[tokio::test]
async fn oversized_chunked_response_is_cut_off_while_reading() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|error| panic!("bind: {error}"));
    let addr = listener
        .local_addr()
        .unwrap_or_else(|error| panic!("addr: {error}"));
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let mut request = [0_u8; 1024];
        let _ = socket.read(&mut request).await;
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await;
        let chunk = format!("{:x}\r\n{}\r\n", 1024, "y".repeat(1024));
        // 1 MiB in total, sixteen times the limit.
        for _ in 0..1024 {
            if socket.write_all(chunk.as_bytes()).await.is_err() {
                return;
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });

    let target = url(&format!("http://{addr}/.well-known/lnurlp/alice"));
    assert_eq!(
        insecure_client().fetch_pay_request_url(target).await,
        Err(ClientError::ResponseTooLarge { limit: 64 * 1024 })
    );
}

async fn mount_redirect_chain(server: &MockServer, hops: usize) -> Url {
    for hop in 0..hops {
        Mock::given(path(format!("/r{hop}")))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("/r{}", hop + 1)),
            )
            .mount(server)
            .await;
    }
    let callback = format!("{}/cb", server.uri());
    Mock::given(path(format!("/r{hops}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(pay_request_body(&callback, 1, 2)))
        .mount(server)
        .await;
    url(&format!("{}/r0", server.uri()))
}

#[tokio::test]
async fn three_redirects_are_followed() {
    let server = MockServer::start().await;
    let start = mount_redirect_chain(&server, 3).await;
    assert!(insecure_client().fetch_pay_request_url(start).await.is_ok());
}

#[tokio::test]
async fn too_many_redirects_are_refused() {
    let server = MockServer::start().await;
    let start = mount_redirect_chain(&server, 4).await;
    assert_eq!(
        insecure_client().fetch_pay_request_url(start).await,
        Err(ClientError::TooManyRedirects)
    );
}

// Network protection, with the default (secure) config. None of these
// requests leave the machine: each is refused before connecting.

fn secure_client() -> LnClient {
    client(ClientConfig::default())
}

#[tokio::test]
async fn plain_http_is_refused() {
    assert!(matches!(
        secure_client()
            .fetch_pay_request_url(url("http://pay.example.com/.well-known/lnurlp/alice"))
            .await,
        Err(ClientError::InsecureScheme { .. })
    ));
}

#[tokio::test]
async fn localhost_is_refused_after_resolution() {
    let result = secure_client()
        .fetch_pay_request_url(url("https://localhost/.well-known/lnurlp/alice"))
        .await;
    match result {
        Err(ClientError::ForbiddenAddress { host, ip }) => {
            assert_eq!(host, "localhost");
            assert!(ip.is_loopback(), "{ip} must be loopback");
        }
        other => panic!("localhost must be refused, got {other:?}"),
    }
}

#[tokio::test]
async fn private_ip_literals_are_refused() {
    for target in [
        "https://127.0.0.1/.well-known/lnurlp/alice",
        "https://10.0.0.1/.well-known/lnurlp/alice",
        "https://10.255.1.2:8443/x",
        "https://192.168.1.1/.well-known/lnurlp/alice",
        "https://169.254.169.254/latest/meta-data",
        "https://[::1]/x",
    ] {
        assert!(
            matches!(
                secure_client().fetch_pay_request_url(url(target)).await,
                Err(ClientError::ForbiddenAddress { .. })
            ),
            "{target} must be refused"
        );
    }
}

#[tokio::test]
async fn callbacks_are_held_to_the_same_rules() {
    let client = secure_client();
    for callback in ["https://192.168.0.7/cb", "http://pay.example.com/cb"] {
        let pay_request = PayRequest {
            callback: url(callback),
            min_sendable_msat: 1,
            max_sendable_msat: 100_000_000,
            metadata: METADATA.to_owned(),
        };
        assert!(
            matches!(
                client.request_invoice(&pay_request, 21_000, NOW).await,
                Err(ClientError::ForbiddenAddress { .. } | ClientError::InsecureScheme { .. })
            ),
            "{callback} must be refused"
        );
    }
}

#[tokio::test]
async fn verify_url_is_kept_and_settlement_is_checked() {
    let server = MockServer::start().await;
    mount_pay_request(&server).await;
    let bolt11 = invoice(&Spec::new(21_000_000));
    let verify = format!("{}/verify/abc", server.uri());
    let body = serde_json::json!({ "pr": bolt11, "routes": [], "verify": verify }).to_string();
    mount_callback(&server, 21_000_000, body).await;
    Mock::given(method("GET"))
        .and(path("/verify/abc"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                serde_json::json!({
                    "status": "OK", "settled": true, "preimage": "ab".repeat(32), "pr": bolt11,
                })
                .to_string(),
            ),
        )
        .mount(&server)
        .await;

    let client = insecure_client();
    let pay_request = client
        .fetch_pay_request_url(well_known(&server))
        .await
        .expect("pay request");
    let fetched = client
        .request_invoice_with_verify(&pay_request, 21_000, NOW)
        .await
        .expect("invoice");
    assert_eq!(fetched.invoice.bolt11, bolt11);
    assert_eq!(fetched.verify_url, Some(url(&verify)));

    let settlement = client
        .check_settlement(&url(&verify))
        .await
        .expect("settlement");
    assert!(settlement.settled);
    assert_eq!(settlement.preimage, Some("ab".repeat(32)));
    assert_eq!(settlement.bolt11, bolt11);
}

#[tokio::test]
async fn verify_urls_are_held_to_the_network_rules() {
    assert!(matches!(
        secure_client()
            .check_settlement(&url("https://127.0.0.1/verify/abc"))
            .await,
        Err(ClientError::ForbiddenAddress { .. })
    ));
    assert!(matches!(
        secure_client()
            .check_settlement(&url("http://pay.example/verify/abc"))
            .await,
        Err(ClientError::InsecureScheme { .. })
    ));
}
