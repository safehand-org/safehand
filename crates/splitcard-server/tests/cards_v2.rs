//! Version 2 cards: members listed by npub who need no account, payout
//! addresses signed by each member, declines, and the pending-card list.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    ALICE, BOB, CHIDI, DAYO, EVE, TEAM_NAME, TestApp, app, card, pubkey, signature, v2_card,
    v2_signature,
};
use serde_json::{Value, json};

/// Alice's team, with only Alice as a participant.
async fn alice_team(app: &TestApp) -> String {
    app.create_team(ALICE).await.0
}

/// Alice 2, Bob 1, Chidi 1: Bob and Chidi have never used Split Card.
fn draft() -> splitcard_core::card::Card {
    v2_card(&[(ALICE, 2), (BOB, 1), (CHIDI, 1)], None)
}

/// A name-only profile, as new users get at signup.
async fn name_only(app: &TestApp, number: u8, name: &str) {
    let body = json!({ "name": name }).to_string();
    let (status, response) = app.authed(number, Method::PUT, "/me/profile", &body).await;
    assert_eq!(status, StatusCode::OK, "{response}");
}

async fn sign_v2(
    app: &TestApp,
    hash: &str,
    signer: u8,
    address: Option<&str>,
) -> (StatusCode, Value) {
    app.sign(hash, &v2_signature(signer, hash, app.now(), address))
        .await
}

/// Alice submits the draft and signs it with her address.
async fn alice_signed_draft(app: &TestApp) -> (String, String) {
    let team_id = alice_team(app).await;
    let hash = app.submit_ok(&team_id, &draft()).await;
    let (status, view) = sign_v2(app, &hash, ALICE, Some("alice@blink.sv")).await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    (team_id, hash)
}

async fn me(app: &TestApp, number: u8) -> Value {
    let (status, me) = app.authed(number, Method::GET, "/me", "").await;
    assert_eq!(status, StatusCode::OK, "{me}");
    me
}

#[tokio::test]
async fn a_version_2_card_lists_members_who_have_no_account() {
    let app = app().await;
    let team_id = alice_team(&app).await;
    let (status, view) = app.submit_card(ALICE, &team_id, &draft()).await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["status"], "draft");
    let members = view["members"].as_array().expect("members");
    assert_eq!(members.len(), 3);
    assert_eq!(members[0]["name"], "Alice");
    assert_eq!(members[0]["share_basis_points"], 5000);
    assert_eq!(
        members[1]["name"],
        Value::Null,
        "Bob has no label or profile"
    );
    assert!(
        members[1]["npub"]
            .as_str()
            .is_some_and(|npub| npub.starts_with("npub1"))
    );
    assert_eq!(members[1]["signed"], false);
    assert_eq!(members[1]["lightning_address"], Value::Null);
}

#[tokio::test]
async fn the_submitter_must_be_listed_on_a_version_2_card() {
    let app = app().await;
    let team_id = alice_team(&app).await;
    let without_alice = v2_card(&[(BOB, 1), (CHIDI, 1)], None);
    let (status, body) = app.submit_card(ALICE, &team_id, &without_alice).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "submitter_not_member");
}

#[tokio::test]
async fn signing_attaches_the_signed_address_and_joins_the_team() {
    let app = app().await;
    let (team_id, hash) = alice_signed_draft(&app).await;
    name_only(&app, BOB, "Bob").await;
    let (status, view) = sign_v2(&app, &hash, BOB, Some("bob@getalby.com")).await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(
        view["signing"]["signed"],
        json!([pubkey(ALICE), pubkey(BOB)])
    );
    assert_eq!(
        view["members"][1]["name"], "Bob",
        "the profile name fills in"
    );
    assert_eq!(view["members"][1]["lightning_address"], "bob@getalby.com");
    let (_, team) = app.get(&format!("/teams/{team_id}")).await;
    let participants: Vec<&str> = team["participants"]
        .as_array()
        .expect("participants")
        .iter()
        .filter_map(|participant| participant["pubkey"].as_str())
        .collect();
    assert!(participants.contains(&pubkey(BOB).as_str()), "{team}");
    assert_eq!(team["draft_card_hash"], hash);
}

