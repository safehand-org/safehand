//! The organizer features are gone: their endpoints are unknown routes and
//! their tables no longer exist.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, app};

#[tokio::test]
async fn event_and_payout_endpoints_are_not_found() {
    let app = app().await;
    let (team_id, _) = app.active_first_card().await;
    for path in [
        "/events".to_owned(),
        "/events/nostr-hack-2026/teams".to_owned(),
        format!("/teams/{team_id}/payout"),
        "/payouts/1/preflight".to_owned(),
        "/payouts/1/items/0/invoice".to_owned(),
        "/payouts/1/items/0/attempt".to_owned(),
        "/payouts/1/items/0/result".to_owned(),
        "/payouts/1/items/0/failed".to_owned(),
        "/payouts/1/items/0/check".to_owned(),
        "/payouts/1/items/0/mark-unpaid".to_owned(),
    ] {
        let (status, body) = app.authed(ALICE, Method::POST, &path, "{}").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert_eq!(body["code"], "not_found", "{path}");
    }
    let (status, body) = app.get("/payouts/1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}

#[tokio::test]
async fn event_and_payout_tables_are_gone() {
    let app = app().await;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&app.state.db)
            .await
            .expect("tables");
    for removed in ["events", "payouts", "payout_items"] {
        assert!(!tables.iter().any(|table| table == removed), "{tables:?}");
    }
    assert!(tables.iter().any(|table| table == "profiles"), "{tables:?}");
}
