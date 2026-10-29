//! Member profiles: each member sets their own, and their Lightning Address
//! is shown only to themselves and their teammates.

mod common;

use axum::http::{Method, StatusCode};
use common::{ALICE, BOB, CHIDI, EVE, TestApp, app, person, pubkey};
use serde_json::{Value, json};

async fn profile_as(app: &TestApp, viewer: Option<u8>, of: u8) -> Value {
    let path = format!("/profiles/{}", pubkey(of));
    let (status, profile) = match viewer {
        None => app.get(&path).await,
        Some(viewer) => app.authed(viewer, Method::GET, &path, "").await,
    };
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile
}

#[tokio::test]
async fn members_create_and_update_their_profile() {
    let app = app().await;
    let (status, created) = app.put_profile(ALICE, "Alice", "alice@blink.sv").await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        created,
        json!({
            "pubkey": pubkey(ALICE),
            "name": "Alice",
            "lightning_address": "alice@blink.sv",
            "updated_at": common::NOW,
        })
    );

    app.clock.advance(5);
    let (status, updated) = app
        .put_profile(ALICE, "Alice O.", "alice@getalby.com")
        .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["name"], "Alice O.");
    assert_eq!(updated["lightning_address"], "alice@getalby.com");
    assert_eq!(updated["updated_at"], common::NOW + 5);
    assert_eq!(profile_as(&app, Some(ALICE), ALICE).await, updated);
}

#[tokio::test]
async fn profiles_need_a_name_and_a_valid_lightning_address() {
    let app = app().await;
    for address in [
        "Alice@Blink.sv",
        "alice",
        "alice@localhost",
        "",
        "alice@blink.sv ",
    ] {
        let (status, body) = app.put_profile(ALICE, "Alice", address).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{address}: {body}"
        );
        assert_eq!(body["code"], "invalid_lightning_address");
    }
    let (status, body) = app.put_profile(ALICE, "", "alice@blink.sv").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "empty_field");
    let extra =
        json!({ "name": "Alice", "lightning_address": "alice@blink.sv", "pubkey": pubkey(BOB) })
            .to_string();
    let (status, body) = app.authed(ALICE, Method::PUT, "/me/profile", &extra).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_request_json");
    let (status, body) = app.get(&format!("/profiles/{}", pubkey(ALICE))).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "nothing was stored: {body}");
    assert_eq!(body["code"], "profile_not_found");
}

#[tokio::test]
async fn only_the_signed_in_member_changes_their_profile() {
    let app = app().await;
    app.profile_ok(ALICE).await;
    app.profile_ok(BOB).await;

    // Bob's request changes Bob's profile, never Alice's.
    let (status, mine) = app.put_profile(BOB, "Alice", "bob@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine["pubkey"], pubkey(BOB));
    let alice = profile_as(&app, Some(ALICE), ALICE).await;
    assert_eq!(alice["name"], "Alice");
    assert_eq!(alice["lightning_address"], "alice@blink.sv");

    // There is no way to write someone else's profile, signed in or not.
    let body = json!({ "name": "Mallory", "lightning_address": "mallory@blink.sv" }).to_string();
    let path = format!("/profiles/{}", pubkey(ALICE));
    let (status, _) = app.authed(BOB, Method::PUT, &path, &body).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    let (status, body) = app
        .raw(Method::PUT, "/me/profile", None, body.as_bytes())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(profile_as(&app, Some(ALICE), ALICE).await["name"], "Alice");
}

#[tokio::test]
async fn profiles_can_change_while_on_a_team_with_an_active_card() {
    let app = app().await;
    app.active_first_card().await;
    let (status, body) = app.put_profile(BOB, "Robert", "robert@getalby.com").await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn the_lightning_address_is_hidden_from_strangers_and_shown_to_teammates() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    app.profile_ok(EVE).await;
    let (name, address) = person(BOB);

    for viewer in [None, Some(EVE)] {
        let profile = profile_as(&app, viewer, BOB).await;
        assert_eq!(
            profile,
            json!({ "pubkey": pubkey(BOB), "name": name }),
            "{viewer:?}"
        );
    }
    for viewer in [BOB, ALICE, CHIDI] {
        let profile = profile_as(&app, Some(viewer), BOB).await;
        assert_eq!(profile["lightning_address"], address, "{viewer}");
    }

    // The team view follows the same rule.
    let path = format!("/teams/{team_id}");
    let (_, public) = app.get(&path).await;
    let (_, stranger) = app.authed(EVE, Method::GET, &path, "").await;
    for team in [&public, &stranger] {
        assert_eq!(team["participants"][1]["name"], name);
        assert!(
            team["participants"][1].get("lightning_address").is_none(),
            "{team}"
        );
        assert!(team.get("join_code").is_none());
    }
    let (_, teammate) = app.authed(CHIDI, Method::GET, &path, "").await;
    assert_eq!(teammate["participants"][1]["lightning_address"], address);
    for team in [&public, &stranger] {
        assert!(!team.to_string().contains('@'), "an address leaked: {team}");
    }
}

#[tokio::test]
async fn unknown_profiles_are_not_found() {
    let app = app().await;
    for path in [
        format!("/profiles/{}", pubkey(EVE)),
        "/profiles/not-a-key".to_owned(),
        format!("/profiles/{}", pubkey(EVE).to_uppercase()),
    ] {
        let (status, body) = app.get(&path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["code"], "profile_not_found");
    }
}

#[tokio::test]
async fn new_members_sign_up_with_a_name_and_add_their_address_later() {
    let app = app().await;
    let body = json!({ "name": "Alice", "bio": "Builds things" }).to_string();
    let (status, created) = app.authed(ALICE, Method::PUT, "/me/profile", &body).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        created,
        json!({
            "pubkey": pubkey(ALICE),
            "name": "Alice",
            "bio": "Builds things",
            "updated_at": common::NOW,
        })
    );

    // Leaving out the address or the bio keeps what is stored.
    let (status, updated) = app.put_profile(ALICE, "Alice", "alice@blink.sv").await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["lightning_address"], "alice@blink.sv");
    assert_eq!(updated["bio"], "Builds things");
    for body in [
        json!({ "name": "Alice O." }),
        json!({ "name": "Alice O.", "lightning_address": null, "bio": null }),
    ] {
        let (status, kept) = app
            .authed(ALICE, Method::PUT, "/me/profile", &body.to_string())
            .await;
        assert_eq!(status, StatusCode::OK, "{kept}");
        assert_eq!(kept["name"], "Alice O.");
        assert_eq!(kept["lightning_address"], "alice@blink.sv");
        assert_eq!(kept["bio"], "Builds things");
    }

    // An empty bio clears it.
    let body = json!({ "name": "Alice O.", "bio": "" }).to_string();
    let (status, cleared) = app.authed(ALICE, Method::PUT, "/me/profile", &body).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(cleared.get("bio").is_none(), "{cleared}");
    assert_eq!(cleared["lightning_address"], "alice@blink.sv");
}

#[tokio::test]
async fn bios_are_public_and_limited_in_length() {
    let app = app().await;
    let bio = "é".repeat(splitcard_server::routes::profiles::MAX_BIO_CHARS);
    let body = json!({ "name": "Alice", "bio": bio }).to_string();
    let (status, _) = app.authed(ALICE, Method::PUT, "/me/profile", &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(profile_as(&app, None, ALICE).await["bio"], bio.as_str());

    let body = json!({ "name": "Alice", "bio": format!("{bio}x") }).to_string();
    let (status, body) = app.authed(ALICE, Method::PUT, "/me/profile", &body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "bio_too_long");
    assert_eq!(body["details"]["field"], "bio");
}
