//! Background stamping: submitting activated cards to calendars and
//! upgrading pending proofs.
//!
//! Activation only inserts a `queued` row in `card_timestamps`; these workers
//! do the rest outside any API request. Network calls happen with no database
//! transaction open; results are merged into a freshly re-read proof inside a
//! short write transaction, so the two workers never overwrite each other.
//!
//! `complete` means a calendar supplied a Bitcoin block attestation. The
//! server does not check block headers against Bitcoin itself; independent
//! verification is `ots verify <hash>.json.ots` with the canonical card JSON
//! saved as `<hash>.json`.

use std::{sync::Arc, time::Duration};

use sqlx::{FromRow, SqlitePool};
use tokio::task::JoinSet;
use url::Url;

use super::{
    calendar::CalendarClient,
    format::{
        DetachedTimestampFile, FileHashOp, MAX_FILE_BYTES, OtsError, PendingRef, Timestamp,
        parse_file, parse_timestamp, serialize_file,
    },
};
use crate::{Clock, db::begin_write, db_time, shutdown::Shutdown};

/// Stored stamping state.
pub mod status {
    /// Waiting for the first calendar to accept the card hash.
    pub const QUEUED: &str = "queued";
    /// At least one calendar accepted; waiting for a Bitcoin attestation.
    pub const PENDING: &str = "pending";
    /// A calendar supplied a Bitcoin block attestation. Not checked against
    /// Bitcoin by this server.
    pub const COMPLETE: &str = "complete";
    /// No calendar accepted, or no Bitcoin attestation arrived in time.
    pub const FAILED: &str = "failed";
}

/// Calendar hosts the reference client trusts for upgrades (its
/// `DEFAULT_CALENDAR_WHITELIST`). A pending attestation is followed only if
/// its URI matches one of these or is a configured calendar, so a calendar
/// response cannot make this server request arbitrary URLs.
pub const UPGRADE_ALLOWED_HOST_SUFFIXES: [&str; 3] = [
    ".calendar.opentimestamps.org",
    ".calendar.eternitywall.com",
    ".calendar.catallaxy.com",
];

/// Most pending attestations queried per proof per upgrade pass.
pub const MAX_UPGRADE_QUERIES: usize = 16;

/// Worker timing and limits.
#[derive(Debug, Clone)]
pub struct TimestampSettings {
    /// Calendars cards are submitted to (`OTS_CALENDARS`).
    pub calendars: Vec<Url>,
    /// Pause between submit passes.
    pub submit_poll_interval: Duration,
    /// Pause between upgrade passes (`OTS_UPGRADE_INTERVAL_SECS`).
    pub upgrade_interval: Duration,
    /// Delay before the first submit retry; each later retry doubles it.
    pub base_delay_secs: u64,
    /// Upper bound on the submit retry delay.
    pub max_delay_secs: u64,
    /// Submit attempts before giving up on the calendars that failed.
    pub max_submit_attempts: u32,
    /// How long a proof may stay pending before it is marked failed.
    pub max_pending_secs: u64,
    /// Rows handled per pass.
    pub batch_size: u32,
}

impl TimestampSettings {
    /// Defaults for these calendars and upgrade interval: retries after 30 s,
    /// 60 s, 120 s, … up to an hour apart, 8 attempts, pending for 7 days.
    #[must_use]
    pub fn new(calendars: Vec<Url>, upgrade_interval: Duration) -> Self {
        Self {
            calendars,
            submit_poll_interval: Duration::from_secs(2),
            upgrade_interval,
            base_delay_secs: 30,
            max_delay_secs: 60 * 60,
            max_submit_attempts: 8,
            max_pending_secs: 7 * 24 * 60 * 60,
            batch_size: 20,
        }
    }

    /// Delay after the `attempts`-th failed submission.
    #[must_use]
    pub fn retry_delay_secs(&self, attempts: u32) -> u64 {
        let doublings = attempts.saturating_sub(1).min(63);
        self.base_delay_secs
            .saturating_mul(1_u64 << doublings)
            .min(self.max_delay_secs)
    }
}

