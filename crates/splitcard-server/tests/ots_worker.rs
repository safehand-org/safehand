//! Stamping workers with fake calendars, the activation hook, and the
//! timestamp endpoints.

mod common;

use std::{
    collections::HashMap,
    future::{Future, ready},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::http::{StatusCode, header};
use common::{ALICE, BOB, CHIDI, TestApp, app, first_card};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use splitcard_core::hash::canonical_json;
use splitcard_server::ots::{
    calendar::{CalendarClient, CalendarError},
    format::{Attestation, Item, Op, Timestamp, parse_file},
    worker::{self, TimestampSettings, is_allowed_upgrade_calendar, submit_due, upgrade_pending},
};
use splitcard_server::shutdown;
use url::Url;

const ALICE_CAL: &str = "https://alice.btc.calendar.opentimestamps.org";
const BOB_CAL: &str = "https://bob.btc.calendar.opentimestamps.org";
const CAROL_CAL: &str = "https://carol.btc.calendar.opentimestamps.org";

fn url(text: &str) -> Url {
    Url::parse(text).expect("url")
}

fn pools() -> Vec<Url> {
    [
        "https://a.pool.test",
        "https://b.pool.test",
        "https://c.pool.test",
    ]
    .into_iter()
    .map(url)
    .collect()
}

/// Shaped like a real calendar's reply: a nonce, sha256, then the calendar's
/// own prefix and suffix around the commitment it will later upgrade.
fn calendar_reply(nonce: u8, pending_uri: &str) -> Timestamp {
    Timestamp::new(vec![Item::Op(
        Op::Append(vec![nonce; 16]),
        Timestamp::new(vec![Item::Op(
            Op::Sha256,
            Timestamp::new(vec![Item::Op(
                Op::Prepend(vec![0x57, 0xd4, 0x65, nonce]),
                Timestamp::new(vec![Item::Op(
                    Op::Append(vec![nonce; 8]),
                    Timestamp::new(vec![Item::Attestation(Attestation::Pending {
                        uri: pending_uri.to_owned(),
                    })]),
                )]),
            )]),
        )]),
    )])
}

/// A calendar's upgrade: the commitment reaches a Bitcoin block.
fn bitcoin_upgrade(height: u32) -> Timestamp {
    Timestamp::new(vec![Item::Op(
        Op::Sha256,
        Timestamp::new(vec![Item::Op(
            Op::Prepend(vec![0xbb; 32]),
            Timestamp::new(vec![Item::Op(
                Op::Sha256,
                Timestamp::new(vec![Item::Attestation(Attestation::Bitcoin { height })]),
            )]),
        )]),
    )])
}

/// Each pool calendar replies (or fails) as configured; upgrades are keyed
/// by calendar URI and commitment hex.
#[derive(Default)]
struct FakeCalendars {
    replies: Mutex<HashMap<Url, Result<Timestamp, CalendarError>>>,
    upgrades: Mutex<HashMap<(String, String), Timestamp>>,
    submits: AtomicUsize,
    upgrade_calls: Mutex<Vec<String>>,
}

impl FakeCalendars {
    fn reply(&self, pool: &Url, reply: Result<Timestamp, CalendarError>) {
        self.replies
            .lock()
            .expect("lock")
            .insert(pool.clone(), reply);
    }
}

impl CalendarClient for FakeCalendars {
    fn submit(
        &self,
        calendar: &Url,
        _digest: [u8; 32],
    ) -> impl Future<Output = Result<Vec<u8>, CalendarError>> + Send {
        self.submits.fetch_add(1, Ordering::SeqCst);
        let reply = self.replies.lock().expect("lock").get(calendar).cloned();
        ready(match reply {
            Some(Ok(timestamp)) => Ok(timestamp.serialize()),
            Some(Err(error)) => Err(error),
            None => Err(CalendarError::Timeout),
        })
    }

    fn upgrade(
        &self,
        calendar: &Url,
        commitment: &[u8],
    ) -> impl Future<Output = Result<Option<Vec<u8>>, CalendarError>> + Send {
        let uri = calendar.as_str().trim_end_matches('/').to_owned();
        self.upgrade_calls.lock().expect("lock").push(uri.clone());
        let key = (uri, hex::encode(commitment));
        ready(Ok(self
            .upgrades
            .lock()
            .expect("lock")
            .get(&key)
            .map(Timestamp::serialize)))
    }
}

fn down() -> CalendarError {
    CalendarError::HttpStatus { status: 503 }
}

fn settings() -> TimestampSettings {
    let mut settings = TimestampSettings::new(pools(), Duration::from_secs(600));
    settings.base_delay_secs = 30;
    settings.max_delay_secs = 300;
    settings.max_submit_attempts = 3;
    settings
}

async fn activated(app: &TestApp) -> String {
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }
    hash
}

