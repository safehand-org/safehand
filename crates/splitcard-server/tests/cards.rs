//! Teams, card submission, signatures, activation and superseding.

mod common;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use common::{
    ALICE, BOB, CHIDI, DAYO, EVE, EVENT_LABEL, SERVER_SECRET_HEX, TEAM_NAME, TestApp, app, card,
    first_card, person, pubkey, signature, v1,
};
use nostr::{event::Event, key::Keys};
use serde_json::{Value, json};
use splitcard_core::hash::{canonical_json, card_hash_hex};

fn assert_error(response: (StatusCode, Value), status: StatusCode, code: &str) {
    let (actual, body) = response;
    assert_eq!(actual, status, "{body}");
    assert_eq!(body["code"], code, "{body}");
}

async fn card_status(app: &TestApp, hash: &str) -> Value {
    let (status, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    view
}

/// Alice, Bob and Chidi with an active first card; returns the team id and hash.
async fn active_first_card(app: &TestApp) -> (String, String) {
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }
    (team_id, hash)
}

#[tokio::test]
async fn full_happy_path() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;

    let (status, team) = app.get(&format!("/teams/{team_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(team["participants"].as_array().map(Vec::len), Some(3));

    let card = first_card();
    let (status, submitted) = app.submit_card(ALICE, &team_id, &card).await;
    assert_eq!(status, StatusCode::CREATED, "{submitted}");
    let hash = card_hash_hex(&card).expect("hash");
    assert_eq!(submitted["hash"], hash);
    assert_eq!(submitted["status"], "draft");
    assert_eq!(submitted["card"], canonical_json(&card).expect("canonical"));
    assert_eq!(submitted["signing"]["state"], "draft");
    assert_eq!(submitted["supersede"], Value::Null);

    let after_alice = app.sign_ok(&hash, ALICE).await;
    assert_eq!(after_alice["status"], "draft");
    let after_bob = app.sign_ok(&hash, BOB).await;
    assert_eq!(after_bob["status"], "draft");
    assert_eq!(after_bob["signing"]["missing"], json!([pubkey(CHIDI)]));
    assert!(app.outbox().await.is_empty());

    let after_chidi = app.sign_ok(&hash, CHIDI).await;
    assert_eq!(after_chidi["status"], "active");
    assert_eq!(after_chidi["signing"]["state"], "active");
    assert!(after_chidi["activated_at"].is_i64());

    let (status, active) = app.get(&format!("/teams/{team_id}/card")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(active["hash"], hash);
    let (_, team) = app.get(&format!("/teams/{team_id}")).await;
    assert_eq!(team["active_card_hash"], hash);

    // Three signature events exactly as received, then the server card event.
    let outbox = app.outbox().await;
    assert_eq!(outbox.len(), 4);
    let signatures = after_chidi["signatures"].as_array().expect("signatures");
    for (entry, received) in outbox.iter().zip(signatures) {
        assert_eq!(Some(entry.0.as_str()), received.as_str());
        assert_eq!(entry.1, "pending");
    }
    let card_event: Event = serde_json::from_str(&outbox[3].0).expect("card event");
    card_event.verify().expect("server event is signed");
    let server_keys = Keys::parse(SERVER_SECRET_HEX).expect("server key");
    assert_eq!(card_event.pubkey, server_keys.public_key());
    assert_eq!(
        card_event.kind.as_u16(),
        splitcard_core::signature::SIGNATURE_KIND
    );
    assert_eq!(
        card_event.content,
        canonical_json(&card).expect("canonical")
    );
    let tags: Vec<Vec<String>> = card_event
        .tags
        .iter()
        .map(|tag| tag.as_slice().to_vec())
        .collect();
    assert_eq!(
        tags,
        vec![
            vec!["d".to_owned(), hash.clone()],
            vec!["splitcard".to_owned(), hash.clone()]
        ]
    );
}

#[tokio::test]
async fn submitting_the_same_card_again_returns_it() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    let (status, again) = app.submit_card(BOB, &team_id, &first_card()).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["hash"], hash);
}

