//! A `PUBLIC_BASE_URL` with a path, as behind a proxy that serves the API at
//! `/api` and strips that prefix: NIP-98 `u` tags and URLs the server builds
//! include the path, while routes are matched without it.

mod common;

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use axum::http::{Method, StatusCode};
use common::{ALICE, BOB, NOW, app_with, auth_event, header_for, sha256_hex};
use serde_json::json;
use splitcard_server::{config::Config, db};

const PUBLIC: &str = "https://splitcard.test/api";

fn config_with_base(base: &str) -> Config {
    let mut env: HashMap<&str, String> = common::env();
    env.insert("PUBLIC_BASE_URL", base.to_owned());
    Config::from_lookup(|name| env.get(name).cloned()).expect("valid config")
}

fn header(url: &str, method: &str, body: &[u8], nonce: &str) -> String {
    let payload = sha256_hex(body);
    let (url_tag, method_tag, nonce_tag) = (["u", url], ["method", method], ["nonce", nonce]);
    let mut tags: Vec<&[&str]> = vec![&url_tag, &method_tag, &nonce_tag];
    let payload_tag = ["payload", payload.as_str()];
    if !body.is_empty() {
        tags.push(&payload_tag);
    }
    header_for(&auth_event(ALICE, 27_235, NOW, &tags))
}

#[tokio::test]
async fn signed_urls_include_the_base_path() {
    for base in [PUBLIC, "https://splitcard.test/api/"] {
        let db = db::connect_in_memory().await.expect("database");
        let app = app_with(db, config_with_base(base));
        let body = json!({ "name": "Alice", "lightning_address": "alice@blink.sv" }).to_string();

        // The proxy strips `/api`; the signature covers the public URL.
        let signed = header(
            &format!("{PUBLIC}/me/profile?via=proxy"),
            "PUT",
            body.as_bytes(),
            "1",
        );
        let (status, response) = app
            .raw(
                Method::PUT,
                "/me/profile?via=proxy",
                Some(&signed),
                body.as_bytes(),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{base}: {response}");

        for (url, nonce) in [
            ("https://splitcard.test/me/profile", "2"),
            ("https://splitcard.test/api/api/me/profile", "3"),
            ("https://splitcard.test/api/me/profile?via=other", "4"),
        ] {
            let signed = header(url, "PUT", body.as_bytes(), nonce);
            let (status, response) = app
                .raw(
                    Method::PUT,
                    "/me/profile?via=proxy",
                    Some(&signed),
                    body.as_bytes(),
                )
                .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{url}");
            assert_eq!(response["code"], "auth_url_mismatch", "{url}");
        }

        let get = header(&format!("{PUBLIC}/me"), "GET", b"", "5");
        let (status, me) = app.raw(Method::GET, "/me", Some(&get), b"").await;
        assert_eq!(status, StatusCode::OK, "{me}");
        assert_eq!(me["profile"]["name"], "Alice");
    }
}

#[tokio::test]
async fn urls_the_server_builds_include_the_base_path() {
    let db = db::connect_in_memory().await.expect("database");
    let app = app_with(db, config_with_base(PUBLIC));
    let team_id = app_alice_card(&app).await;
    let (status, report) = app.get(&format!("/teams/{team_id}/verify")).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    let hash = report["card_hash"].as_str().expect("hash");
    assert_eq!(
        report["timestamp"]["ots_url"],
        format!("{PUBLIC}/cards/{hash}/timestamp.ots")
    );
}

/// Sends a request signed by `signer` for `PUBLIC` plus `path`.
async fn signed(
    app: &common::TestApp,
    signer: u8,
    method: Method,
    path: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    static NONCE: AtomicU64 = AtomicU64::new(100);
    let nonce = NONCE.fetch_add(1, Ordering::SeqCst).to_string();
    let payload = sha256_hex(body.as_bytes());
    let url = format!("{PUBLIC}{path}");
    let mut tags: Vec<[&str; 2]> =
        vec![["u", &url], ["method", method.as_str()], ["nonce", &nonce]];
    if !body.is_empty() {
        tags.push(["payload", &payload]);
    }
    let tags: Vec<&[&str]> = tags.iter().map(<[&str; 2]>::as_slice).collect();
    let header = header_for(&auth_event(signer, 27_235, NOW, &tags));
    app.raw(method, path, Some(&header), body.as_bytes()).await
}

/// An active card for Alice and Bob, made with headers for this base URL.
async fn app_alice_card(app: &common::TestApp) -> String {
    for (number, name, address) in [
        (ALICE, "Alice", "alice@blink.sv"),
        (BOB, "Bob", "bob@getalby.com"),
    ] {
        let profile = json!({ "name": name, "lightning_address": address }).to_string();
        let (status, _) = signed(app, number, Method::PUT, "/me/profile", &profile).await;
        assert_eq!(status, StatusCode::OK);
    }
    let team =
        json!({ "slug": "pair", "name": common::TEAM_NAME, "event_label": common::EVENT_LABEL })
            .to_string();
    let (status, team) = signed(app, ALICE, Method::POST, "/teams", &team).await;
    assert_eq!(status, StatusCode::CREATED, "{team}");
    let team_id = team["id"].as_str().expect("id").to_owned();
    let join = json!({ "join_code": team["join_code"] }).to_string();
    let (status, _) = signed(
        app,
        BOB,
        Method::POST,
        &format!("/teams/{team_id}/join"),
        &join,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let card = serde_json::to_string(&common::card(&[(ALICE, 1), (BOB, 1)], None)).expect("card");
    let (status, submitted) = signed(
        app,
        ALICE,
        Method::POST,
        &format!("/teams/{team_id}/cards"),
        &card,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{submitted}");
    let hash = submitted["hash"].as_str().expect("hash").to_owned();
    app.sign_ok(&hash, ALICE).await;
    app.sign_ok(&hash, BOB).await;
    team_id
}
