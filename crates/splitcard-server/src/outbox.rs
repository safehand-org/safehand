//! Publishing queued events from `relay_outbox` to Nostr relays.
//!
//! API handlers only insert rows; [`run`] publishes them in the background, so
//! relays being down never blocks or fails a request. Delivery is tracked per
//! relay in `relay_deliveries`: an entry is `sent` (published) as soon as one
//! relay accepts it, relays that have not accepted keep retrying with their
//! own exponential backoff, and the entry is `failed` only if no relay
//! accepted after all attempts.

use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};

use nostr::{event::Event, types::RelayUrl};
use nostr_sdk::client::Client;
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use url::Url;

use crate::{Clock, db_time, shutdown::Shutdown};

/// One relay's answer to a publish: `Ok` if it accepted the event.
pub type RelayResult = Result<(), String>;

/// Publishes signed events to Nostr relays.
pub trait RelayPublisher: Send + Sync + 'static {
    /// Every relay events are delivered to.
    fn relays(&self) -> Vec<Url>;

    /// Publishes `event` to `relays`, returning each relay's result. A relay
    /// missing from the result counts as not having accepted.
    fn publish(
        &self,
        event: &Event,
        relays: &[Url],
    ) -> impl Future<Output = Vec<(Url, RelayResult)>> + Send;
}

impl<P: RelayPublisher> RelayPublisher for Arc<P> {
    fn relays(&self) -> Vec<Url> {
        P::relays(self)
    }

    fn publish(
        &self,
        event: &Event,
        relays: &[Url],
    ) -> impl Future<Output = Vec<(Url, RelayResult)>> + Send {
        P::publish(self, event, relays)
    }
}

/// Retry policy for the outbox worker.
#[derive(Debug, Clone)]
pub struct OutboxSettings {
    /// Pause between passes over due entries.
    pub poll_interval: Duration,
    /// Delay before the first retry; each later retry doubles it.
    pub base_delay_secs: u64,
    /// Upper bound on the retry delay.
    pub max_delay_secs: u64,
    /// Attempts per relay before that relay gives up on an entry.
    pub max_attempts: u32,
    /// Relay deliveries attempted per pass.
    pub batch_size: u32,
}

impl Default for OutboxSettings {
    /// Retries after 10 s, 20 s, 40 s, … up to an hour apart, eight attempts
    /// per relay in all.
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            base_delay_secs: 10,
            max_delay_secs: 60 * 60,
            max_attempts: 8,
            batch_size: 50,
        }
    }
}

impl OutboxSettings {
    /// Delay after the `attempts`-th failed attempt: `base · 2^(attempts − 1)`,
    /// capped at `max_delay_secs`.
    #[must_use]
    pub fn retry_delay_secs(&self, attempts: u32) -> u64 {
        let doublings = attempts.saturating_sub(1).min(63);
        self.base_delay_secs
            .saturating_mul(1_u64 << doublings)
            .min(self.max_delay_secs)
    }
}

/// Queues a signed event's JSON for publishing. `server_pubkey` is the
/// signer of a server card event, recorded so verification never depends on
/// the current key; `None` for a member's signature event.
pub(crate) async fn enqueue(
    conn: &mut SqliteConnection,
    event_json: &str,
    server_pubkey: Option<&str>,
    now: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO relay_outbox (event_json, server_pubkey, status, attempts, \
         next_attempt_at) VALUES (?, ?, 'pending', 0, ?)",
    )
    .bind(event_json)
    .bind(server_pubkey)
    .bind(db_time(now))
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(FromRow)]
struct DueDelivery {
    outbox_id: i64,
    relay: String,
    attempts: i64,
    event_json: String,
}

