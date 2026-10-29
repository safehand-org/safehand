//! NIP-98: one test per rejection reason, against `PUT /me/profile`.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, BASE_URL, app, auth_event, base64, header_for, sha256_hex};
use serde_json::{Value, json};

const PATH: &str = "/me/profile";

fn body() -> String {
    json!({ "name": "Alice", "lightning_address": "alice@blink.sv" }).to_string()
}

fn url() -> String {
    format!("{BASE_URL}{PATH}")
}

async fn put_with(header: Option<&str>) -> (StatusCode, Value) {
    let app = app().await;
    app.raw(Method::PUT, PATH, header, body().as_bytes()).await
}

fn assert_code((status, response): (StatusCode, Value), code: &str) {
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{response}");
    assert_eq!(response["code"], code, "{response}");
    assert!(response["message"].is_string());
    assert!(response.get("details").is_some());
}

/// A correctly signed auth event with these tags, re-serialized after `edit`.
fn edited_header(edit: impl FnOnce(&mut Value)) -> String {
    let payload = sha256_hex(body().as_bytes());
    let event = auth_event(
        ALICE,
        27_235,
        common::NOW,
        &[&["u", &url()], &["method", "PUT"], &["payload", &payload]],
    );
    let mut value: Value = serde_json::from_str(&event.as_json()).expect("event JSON");
    edit(&mut value);
    format!("Nostr {}", base64(value.to_string().as_bytes()))
}

fn header(kind: u16, created_at: u64, tags: &[&[&str]]) -> String {
    header_for(&auth_event(ALICE, kind, created_at, tags))
}

#[tokio::test]
async fn valid_header_is_accepted() {
    let app = app().await;
    let header = app.auth_header(ALICE, "PUT", PATH, body().as_bytes());
    let (status, response) = app
        .raw(Method::PUT, PATH, Some(&header), body().as_bytes())
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["pubkey"], common::pubkey(ALICE));
}

#[tokio::test]
async fn missing_header_is_rejected() {
    assert_code(put_with(None).await, "auth_missing");
}

#[tokio::test]
async fn malformed_headers_are_rejected() {
    for header in [
        "Bearer abc",
        "Nostr",
        "Nostr !!!not-base64!!!",
        &format!("Nostr {}", base64(b"{\"not\":\"an event\"}")),
    ] {
        assert_code(put_with(Some(header)).await, "auth_malformed");
    }
}

#[tokio::test]
async fn invalid_event_id_is_rejected() {
    let header = edited_header(|event| event["content"] = json!("changed after signing"));
    assert_code(put_with(Some(&header)).await, "auth_invalid_event_id");
}

#[tokio::test]
async fn invalid_signature_is_rejected() {
    let header = edited_header(|event| {
        let signature = event["sig"].as_str().expect("sig").to_owned();
        let flipped = if signature.starts_with('0') { "1" } else { "0" };
        event["sig"] = json!(format!("{flipped}{}", &signature[1..]));
    });
    assert_code(put_with(Some(&header)).await, "auth_invalid_signature");
}

#[tokio::test]
async fn wrong_kind_is_rejected() {
    let payload = sha256_hex(body().as_bytes());
    let header = header(
        1,
        common::NOW,
        &[&["u", &url()], &["method", "PUT"], &["payload", &payload]],
    );
    assert_code(put_with(Some(&header)).await, "auth_wrong_kind");
}

#[tokio::test]
async fn created_at_more_than_60_seconds_away_is_rejected() {
    let payload = sha256_hex(body().as_bytes());
    let tags: &[&[&str]] = &[&["u", &url()], &["method", "PUT"], &["payload", &payload]];
    for created_at in [common::NOW - 61, common::NOW + 61] {
        assert_code(
            put_with(Some(&header(27_235, created_at, tags))).await,
            "auth_expired",
        );
    }
    // Exactly 60 seconds either way is still accepted.
    for created_at in [common::NOW - 60, common::NOW + 60] {
        let (status, response) = put_with(Some(&header(27_235, created_at, tags))).await;
        assert_eq!(status, StatusCode::OK, "{response}");
    }
}

#[tokio::test]
async fn url_mismatch_is_rejected() {
    let payload = sha256_hex(body().as_bytes());
    for u in [
        "https://splitcard.test/me/profile/".to_owned(),
        "https://splitcard.test/me/profile?x=1".to_owned(),
        "http://splitcard.test/me/profile".to_owned(),
        "https://other.test/me/profile".to_owned(),
        "/me/profile".to_owned(),
    ] {
        let header = header(
            27_235,
            common::NOW,
            &[&["u", &u], &["method", "PUT"], &["payload", &payload]],
        );
        assert_code(put_with(Some(&header)).await, "auth_url_mismatch");
    }
    let missing = header(
        27_235,
        common::NOW,
        &[&["method", "PUT"], &["payload", &payload]],
    );
    assert_code(put_with(Some(&missing)).await, "auth_url_mismatch");
}

#[tokio::test]
async fn query_string_is_part_of_the_signed_url() {
    let app = app().await;
    let path = "/me/profile?source=test";
    let header = app.auth_header(ALICE, "PUT", path, body().as_bytes());
    let (status, response) = app
        .raw(Method::PUT, path, Some(&header), body().as_bytes())
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
}

#[tokio::test]
async fn method_mismatch_is_rejected() {
    let payload = sha256_hex(body().as_bytes());
    for method in ["POST", "put"] {
        let header = header(
            27_235,
            common::NOW,
            &[&["u", &url()], &["method", method], &["payload", &payload]],
        );
        assert_code(put_with(Some(&header)).await, "auth_method_mismatch");
    }
}

#[tokio::test]
async fn payload_mismatch_is_rejected() {
    let wrong = sha256_hex(b"some other body");
    let wrong_payload = header(
        27_235,
        common::NOW,
        &[&["u", &url()], &["method", "PUT"], &["payload", &wrong]],
    );
    assert_code(
        put_with(Some(&wrong_payload)).await,
        "auth_payload_mismatch",
    );
    let no_payload = header(27_235, common::NOW, &[&["u", &url()], &["method", "PUT"]]);
    assert_code(put_with(Some(&no_payload)).await, "auth_payload_mismatch");
}

#[tokio::test]
async fn replayed_event_is_rejected() {
    let app = app().await;
    let header = app.auth_header(ALICE, "PUT", PATH, body().as_bytes());
    let (status, response) = app
        .raw(Method::PUT, PATH, Some(&header), body().as_bytes())
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_code(
        app.raw(Method::PUT, PATH, Some(&header), body().as_bytes())
            .await,
        "auth_replayed",
    );
}

#[tokio::test]
async fn bodies_over_64_kb_are_rejected() {
    let app = app().await;
    let body = vec![b' '; 64 * 1024 + 1];
    let (status, response) = app
        .raw(Method::POST, "/cards/abc/signatures", None, &body)
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response["code"], "payload_too_large");
}
