//! Verification reports, recomputed from stored raw data, using a manual
//! clock and direct database writes to tamper with stored evidence.

mod common;

use std::future::{Future, ready};

use axum::http::StatusCode;
use common::{
    ALICE, BOB, CHIDI, SERVER_SECRET_HEX, TestApp, app, app_with, card, config,
    config_with_server_key, first_card, sha256_hex,
};
use nostr::{event::Event, key::Keys};
use serde_json::{Value, json};
use splitcard_server::{
    ots::format::{
        Attestation, DetachedTimestampFile, FileHashOp, Item, Timestamp, serialize_file,
    },
    outbox::{OutboxSettings, RelayPublisher, RelayResult, process_due},
    routes::verify::CHECK_NAMES,
};
use url::Url;

/// Accepts every event on every relay.
struct AcceptingRelays;

impl RelayPublisher for AcceptingRelays {
    fn relays(&self) -> Vec<Url> {
        vec![Url::parse("wss://relay.one.test").expect("url")]
    }

    fn publish(
        &self,
        _event: &Event,
        relays: &[Url],
    ) -> impl Future<Output = Vec<(Url, RelayResult)>> + Send {
        ready(relays.iter().map(|relay| (relay.clone(), Ok(()))).collect())
    }
}

async fn publish_all(app: &TestApp) {
    process_due(
        &app.state.db,
        &AcceptingRelays,
        app.clock.as_ref(),
        &OutboxSettings::default(),
    )
    .await
    .expect("outbox pass");
}

/// Stores a `.ots` proof over `digest` ending in `attestation`.
async fn store_proof(app: &TestApp, hash: &str, digest: Vec<u8>, attestation: Attestation) {
    let file = DetachedTimestampFile {
        file_hash_op: FileHashOp::Sha256,
        digest,
        timestamp: Timestamp::new(vec![Item::Attestation(attestation)]),
    };
    sqlx::query(
        "UPDATE card_timestamps SET ots_proof = ?, status = 'complete' WHERE card_hash = ?",
    )
    .bind(serialize_file(&file))
    .bind(hash)
    .execute(&app.state.db)
    .await
    .expect("store proof");
}

async fn stamp_bitcoin(app: &TestApp, hash: &str) {
    let digest = hex::decode(hash).expect("hex hash");
    store_proof(app, hash, digest, Attestation::Bitcoin { height: 912_345 }).await;
}

async fn sql(app: &TestApp, statement: &str) {
    sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
        .execute(&app.state.db)
        .await
        .expect("tampering statement");
}

async fn verify(app: &TestApp, hash: &str) -> Value {
    let (status, report) = app.get(&format!("/cards/{hash}/verify")).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_shape(&report);
    report
}

fn assert_shape(report: &Value) {
    let names: Vec<&str> = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|check| check["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, CHECK_NAMES, "every check exactly once, in order");
}

fn check<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["name"] == name)
        .expect("check present")
}

fn result<'a>(report: &'a Value, name: &str) -> &'a str {
    check(report, name)["result"].as_str().expect("result")
}

/// A fully signed, published, Bitcoin-stamped card.
async fn stamped_card(app: &TestApp) -> (String, String) {
    let (team_id, hash) = app.active_first_card().await;
    publish_all(app).await;
    stamp_bitcoin(app, &hash).await;
    (team_id, hash)
}

