//! `GET /invites/{slug}`: the join screen's preview of a team and its card.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, BOB, CHIDI, DAYO, TEAM_SLUG, TestApp, app, first_card, pubkey};
use serde_json::{Value, json};

async fn invite(app: &TestApp, viewer: Option<u8>, query: &str) -> Value {
    let path = format!("/invites/{TEAM_SLUG}{query}");
    let (status, body) = match viewer {
        None => app.get(&path).await,
        Some(viewer) => app.authed(viewer, Method::GET, &path, "").await,
    };
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn unknown_slugs_are_not_found() {
    let app = app().await;
    app.create_team(ALICE).await;
    for path in ["/invites/no-such-team", "/invites/LIGHTNING-LLAMAS"] {
        let (status, body) = app.get(path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["code"], "team_not_found");
    }
}

#[tokio::test]
async fn the_preview_reports_the_join_code_without_echoing_it() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    // Creating the team returned the code to Alice; check only what follows.
    app.bodies.lock().expect("lock").clear();
    let expected = |ok: bool| {
        json!({
            "team_id": team_id,
            "slug": TEAM_SLUG,
            "name": common::TEAM_NAME,
            "event_label": common::EVENT_LABEL,
            "created_by_name": "Alice",
            "join_code_ok": ok,
            "viewer": "anonymous",
            "participant_count": 1,
            "card": null,
        })
    };
    assert_eq!(invite(&app, None, "").await, expected(false));
    assert_eq!(
        invite(&app, None, &format!("?join={code}")).await,
        expected(true)
    );
    for wrong in ["?join=", "?join=00", "?code=x", &format!("?join={code}0")] {
        assert_eq!(invite(&app, None, wrong).await, expected(false), "{wrong}");
    }
    for body in app.bodies.lock().expect("lock").iter() {
        assert!(!body.contains(&code), "the join code leaked: {body}");
    }
}

#[tokio::test]
async fn the_viewer_state_follows_the_join_and_sign_steps() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    let viewer = |body: Value| body["viewer"].as_str().expect("viewer").to_owned();

    assert_eq!(viewer(invite(&app, Some(DAYO), "").await), "needs_profile");
    app.profile_ok(DAYO).await;
    assert_eq!(viewer(invite(&app, Some(DAYO), "").await), "needs_join");
    for joiner in [BOB, CHIDI] {
        let (status, _) = app.join(&team_id, &code, joiner).await;
        assert_eq!(status, StatusCode::OK);
    }
    assert_eq!(viewer(invite(&app, Some(BOB), "").await), "joined");

    let hash = app.submit_ok(&team_id, &first_card()).await;
    assert_eq!(
        viewer(invite(&app, Some(BOB), "").await),
        "missing_signature"
    );
    app.sign_ok(&hash, BOB).await;
    assert_eq!(viewer(invite(&app, Some(BOB), "").await), "signed");
    assert_eq!(
        viewer(invite(&app, Some(ALICE), "").await),
        "missing_signature"
    );
    // Dayo joined the team but is not on the card.
    let (status, _) = app.join_only(&team_id, &code, DAYO).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(viewer(invite(&app, Some(DAYO), "").await), "joined");

    app.sign_ok(&hash, ALICE).await;
    app.sign_ok(&hash, CHIDI).await;
    for number in [ALICE, BOB, DAYO] {
        assert_eq!(viewer(invite(&app, Some(number), "").await), "locked");
    }
    assert_eq!(viewer(invite(&app, None, "").await), "anonymous");
}

#[tokio::test]
async fn the_card_shows_shares_and_signatures_and_only_the_viewer_s_address() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.sign_ok(&hash, ALICE).await;

    let body = invite(&app, Some(BOB), "").await;
    let card = &body["card"];
    assert_eq!(card["hash"], hash.as_str());
    assert_eq!(card["status"], "draft");
    assert_eq!(card["created_by_name"], "Alice");
    assert_eq!(card["created_at"], common::NOW);
    assert_eq!(card["activated_at"], Value::Null);
    assert_eq!(card["signed_count"], 1);
    assert_eq!(
        card["members"],
        json!([
            { "pubkey": pubkey(ALICE), "name": "Alice", "weight": "36",
              "share_basis_points": 3600, "signed": true },
            { "pubkey": pubkey(BOB), "name": "Bob", "weight": "35",
              "share_basis_points": 3500, "signed": false,
              "lightning_address": "bob@getalby.com" },
            { "pubkey": pubkey(CHIDI), "name": "Chidi", "weight": "29",
              "share_basis_points": 2900, "signed": false },
        ])
    );

    let anonymous = invite(&app, None, "").await;
    assert!(!anonymous.to_string().contains('@'), "{anonymous}");
}

#[tokio::test]
async fn a_bad_header_is_rejected_not_treated_as_anonymous() {
    let app = app().await;
    app.create_team(ALICE).await;
    let (status, body) = app
        .raw(
            Method::GET,
            &format!("/invites/{TEAM_SLUG}"),
            Some("Nostr not-base64!"),
            b"",
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "auth_malformed");
}