/// Makes one attempt at every due relay delivery, oldest entry first, up to
/// `batch_size`, and returns how many deliveries were attempted.
///
/// New `pending` entries first get a delivery row for each of the
/// publisher's relays. Each entry's due relays are published to together.
///
/// # Errors
///
/// Returns a database error; publish failures are recorded, not returned.
pub async fn process_due<P: RelayPublisher>(
    pool: &SqlitePool,
    publisher: &P,
    clock: &dyn Clock,
    settings: &OutboxSettings,
) -> Result<usize, sqlx::Error> {
    let now = clock.now_unix();
    for relay in publisher.relays() {
        sqlx::query(
            "INSERT OR IGNORE INTO relay_deliveries (outbox_id, relay, status, attempts, \
             next_attempt_at) SELECT id, ?, 'pending', 0, next_attempt_at FROM relay_outbox \
             WHERE status = 'pending'",
        )
        .bind(relay.as_str())
        .execute(pool)
        .await?;
    }
    let due: Vec<DueDelivery> = sqlx::query_as(
        "SELECT d.outbox_id, d.relay, d.attempts, o.event_json FROM relay_deliveries d \
         JOIN relay_outbox o ON o.id = d.outbox_id \
         WHERE d.status = 'pending' AND d.next_attempt_at <= ? \
         ORDER BY d.outbox_id, d.relay LIMIT ?",
    )
    .bind(db_time(now))
    .bind(settings.batch_size)
    .fetch_all(pool)
    .await?;

    for entry in due.chunk_by(|left, right| left.outbox_id == right.outbox_id) {
        let relays: Vec<Url> = entry
            .iter()
            .filter_map(|delivery| Url::parse(&delivery.relay).ok())
            .collect();
        let results: HashMap<String, RelayResult> =
            match serde_json::from_str::<Event>(&entry[0].event_json) {
                Ok(event) => publisher
                    .publish(&event, &relays)
                    .await
                    .into_iter()
                    .map(|(relay, result)| (relay.to_string(), result))
                    .collect(),
                Err(error) => {
                    let error = format!("stored event is not valid JSON: {error}");
                    entry
                        .iter()
                        .map(|delivery| (delivery.relay.clone(), Err(error.clone())))
                        .collect()
                }
            };
        let mut conn = pool.acquire().await?;
        for delivery in entry {
            let result = results
                .get(&delivery.relay)
                .cloned()
                .unwrap_or_else(|| Err("relay did not answer".to_owned()));
            record_delivery(&mut conn, delivery, result, now, settings).await?;
        }
        summarize_entry(&mut conn, entry[0].outbox_id).await?;
    }
    Ok(due.len())
}

/// Records one relay's result for an entry.
async fn record_delivery(
    conn: &mut SqliteConnection,
    delivery: &DueDelivery,
    result: RelayResult,
    now: u64,
    settings: &OutboxSettings,
) -> Result<(), sqlx::Error> {
    let attempts = u32::try_from(delivery.attempts.saturating_add(1)).unwrap_or(u32::MAX);
    match result {
        Ok(()) => {
            sqlx::query(
                "UPDATE relay_deliveries SET status = 'accepted', attempts = ?, \
                 last_error = NULL WHERE outbox_id = ? AND relay = ?",
            )
            .bind(attempts)
            .bind(delivery.outbox_id)
            .bind(&delivery.relay)
            .execute(conn)
            .await?;
        }
        Err(error) => {
            let status = if attempts >= settings.max_attempts {
                "failed"
            } else {
                "pending"
            };
            let next_attempt_at = now.saturating_add(settings.retry_delay_secs(attempts));
            tracing::warn!(
                id = delivery.outbox_id,
                relay = %delivery.relay,
                attempts,
                %error,
                "relay publish failed"
            );
            sqlx::query(
                "UPDATE relay_deliveries SET status = ?, attempts = ?, last_error = ?, \
                 next_attempt_at = ? WHERE outbox_id = ? AND relay = ?",
            )
            .bind(status)
            .bind(attempts)
            .bind(&error)
            .bind(db_time(next_attempt_at))
            .bind(delivery.outbox_id)
            .bind(&delivery.relay)
            .execute(conn)
            .await?;
        }
    }
    Ok(())
}