#[tokio::test]
async fn a_signed_published_stamped_card_verifies() {
    let app = app().await;
    let (team_id, hash) = stamped_card(&app).await;

    let report = verify(&app, &hash).await;
    for name in CHECK_NAMES {
        let expected = if name == "supersede_valid" {
            "not_applicable"
        } else {
            "pass"
        };
        assert_eq!(
            result(&report, name),
            expected,
            "{name}: {}",
            check(&report, name)
        );
    }
    assert_eq!(report["overall"], "verified");
    assert_eq!(report["card_hash"], hash.as_str());
    let stored: String = sqlx::query_scalar("SELECT canonical_json FROM cards WHERE hash = ?")
        .bind(&hash)
        .fetch_one(&app.state.db)
        .await
        .expect("card");
    assert_eq!(report["canonical_card_json"], stored.as_str());
    assert_eq!(sha256_hex(stored.as_bytes()), hash);

    let timestamp = check(&report, "timestamp");
    assert_eq!(timestamp["details"]["bitcoin_block_height"], 912_345);
    assert_eq!(
        timestamp["details"]["verify_command"],
        format!("ots verify {hash}.json.ots")
    );
    assert!(
        timestamp["details"]["note"]
            .as_str()
            .expect("note")
            .contains("does not check block headers against Bitcoin")
    );
    assert_eq!(
        report["timestamp"]["ots_url"],
        format!("https://splitcard.test/cards/{hash}/timestamp.ots")
    );

    let signatures = report["signatures"].as_array().expect("signatures");
    assert_eq!(signatures.len(), 3);
    let names: Vec<&str> = signatures
        .iter()
        .map(|s| s["member_name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["Alice", "Bob", "Chidi"]);
    assert!(signatures.iter().all(|s| s["result"] == "pass"));

    // The card event and the three signature events, each accepted.
    let relays = report["relays"].as_array().expect("relays");
    assert_eq!(relays.len(), 4);
    assert_eq!(relays[0]["kind"], "card");
    assert!(
        relays
            .iter()
            .all(|r| r["deliveries"][0]["status"] == "accepted")
    );

    assert!(report.get("payout").is_none(), "no payout section");

    let (status, team_report) = app.get(&format!("/teams/{team_id}/verify")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(team_report["card_hash"], hash.as_str());
    assert_eq!(team_report["overall"], "verified");
}

// Pending states.

#[tokio::test]
async fn a_draft_card_is_pending_and_lists_missing_members() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.sign_ok(&hash, ALICE).await;

    let report = verify(&app, &hash).await;
    let signed = check(&report, "all_members_signed");
    assert_eq!(signed["result"], "pending");
    let missing: Vec<&str> = signed["details"]["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .map(|member| member["name"].as_str().expect("name"))
        .collect();
    assert_eq!(missing, ["Bob", "Chidi"]);
    assert_eq!(result(&report, "status_consistent"), "pass");
    assert_eq!(result(&report, "published_to_relays"), "not_applicable");
    assert_eq!(result(&report, "timestamp"), "not_applicable");
    assert_eq!(report["overall"], "pending");

    let (status, body) = app.get(&format!("/teams/{team_id}/verify")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "no_active_card");
}

#[tokio::test]
async fn a_proof_with_only_pending_attestations_is_pending() {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    let digest = hex::decode(&hash).expect("hex");
    let uri = "https://alice.btc.calendar.opentimestamps.org".to_owned();
    store_proof(
        &app,
        &hash,
        digest,
        Attestation::Pending { uri: uri.clone() },
    )
    .await;

    let report = verify(&app, &hash).await;
    let timestamp = check(&report, "timestamp");
    assert_eq!(timestamp["result"], "pending");
    assert_eq!(timestamp["details"]["pending_calendars"], json!([uri]));
}

// Tampering.

#[tokio::test]
async fn an_edited_canonical_json_fails_the_hash_check() {
    let app = app().await;
    let (_, hash) = stamped_card(&app).await;
    sql(
        &app,
        "UPDATE cards SET canonical_json = replace(canonical_json, 'Lightning Llamas', \
         'Lightning Llamaz')",
    )
    .await;
    let report = verify(&app, &hash).await;
    assert_eq!(result(&report, "card_hash_matches"), "fail");
    assert_eq!(report["overall"], "failed");
}

#[tokio::test]
async fn a_corrupted_signature_event_fails_for_that_event() {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    let stored: String =
        sqlx::query_scalar("SELECT event_json FROM card_signatures WHERE pubkey = ?")
            .bind(common::pubkey(BOB))
            .fetch_one(&app.state.db)
            .await
            .expect("signature");
    let mut event: Value = serde_json::from_str(&stored).expect("event");
    let sig = event["sig"].as_str().expect("sig").to_owned();
    let flipped = if sig.starts_with('0') { "1" } else { "0" };
    event["sig"] = json!(format!("{flipped}{}", &sig[1..]));
    sqlx::query("UPDATE card_signatures SET event_json = ? WHERE pubkey = ?")
        .bind(event.to_string())
        .bind(common::pubkey(BOB))
        .execute(&app.state.db)
        .await
        .expect("tamper");

    let report = verify(&app, &hash).await;
    let signatures = check(&report, "signatures_valid");
    assert_eq!(signatures["result"], "fail");
    let results: Vec<&str> = signatures["details"]["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["result"].as_str().expect("result"))
        .collect();
    assert_eq!(results, ["pass", "fail", "pass"]);
    assert_eq!(
        signatures["details"]["events"][1]["error"]["code"],
        "invalid_signature"
    );
    assert_eq!(report["signatures"][1]["result"], "fail");
}

#[tokio::test]
async fn an_active_status_without_enough_signatures_is_inconsistent() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.sign_ok(&hash, ALICE).await;
    sql(&app, "UPDATE cards SET status = 'active'").await;

    let report = verify(&app, &hash).await;
    let status = check(&report, "status_consistent");
    assert_eq!(status["result"], "fail");
    assert_eq!(
        status["details"],
        json!({ "stored": "active", "implied": "draft" })
    );
}

#[tokio::test]
async fn a_proof_for_a_different_digest_fails() {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    let other = hex::decode(sha256_hex(b"some other file")).expect("hex");
    store_proof(&app, &hash, other, Attestation::Bitcoin { height: 1 }).await;
    let report = verify(&app, &hash).await;
    let timestamp = check(&report, "timestamp");
    assert_eq!(timestamp["result"], "fail");
    assert_eq!(
        timestamp["details"]["reason"],
        "the proof is for a different digest"
    );
}

#[tokio::test]
async fn an_outbox_entry_that_failed_before_per_relay_deliveries_fails() {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    let report = verify(&app, &hash).await;
    assert_eq!(result(&report, "published_to_relays"), "pending");

    // As left by an entry that gave up before migration 0003 existed.
    sql(
        &app,
        "UPDATE relay_outbox SET status = 'failed' WHERE id = (SELECT min(id) FROM relay_outbox)",
    )
    .await;
    sql(
        &app,
        "DELETE FROM relay_deliveries WHERE outbox_id = (SELECT min(id) FROM relay_outbox)",
    )
    .await;
    let report = verify(&app, &hash).await;
    assert_eq!(result(&report, "published_to_relays"), "fail");
    assert_eq!(report["overall"], "failed");
}

#[tokio::test]
async fn competing_cards_are_inconsistent() {
    let app = app().await;
    let (team_id, old) = app.active_first_card().await;
    let winner = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 40), (BOB, 35), (CHIDI, 25)], Some(&old)),
        )
        .await;
    let rival = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 30), (BOB, 40), (CHIDI, 30)], Some(&old)),
        )
        .await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&winner, signer).await;
    }
    // Signing refuses the stale rival, so sign it behind the server's back.
    for signer in [ALICE, BOB, CHIDI] {
        sqlx::query(
            "INSERT INTO card_signatures (card_hash, pubkey, event_json, received_at) \
             VALUES (?, ?, ?, 0)",
        )
        .bind(&rival)
        .bind(common::pubkey(signer))
        .bind(common::signature(signer, &rival, app.now()))
        .execute(&app.state.db)
        .await
        .expect("tamper");
    }

    for (hash, other) in [(&winner, &rival), (&rival, &winner)] {
        let report = verify(&app, hash).await;
        let status = check(&report, "status_consistent");
        assert_eq!(status["result"], "fail", "{status}");
        assert_eq!(status["details"]["implied"], "competing");
        assert_eq!(status["details"]["competing_with"], json!([other]));
        assert_eq!(report["overall"], "failed");
    }
}