async fn timestamp_status(app: &TestApp, hash: &str) -> Value {
    let (status, body) = app.get(&format!("/cards/{hash}/timestamp")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

async fn submit(app: &TestApp, calendars: &Arc<FakeCalendars>) {
    submit_due(&app.state.db, calendars, app.clock.as_ref(), &settings())
        .await
        .expect("submit pass");
}

async fn upgrade(app: &TestApp, calendars: &Arc<FakeCalendars>) {
    upgrade_pending(&app.state.db, calendars, app.clock.as_ref(), &settings())
        .await
        .expect("upgrade pass");
}

async fn proof(app: &TestApp, hash: &str) -> Vec<u8> {
    let (status, _, body) = app.get_bytes(&format!("/cards/{hash}/timestamp.ots")).await;
    assert_eq!(status, StatusCode::OK);
    body
}

/// Pools a and b accept, pointing at Alice's and Bob's calendars; c fails.
fn two_of_three() -> Arc<FakeCalendars> {
    let calendars = Arc::new(FakeCalendars::default());
    let pools = pools();
    calendars.reply(&pools[0], Ok(calendar_reply(1, ALICE_CAL)));
    calendars.reply(&pools[1], Ok(calendar_reply(2, BOB_CAL)));
    calendars.reply(&pools[2], Err(down()));
    calendars
}

#[tokio::test]
async fn activation_queues_exactly_one_timestamp_row() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    app.sign_ok(&hash, ALICE).await;
    app.sign_ok(&hash, BOB).await;

    // A draft has no timestamp.
    let (status, body) = app.get(&format!("/cards/{hash}/timestamp")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "timestamp_not_found");
    let (_, card) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(card["timestamp"], Value::Null);

    let view = app.sign_ok(&hash, CHIDI).await;
    assert_eq!(view["status"], "active");
    assert_eq!(view["timestamp"]["status"], "queued");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM card_timestamps")
        .fetch_one(&app.state.db)
        .await
        .expect("count");
    assert_eq!(rows, 1);

    assert_eq!(
        timestamp_status(&app, &hash).await,
        json!({
            "status": "queued",
            "calendars_accepted": 0,
            "bitcoin_block_height": null,
            "submitted_at": null,
            "completed_at": null,
            "last_error": null,
        })
    );
    let (status, _, body) = app.get_bytes(&format!("/cards/{hash}/timestamp.ots")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let body: Value = serde_json::from_slice(&body).expect("json error");
    assert_eq!(body["code"], "proof_not_available");
}

#[tokio::test]
async fn activation_succeeds_while_every_calendar_is_down() {
    let app = app().await;
    let calendars = Arc::new(FakeCalendars::default());
    for pool in pools() {
        calendars.reply(&pool, Err(down()));
    }
    let (stop, until) = shutdown::channel();
    let worker = tokio::spawn(worker::run_submitter(
        app.state.db.clone(),
        Arc::clone(&calendars),
        app.clock.clone(),
        TimestampSettings {
            submit_poll_interval: Duration::from_millis(5),
            ..settings()
        },
        until,
    ));
    let hash = activated(&app).await;
    while calendars.submits.load(Ordering::SeqCst) < 3 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (status, card) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(card["status"], "active");
    assert_eq!(card["timestamp"]["status"], "queued");

    // Shutdown stops the loop after its current pass.
    stop.trigger();
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("the submitter stops on shutdown")
        .expect("joined");
}

#[tokio::test]
async fn one_failing_calendar_of_three_still_gives_pending_with_two_accepted() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = two_of_three();
    submit(&app, &calendars).await;

    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["status"], "pending");
    assert_eq!(status["calendars_accepted"], 2);
    assert_eq!(status["submitted_at"], common::NOW);
    assert!(
        status["last_error"]
            .as_str()
            .is_some_and(|error| error.contains("c.pool.test"))
    );

    // The proof stamps the canonical JSON and lists both calendars.
    let (status, headers, body) = app.get_bytes(&format!("/cards/{hash}/timestamp.ots")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        format!("attachment; filename=\"{hash}.json.ots\"")
    );
    let file = parse_file(&body).expect("valid .ots");
    let canonical = canonical_json(&first_card()).expect("canonical");
    assert_eq!(file.digest, Sha256::digest(canonical.as_bytes()).to_vec());
    assert_eq!(hex::encode(&file.digest), hash);
    let uris: Vec<String> = file
        .timestamp
        .pending_attestations(&file.digest)
        .expect("computable")
        .into_iter()
        .map(|pending| pending.uri)
        .collect();
    assert_eq!(uris, [ALICE_CAL, BOB_CAL]);

    // The failed calendar is retried after the backoff and merged in.
    calendars.reply(&pools()[2], Ok(calendar_reply(3, CAROL_CAL)));
    submit(&app, &calendars).await;
    assert_eq!(timestamp_status(&app, &hash).await["calendars_accepted"], 2);
    app.clock.advance(30);
    submit(&app, &calendars).await;
    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["calendars_accepted"], 3);
    assert_eq!(status["last_error"], Value::Null);
    let file = parse_file(&proof(&app, &hash).await).expect("valid .ots");
    assert_eq!(
        file.timestamp
            .pending_attestations(&file.digest)
            .expect("computable")
            .len(),
        3
    );
}

