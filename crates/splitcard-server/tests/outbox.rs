//! Relay publishing: per-relay delivery, first-accept success, retries,
//! backoff, failure, and never affecting the API.

mod common;

use std::{
    collections::HashSet,
    future::{Future, ready},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::http::StatusCode;
use common::{ALICE, BOB, CHIDI, TestApp, app, first_card};
use nostr::event::Event;
use splitcard_server::outbox::{self, OutboxSettings, RelayPublisher, RelayResult, process_due};
use url::Url;

const ONE: &str = "wss://relay.one.test/";
const TWO: &str = "wss://relay.two.test/";
const THREE: &str = "wss://relay.three.test/";

fn urls(relays: &[&str]) -> Vec<Url> {
    relays
        .iter()
        .map(|relay| Url::parse(relay).expect("relay URL"))
        .collect()
}

/// Accepts on the relays in `accepting` and refuses on the rest, counting
/// publish calls and recording every relay asked.
struct ScriptedPublisher {
    relays: Vec<Url>,
    accepting: Mutex<HashSet<String>>,
    calls: AtomicUsize,
    asked: Mutex<Vec<String>>,
}

impl ScriptedPublisher {
    fn new(relays: &[&str], accepting: &[&str]) -> Self {
        Self {
            relays: urls(relays),
            accepting: Mutex::new(accepting.iter().map(|relay| (*relay).to_owned()).collect()),
            calls: AtomicUsize::new(0),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn accept(&self, relay: &str) {
        self.accepting
            .lock()
            .expect("lock")
            .insert(relay.to_owned());
    }
}

impl RelayPublisher for ScriptedPublisher {
    fn relays(&self) -> Vec<Url> {
        self.relays.clone()
    }

    fn publish(
        &self,
        event: &Event,
        relays: &[Url],
    ) -> impl Future<Output = Vec<(Url, RelayResult)>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let verified = event.verify().map_err(|error| error.to_string());
        let accepting = self.accepting.lock().expect("lock").clone();
        let mut asked = self.asked.lock().expect("lock");
        let results = relays
            .iter()
            .map(|relay| {
                asked.push(relay.to_string());
                let result = match &verified {
                    Err(error) => Err(error.clone()),
                    Ok(()) if accepting.contains(relay.as_str()) => Ok(()),
                    Ok(()) => Err("connection refused".to_owned()),
                };
                (relay.clone(), result)
            })
            .collect();
        ready(results)
    }
}

/// `(outbox id, relay, status, attempts)` for every delivery.
async fn deliveries(app: &TestApp) -> Vec<(i64, String, String, i64)> {
    sqlx::query_as(
        "SELECT outbox_id, relay, status, attempts FROM relay_deliveries \
         ORDER BY outbox_id, relay",
    )
    .fetch_all(&app.state.db)
    .await
    .expect("deliveries query")
}

async fn pass(app: &TestApp, publisher: &ScriptedPublisher) -> usize {
    process_due(&app.state.db, publisher, app.clock.as_ref(), &settings())
        .await
        .expect("outbox pass")
}

fn settings() -> OutboxSettings {
    OutboxSettings {
        poll_interval: Duration::from_millis(5),
        base_delay_secs: 10,
        max_delay_secs: 60,
        max_attempts: 4,
        batch_size: 50,
    }
}

async fn activate(app: &TestApp) -> String {
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }
    hash
}

#[test]
fn retry_delay_doubles_up_to_the_cap() {
    let settings = settings();
    let delays: Vec<u64> = (1..=6)
        .map(|attempt| settings.retry_delay_secs(attempt))
        .collect();
    assert_eq!(delays, [10, 20, 40, 60, 60, 60]);
    assert_eq!(OutboxSettings::default().retry_delay_secs(u32::MAX), 3600);
}

#[tokio::test]
async fn all_relays_failing_retries_with_backoff_then_marks_failed() {
    let app = app().await;
    activate(&app).await;
    let publisher = ScriptedPublisher::new(&[ONE, TWO], &[]);

    // Attempt 1 happens at once; attempts 2-4 only once each backoff passes.
    assert_eq!(pass(&app, &publisher).await, 8);
    for (attempts, delay) in [(1, 10), (2, 20), (3, 40)] {
        assert!(
            app.outbox()
                .await
                .iter()
                .all(|entry| entry.1 == "pending" && entry.2 == attempts)
        );
        app.clock.advance(delay - 1);
        assert_eq!(pass(&app, &publisher).await, 0);
        app.clock.advance(1);
        assert_eq!(pass(&app, &publisher).await, 8);
    }

    let outbox = app.outbox().await;
    assert_eq!(outbox.len(), 4);
    for (_, status, attempts, last_error) in &outbox {
        assert_eq!(status, "failed");
        assert_eq!(*attempts, 4);
        assert_eq!(
            last_error.as_deref(),
            Some(
                "wss://relay.one.test/: connection refused; wss://relay.two.test/: connection refused"
            )
        );
    }
    assert!(
        deliveries(&app)
            .await
            .iter()
            .all(|d| d.2 == "failed" && d.3 == 4)
    );
    // One publish call per entry per pass, to all its due relays at once.
    assert_eq!(publisher.calls.load(Ordering::SeqCst), 16);
    app.clock.advance(3600);
    assert_eq!(pass(&app, &publisher).await, 0);
}

#[tokio::test]
async fn one_relay_accepting_publishes_while_the_others_keep_retrying() {
    let app = app().await;
    activate(&app).await;
    let publisher = ScriptedPublisher::new(&[ONE, TWO, THREE], &[TWO]);

    assert_eq!(pass(&app, &publisher).await, 12);
    for (_, status, attempts, last_error) in app.outbox().await {
        assert_eq!(status, "sent", "one accept is enough");
        assert_eq!(attempts, 1);
        assert_eq!(
            last_error.as_deref(),
            Some(
                "wss://relay.one.test/: connection refused; wss://relay.three.test/: connection refused"
            )
        );
    }
    for (_, relay, status, _) in deliveries(&app).await {
        let expected = if relay == TWO { "accepted" } else { "pending" };
        assert_eq!(status, expected, "{relay}");
    }

    // Only the two relays that have not accepted are retried.
    publisher.asked.lock().expect("lock").clear();
    app.clock.advance(10);
    assert_eq!(pass(&app, &publisher).await, 8);
    let asked: HashSet<String> = publisher
        .asked
        .lock()
        .expect("lock")
        .iter()
        .cloned()
        .collect();
    assert_eq!(asked, HashSet::from([ONE.to_owned(), THREE.to_owned()]));
    assert!(app.outbox().await.iter().all(|entry| entry.1 == "sent"));

    // Giving up on the others leaves the entries published.
    for delay in [20, 40] {
        app.clock.advance(delay);
        pass(&app, &publisher).await;
    }
    assert!(app.outbox().await.iter().all(|entry| entry.1 == "sent"));
    for (_, relay, status, _) in deliveries(&app).await {
        let expected = if relay == TWO { "accepted" } else { "failed" };
        assert_eq!(status, expected, "{relay}");
    }
}

#[tokio::test]
async fn a_relay_recovering_later_is_marked_accepted() {
    let app = app().await;
    activate(&app).await;
    let publisher = ScriptedPublisher::new(&[ONE, TWO], &[ONE]);
    pass(&app, &publisher).await;
    assert!(
        deliveries(&app)
            .await
            .iter()
            .all(|d| (d.1 == ONE) == (d.2 == "accepted"))
    );

    publisher.accept(TWO);
    app.clock.advance(10);
    assert_eq!(pass(&app, &publisher).await, 4);
    for (_, relay, status, attempts) in deliveries(&app).await {
        assert_eq!(status, "accepted", "{relay}");
        assert_eq!(attempts, if relay == ONE { 1 } else { 2 });
    }
    for (_, status, _, last_error) in app.outbox().await {
        assert_eq!(status, "sent");
        assert_eq!(last_error, None);
    }
    app.clock.advance(3600);
    assert_eq!(pass(&app, &publisher).await, 0);
}

#[tokio::test]
async fn failing_relays_never_affect_the_api() {
    let app = app().await;
    let publisher = Arc::new(ScriptedPublisher::new(&[ONE, TWO], &[]));
    let worker = tokio::spawn(outbox::run(
        app.state.db.clone(),
        Arc::clone(&publisher),
        app.clock.clone(),
        settings(),
    ));

    // Activation succeeds while every publish fails in the background.
    let hash = activate(&app).await;
    while publisher.calls.load(Ordering::SeqCst) < 4 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (status, view) = app.get(&format!("/cards/{hash}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["status"], "active");
    worker.abort();
}

#[tokio::test]
async fn working_relays_get_every_entry_once() {
    let app = app().await;
    activate(&app).await;
    let publisher = ScriptedPublisher::new(&[ONE, TWO], &[ONE, TWO]);
    assert_eq!(pass(&app, &publisher).await, 8);
    assert_eq!(pass(&app, &publisher).await, 0);
    assert!(
        app.outbox()
            .await
            .iter()
            .all(|entry| entry.1 == "sent" && entry.3.is_none())
    );
    assert_eq!(publisher.calls.load(Ordering::SeqCst), 4);
}