/// A team with `first` superseded by active `one`, and two stale drafts:
/// `rival`, a second first card Alice signed, and `two`, which supersedes
/// `first` and was signed by Alice and Bob before `one` activated.
struct StaleDrafts {
    team_id: String,
    first: String,
    one: String,
    rival: String,
    two: String,
}

async fn stale_drafts(app: &TestApp) -> StaleDrafts {
    let team_id = app.alice_bob_chidi().await;
    let first = app.submit_ok(&team_id, &first_card()).await;
    let rival = app
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 34), (BOB, 33), (CHIDI, 33)], None),
        )
        .await;
    app.sign_ok(&rival, ALICE).await;
    assert_eq!(verify(app, &rival).await["overall"], "pending");
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&first, signer).await;
    }
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
    // Still collecting signatures: an ordinary, pending draft.
    let report = verify(app, &two).await;
    assert_eq!(report["status"], "draft");
    assert_eq!(report["overall"], "pending");
    assert_eq!(result(&report, "all_members_signed"), "pending");
    assert_eq!(result(&report, "supersede_valid"), "pending");
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&one, signer).await;
    }
    StaleDrafts {
        team_id,
        first,
        one,
        rival,
        two,
    }
}

const STALE_NOT_APPLICABLE: [&str; 3] = ["all_members_signed", "published_to_relays", "timestamp"];