/// Inserts the `queued` row for a card that just activated. Call inside the
/// activating transaction; this is the only stamping work activation does.
pub(crate) async fn enqueue(
    conn: &mut sqlx::SqliteConnection,
    card_hash: &str,
    now: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO card_timestamps (card_hash, status, next_attempt_at) VALUES (?, 'queued', ?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(card_hash)
    .bind(db_time(now))
    .execute(conn)
    .await?;
    Ok(())
}

/// Whether an upgrade may be requested from a pending attestation's URI.
#[must_use]
pub fn is_allowed_upgrade_calendar(uri: &Url, configured: &[Url]) -> bool {
    let plain = uri.scheme() == "https"
        && uri.port().is_none()
        && uri.path() == "/"
        && uri.query().is_none()
        && uri.fragment().is_none()
        && uri.username().is_empty()
        && uri.password().is_none();
    let Some(host) = uri.host_str() else {
        return false;
    };
    configured.iter().any(|calendar| calendar == uri)
        || (plain
            && UPGRADE_ALLOWED_HOST_SUFFIXES
                .iter()
                .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix)))
}

/// A new proof for `digest` from calendar responses, merged under the root.
/// `None` if there are no responses.
#[must_use]
pub fn proof_from_responses(
    digest: [u8; 32],
    responses: Vec<Timestamp>,
) -> Option<DetachedTimestampFile> {
    if responses.is_empty() {
        return None;
    }
    let mut timestamp = Timestamp::default();
    for response in responses {
        timestamp.merge(response);
    }
    Some(DetachedTimestampFile {
        file_hash_op: FileHashOp::Sha256,
        digest: digest.to_vec(),
        timestamp,
    })
}

/// Parses a calendar's submit response and checks it against the digest.
fn parse_submit_response(bytes: &[u8], digest: &[u8; 32]) -> Result<Timestamp, OtsError> {
    let timestamp = parse_timestamp(bytes, 1)?;
    timestamp.check_messages(digest)?;
    Ok(timestamp)
}

#[derive(FromRow)]
struct SubmitRow {
    card_hash: String,
    attempts: i64,
    calendars_accepted: i64,
    retry_calendars: Option<String>,
}

#[derive(FromRow)]
struct ProofRow {
    card_hash: String,
    ots_proof: Option<Vec<u8>>,
    submitted_at: Option<i64>,
}

/// Submits every due row once: `queued` rows to all calendars, `pending`
/// rows to the calendars that failed before. Returns how many rows it tried.
///
/// # Errors
///
/// Returns a database error; calendar failures are recorded, not returned.
pub async fn submit_due<C: CalendarClient>(
    pool: &SqlitePool,
    client: &Arc<C>,
    clock: &dyn Clock,
    settings: &TimestampSettings,
) -> Result<usize, sqlx::Error> {
    let now = clock.now_unix();
    let due: Vec<SubmitRow> = sqlx::query_as(
        "SELECT card_hash, attempts, calendars_accepted, retry_calendars \
         FROM card_timestamps WHERE next_attempt_at <= ? \
         AND (status = 'queued' OR (status = 'pending' AND retry_calendars IS NOT NULL)) \
         ORDER BY next_attempt_at LIMIT ?",
    )
    .bind(db_time(now))
    .bind(settings.batch_size)
    .fetch_all(pool)
    .await?;
    for row in &due {
        submit_one(pool, client, clock, settings, row).await?;
    }
    Ok(due.len())
}

async fn submit_one<C: CalendarClient>(
    pool: &SqlitePool,
    client: &Arc<C>,
    clock: &dyn Clock,
    settings: &TimestampSettings,
    row: &SubmitRow,
) -> Result<(), sqlx::Error> {
    let Some(digest) = decode_hash(&row.card_hash) else {
        return give_up(pool, &row.card_hash, "card hash is not 32 bytes of hex").await;
    };
    let targets: Vec<Url> = match &row.retry_calendars {
        Some(list) => list
            .lines()
            .filter_map(|line| Url::parse(line).ok())
            .collect(),
        None => settings.calendars.clone(),
    };
    let (accepted, failed) = submit_to_calendars(client, targets, digest).await;
    store_submission(pool, clock, settings, row, digest, accepted, &failed).await
}

