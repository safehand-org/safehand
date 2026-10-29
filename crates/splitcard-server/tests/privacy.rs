//! Join codes are only shown to participants. The secret key check is in
//! `secret_key.rs`, alone in its binary so its log capture sees every event.

mod common;

use axum::http::{Method, StatusCode};
use common::{BOB, EVE, app};

#[tokio::test]
async fn join_codes_are_hidden_from_non_participants() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let path = format!("/teams/{team_id}");

    let (status, anonymous) = app.get(&path).await;
    assert_eq!(status, StatusCode::OK);
    assert!(anonymous.get("join_code").is_none(), "{anonymous}");
    assert_eq!(anonymous["participants"].as_array().map(Vec::len), Some(3));

    let (status, outsider) = app.authed(EVE, Method::GET, &path, "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(outsider.get("join_code").is_none(), "{outsider}");

    let (status, member) = app.authed(BOB, Method::GET, &path, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(member["join_code"].as_str().map(str::len), Some(32));

    // A present but invalid header is rejected, not treated as anonymous.
    let (status, body) = app
        .raw(Method::GET, &path, Some("Nostr not-base64!"), b"")
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "auth_malformed");
}