fn assert_stale(report: &Value, supersedes: bool) {
    assert_eq!(report["status"], "stale");
    assert_eq!(report["overall"], "stale", "{report}");
    for name in STALE_NOT_APPLICABLE {
        let check = check(report, name);
        assert_eq!(check["result"], "not_applicable", "{name}");
        assert_eq!(check["details"]["reason"], "stale_draft", "{name}");
    }
    for name in [
        "card_hash_matches",
        "card_valid",
        "signatures_valid",
        "status_consistent",
    ] {
        assert_eq!(
            result(report, name),
            "pass",
            "{name}: {}",
            check(report, name)
        );
    }
    assert_eq!(
        check(report, "status_consistent")["details"],
        json!({ "stored": "draft", "implied": "draft", "stale": true })
    );
    let supersede = check(report, "supersede_valid");
    if supersedes {
        assert_eq!(supersede["result"], "pass", "{supersede}");
        assert_eq!(supersede["details"]["signatures"], "stale_draft");
    } else {
        assert_eq!(supersede["result"], "not_applicable");
    }
}

#[tokio::test]
async fn a_draft_superseding_a_card_no_longer_active_is_stale() {
    let app = app().await;
    let cards = stale_drafts(&app).await;
    assert_stale(&verify(&app, &cards.two).await, true);

    // The team report is for its active card, which stale drafts never are.
    let (status, team) = app.get(&format!("/teams/{}/verify", cards.team_id)).await;
    assert_eq!(status, StatusCode::OK, "{team}");
    assert_eq!(team["card_hash"], cards.one.as_str());
    assert_eq!(team["status"], "active");
    assert_ne!(team["overall"], "stale");
    assert_eq!(result(&team, "all_members_signed"), "pass");
    assert_eq!(verify(&app, &cards.first).await["status"], "superseded");
}

#[tokio::test]
async fn a_second_first_card_after_another_activated_is_stale() {
    let app = app().await;
    let cards = stale_drafts(&app).await;
    assert_stale(&verify(&app, &cards.rival).await, false);
}

