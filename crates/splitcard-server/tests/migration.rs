//! Migrating a database from before standalone teams keeps every team, card,
//! signature, outbox entry and timestamp, and the card still verifies.
//!
//! `fixtures/migration/v5.sql` is a dump of a database at migration 5 with
//! one event, one team, three participants, an active signed card, its
//! signatures, published outbox entries and a completed timestamp.
//! `v5_report.json` is that card's verify report from the code of the time.
//!
//! This is the only test in its binary: tracing caches per-callsite interest
//! process-wide, so a parallel test without a subscriber could hide the logs.

mod common;

use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use axum::http::StatusCode;
use common::{TestApp, app_with, config};
use serde_json::Value;
use splitcard_server::db::{self, MIGRATOR};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tracing_subscriber::fmt::MakeWriter;

const V5_DUMP: &str = include_str!("fixtures/migration/v5.sql");
const V5_REPORT: &str = include_str!("fixtures/migration/v5_report.json");
const REMOVED_CHECKS: [&str; 3] = [
    "payout_amounts_match_card",
    "payout_card_matches",
    "payments_proven",
];

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// A pool on `path` that runs no migrations.
async fn raw_pool(path: &Path) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("raw pool")
}

async fn rows(pool: &SqlitePool, sql: &str) -> Vec<(String, String)> {
    sqlx::query_as(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_all(pool)
        .await
        .expect("query")
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .expect("count")
}

const KEPT_TABLES: [&str; 6] = [
    "cards",
    "card_signatures",
    "relay_outbox",
    "relay_deliveries",
    "card_timestamps",
    "used_auth_events",
];

async fn fixture_card(dir: &Path) {
    let path = dir.join("fixture.db");
    let before = raw_pool(&path).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(V5_DUMP))
        .execute(&before)
        .await
        .expect("load the dump");
    let version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&before)
        .await
        .expect("version");
    assert_eq!(version, 5, "the fixture is at the previous migration");
    let mut kept = Vec::new();
    for table in KEPT_TABLES {
        kept.push(count(&before, table).await);
    }
    assert_eq!(
        &kept[..3],
        [1, 3, 4],
        "a card, three signatures, four outbox entries"
    );
    before.close().await;

    let pool = db::connect(path.to_str().expect("UTF-8 path"))
        .await
        .expect("migrate");
    for (table, before) in KEPT_TABLES.iter().zip(kept) {
        assert_eq!(count(&pool, table).await, before, "{table}");
    }
    let problems = rows(
        &pool,
        "SELECT \"table\", parent FROM pragma_foreign_key_check",
    )
    .await;
    assert!(problems.is_empty(), "{problems:?}");

    // The team keeps its event's slug as its label, so its card still matches.
    let teams = rows(&pool, "SELECT slug, event_label FROM teams").await;
    assert_eq!(
        teams,
        [("lightning-llamas".to_owned(), "nostr-hack-2026".to_owned())]
    );
    let profiles = rows(
        &pool,
        "SELECT name, lightning_address FROM profiles ORDER BY name",
    )
    .await;
    assert_eq!(
        profiles,
        [
            ("Alice", "alice@blink.sv"),
            ("Bob", "bob@getalby.com"),
            ("Chidi", "chidi@walletofsatoshi.com")
        ]
        .map(|(name, address)| (name.to_owned(), address.to_owned()))
    );
    assert_eq!(count(&pool, "participants").await, 3);

    // The report is the old one, less the removed payout checks.
    let mut expected: Value = serde_json::from_str(V5_REPORT).expect("old report");
    let hash = expected["card_hash"].as_str().expect("hash").to_owned();
    let old = expected.as_object_mut().expect("object");
    assert_eq!(old.remove("payout"), Some(Value::Null));
    old.get_mut("checks")
        .and_then(Value::as_array_mut)
        .expect("checks")
        .retain(|check| !REMOVED_CHECKS.contains(&check["name"].as_str().unwrap_or_default()));
    assert_eq!(expected["overall"], "verified");

    let app: TestApp = app_with(pool, config());
    let (status, report) = app.get(&format!("/cards/{hash}/verify")).await;
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report, expected);
    let (status, team_report) = app.get("/teams/1/verify").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(team_report, expected);
}

/// Several events and teams written directly at migration 5.
async fn several_teams(dir: &Path) {
    let path = dir.join("teams.db");
    let before = raw_pool(&path).await;
    MIGRATOR.run_to(5, &before).await.expect("migrate to 5");
    sqlx::raw_sql(
        "INSERT INTO events (id, slug, name, created_by, created_at) VALUES \
           (1, 'hack-a', 'Hack A', 'org', 1), (2, 'hack-b', 'Hack B', 'org', 1); \
         INSERT INTO teams (id, event_id, name, join_code, created_at) VALUES \
           (1, 1, 'Llamas', 'code1', 1), (2, 2, 'Llamas', 'code2', 1), \
           (3, 1, 'API', 'code3', 1), (4, 1, 'Ñ!', 'code4', 1); \
         INSERT INTO participants (team_id, pubkey, name, lightning_address, joined_at) VALUES \
           (1, 'aa', 'Ann', 'ann@blink.sv', 100), (2, 'aa', 'Annie', 'annie@blink.sv', 200), \
           (3, 'bb', 'Ben', 'ben@blink.sv', 150), (2, 'bb', 'Benny', 'ben@blink.sv', 120);",
    )
    .execute(&before)
    .await
    .expect("old rows");
    before.close().await;

    let pool = db::connect(path.to_str().expect("UTF-8 path"))
        .await
        .expect("migrate");
    let teams = rows(&pool, "SELECT slug, event_label FROM teams ORDER BY id").await;
    assert_eq!(
        teams,
        [
            ("llamas", "hack-a"),
            ("llamas-2", "hack-b"),
            ("api-team", "hack-a"),
            ("team", "hack-a")
        ]
        .map(|(slug, label)| (slug.to_owned(), label.to_owned()))
    );
    // Someone on several teams keeps the details of their latest join.
    let profiles = rows(&pool, "SELECT pubkey, name FROM profiles ORDER BY pubkey").await;
    assert_eq!(
        profiles,
        [("aa", "Annie"), ("bb", "Ben")].map(|(key, name)| (key.to_owned(), name.to_owned()))
    );
    assert_eq!(count(&pool, "participants").await, 4);

    // Migrating again changes nothing.
    pool.close().await;
    let again = db::connect(path.to_str().expect("UTF-8 path"))
        .await
        .expect("reopen");
    assert_eq!(count(&again, "teams").await, 4);
}

#[tokio::test]
async fn existing_teams_and_cards_survive_the_migration() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let dir = std::env::temp_dir().join(format!("splitcard-migration-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    fixture_card(&dir).await;
    several_teams(&dir).await;
    std::fs::remove_dir_all(&dir).expect("clean up");

    let logs = String::from_utf8(logs.0.lock().expect("lock").clone()).expect("UTF-8 logs");
    for line in [
        "team_id=1 name=Lightning Llamas slug=lightning-llamas",
        "team_id=1 name=Llamas slug=llamas",
        "team_id=2 name=Llamas slug=llamas-2",
        "team_id=3 name=API slug=api-team",
        "team_id=4 name=Ñ! slug=team",
    ] {
        assert!(logs.contains(line), "{line} not in {logs}");
    }
    assert_eq!(logs.matches("derived a slug").count(), 5, "{logs}");
}