#[tokio::test]
async fn stored_card_is_canonical_not_the_raw_body() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let canonical = canonical_json(&first_card()).expect("canonical");
    let mut value: Value = serde_json::from_str(&canonical).expect("json");
    let pretty = serde_json::to_string_pretty(&value.take()).expect("pretty");
    let (status, response) = app
        .authed(
            ALICE,
            Method::POST,
            &format!("/teams/{team_id}/cards"),
            &pretty,
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    assert_eq!(response["card"], canonical);
}

#[tokio::test]
async fn card_member_who_is_not_a_participant_is_rejected() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let card = card(&[(ALICE, 50), (DAYO, 50)], None);
    let (status, body) = app.submit_card(ALICE, &team_id, &card).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "member_not_participant");
    assert_eq!(
        body["details"],
        json!({ "index": 1, "pubkey": pubkey(DAYO) })
    );
}

#[tokio::test]
async fn card_member_with_a_different_lightning_address_is_rejected() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let mut card = first_card();
    v1(&mut card).members[1].payout = splitcard_core::card::Payout::Lightning {
        address: "bob@other.example".to_owned(),
    };
    let (status, body) = app.submit_card(ALICE, &team_id, &card).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "member_address_mismatch");
    assert_eq!(body["details"]["index"], 1);
}

#[tokio::test]
async fn card_member_with_a_different_name_is_rejected() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let mut card = first_card();
    v1(&mut card).members[2].name = "Chidinma".to_owned();
    assert_error(
        app.submit_card(ALICE, &team_id, &card).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "member_name_mismatch",
    );
}

#[tokio::test]
async fn card_with_the_wrong_event_or_team_is_rejected() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let mut wrong_event = first_card();
    v1(&mut wrong_event).event = "nostr-hack-2026".to_owned();
    let (status, body) = app.submit_card(ALICE, &team_id, &wrong_event).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "wrong_event");
    assert_eq!(body["details"]["expected"], EVENT_LABEL);

    let mut wrong_team = first_card();
    v1(&mut wrong_team).team = "Other Team".to_owned();
    let (status, body) = app.submit_card(ALICE, &team_id, &wrong_team).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "wrong_team");
    assert_eq!(body["details"]["expected"], TEAM_NAME);
}

#[tokio::test]
async fn first_card_with_supersedes_is_rejected() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let card = card(
        &[(ALICE, 36), (BOB, 35), (CHIDI, 29)],
        Some(&"a".repeat(64)),
    );
    assert_error(
        app.submit_card(ALICE, &team_id, &card).await,
        StatusCode::CONFLICT,
        "unexpected_supersedes",
    );
}

#[tokio::test]
async fn new_card_while_one_is_active_must_supersede_it() {
    let app = app().await;
    let (team_id, active_hash) = active_first_card(&app).await;

    let missing = card(&[(ALICE, 40), (BOB, 30), (CHIDI, 30)], None);
    let (status, body) = app.submit_card(ALICE, &team_id, &missing).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "missing_supersedes");
    assert_eq!(body["details"]["active_card_hash"], active_hash);

    let wrong = card(
        &[(ALICE, 40), (BOB, 30), (CHIDI, 30)],
        Some(&"b".repeat(64)),
    );
    let (status, body) = app.submit_card(ALICE, &team_id, &wrong).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "wrong_supersedes_hash");

    let right = card(&[(ALICE, 40), (BOB, 30), (CHIDI, 30)], Some(&active_hash));
    app.submit_ok(&team_id, &right).await;
}

#[tokio::test]
async fn core_validation_errors_use_wasm_codes() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let mut card = first_card();
    v1(&mut card).members[0].weight = splitcard_core::validate::MAX_WEIGHT + 1;
    let (status, body) = app.submit_card(ALICE, &team_id, &card).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "weight_too_large");
    assert_eq!(
        body["details"],
        json!({ "index": 0, "weight": "9007199254740992" })
    );

    let (status, body) = app
        .authed(
            ALICE,
            Method::POST,
            &format!("/teams/{team_id}/cards"),
            "{\"version\":1}",
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalid_card_json");
}

#[tokio::test]
async fn only_participants_can_submit_cards() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    assert_error(
        app.submit_card(EVE, &team_id, &first_card()).await,
        StatusCode::FORBIDDEN,
        "not_a_participant",
    );
}