#[tokio::test]
async fn a_version_2_signature_without_a_lud16_tag_is_refused() {
    let app = app().await;
    let (_, hash) = alice_signed_draft(&app).await;
    name_only(&app, BOB, "Bob").await;
    let (status, body) = sign_v2(&app, &hash, BOB, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "missing_lud16_tag");
}

#[tokio::test]
async fn an_unreachable_address_is_refused_with_the_providers_reason() {
    let app = app().await;
    let (_, hash) = alice_signed_draft(&app).await;
    name_only(&app, BOB, "Bob").await;
    let (status, body) = sign_v2(&app, &hash, BOB, Some("unreachable@getalby.com")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "lightning_address_unreachable");
    assert_eq!(
        body["details"]["lightning_address"],
        "unreachable@getalby.com"
    );
    assert_eq!(body["details"]["reason"], "HTTP 404: no such user");
}

#[tokio::test]
async fn a_signer_needs_a_profile() {
    let app = app().await;
    let (_, hash) = alice_signed_draft(&app).await;
    let (status, body) = sign_v2(&app, &hash, BOB, Some("bob@getalby.com")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "profile_required");
}

#[tokio::test]
async fn a_non_member_cannot_sign() {
    let app = app().await;
    let (_, hash) = alice_signed_draft(&app).await;
    name_only(&app, DAYO, "Dayo").await;
    let (status, body) = sign_v2(&app, &hash, DAYO, Some("dayo@strike.me")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "non_member");
}

#[tokio::test]
async fn a_declined_draft_can_never_activate() {
    let app = app().await;
    let (_, hash) = alice_signed_draft(&app).await;
    name_only(&app, BOB, "Bob").await;
    let (status, _) = sign_v2(&app, &hash, BOB, Some("bob@getalby.com")).await;
    assert_eq!(status, StatusCode::CREATED);

    // Only an unsigned member may decline.
    let decline = format!("/cards/{hash}/decline");
    let (status, body) = app.authed(BOB, Method::POST, &decline, "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "already_signed");
    let (status, body) = app.authed(DAYO, Method::POST, &decline, "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "not_a_card_member");

    let (status, view) = app.authed(CHIDI, Method::POST, &decline, "").await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["status"], "declined");
    assert_eq!(view["declined"][0]["pubkey"], pubkey(CHIDI));
    assert_eq!(view["members"][2]["declined"], true);
    let (status, again) = app.authed(CHIDI, Method::POST, &decline, "").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "declining twice changes nothing: {again}"
    );

    name_only(&app, CHIDI, "Chidi").await;
    let (status, body) = sign_v2(&app, &hash, CHIDI, Some("chidi@walletofsatoshi.com")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "card_declined");
    let (_, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(view["status"], "declined");
}

#[tokio::test]
async fn me_lists_a_pending_card_for_someone_who_never_used_split_card() {
    let app = app().await;
    let (team_id, hash) = alice_signed_draft(&app).await;
    // Before any profile: GET /me works and already lists the card.
    let before = me(&app, BOB).await;
    assert_eq!(before["profile"], Value::Null);
    assert_eq!(before["pending_cards"].as_array().map(Vec::len), Some(1));

    name_only(&app, BOB, "Bob").await;
    let pending = me(&app, BOB).await["pending_cards"].clone();
    assert_eq!(
        pending,
        json!([{
            "team_id": team_id,
            "slug": "lightning-llamas",
            "name": TEAM_NAME,
            "card_hash": hash,
            "creator_name": "Alice",
            "share_basis_points": 2500,
            "created_at": app.now(),
        }])
    );
    // Alice signed, so nothing is pending for her; Bob's list empties once he signs.
    assert_eq!(me(&app, ALICE).await["pending_cards"], json!([]));
    let (status, _) = sign_v2(&app, &hash, BOB, Some("bob@getalby.com")).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(me(&app, BOB).await["pending_cards"], json!([]));
    // A decline by anyone removes the card from everyone's list.
    let (status, _) = app
        .authed(CHIDI, Method::POST, &format!("/cards/{hash}/decline"), "")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me(&app, CHIDI).await["pending_cards"], json!([]));
}