#[tokio::test]
async fn a_tampered_stale_draft_fails_rather_than_being_stale() {
    let app = app().await;
    let cards = stale_drafts(&app).await;
    sqlx::query(
        "UPDATE cards SET canonical_json = replace(canonical_json, 'Lightning Llamas', \
         'Lightning Llamaz') WHERE hash = ?",
    )
    .bind(&cards.two)
    .execute(&app.state.db)
    .await
    .expect("tamper");
    let report = verify(&app, &cards.two).await;
    assert_eq!(report["status"], "stale");
    assert_eq!(result(&report, "card_hash_matches"), "fail");
    assert_eq!(report["overall"], "failed");
}

#[tokio::test]
async fn a_stale_draft_with_a_corrupted_signature_fails() {
    let app = app().await;
    let cards = stale_drafts(&app).await;
    let stored: String = sqlx::query_scalar(
        "SELECT event_json FROM card_signatures WHERE card_hash = ? AND pubkey = ?",
    )
    .bind(&cards.two)
    .bind(common::pubkey(BOB))
    .fetch_one(&app.state.db)
    .await
    .expect("signature");
    let mut event: Value = serde_json::from_str(&stored).expect("event");
    let sig = event["sig"].as_str().expect("sig").to_owned();
    let flipped = if sig.starts_with('0') { "1" } else { "0" };
    event["sig"] = json!(format!("{flipped}{}", &sig[1..]));
    sqlx::query("UPDATE card_signatures SET event_json = ? WHERE card_hash = ? AND pubkey = ?")
        .bind(event.to_string())
        .bind(&cards.two)
        .bind(common::pubkey(BOB))
        .execute(&app.state.db)
        .await
        .expect("tamper");
    let report = verify(&app, &cards.two).await;
    assert_eq!(report["status"], "stale");
    assert_eq!(result(&report, "signatures_valid"), "fail");
    assert_eq!(report["overall"], "failed");
}

// Card events are checked against the key that signed them.

/// A second server key, as after a key rotation.
const SECOND_SERVER_KEY: &str = "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2";

fn server_pubkey(secret_hex: &str) -> String {
    Keys::parse(secret_hex).expect("key").public_key().to_hex()
}

fn card_event_check(report: &Value) -> &Value {
    let event = &check(report, "published_to_relays")["details"]["events"][0];
    assert_eq!(event["kind"], "card");
    event
}