#[tokio::test]
async fn superseding_card_needs_old_and_new_members() {
    let app = app().await;
    let (team_id, old_hash) = active_first_card(&app).await;
    let (_, team) = app
        .authed(ALICE, Method::GET, &format!("/teams/{team_id}"), "")
        .await;
    let code = team["join_code"].as_str().expect("join code").to_owned();
    let (status, _) = app.join(&team_id, &code, DAYO).await;
    assert_eq!(status, StatusCode::OK);

    // Chidi leaves and Dayo joins the agreement.
    let new_card = card(&[(ALICE, 40), (BOB, 35), (DAYO, 25)], Some(&old_hash));
    let new_hash = app.submit_ok(&team_id, &new_card).await;
    for signer in [ALICE, BOB, DAYO] {
        let view = app.sign_ok(&new_hash, signer).await;
        assert_eq!(view["status"], "draft");
    }
    // Every new member signed, but removed member Chidi has not.
    let view = card_status(&app, &new_hash).await;
    assert_eq!(view["signing"]["state"], "active");
    assert_eq!(view["supersede"]["ready"], false);
    assert_eq!(
        view["supersede"]["error"]["code"],
        "missing_old_member_signatures"
    );
    assert_eq!(
        view["supersede"]["error"]["details"]["missing"],
        json!([pubkey(CHIDI)])
    );
    assert_eq!(card_status(&app, &old_hash).await["status"], "active");

    let view = app.sign_ok(&new_hash, CHIDI).await;
    assert_eq!(view["status"], "active");
    assert_eq!(view["supersede"]["ready"], true);
    assert_eq!(card_status(&app, &old_hash).await["status"], "superseded");
    let (_, team) = app.get(&format!("/teams/{team_id}")).await;
    assert_eq!(team["active_card_hash"], new_hash);

    // First activation queued 4; this one 4 signatures plus the card event.
    assert_eq!(app.outbox().await.len(), 9);

    // The superseded card takes no more signatures.
    assert_error(
        app.sign(&old_hash, &signature(DAYO, &old_hash, app.now()))
            .await,
        StatusCode::CONFLICT,
        "card_superseded",
    );
}

async fn signature_count(app: &TestApp, hash: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM card_signatures WHERE card_hash = ?")
        .bind(hash)
        .fetch_one(&app.state.db)
        .await
        .expect("count")
}

#[tokio::test]
async fn drafts_that_no_longer_follow_the_active_card_take_no_signatures() {
    let app = app().await;

    // Two first cards: once one activates, the other is stale.
    let team_id = app.alice_bob_chidi().await;
    let first = app.submit_ok(&team_id, &first_card()).await;
    let rival = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 34), (BOB, 33), (CHIDI, 33)], None),
        )
        .await;
    app.sign_ok(&rival, ALICE).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&first, signer).await;
    }
    assert_error(
        app.sign(&rival, &signature(BOB, &rival, app.now())).await,
        StatusCode::CONFLICT,
        "card_stale",
    );
    assert_eq!(signature_count(&app, &rival).await, 1);

    // Two drafts replacing the same card: once one activates, the other is stale.
    let one = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 40), (BOB, 35), (CHIDI, 25)], Some(&first)),
        )
        .await;
    let two = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 30), (BOB, 40), (CHIDI, 30)], Some(&first)),
        )
        .await;
    for signer in [ALICE, BOB] {
        app.sign_ok(&two, signer).await;
    }
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&one, signer).await;
    }
    assert_eq!(card_status(&app, &one).await["status"], "active");
    for signer in [CHIDI, ALICE] {
        assert_error(
            app.sign(&two, &signature(signer, &two, app.now())).await,
            StatusCode::CONFLICT,
            "card_stale",
        );
    }
    assert_eq!(signature_count(&app, &two).await, 2);
    assert_eq!(card_status(&app, &two).await["status"], "stale");
}

async fn stored_status(app: &TestApp, hash: &str) -> String {
    sqlx::query_scalar("SELECT status FROM cards WHERE hash = ?")
        .bind(hash)
        .fetch_one(&app.state.db)
        .await
        .expect("status")
}