#[tokio::test]
async fn payouts_go_to_the_addresses_members_signed() {
    let app = app().await;
    let (team_id, hash) = alice_signed_draft(&app).await;
    for (number, name, address) in [
        (BOB, "Bob", "bob.signed@getalby.com"),
        (CHIDI, "Chidi", "chidi.signed@walletofsatoshi.com"),
    ] {
        name_only(&app, number, name).await;
        let (status, view) = sign_v2(&app, &hash, number, Some(address)).await;
        assert_eq!(status, StatusCode::CREATED, "{view}");
    }
    let (_, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(view["status"], "active");

    let body = json!({ "prize_sats": "100000" }).to_string();
    let path = format!("/teams/{team_id}/prize");
    let mut payout = Value::Null;
    for number in [ALICE, BOB, CHIDI] {
        let (status, response) = app.authed(number, Method::PUT, &path, &body).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        payout = response;
    }
    assert_eq!(payout["phase"], "prize-set-waiting");
    let items: Vec<(String, String, String)> = payout["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| {
            let text = |key: &str| item[key].as_str().unwrap_or_default().to_owned();
            (text("name"), text("lightning_address"), text("amount_sats"))
        })
        .collect();
    assert_eq!(
        items,
        [
            ("Alice", "alice@blink.sv", "50000"),
            ("Bob", "bob.signed@getalby.com", "25000"),
            ("Chidi", "chidi.signed@walletofsatoshi.com", "25000"),
        ]
        .map(|(name, address, sats)| (
            name.to_owned(),
            address.to_owned(),
            sats.to_owned()
        ))
    );

    // The verify report shows which address each member signed.
    let (_, report) = app.get(&format!("/cards/{hash}/verify")).await;
    let check = report["checks"]
        .as_array()
        .and_then(|checks| {
            checks
                .iter()
                .find(|check| check["name"] == "signatures_valid")
        })
        .expect("signatures_valid");
    assert_eq!(check["result"], "pass");
    let signed: Vec<&str> = check["details"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(|event| event["lightning_address"].as_str())
        .collect();
    assert_eq!(
        signed,
        [
            "alice@blink.sv",
            "bob.signed@getalby.com",
            "chidi.signed@walletofsatoshi.com"
        ]
    );
}

#[tokio::test]
async fn version_1_cards_still_follow_their_own_rules() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app
        .submit_ok(&team_id, &card(&[(ALICE, 1), (BOB, 1)], None))
        .await;
    // A lud16 tag has no place on a version 1 signature.
    let (status, body) = sign_v2(&app, &hash, ALICE, Some("alice@blink.sv")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "unexpected_lud16_tag");
    let (status, view) = app.sign(&hash, &signature(BOB, &hash, app.now())).await;
    assert_eq!(status, StatusCode::CREATED, "{view}");
    assert_eq!(view["members"][1]["lightning_address"], "bob@getalby.com");
    // Members of a version 1 card must have joined: Eve has not.
    let with_eve = card(&[(ALICE, 1), (EVE, 1)], None);
    let (status, body) = app.submit_card(ALICE, &team_id, &with_eve).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "member_not_participant");
}