#[tokio::test]
async fn all_calendars_failing_retries_then_fails() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = Arc::new(FakeCalendars::default());
    for pool in pools() {
        calendars.reply(&pool, Err(down()));
    }

    submit(&app, &calendars).await;
    assert_eq!(timestamp_status(&app, &hash).await["status"], "queued");
    // Not due again until the 30 s backoff passes, then 60 s.
    submit(&app, &calendars).await;
    assert_eq!(calendars.submits.load(Ordering::SeqCst), 3);
    for delay in [30, 60] {
        app.clock.advance(delay);
        submit(&app, &calendars).await;
    }
    assert_eq!(calendars.submits.load(Ordering::SeqCst), 9);

    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["status"], "failed");
    assert_eq!(status["calendars_accepted"], 0);
    assert!(
        status["last_error"]
            .as_str()
            .is_some_and(|error| error.contains("HTTP 503"))
    );
    app.clock.advance(3600);
    submit(&app, &calendars).await;
    assert_eq!(calendars.submits.load(Ordering::SeqCst), 9);
    let (status, _, _) = app.get_bytes(&format!("/cards/{hash}/timestamp.ots")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_upgrade_completes_the_proof_and_keeps_other_branches() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = two_of_three();
    submit(&app, &calendars).await;

    let before = parse_file(&proof(&app, &hash).await).expect("valid");
    let pending = before
        .timestamp
        .pending_attestations(&before.digest)
        .expect("computable");
    let alice = pending
        .iter()
        .find(|p| p.uri == ALICE_CAL)
        .expect("alice branch");
    calendars.upgrades.lock().expect("lock").insert(
        (ALICE_CAL.to_owned(), hex::encode(&alice.commitment)),
        bitcoin_upgrade(915_000),
    );
    app.clock.advance(60);
    upgrade(&app, &calendars).await;

    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["status"], "complete");
    assert_eq!(status["bitcoin_block_height"], 915_000);
    assert_eq!(status["completed_at"], common::NOW + 60);
    let (_, card) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(card["timestamp"]["status"], "complete");

    let after = parse_file(&proof(&app, &hash).await).expect("valid");
    assert_eq!(after.timestamp.bitcoin_heights(), vec![915_000]);
    // Bob's still-pending branch and Alice's pending attestation are kept.
    assert_eq!(
        after
            .timestamp
            .pending_attestations(&after.digest)
            .expect("computable"),
        pending
    );
}

