//! `GET /me`: the signed-in member's profile and team summaries.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, BOB, CHIDI, DAYO, NOW, TEAM_SLUG, TestApp, app, card, first_card, pubkey};
use serde_json::{Value, json};

async fn me(app: &TestApp, number: u8) -> Value {
    let (status, body) = app.authed(number, Method::GET, "/me", "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn me_needs_a_signed_in_member() {
    let app = app().await;
    let (status, body) = app.get("/me").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "auth_missing");
}

#[tokio::test]
async fn before_a_profile_there_is_nothing() {
    let app = app().await;
    assert_eq!(
        me(&app, DAYO).await,
        json!({
            "pubkey": pubkey(DAYO),
            "profile": null,
            "teams": [],
            "needs_signature": null,
            "summary": { "received_sats": "0", "paid_team_count": 0 },
        })
    );
}

#[tokio::test]
async fn a_new_team_is_a_draft_without_a_card() {
    let app = app().await;
    let (team_id, _) = app.create_team(ALICE).await;
    let body = me(&app, ALICE).await;
    assert_eq!(
        body["profile"],
        json!({
            "pubkey": pubkey(ALICE),
            "name": "Alice",
            "lightning_address": "alice@blink.sv",
            "updated_at": NOW,
        })
    );
    assert_eq!(
        body["teams"],
        json!([{
            "team_id": team_id,
            "slug": TEAM_SLUG,
            "name": common::TEAM_NAME,
            "event_label": common::EVENT_LABEL,
            "list_status": "draft",
            "phase": null,
            "you_received_sats": null,
            "card_hash": null,
            "signed_count": null,
            "member_count": null,
            "participant_count": 1,
            "you_must_sign": false,
            "joined_at": NOW,
            "updated_at": NOW,
        }])
    );
    assert_eq!(body["needs_signature"], Value::Null);
}

#[tokio::test]
async fn members_who_have_not_signed_are_asked_to() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    app.clock.advance(10);
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.clock.advance(5);
    app.sign_ok(&hash, ALICE).await;

    let alice = me(&app, ALICE).await;
    assert_eq!(alice["needs_signature"], Value::Null);
    let team = &alice["teams"][0];
    assert_eq!(team["list_status"], "draft");
    assert_eq!(team["card_hash"], hash.as_str());
    assert_eq!(team["signed_count"], 1);
    assert_eq!(team["member_count"], 3);
    assert_eq!(team["participant_count"], 3);
    assert_eq!(team["you_must_sign"], false);
    assert_eq!(team["updated_at"], NOW + 15);

    let bob = me(&app, BOB).await;
    assert_eq!(bob["teams"][0]["you_must_sign"], true);
    let ask = &bob["needs_signature"];
    assert_eq!(ask["team_id"], team_id.as_str());
    assert_eq!(ask["slug"], TEAM_SLUG);
    assert_eq!(ask["card_hash"], hash.as_str());
    assert_eq!(ask["inviter_name"], "Alice");
    assert_eq!(ask["share_basis_points"], 3500);
    assert_eq!(ask["member_count"], 3);
    assert_eq!(ask["invited_at"], NOW + 10);
    assert_eq!(ask["join_code"].as_str().map(str::len), Some(32));

    // Nobody else's Lightning Address appears.
    let text = bob.to_string();
    assert!(
        !text.contains("alice@") && !text.contains("chidi@"),
        "{text}"
    );
}

#[tokio::test]
async fn a_fully_signed_team_is_locked_and_waiting_for_the_prize() {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    for number in [ALICE, BOB, CHIDI] {
        let body = me(&app, number).await;
        let team = &body["teams"][0];
        assert_eq!(team["list_status"], "locked");
        assert_eq!(team["phase"], "waiting-for-prize");
        assert_eq!(team["card_hash"], hash.as_str());
        assert_eq!(team["signed_count"], 3);
        assert_eq!(team["you_must_sign"], false);
        assert_eq!(body["needs_signature"], Value::Null);
    }
}

#[tokio::test]
async fn a_draft_replacing_the_active_card_asks_its_members_to_sign() {
    let app = app().await;
    let (team_id, active) = app.active_first_card().await;
    let replacement = card(&[(ALICE, 50), (BOB, 50)], Some(&active));
    let draft = app.submit_ok(&team_id, &replacement).await;

    let bob = me(&app, BOB).await;
    let team = &bob["teams"][0];
    // The row still describes the locked card.
    assert_eq!(team["list_status"], "locked");
    assert_eq!(team["card_hash"], active.as_str());
    assert_eq!(team["you_must_sign"], true);
    assert_eq!(bob["needs_signature"]["card_hash"], draft.as_str());
    assert_eq!(bob["needs_signature"]["share_basis_points"], 5000);

    // Chidi is not on the replacement, so nothing asks him to sign it.
    let chidi = me(&app, CHIDI).await;
    assert_eq!(chidi["teams"][0]["you_must_sign"], false);
    assert_eq!(chidi["needs_signature"], Value::Null);
}

#[tokio::test]
async fn a_draft_that_can_never_activate_asks_nobody() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    app.submit_ok(&team_id, &first_card()).await;
    let pair = app
        .submit_ok(&team_id, &card(&[(ALICE, 50), (BOB, 50)], None))
        .await;
    assert_eq!(me(&app, CHIDI).await["teams"][0]["you_must_sign"], false);
    app.sign_ok(&pair, ALICE).await;
    app.sign_ok(&pair, BOB).await;

    // The three-member draft is stale now that the pair card is active.
    let chidi = me(&app, CHIDI).await;
    assert_eq!(chidi["teams"][0]["card_hash"], pair.as_str());
    assert_eq!(chidi["teams"][0]["you_must_sign"], false);
    assert_eq!(chidi["needs_signature"], Value::Null);
}

#[tokio::test]
async fn teams_are_listed_most_recently_updated_first() {
    let app = app().await;
    let (first_id, code) = app.create_team(ALICE).await;
    app.clock.advance(10);
    let (status, second) = app.post_team(ALICE, "second-team").await;
    assert_eq!(status, StatusCode::CREATED, "{second}");
    let slugs = |body: &Value| -> Vec<String> {
        body["teams"]
            .as_array()
            .expect("teams")
            .iter()
            .map(|team| team["slug"].as_str().expect("slug").to_owned())
            .collect()
    };
    assert_eq!(slugs(&me(&app, ALICE).await), ["second-team", TEAM_SLUG]);

    app.clock.advance(10);
    let (status, _) = app.join(&first_id, &code, BOB).await;
    assert_eq!(status, StatusCode::OK);
    let alice = me(&app, ALICE).await;
    assert_eq!(slugs(&alice), [TEAM_SLUG, "second-team"]);
    assert_eq!(alice["teams"][0]["updated_at"], NOW + 20);
    assert_eq!(alice["teams"][0]["joined_at"], NOW);

    // Bob sees only the team he joined.
    assert_eq!(slugs(&me(&app, BOB).await), [TEAM_SLUG]);
}