#[tokio::test]
async fn nsec_keys_in_a_body_are_refused_and_never_echoed() {
    let app = app().await;
    let nsec = "nsec1vl029mgpspedva04g90vltkh6fvh240zqtv9k0t9af8935ke9laqsnlfe5";
    for body in [
        json!({ "name": nsec }).to_string(),
        json!({ "name": "Ann", "bio": nsec.to_uppercase() }).to_string(),
        // Escaped in JSON.
        format!(r#"{{"name":"n{}"}}"#, &nsec[1..]),
    ] {
        let (status, response) = app.authed(DAYO, Method::PUT, "/me/profile", &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert_eq!(response["code"], "secret_key_rejected");
        assert!(
            !response
                .to_string()
                .to_ascii_lowercase()
                .contains(&nsec[5..])
        );
    }
    // An npub is fine.
    let npub_bio = json!({ "name": "Ann", "bio": "npub1vl029mgpspedva04g90vltkh6fvh240zqtv9k0t9af8935ke9laqsnlfe5" });
    let (status, response) = app
        .authed(DAYO, Method::PUT, "/me/profile", &npub_bio.to_string())
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
}

// The real version 1 card timestamped in Bitcoin block 973,120, rebuilt here
// from its public data: the card and its three member signatures.

const REAL_CARD: &str = r#"{"card_id":"ots-check-1792958355","event":"ots-check-1792958355","members":[{"name":"Alice","payout":{"lightning":{"address":"alice@blink.sv"}},"pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","weight":36},{"name":"Bob","payout":{"lightning":{"address":"bob@getalby.com"}},"pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","weight":35},{"name":"Chidi","payout":{"lightning":{"address":"chidi@walletofsatoshi.com"}},"pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","weight":29}],"rules":{"peer_review":null},"supersedes":null,"team":"Llamas","version":1}"#;
const REAL_HASH: &str = "464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a";
const REAL_SIGNATURES: [&str; 3] = [
    r#"{"id":"03dae6bf7e91891c473911a924a2fff31ccd6e212fed9ac3d49813a80ed9d705","pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"4116b055046d05ed594aceaa72f865ea3ab803218d2a0022c26dffab50eef747a36601f75155b63ab85ef6d01535ffb0d9bfa2bc932436cde8d0ffbac6109964"}"#,
    r#"{"id":"2f212fa7224252df5abaa03d2e1f39b216d4417297410804e5fd16a35ab16f2a","pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"0b65e7142f7299f522f95b52abf7391165d4ac59b4b0be4c8cc5e63559e43f79e66d7247dabbfc27bc1ad9ff6bf17acbc099f64cea5eec776f2905579a5027c7"}"#,
    r#"{"id":"064502d94fb1e6d13118aa2fdaa2e1ec5c7f1df17213883788416c41064160dc","pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"7cb44a25a234e15eaaca331d52d196aef9bd9fe817295da9491af72aae3a903b6a071789af5ed23aa5269fe1500262df86c8186ef06a478bde792d9e96c2e89a"}"#,
];

#[tokio::test]
async fn the_real_version_1_card_still_verifies() {
    let app = app().await;
    let db = &app.state.db;
    sqlx::query(
        "INSERT INTO teams (id, slug, name, event_label, join_code, created_at) \
         VALUES (1, 'llamas', 'Llamas', 'ots-check-1792958355', ?, 1792958355)",
    )
    .bind("0".repeat(32))
    .execute(db)
    .await
    .expect("team");
    sqlx::query(
        "INSERT INTO cards (hash, team_id, canonical_json, status, created_by, created_at) \
         VALUES (?, 1, ?, 'draft', ?, 1792958355)",
    )
    .bind(REAL_HASH)
    .bind(REAL_CARD)
    .bind("1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f")
    .execute(db)
    .await
    .expect("card");
    for event in REAL_SIGNATURES {
        let (status, view) = app.sign(REAL_HASH, event).await;
        assert_eq!(status, StatusCode::CREATED, "{view}");
    }
    let (_, view) = app.get(&format!("/cards/{REAL_HASH}")).await;
    assert_eq!(view["status"], "active");
    assert_eq!(view["card"], REAL_CARD);

    let (status, report) = app.get(&format!("/cards/{REAL_HASH}/verify")).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["card_hash"], REAL_HASH);
    assert_eq!(report["canonical_card_json"], REAL_CARD);
    let results: Vec<(&str, &str)> = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|check| {
            (
                check["name"].as_str().unwrap_or_default(),
                check["result"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    // Relays and the timestamp are pending here: this test publishes nothing.
    assert_eq!(
        results,
        [
            ("card_hash_matches", "pass"),
            ("card_valid", "pass"),
            ("signatures_valid", "pass"),
            ("all_members_signed", "pass"),
            ("status_consistent", "pass"),
            ("supersede_valid", "not_applicable"),
            ("published_to_relays", "pending"),
            ("timestamp", "pending"),
        ]
    );
    // Version 1 signature entries keep their exact shape: no address key.
    let events = &report["checks"][2]["details"]["events"];
    for event in events.as_array().expect("events") {
        let mut keys: Vec<&str> = event
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["error", "event_id", "pubkey", "result"]);
    }
}