/// Sets an entry's status from its deliveries: `sent` once any relay
/// accepted, `failed` once every relay gave up, otherwise `pending`. Its
/// `attempts` is the most any relay made, and `last_error` lists the latest
/// error of each relay that has not accepted.
async fn summarize_entry(conn: &mut SqliteConnection, outbox_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE relay_outbox SET \
         status = CASE \
           WHEN EXISTS (SELECT 1 FROM relay_deliveries \
             WHERE outbox_id = relay_outbox.id AND status = 'accepted') THEN 'sent' \
           WHEN EXISTS (SELECT 1 FROM relay_deliveries \
             WHERE outbox_id = relay_outbox.id AND status = 'pending') THEN 'pending' \
           ELSE 'failed' END, \
         attempts = (SELECT MAX(attempts) FROM relay_deliveries \
           WHERE outbox_id = relay_outbox.id), \
         last_error = (SELECT group_concat(relay || ': ' || last_error, '; ') FROM \
           (SELECT relay, last_error FROM relay_deliveries WHERE outbox_id = relay_outbox.id \
            AND status != 'accepted' AND last_error IS NOT NULL ORDER BY relay)), \
         next_attempt_at = COALESCE((SELECT MIN(next_attempt_at) FROM relay_deliveries \
           WHERE outbox_id = relay_outbox.id AND status = 'pending'), next_attempt_at) \
         WHERE id = ?",
    )
    .bind(outbox_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Runs [`process_due`], pausing `poll_interval` between passes, until
/// `shutdown` is requested; a pass in progress always finishes. Spawn it as a
/// background task.
pub async fn run<P: RelayPublisher>(
    pool: SqlitePool,
    publisher: P,
    clock: Arc<dyn Clock>,
    settings: OutboxSettings,
    mut shutdown: Shutdown,
) {
    loop {
        if let Err(error) = process_due(&pool, &publisher, clock.as_ref(), &settings).await {
            tracing::error!(%error, "relay outbox pass failed");
        }
        if !shutdown.sleep(settings.poll_interval).await {
            break;
        }
    }
    tracing::info!("relay outbox stopped");
}

/// Publishes through `nostr-sdk` to a fixed set of relays.
pub struct NostrRelayPublisher {
    client: Client,
    relays: Vec<Url>,
}

impl NostrRelayPublisher {
    /// Adds `relays` and starts connecting in the background; it does not
    /// wait for any relay to come up.
    ///
    /// # Errors
    ///
    /// Returns an error if a relay URL is rejected.
    pub async fn connect(relays: &[Url]) -> Result<Self, nostr_sdk::error::Error> {
        let client = Client::default();
        for relay in relays {
            client.add_relay(relay.as_str()).await?;
        }
        client.connect().await;
        Ok(Self {
            client,
            relays: relays.to_vec(),
        })
    }
}

impl RelayPublisher for NostrRelayPublisher {
    fn relays(&self) -> Vec<Url> {
        self.relays.clone()
    }

    async fn publish(&self, event: &Event, relays: &[Url]) -> Vec<(Url, RelayResult)> {
        let parsed: Vec<(Url, Result<RelayUrl, String>)> = relays
            .iter()
            .map(|relay| {
                let parsed = RelayUrl::parse(relay.as_str()).map_err(|error| error.to_string());
                (relay.clone(), parsed)
            })
            .collect();
        let targets: Vec<RelayUrl> = parsed
            .iter()
            .filter_map(|(_, parsed)| parsed.as_ref().ok().cloned())
            .collect();
        let output = self
            .client
            .send_event(event)
            .to(targets)
            .await
            .map_err(|error| error.to_string());
        parsed
            .into_iter()
            .map(|(relay, parsed)| {
                let result = parsed.and_then(|url| match &output {
                    Err(error) => Err(error.clone()),
                    Ok(output) if output.success.contains_key(&url) => Ok(()),
                    Ok(output) => Err(output
                        .failed
                        .get(&url)
                        .cloned()
                        .unwrap_or_else(|| "relay did not answer".to_owned())),
                });
                (relay, result)
            })
            .collect()
    }
}