#[tokio::test]
async fn not_yet_responses_leave_the_proof_pending() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = two_of_three();
    submit(&app, &calendars).await;
    let before = proof(&app, &hash).await;

    upgrade(&app, &calendars).await;
    upgrade(&app, &calendars).await;
    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["status"], "pending");
    assert_eq!(status["bitcoin_block_height"], Value::Null);
    assert_eq!(proof(&app, &hash).await, before);
    assert_eq!(calendars.upgrade_calls.lock().expect("lock").len(), 4);
}

#[tokio::test]
async fn proofs_pending_for_seven_days_fail_but_keep_the_proof() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = two_of_three();
    submit(&app, &calendars).await;
    let before = proof(&app, &hash).await;

    app.clock.advance(7 * 24 * 60 * 60);
    upgrade(&app, &calendars).await;
    assert_eq!(timestamp_status(&app, &hash).await["status"], "pending");
    app.clock.advance(1);
    upgrade(&app, &calendars).await;
    let status = timestamp_status(&app, &hash).await;
    assert_eq!(status["status"], "failed");
    assert_eq!(proof(&app, &hash).await, before);
}

#[tokio::test]
async fn upgrades_are_only_requested_from_trusted_calendars() {
    let app = app().await;
    let hash = activated(&app).await;
    let calendars = Arc::new(FakeCalendars::default());
    let pools = pools();
    calendars.reply(&pools[0], Ok(calendar_reply(1, "https://169.254.169.254")));
    calendars.reply(&pools[1], Ok(calendar_reply(2, "https://evil.example")));
    calendars.reply(&pools[2], Ok(calendar_reply(3, "https://a.pool.test")));
    submit(&app, &calendars).await;
    assert_eq!(timestamp_status(&app, &hash).await["calendars_accepted"], 3);

    upgrade(&app, &calendars).await;
    assert_eq!(
        *calendars.upgrade_calls.lock().expect("lock"),
        vec!["https://a.pool.test".to_owned()]
    );
}

#[test]
fn upgrade_allowlist_matches_the_reference_client() {
    let configured = pools();
    for allowed in [
        ALICE_CAL,
        "https://finney.calendar.eternitywall.com",
        "https://a.pool.test",
    ] {
        assert!(
            is_allowed_upgrade_calendar(&url(allowed), &configured),
            "{allowed}"
        );
    }
    for refused in [
        "http://alice.btc.calendar.opentimestamps.org",
        "https://calendar.opentimestamps.org",
        "https://alice.btc.calendar.opentimestamps.org.evil.example",
        "https://alice.btc.calendar.opentimestamps.org:8443",
        "https://alice.btc.calendar.opentimestamps.org/x",
        "https://evil.example",
    ] {
        assert!(
            !is_allowed_upgrade_calendar(&url(refused), &configured),
            "{refused}"
        );
    }
}

#[tokio::test]
async fn timestamp_endpoints_404_for_unknown_cards() {
    let app = app().await;
    let unknown = "f".repeat(64);
    let (status, body) = app.get(&format!("/cards/{unknown}/timestamp")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "timestamp_not_found");
    let (status, _, _) = app
        .get_bytes(&format!("/cards/{unknown}/timestamp.ots"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