/// Submits `digest` to every calendar at once. Returns the parsed accepted
/// responses and each failed calendar with its error, sorted.
async fn submit_to_calendars<C: CalendarClient>(
    client: &Arc<C>,
    targets: Vec<Url>,
    digest: [u8; 32],
) -> (Vec<Timestamp>, Vec<(Url, String)>) {
    let mut requests = JoinSet::new();
    for calendar in targets {
        let client = Arc::clone(client);
        requests.spawn(async move {
            let outcome = client.submit(&calendar, digest).await;
            (calendar, outcome)
        });
    }
    let mut accepted = Vec::new();
    let mut failed: Vec<(Url, String)> = Vec::new();
    while let Some(joined) = requests.join_next().await {
        let Ok((calendar, outcome)) = joined else {
            continue;
        };
        match outcome
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                parse_submit_response(&bytes, &digest)
                    .map_err(|error| format!("invalid response: {error}"))
            }) {
            Ok(timestamp) => accepted.push(timestamp),
            Err(error) => failed.push((calendar, error)),
        }
    }
    failed.sort();
    (accepted, failed)
}

/// Records one submit attempt, merging accepted responses into the proof.
async fn store_submission(
    pool: &SqlitePool,
    clock: &dyn Clock,
    settings: &TimestampSettings,
    row: &SubmitRow,
    digest: [u8; 32],
    accepted: Vec<Timestamp>,
    failed: &[(Url, String)],
) -> Result<(), sqlx::Error> {
    let now = clock.now_unix();
    let attempts = u32::try_from(row.attempts.saturating_add(1)).unwrap_or(u32::MAX);
    let accepted_now = i64::try_from(accepted.len()).unwrap_or(i64::MAX);
    let last_error = (!failed.is_empty()).then(|| {
        failed
            .iter()
            .map(|(calendar, error)| format!("{calendar}: {error}"))
            .collect::<Vec<_>>()
            .join("; ")
    });
    let retry = (!failed.is_empty() && attempts < settings.max_submit_attempts).then(|| {
        failed
            .iter()
            .map(|(calendar, _)| calendar.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    });
    let next_attempt_at = db_time(now.saturating_add(settings.retry_delay_secs(attempts)));

    let mut tx = begin_write(pool).await?;
    let current: Option<ProofRow> = sqlx::query_as(
        "SELECT card_hash, ots_proof, submitted_at FROM card_timestamps WHERE card_hash = ?",
    )
    .bind(&row.card_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current) = current else {
        return Ok(());
    };
    let total_accepted = row.calendars_accepted.saturating_add(accepted_now);
    if total_accepted == 0 {
        let status = if retry.is_some() || failed.is_empty() {
            status::QUEUED
        } else {
            status::FAILED
        };
        sqlx::query(
            "UPDATE card_timestamps SET status = ?, attempts = ?, last_error = ?, \
             next_attempt_at = ? WHERE card_hash = ?",
        )
        .bind(status)
        .bind(attempts)
        .bind(last_error.as_deref().unwrap_or("no calendars configured"))
        .bind(next_attempt_at)
        .bind(&row.card_hash)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(());
    }

    let proof = match merge_into_stored(current.ots_proof.as_deref(), digest, accepted) {
        Ok(proof) => proof,
        Err(error) => {
            drop(tx);
            return give_up(
                pool,
                &row.card_hash,
                &format!("could not merge proof: {error}"),
            )
            .await;
        }
    };
    // The upgrader may have finished this row meanwhile; keep its status.
    sqlx::query(
        "UPDATE card_timestamps SET ots_proof = ?, \
         status = CASE WHEN status IN ('complete', 'failed') THEN status ELSE 'pending' END, \
         calendars_accepted = ?, attempts = ?, last_error = ?, retry_calendars = ?, \
         next_attempt_at = ?, submitted_at = COALESCE(submitted_at, ?) WHERE card_hash = ?",
    )
    .bind(proof)
    .bind(total_accepted)
    .bind(attempts)
    .bind(last_error)
    .bind(retry)
    .bind(next_attempt_at)
    .bind(current.submitted_at.unwrap_or(db_time(now)))
    .bind(&row.card_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Merges new calendar subtrees into the stored proof (or a new one).
fn merge_into_stored(
    stored: Option<&[u8]>,
    digest: [u8; 32],
    responses: Vec<Timestamp>,
) -> Result<Vec<u8>, OtsError> {
    let file = match stored {
        Some(bytes) => {
            let mut file = parse_file(bytes)?;
            for response in responses {
                file.timestamp.merge(response);
            }
            file
        }
        None => proof_from_responses(digest, responses).ok_or(OtsError::Empty)?,
    };
    let bytes = serialize_file(&file);
    if bytes.len() > MAX_FILE_BYTES {
        return Err(OtsError::TooLarge {
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(bytes)
}

/// Stops retrying a row that cannot make progress: a queued row fails, a
/// pending one keeps its proof but stops retrying calendars.
async fn give_up(pool: &SqlitePool, card_hash: &str, error: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE card_timestamps SET last_error = ?, retry_calendars = NULL, \
         status = CASE WHEN status = 'queued' THEN 'failed' ELSE status END WHERE card_hash = ?",
    )
    .bind(error)
    .bind(card_hash)
    .execute(pool)
    .await?;
    Ok(())
}

fn decode_hash(card_hash: &str) -> Option<[u8; 32]> {
    hex::decode(card_hash).ok()?.try_into().ok()
}

/// One upgrade fetched from a calendar, ready to merge.
struct Upgrade {
    path: Vec<usize>,
    timestamp: Timestamp,
}

/// Tries to upgrade every pending proof once, and fails proofs pending longer
/// than `max_pending_secs` (keeping the proof). Returns how many proofs it
/// checked.
///
/// # Errors
///
/// Returns a database error; calendar failures are recorded, not returned.
pub async fn upgrade_pending<C: CalendarClient>(
    pool: &SqlitePool,
    client: &Arc<C>,
    clock: &dyn Clock,
    settings: &TimestampSettings,
) -> Result<usize, sqlx::Error> {
    let now = clock.now_unix();
    let expired_before = db_time(now.saturating_sub(settings.max_pending_secs));
    sqlx::query(
        "UPDATE card_timestamps SET status = 'failed', retry_calendars = NULL, \
         last_error = 'no Bitcoin attestation within the pending time limit' \
         WHERE status = 'pending' AND submitted_at < ?",
    )
    .bind(expired_before)
    .execute(pool)
    .await?;

    let pending: Vec<ProofRow> = sqlx::query_as(
        "SELECT card_hash, ots_proof, submitted_at FROM card_timestamps \
         WHERE status = 'pending' AND ots_proof IS NOT NULL ORDER BY card_hash",
    )
    .fetch_all(pool)
    .await?;
    for row in &pending {
        upgrade_one(pool, client, clock, settings, row).await?;
    }
    Ok(pending.len())
}

async fn upgrade_one<C: CalendarClient>(
    pool: &SqlitePool,
    client: &Arc<C>,
    clock: &dyn Clock,
    settings: &TimestampSettings,
    row: &ProofRow,
) -> Result<(), sqlx::Error> {
    let Some(stored) = row.ots_proof.as_deref() else {
        return Ok(());
    };
    let now = clock.now_unix();
    let refs = match parse_file(stored)
        .and_then(|file| file.timestamp.pending_attestations(&file.digest))
    {
        Ok(refs) => refs,
        Err(error) => {
            return give_up(
                pool,
                &row.card_hash,
                &format!("stored proof is invalid: {error}"),
            )
            .await;
        }
    };
    let (upgrades, errors) = fetch_upgrades(client, settings, refs).await;
    if upgrades.is_empty() && errors.is_empty() {
        return Ok(());
    }
    store_upgrades(pool, &row.card_hash, upgrades, errors, now).await
}

/// Asks each allowed calendar once per commitment for an upgrade. Returns the
/// parsed upgrades and the errors met.
async fn fetch_upgrades<C: CalendarClient>(
    client: &Arc<C>,
    settings: &TimestampSettings,
    refs: Vec<PendingRef>,
) -> (Vec<Upgrade>, Vec<String>) {
    let mut upgrades = Vec::new();
    let mut errors = Vec::new();
    let mut asked = Vec::new();
    for pending in refs {
        if asked.len() >= MAX_UPGRADE_QUERIES {
            break;
        }
        let calendar = match Url::parse(&pending.uri) {
            Ok(url) if is_allowed_upgrade_calendar(&url, &settings.calendars) => url,
            _ => continue,
        };
        if asked.contains(&(calendar.clone(), pending.commitment.clone())) {
            continue;
        }
        asked.push((calendar.clone(), pending.commitment.clone()));
        match client.upgrade(&calendar, &pending.commitment).await {
            Ok(None) => {}
            Ok(Some(bytes)) => {
                let parsed =
                    parse_timestamp(&bytes, pending.path.len() + 1).and_then(|timestamp| {
                        timestamp.check_messages(&pending.commitment)?;
                        Ok(timestamp)
                    });
                match parsed {
                    Ok(timestamp) => upgrades.push(Upgrade {
                        path: pending.path,
                        timestamp,
                    }),
                    Err(error) => errors.push(format!("{calendar}: invalid upgrade: {error}")),
                }
            }
            Err(error) => errors.push(format!("{calendar}: {error}")),
        }
    }
    (upgrades, errors)
}

/// Merges fetched upgrades into the freshly re-read proof, completing it if
/// any branch now has a Bitcoin attestation.
async fn store_upgrades(
    pool: &SqlitePool,
    card_hash: &str,
    upgrades: Vec<Upgrade>,
    mut errors: Vec<String>,
    now: u64,
) -> Result<(), sqlx::Error> {
    let mut tx = begin_write(pool).await?;
    let current: Option<ProofRow> = sqlx::query_as(
        "SELECT card_hash, ots_proof, submitted_at FROM card_timestamps \
         WHERE card_hash = ? AND status = 'pending'",
    )
    .bind(card_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(bytes) = current.and_then(|current| current.ots_proof) else {
        return Ok(());
    };
    let merged = parse_file(&bytes).and_then(|mut file| {
        for upgrade in upgrades {
            file.timestamp.merge_at(&upgrade.path, upgrade.timestamp)?;
        }
        let height = file.timestamp.bitcoin_heights().into_iter().min();
        let bytes = serialize_file(&file);
        if bytes.len() > MAX_FILE_BYTES {
            return Err(OtsError::TooLarge {
                limit: MAX_FILE_BYTES,
            });
        }
        Ok((bytes, height))
    });
    let (proof, height) = match merged {
        Ok(merged) => merged,
        Err(error) => {
            errors.push(format!("could not merge upgrade: {error}"));
            (bytes, None)
        }
    };
    let last_error = (!errors.is_empty()).then(|| errors.join("; "));
    match height {
        Some(height) => {
            sqlx::query(
                "UPDATE card_timestamps SET ots_proof = ?, status = 'complete', \
                 bitcoin_block_height = ?, completed_at = ?, retry_calendars = NULL, \
                 last_error = ? WHERE card_hash = ?",
            )
            .bind(proof)
            .bind(height)
            .bind(db_time(now))
            .bind(last_error)
            .bind(card_hash)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            sqlx::query(
                "UPDATE card_timestamps SET ots_proof = ?, last_error = ? WHERE card_hash = ?",
            )
            .bind(proof)
            .bind(last_error)
            .bind(card_hash)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await
}

/// Runs [`submit_due`] until `shutdown` is requested; a pass in progress
/// always finishes. Spawn it as a background task.
pub async fn run_submitter<C: CalendarClient>(
    pool: SqlitePool,
    client: Arc<C>,
    clock: Arc<dyn Clock>,
    settings: TimestampSettings,
    mut shutdown: Shutdown,
) {
    loop {
        if let Err(error) = submit_due(&pool, &client, clock.as_ref(), &settings).await {
            tracing::error!(%error, "timestamp submit pass failed");
        }
        if !shutdown.sleep(settings.submit_poll_interval).await {
            break;
        }
    }
    tracing::info!("timestamp submitter stopped");
}

/// Runs [`upgrade_pending`] every `upgrade_interval` until `shutdown` is
/// requested; a pass in progress always finishes. Spawn it as a background
/// task.
pub async fn run_upgrader<C: CalendarClient>(
    pool: SqlitePool,
    client: Arc<C>,
    clock: Arc<dyn Clock>,
    settings: TimestampSettings,
    mut shutdown: Shutdown,
) {
    while shutdown.sleep(settings.upgrade_interval).await {
        if let Err(error) = upgrade_pending(&pool, &client, clock.as_ref(), &settings).await {
            tracing::error!(%error, "timestamp upgrade pass failed");
        }
    }
    tracing::info!("timestamp upgrader stopped");
}