async fn card_event_view(app: &TestApp, hash: &str) -> Value {
    let (status, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    view["card_event"].clone()
}

#[tokio::test]
async fn card_events_still_verify_after_the_server_key_changes() {
    let first_key = server_pubkey(SERVER_SECRET_HEX);
    let second_key = server_pubkey(SECOND_SERVER_KEY);
    let before = app().await;
    let (team_id, old) = before.active_first_card().await;
    publish_all(&before).await;

    // Restart on the same database with another key.
    let after = app_with(
        before.state.db.clone(),
        config_with_server_key(SECOND_SERVER_KEY),
    );
    let report = verify(&after, &old).await;
    assert_eq!(result(&report, "published_to_relays"), "pass", "{report}");
    assert_eq!(
        card_event_check(&report)["server_pubkey"],
        first_key.as_str()
    );
    assert_eq!(card_event_check(&report)["problems"], json!([]));
    assert_eq!(
        card_event_view(&after, &old).await["server_pubkey"],
        first_key.as_str()
    );

    // A card activated after the change is signed with the new key.
    let new = after
        .submit_ok(
            &team_id,
            &card(&[(ALICE, 40), (BOB, 35), (CHIDI, 25)], Some(&old)),
        )
        .await;
    for signer in [ALICE, BOB, CHIDI] {
        after.sign_ok(&new, signer).await;
    }
    publish_all(&after).await;
    let report = verify(&after, &new).await;
    assert_eq!(result(&report, "published_to_relays"), "pass", "{report}");
    assert_eq!(
        card_event_check(&report)["server_pubkey"],
        second_key.as_str()
    );
    assert_eq!(report["relays"][0]["pubkey"], second_key.as_str());
    let view = card_event_view(&after, &new).await;
    assert_eq!(view["server_pubkey"], second_key.as_str());
    assert_eq!(view["event_id"], card_event_check(&report)["event_id"]);
    assert_eq!(
        result(&verify(&after, &old).await, "published_to_relays"),
        "pass"
    );
}

#[tokio::test]
async fn the_migration_backfills_the_signer_of_existing_card_events() {
    let path = std::env::temp_dir().join(format!(
        "splitcard-backfill-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let url = path.to_str().expect("UTF-8 path").to_owned();
    let pool = splitcard_server::db::connect(&url).await.expect("database");
    let app = app_with(pool.clone(), config());
    let (_, hash) = app.active_first_card().await;
    publish_all(&app).await;

    // Take the database back to just before the migration: an active card,
    // its events queued and published, and no signer column.
    sqlx::query("ALTER TABLE relay_outbox DROP COLUMN server_pubkey")
        .execute(&pool)
        .await
        .expect("drop column");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 5")
        .execute(&pool)
        .await
        .expect("forget migration");
    drop(app);
    pool.close().await;

    let pool = splitcard_server::db::connect(&url).await.expect("migrated");
    let signers: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT server_pubkey, json_extract(event_json, '$.pubkey') FROM relay_outbox ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("signers");
    let first_key = server_pubkey(SERVER_SECRET_HEX);
    let expected: Vec<(Option<String>, String)> = [ALICE, BOB, CHIDI]
        .into_iter()
        .map(|member| (None, common::pubkey(member)))
        .chain([(Some(first_key.clone()), first_key.clone())])
        .collect();
    assert_eq!(signers, expected, "only the card event gets a signer");

    let app = app_with(pool.clone(), config());
    let report = verify(&app, &hash).await;
    assert_eq!(result(&report, "published_to_relays"), "pass", "{report}");
    assert_eq!(
        report["overall"], "pending",
        "only the timestamp is outstanding"
    );
    assert_eq!(
        card_event_view(&app, &hash).await["server_pubkey"],
        first_key.as_str()
    );
    drop(app);
    pool.close().await;
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{url}{suffix}"));
    }
}

/// A published card whose stored card event `tamper` then rewrites.
async fn tampered_card_event(tamper: &str) -> Value {
    let app = app().await;
    let (_, hash) = app.active_first_card().await;
    publish_all(&app).await;
    assert_eq!(
        result(&verify(&app, &hash).await, "published_to_relays"),
        "pass"
    );
    sqlx::query(sqlx::AssertSqlSafe(
        tamper
            .replace("{hash}", &hash)
            .replace("{other}", &"ab".repeat(32)),
    ))
    .execute(&app.state.db)
    .await
    .expect("tamper");
    let report = verify(&app, &hash).await;
    assert_eq!(result(&report, "published_to_relays"), "fail", "{report}");
    assert_eq!(report["overall"], "failed");
    card_event_check(&report)["problems"].clone()
}

fn has(problems: &Value, problem: &str) -> bool {
    problems
        .as_array()
        .expect("problems")
        .iter()
        .any(|p| p == problem)
}

#[tokio::test]
async fn an_edited_card_event_content_fails() {
    let problems = tampered_card_event(
        "UPDATE relay_outbox SET event_json = replace(event_json, 'Lightning Llamas', \
         'Lightning Llamaz') WHERE server_pubkey IS NOT NULL",
    )
    .await;
    assert!(has(&problems, "content mismatch"), "{problems}");
}

#[tokio::test]
async fn a_changed_card_event_d_tag_fails() {
    let problems = tampered_card_event(
        r#"UPDATE relay_outbox SET event_json = replace(event_json, '["d","{hash}"]',
           '["d","{other}"]') WHERE server_pubkey IS NOT NULL"#,
    )
    .await;
    assert!(has(&problems, "d tag mismatch"), "{problems}");
}

#[tokio::test]
async fn a_changed_card_event_splitcard_tag_fails() {
    let problems = tampered_card_event(
        r#"UPDATE relay_outbox SET event_json = replace(event_json, '["splitcard","{hash}"]',
           '["splitcard","{other}"]') WHERE server_pubkey IS NOT NULL"#,
    )
    .await;
    assert!(has(&problems, "splitcard tag mismatch"), "{problems}");
}

