//! No endpoint accepts an NWC connection string: requests carrying one are
//! rejected, and it never reaches the database, the logs or a response.
//!
//! This is the only test in its binary: tracing caches per-callsite interest
//! process-wide, so a parallel test without a subscriber could hide events.

mod common;

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use axum::http::{Method, StatusCode};
use common::{ALICE, app};
use serde_json::json;
use sqlx::{Column, Row};
use tracing_subscriber::fmt::MakeWriter;

const WALLET_SECRET: &str = "71a8c14c1407c113601079c4302dab36460f0ccd0ad506f1f2dc73b5100e4f3c";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Every text value in every table.
async fn database_text(app: &common::TestApp) -> String {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&app.state.db)
    .await
    .expect("tables");
    let mut text = String::new();
    for table in tables {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT * FROM \"{table}\"")))
            .fetch_all(&app.state.db)
            .await
            .expect("rows");
        for row in rows {
            for column in row.columns() {
                if let Ok(Some(value)) = row.try_get::<Option<String>, _>(column.ordinal()) {
                    text.push_str(&value);
                    text.push('\n');
                }
            }
        }
    }
    text
}

#[tokio::test]
async fn nwc_connection_strings_are_rejected_and_never_stored_or_logged() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let app = app().await;
    let (team_id, _) = app.active_first_card().await;
    let nwc = format!(
        "nostr+walletconnect://b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4\
         ?relay=wss%3A%2F%2Frelay.damus.io&secret={WALLET_SECRET}"
    );
    let escaped = nwc.replacen('+', "\\u002b", 1);

    // As an extra field, as a field value, and with the scheme JSON-escaped.
    let address = "alice@blink.sv";
    let bodies = [
        json!({ "name": "Alice", "lightning_address": address, "nwc": nwc }).to_string(),
        json!({ "name": nwc, "lightning_address": address }).to_string(),
        format!(r#"{{"name":"Alice","lightning_address":"{address}","wallet":"{escaped}"}}"#),
        json!({ "name": "Alice", "lightning_address": address, "nwc": nwc.to_uppercase() })
            .to_string(),
    ];
    for body in &bodies {
        let (status, response) = app.authed(ALICE, Method::PUT, "/me/profile", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert_eq!(response["code"], "wallet_secret_rejected");
    }

    // Nor through any other body, such as a new team, a join or a submission.
    let team = json!({ "slug": "wallets", "name": "Wallets", "event_label": nwc }).to_string();
    let (status, _) = app.authed(ALICE, Method::POST, "/teams", &team).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let join = json!({ "join_code": nwc }).to_string();
    let (status, _) = app
        .authed(
            ALICE,
            Method::POST,
            &format!("/teams/{team_id}/join"),
            &join,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let submission = json!({ "submission_url": nwc }).to_string();
    let (status, _) = app
        .authed(
            ALICE,
            Method::PUT,
            &format!("/teams/{team_id}/submission"),
            &submission,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let prize = json!({ "prize_sats": "1000", "wallet": nwc }).to_string();
    let (status, _) = app
        .authed(
            ALICE,
            Method::PUT,
            &format!("/teams/{team_id}/prize"),
            &prize,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let payment = json!({ "bolt11": "lnbc1", "preimage": "00", "metadata": nwc }).to_string();
    let (status, _) = app
        .authed(
            ALICE,
            Method::POST,
            &format!("/teams/{team_id}/payout/items/0/payment"),
            &payment,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stored = database_text(&app).await;
    assert!(stored.contains(&team_id) || stored.contains("Lightning Llamas"));
    let logs = String::from_utf8(logs.0.lock().expect("lock").clone()).expect("UTF-8 logs");
    assert!(logs.contains("/me/profile"), "request logs were captured");
    let responses = app.bodies.lock().expect("lock").join("\n");
    for (place, text) in [
        ("database", &stored),
        ("logs", &logs),
        ("responses", &responses),
    ] {
        let lower = text.to_lowercase();
        assert!(!lower.contains(WALLET_SECRET), "secret in {place}");
        assert!(
            !lower.contains("walletconnect"),
            "connection string in {place}"
        );
    }
}