#[tokio::test]
async fn stale_drafts_show_as_stale_without_storing_it() {
    let app = app().await;

    // A second first card, stale once another card is active.
    let team_id = app.alice_bob_chidi().await;
    let first = app.submit_ok(&team_id, &first_card()).await;
    let rival = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 34), (BOB, 33), (CHIDI, 33)], None),
        )
        .await;
    assert_eq!(card_status(&app, &rival).await["status"], "draft");
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&first, signer).await;
    }
    assert_eq!(card_status(&app, &rival).await["status"], "stale");
    assert_eq!(card_status(&app, &first).await["status"], "active");
    assert_eq!(stored_status(&app, &rival).await, "draft");

    // A draft whose supersedes is no longer the active card.
    let one = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 40), (BOB, 35), (CHIDI, 25)], Some(&first)),
        )
        .await;
    let two = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 30), (BOB, 40), (CHIDI, 30)], Some(&first)),
        )
        .await;
    assert_eq!(card_status(&app, &two).await["status"], "draft");
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&one, signer).await;
    }
    assert_eq!(card_status(&app, &two).await["status"], "stale");
    assert_eq!(card_status(&app, &first).await["status"], "superseded");
    assert_eq!(stored_status(&app, &two).await, "draft");
}

#[tokio::test]
async fn non_member_signature_is_rejected_and_not_stored() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    assert_error(
        app.sign(&hash, &signature(EVE, &hash, app.now())).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "non_member",
    );
    assert_eq!(card_status(&app, &hash).await["signatures"], json!([]));

    // A signature over another hash is a core error too.
    let other = "c".repeat(64);
    assert_error(
        app.sign(&hash, &signature(ALICE, &other, app.now())).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "wrong_d_tag_hash",
    );
    assert_error(
        app.sign(&hash, "{\"not\":\"an event\"}").await,
        StatusCode::BAD_REQUEST,
        "invalid_event_json",
    );
    assert_error(
        app.sign(&"d".repeat(64), &signature(ALICE, &hash, app.now()))
            .await,
        StatusCode::NOT_FOUND,
        "card_not_found",
    );
}

