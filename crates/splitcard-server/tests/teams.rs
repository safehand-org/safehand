//! Standalone teams: slugs, creation and joining from profiles.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, BOB, DAYO, EVENT_LABEL, TEAM_NAME, TEAM_SLUG, TestApp, app, person, pubkey};
use serde_json::{Value, json};

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    let (actual, body) = response;
    assert_eq!(*actual, status, "{body}");
    assert_eq!(body["code"], code, "{body}");
}

async fn team_count(app: &TestApp) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM teams")
        .fetch_one(&app.state.db)
        .await
        .expect("count")
}

#[tokio::test]
async fn a_member_with_a_profile_creates_a_team_with_a_valid_slug() {
    let app = app().await;
    app.profile_ok(ALICE).await;
    let (status, team) = app.post_team(ALICE, TEAM_SLUG).await;
    assert_eq!(status, StatusCode::CREATED, "{team}");
    assert_eq!(team["slug"], TEAM_SLUG);
    assert_eq!(team["name"], TEAM_NAME);
    assert_eq!(team["event_label"], EVENT_LABEL);
    assert_eq!(team["join_code"].as_str().map(str::len), Some(32));
    assert!(team.get("event").is_none() && team.get("payout").is_none());
    let (name, address) = person(ALICE);
    assert_eq!(
        team["participants"],
        json!([{
            "pubkey": pubkey(ALICE),
            "name": name,
            "lightning_address": address,
            "joined_at": common::NOW,
        }])
    );
    let (status, fetched) = app
        .get(&format!("/teams/{}", team["id"].as_str().expect("id")))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["slug"], TEAM_SLUG);
}

#[tokio::test]
async fn teams_can_be_found_by_slug() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let by_id = format!("/teams/{team_id}");
    let by_slug = format!("/teams/by-slug/{TEAM_SLUG}");
    assert_eq!(app.get(&by_slug).await, app.get(&by_id).await);
    let (status, member) = app.authed(BOB, Method::GET, &by_slug, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(member["id"], team_id.as_str());
    assert_eq!(member["join_code"].as_str().map(str::len), Some(32));
    let (_, public) = app.get(&by_slug).await;
    assert!(public.get("join_code").is_none());
    assert!(!public.to_string().contains('@'), "{public}");

    for path in [
        "/teams/by-slug/no-such-team",
        "/teams/by-slug/Lightning-Llamas",
    ] {
        assert_error(
            &app.get(path).await,
            StatusCode::NOT_FOUND,
            "team_not_found",
        );
    }
}

#[tokio::test]
async fn slugs_must_follow_the_rules() {
    let app = app().await;
    app.profile_ok(ALICE).await;
    let too_long = "a".repeat(33);
    for (slug, reason) in [
        ("Lightning-Llamas", "uppercase"),
        ("LLAMAS", "uppercase"),
        ("lightning llamas", "characters"),
        ("llamas!", "characters"),
        ("llamas@home", "characters"),
        ("lláma", "characters"),
        ("ab", "length"),
        ("", "length"),
        (too_long.as_str(), "length"),
        ("admin", "reserved"),
        ("api", "reserved"),
        ("www", "reserved"),
    ] {
        let response = app.post_team(ALICE, slug).await;
        assert_error(&response, StatusCode::UNPROCESSABLE_ENTITY, "invalid_slug");
        assert_eq!(response.1["details"]["reason"], reason, "{slug}");
        assert_eq!(response.1["details"]["field"], "slug");
    }
    for slug in ["abc", &"a".repeat(32), "team_1.x+y", "lightning-llamas"] {
        let (status, body) = app.post_team(ALICE, slug).await;
        assert_eq!(status, StatusCode::CREATED, "{slug}: {body}");
    }
}

#[tokio::test]
async fn slugs_are_unique_across_the_server() {
    let app = app().await;
    app.create_team(ALICE).await;
    app.profile_ok(BOB).await;
    assert_error(
        &app.post_team(BOB, TEAM_SLUG).await,
        StatusCode::CONFLICT,
        "team_slug_taken",
    );
    // Names may repeat; only slugs identify teams.
    let (status, body) = app.post_team(BOB, "other-llamas").await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], TEAM_NAME);
    assert_eq!(team_count(&app).await, 2);
}

#[tokio::test]
async fn creating_a_team_needs_a_profile_a_name_and_an_event_label() {
    let app = app().await;
    assert_error(
        &app.post_team(DAYO, TEAM_SLUG).await,
        StatusCode::CONFLICT,
        "profile_required",
    );
    app.profile_ok(DAYO).await;
    for (name, label, field) in [("", EVENT_LABEL, "name"), (TEAM_NAME, "", "event_label")] {
        let body = json!({ "slug": TEAM_SLUG, "name": name, "event_label": label }).to_string();
        let response = app.authed(DAYO, Method::POST, "/teams", &body).await;
        assert_error(&response, StatusCode::UNPROCESSABLE_ENTITY, "empty_field");
        assert_eq!(response.1["details"]["field"], field);
    }
    assert_eq!(team_count(&app).await, 0);
}

#[tokio::test]
async fn joining_needs_a_profile() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    assert_error(
        &app.join_only(&team_id, &code, DAYO).await,
        StatusCode::CONFLICT,
        "profile_required",
    );
    let participants: i64 = sqlx::query_scalar("SELECT count(*) FROM participants")
        .fetch_one(&app.state.db)
        .await
        .expect("count");
    assert_eq!(participants, 1);
}

#[tokio::test]
async fn creating_or_joining_a_team_needs_a_lightning_address() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    let name_only = json!({ "name": "Dayo" }).to_string();
    let (status, _) = app
        .authed(DAYO, Method::PUT, "/me/profile", &name_only)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_error(
        &app.post_team(DAYO, "dayo-team").await,
        StatusCode::CONFLICT,
        "lightning_address_required",
    );
    assert_error(
        &app.join_only(&team_id, &code, DAYO).await,
        StatusCode::CONFLICT,
        "lightning_address_required",
    );
    assert_eq!(team_count(&app).await, 1);

    app.profile_ok(DAYO).await;
    let (status, team) = app.join_only(&team_id, &code, DAYO).await;
    assert_eq!(status, StatusCode::OK, "{team}");
}

#[tokio::test]
async fn a_participant_s_name_and_address_come_from_their_profile() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    let (status, _) = app.put_profile(BOB, "Bobby", "bobby@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    let (status, team) = app.join_only(&team_id, &code, BOB).await;
    assert_eq!(status, StatusCode::OK, "{team}");
    assert_eq!(team["participants"][1]["name"], "Bobby");
    assert_eq!(
        team["participants"][1]["lightning_address"],
        "bobby@getalby.com"
    );

    // A later profile change shows on the team at once.
    let (status, _) = app.put_profile(BOB, "Bob", "bob@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    let (_, team) = app
        .authed(ALICE, Method::GET, &format!("/teams/{team_id}"), "")
        .await;
    assert_eq!(team["participants"][1]["name"], "Bob");
    assert_eq!(
        team["participants"][1]["lightning_address"],
        "bob@getalby.com"
    );

    // The join request carries no name or address of its own.
    let body = json!({ "join_code": code, "name": "Robert", "lightning_address": "r@blink.sv" })
        .to_string();
    assert_error(
        &app.authed(BOB, Method::POST, &format!("/teams/{team_id}/join"), &body)
            .await,
        StatusCode::BAD_REQUEST,
        "invalid_request_json",
    );
}
