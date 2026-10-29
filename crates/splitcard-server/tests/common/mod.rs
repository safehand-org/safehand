//! Shared test harness: an in-memory database per app, fixed keys, a manual
//! clock and NIP-98 header construction.

#![allow(dead_code, clippy::missing_panics_doc)]

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode},
};
use nostr::{
    event::{Event, EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, SecretKey},
    types::Timestamp,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use splitcard_core::{
    card::{Card, Member, Payout, Rules},
    signature::unsigned_signature_template,
};
use splitcard_server::{AppState, Clock, config::Config, db, router};
use tower::ServiceExt;

pub const NOW: u64 = 1_780_000_000;
pub const BASE_URL: &str = "https://splitcard.test";
pub const EVENT_LABEL: &str = "Nostr Hack 2026";
pub const TEAM_NAME: &str = "Lightning Llamas";
pub const TEAM_SLUG: &str = "lightning-llamas";
pub const SERVER_SECRET_HEX: &str =
    "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";

pub const ALICE: u8 = 1;
pub const BOB: u8 = 2;
pub const CHIDI: u8 = 3;
pub const DAYO: u8 = 4;
pub const EVE: u8 = 5;

/// Makes every [`TestApp::auth_header`] event unique.
static NONCE: AtomicU64 = AtomicU64::new(0);

/// A clock tests move by hand.
#[derive(Debug)]
pub struct ManualClock(AtomicU64);

impl ManualClock {
    pub fn advance(&self, secs: u64) {
        self.0.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_unix(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn keys(number: u8) -> Keys {
    Keys::new(SecretKey::from_slice(&[number; 32]).expect("fixed secret is valid"))
}

pub fn pubkey(number: u8) -> String {
    keys(number).public_key().to_hex()
}

/// Display name and Lightning Address in each test person's profile.
pub fn person(number: u8) -> (&'static str, &'static str) {
    match number {
        ALICE => ("Alice", "alice@blink.sv"),
        BOB => ("Bob", "bob@getalby.com"),
        CHIDI => ("Chidi", "chidi@walletofsatoshi.com"),
        DAYO => ("Dayo", "dayo@strike.me"),
        EVE => ("Eve", "eve@coinos.io"),
        _ => ("Someone", "someone@example.com"),
    }
}

pub fn env() -> HashMap<&'static str, String> {
    HashMap::from([
        ("DATABASE_URL", "sqlite::memory:".to_owned()),
        ("PUBLIC_BASE_URL", format!("{BASE_URL}/")),
        (
            "RELAYS",
            "wss://relay.one.test,wss://relay.two.test".to_owned(),
        ),
        ("SERVER_SECRET_KEY", SERVER_SECRET_HEX.to_owned()),
    ])
}

pub fn config() -> Config {
    config_with_server_key(SERVER_SECRET_HEX)
}

/// The test config, signing card events with `server_secret_hex`.
pub fn config_with_server_key(server_secret_hex: &str) -> Config {
    let mut env = env();
    env.insert("SERVER_SECRET_KEY", server_secret_hex.to_owned());
    Config::from_lookup(|name| env.get(name).cloned()).expect("test config is valid")
}

pub struct TestApp {
    pub state: AppState,
    pub router: Router,
    pub clock: Arc<ManualClock>,
    /// Every response body, for leak checks.
    pub bodies: Mutex<Vec<String>>,
}

pub async fn app() -> TestApp {
    let db = db::connect_in_memory().await.expect("in-memory database");
    app_with(db, config())
}

/// A test app on an existing database, as after a restart with `config`.
pub fn app_with(db: sqlx::SqlitePool, config: Config) -> TestApp {
    let clock = Arc::new(ManualClock(AtomicU64::new(NOW)));
    let state = AppState {
        db,
        config: Arc::new(config),
        clock: clock.clone(),
    };
    TestApp {
        router: router(state.clone()),
        state,
        clock,
        bodies: Mutex::new(Vec::new()),
    }
}

/// Standard Base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A NIP-98 event with the given parts.
pub fn auth_event(signer: u8, kind: u16, created_at: u64, tags: &[&[&str]]) -> Event {
    let tags: Vec<Tag> = tags
        .iter()
        .map(|tag| Tag::parse(tag.iter().copied()).expect("valid tag"))
        .collect();
    EventBuilder::new(Kind::from_u16(kind), "")
        .tags(tags)
        .custom_created_at(Timestamp::from_secs(created_at))
        .finalize(&keys(signer))
        .expect("signing succeeds")
}

pub fn header_for(event: &Event) -> String {
    format!("Nostr {}", base64(event.as_json().as_bytes()))
}

impl TestApp {
    pub fn now(&self) -> u64 {
        self.clock.now_unix()
    }

    /// A valid NIP-98 header for this request. A per-header nonce tag keeps
    /// repeated identical requests in the same second from being replays.
    pub fn auth_header(&self, signer: u8, method: &str, path: &str, body: &[u8]) -> String {
        let url = format!("{BASE_URL}{path}");
        let payload = sha256_hex(body);
        let nonce = NONCE.fetch_add(1, Ordering::SeqCst).to_string();
        let (url_tag, method_tag) = (["u", url.as_str()], ["method", method]);
        let nonce_tag = ["nonce", nonce.as_str()];
        let mut tags: Vec<&[&str]> = vec![&url_tag, &method_tag, &nonce_tag];
        let payload_tag = ["payload", payload.as_str()];
        if !body.is_empty() {
            tags.push(&payload_tag);
        }
        header_for(&auth_event(signer, 27_235, self.now(), &tags))
    }

    pub async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router is infallible");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body is readable");
        let text = String::from_utf8(bytes.to_vec()).expect("body is UTF-8");
        let value = serde_json::from_str(&text).unwrap_or(Value::Null);
        self.bodies.lock().expect("lock").push(text);
        (status, value)
    }

    pub async fn raw(
        &self,
        method: Method,
        path: &str,
        header: Option<&str>,
        body: &[u8],
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(header) = header {
            request = request.header("authorization", header);
        }
        self.send(request.body(Body::from(body.to_vec())).expect("request"))
            .await
    }

    pub async fn authed(
        &self,
        signer: u8,
        method: Method,
        path: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let header = self.auth_header(signer, method.as_str(), path, body.as_bytes());
        self.raw(method, path, Some(&header), body.as_bytes()).await
    }

    /// GETs `path` and returns the status, headers and raw body.
    pub async fn get_bytes(&self, path: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let request = Request::builder()
            .uri(path)
            .body(Body::empty())
            .expect("request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("router is infallible");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body is readable");
        (status, headers, bytes.to_vec())
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.raw(Method::GET, path, None, b"").await
    }

    /// Sets `number`'s profile to `name` and `address`.
    pub async fn put_profile(&self, number: u8, name: &str, address: &str) -> (StatusCode, Value) {
        let body = json!({ "name": name, "lightning_address": address }).to_string();
        self.authed(number, Method::PUT, "/me/profile", &body).await
    }

    /// Gives `number` their [`person`] profile, expecting 200.
    pub async fn profile_ok(&self, number: u8) {
        let (name, address) = person(number);
        let (status, response) = self.put_profile(number, name, address).await;
        assert_eq!(status, StatusCode::OK, "{response}");
    }

    /// POSTs a team with `slug` as `creator`.
    pub async fn post_team(&self, creator: u8, slug: &str) -> (StatusCode, Value) {
        let body =
            json!({ "slug": slug, "name": TEAM_NAME, "event_label": EVENT_LABEL }).to_string();
        self.authed(creator, Method::POST, "/teams", &body).await
    }

    /// Gives `creator` a profile and creates the test team with them as
    /// first participant; returns the team id and join code.
    pub async fn create_team(&self, creator: u8) -> (String, String) {
        self.profile_ok(creator).await;
        let (status, team) = self.post_team(creator, TEAM_SLUG).await;
        assert_eq!(status, StatusCode::CREATED, "{team}");
        (
            team["id"].as_str().expect("id").to_owned(),
            team["join_code"].as_str().expect("join code").to_owned(),
        )
    }

    /// Joins with `join_code` as `joiner`, without touching their profile.
    pub async fn join_only(
        &self,
        team_id: &str,
        join_code: &str,
        joiner: u8,
    ) -> (StatusCode, Value) {
        let body = json!({ "join_code": join_code }).to_string();
        self.authed(
            joiner,
            Method::POST,
            &format!("/teams/{team_id}/join"),
            &body,
        )
        .await
    }

    /// Gives `joiner` their [`person`] profile and joins.
    pub async fn join(&self, team_id: &str, join_code: &str, joiner: u8) -> (StatusCode, Value) {
        self.profile_ok(joiner).await;
        self.join_only(team_id, join_code, joiner).await
    }

    /// Alice's team with Bob and Chidi joined.
    pub async fn alice_bob_chidi(&self) -> String {
        let (team_id, code) = self.create_team(ALICE).await;
        for joiner in [BOB, CHIDI] {
            let (status, response) = self.join(&team_id, &code, joiner).await;
            assert_eq!(status, StatusCode::OK, "{response}");
        }
        team_id
    }

    pub async fn submit_card(
        &self,
        submitter: u8,
        team_id: &str,
        card: &Card,
    ) -> (StatusCode, Value) {
        let body = serde_json::to_string(card).expect("card serializes");
        self.authed(
            submitter,
            Method::POST,
            &format!("/teams/{team_id}/cards"),
            &body,
        )
        .await
    }

    /// Submits `card` and returns its hash, expecting 201.
    pub async fn submit_ok(&self, team_id: &str, card: &Card) -> String {
        let (status, response) = self.submit_card(ALICE, team_id, card).await;
        assert_eq!(status, StatusCode::CREATED, "{response}");
        response["hash"].as_str().expect("hash").to_owned()
    }

    pub async fn sign(&self, hash: &str, event_json: &str) -> (StatusCode, Value) {
        self.raw(
            Method::POST,
            &format!("/cards/{hash}/signatures"),
            None,
            event_json.as_bytes(),
        )
        .await
    }

    /// Signs `hash` as `signer` and expects the signature to be stored.
    pub async fn sign_ok(&self, hash: &str, signer: u8) -> Value {
        let (status, response) = self.sign(hash, &signature(signer, hash, self.now())).await;
        assert_eq!(status, StatusCode::CREATED, "{response}");
        response
    }

    /// Alice 36, Bob 35, Chidi 29, signed by all three, so it is active.
    pub async fn active_first_card(&self) -> (String, String) {
        let team_id = self.alice_bob_chidi().await;
        let hash = self.submit_ok(&team_id, &first_card()).await;
        for signer in [ALICE, BOB, CHIDI] {
            self.sign_ok(&hash, signer).await;
        }
        (team_id, hash)
    }

    pub async fn outbox(&self) -> Vec<(String, String, i64, Option<String>)> {
        sqlx::query_as(
            "SELECT event_json, status, attempts, last_error FROM relay_outbox ORDER BY id",
        )
        .fetch_all(&self.state.db)
        .await
        .expect("outbox query")
    }
}

/// A signature event over `hash` by `signer`, as JSON.
pub fn signature(signer: u8, hash: &str, created_at: u64) -> String {
    let unsigned =
        unsigned_signature_template(hash, &pubkey(signer), created_at).expect("valid template");
    unsigned
        .finalize(&keys(signer))
        .expect("signing succeeds")
        .as_json()
}

/// A card for `members` (person number, weight) with this test event label
/// and team.
pub fn card(members: &[(u8, u64)], supersedes: Option<&str>) -> Card {
    Card {
        version: 1,
        card_id: "llamas-split".to_owned(),
        event: EVENT_LABEL.to_owned(),
        team: TEAM_NAME.to_owned(),
        members: members
            .iter()
            .map(|&(number, weight)| {
                let (name, address) = person(number);
                Member {
                    pubkey: pubkey(number),
                    name: name.to_owned(),
                    weight,
                    payout: Payout::Lightning {
                        address: address.to_owned(),
                    },
                }
            })
            .collect(),
        rules: Rules { peer_review: None },
        supersedes: supersedes.map(str::to_owned),
    }
}

/// Alice 36, Bob 35, Chidi 29.
pub fn first_card() -> Card {
    card(&[(ALICE, 36), (BOB, 35), (CHIDI, 29)], None)
}