#[tokio::test]
async fn duplicate_signature_is_ignored() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    let first = signature(ALICE, &hash, app.now());
    let (status, _) = app.sign(&hash, &first).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, view) = app.sign(&hash, &first).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    // A different valid event from the same member is ignored too.
    let (status, view) = app
        .sign(&hash, &signature(ALICE, &hash, app.now() + 5))
        .await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["signatures"], json!([first]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_final_signatures_activate_exactly_once() {
    for _ in 0..10 {
        let app = Arc::new(app().await);
        let team_id = app.alice_bob_chidi().await;
        let hash = app.submit_ok(&team_id, &first_card()).await;
        app.sign_ok(&hash, ALICE).await;

        let tasks: Vec<_> = [BOB, CHIDI]
            .into_iter()
            .map(|signer| {
                let app = Arc::clone(&app);
                let hash = hash.clone();
                tokio::spawn(
                    async move { app.sign(&hash, &signature(signer, &hash, app.now())).await },
                )
            })
            .collect();
        for task in tasks {
            let (status, body) = task.await.expect("task");
            assert_eq!(status, StatusCode::CREATED, "{body}");
        }

        let view = card_status(&app, &hash).await;
        assert_eq!(view["status"], "active");
        assert_eq!(view["signatures"].as_array().map(Vec::len), Some(3));
        assert_eq!(app.outbox().await.len(), 4, "activated more than once");
    }
}

#[tokio::test]
async fn a_member_who_changes_their_profile_after_submission_keeps_a_valid_card() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.sign_ok(&hash, ALICE).await;

    // Bob changes both details before signing, and Chidi after activation.
    let (status, _) = app.put_profile(BOB, "Robert", "robert@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    app.sign_ok(&hash, BOB).await;
    let activated = app.sign_ok(&hash, CHIDI).await;
    assert_eq!(activated["status"], "active", "{activated}");
    let (status, _) = app
        .put_profile(CHIDI, "Chidinma", "chidinma@blink.sv")
        .await;
    assert_eq!(status, StatusCode::OK);

    let (_, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(view["status"], "active");
    assert_eq!(
        view["card"],
        canonical_json(&first_card()).expect("canonical")
    );
    let (_, report) = app.get(&format!("/cards/{hash}/verify")).await;
    for check in report["checks"].as_array().expect("checks") {
        if [
            "card_hash_matches",
            "card_valid",
            "signatures_valid",
            "all_members_signed",
            "status_consistent",
        ]
        .contains(&check["name"].as_str().expect("name"))
        {
            assert_eq!(check["result"], "pass", "{check}");
        }
    }
    // The team shows the profiles as they are now.
    let (_, team) = app
        .authed(ALICE, Method::GET, &format!("/teams/{team_id}"), "")
        .await;
    assert_eq!(team["participants"][1]["name"], "Robert");
    assert_eq!(
        team["participants"][2]["lightning_address"],
        "chidinma@blink.sv"
    );
}

#[tokio::test]
async fn a_card_must_match_member_profiles_when_submitted() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let (status, _) = app.put_profile(BOB, "Robert", "bob@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    assert_error(
        app.submit_card(ALICE, &team_id, &first_card()).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "member_name_mismatch",
    );
    let (status, _) = app.put_profile(BOB, "Bob", "robert@getalby.com").await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = app.submit_card(ALICE, &team_id, &first_card()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "member_address_mismatch");
    assert_eq!(
        body["details"],
        json!({ "index": 1, "pubkey": pubkey(BOB) })
    );

    // A card with Bob's current profile is accepted.
    let mut card = first_card();
    v1(&mut card).members[1].payout = splitcard_core::card::Payout::Lightning {
        address: "robert@getalby.com".to_owned(),
    };
    app.submit_ok(&team_id, &card).await;
}

#[tokio::test]
async fn join_is_idempotent() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    let (status, _) = app.join(&team_id, &code, BOB).await;
    assert_eq!(status, StatusCode::OK);
    let (status, team) = app.join_only(&team_id, &code, BOB).await;
    assert_eq!(status, StatusCode::OK, "{team}");
    assert_eq!(team["participants"].as_array().map(Vec::len), Some(2));
    let (name, address) = person(BOB);
    assert_eq!(team["participants"][1]["name"], name);
    assert_eq!(team["participants"][1]["lightning_address"], address);
}

#[tokio::test]
async fn join_rejects_a_wrong_code_and_unknown_teams() {
    let app = app().await;
    let (team_id, code) = app.create_team(ALICE).await;
    assert_error(
        app.join(&team_id, "0".repeat(32).as_str(), BOB).await,
        StatusCode::FORBIDDEN,
        "invalid_join_code",
    );
    assert_error(
        app.join("999", &code, BOB).await,
        StatusCode::NOT_FOUND,
        "team_not_found",
    );
}

#[tokio::test]
async fn participants_set_the_submission_url() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let path = format!("/teams/{team_id}/submission");
    let body = json!({ "submission_url": "https://devpost.com/software/llamas" }).to_string();
    let (status, team) = app.authed(BOB, Method::PUT, &path, &body).await;
    assert_eq!(status, StatusCode::OK, "{team}");
    assert_eq!(
        team["submission_url"],
        "https://devpost.com/software/llamas"
    );
    assert_error(
        app.authed(EVE, Method::PUT, &path, &body).await,
        StatusCode::FORBIDDEN,
        "not_a_participant",
    );
    let bad = json!({ "submission_url": "javascript:alert(1)" }).to_string();
    assert_error(
        app.authed(BOB, Method::PUT, &path, &bad).await,
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_submission_url",
    );
}

#[tokio::test]
async fn missing_resources_are_404_json() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    assert_error(
        app.get(&format!("/teams/{team_id}/card")).await,
        StatusCode::NOT_FOUND,
        "no_active_card",
    );
    assert_error(
        app.get("/teams/abc").await,
        StatusCode::NOT_FOUND,
        "team_not_found",
    );
    assert_error(
        app.get(&format!("/cards/{}", "e".repeat(64))).await,
        StatusCode::NOT_FOUND,
        "card_not_found",
    );
    assert_error(app.get("/nope").await, StatusCode::NOT_FOUND, "not_found");
}