#[tokio::test]
async fn a_corrupted_card_event_signature_fails() {
    let problems = tampered_card_event(
        "UPDATE relay_outbox SET event_json = json_set(event_json, '$.sig', \
         '00' || substr(json_extract(event_json, '$.sig'), 3)) WHERE server_pubkey IS NOT NULL",
    )
    .await;
    assert_eq!(problems, json!(["invalid signature"]));
}

#[tokio::test]
async fn a_changed_recorded_signer_fails() {
    let other = server_pubkey(SECOND_SERVER_KEY);
    let problems = tampered_card_event(&format!(
        "UPDATE relay_outbox SET server_pubkey = '{other}' WHERE server_pubkey IS NOT NULL"
    ))
    .await;
    assert_eq!(problems, json!(["signer mismatch"]));
}

// Supersede.

#[tokio::test]
async fn a_superseded_card_shows_the_chain_both_ways() {
    let app = app().await;
    let (_, old) = app.active_first_card().await;
    let new_card = card(&[(ALICE, 40), (BOB, 35), (CHIDI, 25)], Some(&old));
    let team_id: String =
        sqlx::query_scalar("SELECT CAST(team_id AS TEXT) FROM cards WHERE hash = ?")
            .bind(&old)
            .fetch_one(&app.state.db)
            .await
            .expect("team");
    let new = app.submit_ok(&team_id, &new_card).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&new, signer).await;
    }

    let old_report = verify(&app, &old).await;
    assert_eq!(
        old_report["supersede_chain"],
        json!({ "previous": null, "next": new })
    );
    assert_eq!(
        check(&old_report, "status_consistent")["details"]["implied"],
        "superseded"
    );
    assert_eq!(result(&old_report, "status_consistent"), "pass");

    let new_report = verify(&app, &new).await;
    assert_eq!(
        new_report["supersede_chain"],
        json!({ "previous": old, "next": null })
    );
    assert_eq!(result(&new_report, "supersede_valid"), "pass");
    assert_eq!(result(&new_report, "signatures_valid"), "pass");
    assert_eq!(result(&new_report, "status_consistent"), "pass");
}

// Read-only.

#[tokio::test]
async fn verification_never_opens_a_write_transaction() {
    let app = app().await;
    let (team_id, hash) = stamped_card(&app).await;
    let before = app.outbox().await;

    // Hold the database write lock: anything that tries to write, or to begin
    // a write transaction, now blocks or fails instead of answering.
    let lock = splitcard_server::db::begin_write(&app.state.db)
        .await
        .expect("write lock");
    let reads = async {
        let card = app.get(&format!("/cards/{hash}/verify")).await;
        let team = app.get(&format!("/teams/{team_id}/verify")).await;
        (card, team)
    };
    let ((card_status, card_report), (team_status, team_report)) =
        tokio::time::timeout(std::time::Duration::from_secs(2), reads)
            .await
            .expect("reports finish while another connection holds the write lock");
    drop(lock);
    assert_eq!(card_status, StatusCode::OK, "{card_report}");
    assert_eq!(team_status, StatusCode::OK, "{team_report}");
    assert_eq!(card_report["overall"], "verified");
    assert_eq!(app.outbox().await, before);
}

#[tokio::test]
async fn unknown_cards_and_teams_are_not_found() {
    let app = app().await;
    let (status, body) = app.get(&format!("/cards/{}/verify", "ab".repeat(32))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "card_not_found");
    let (status, body) = app.get("/teams/999/verify").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "team_not_found");
}
