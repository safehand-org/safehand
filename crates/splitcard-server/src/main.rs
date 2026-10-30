//! Runs the Split Card HTTP API and its background workers.

use std::{process::ExitCode, sync::Arc, time::Duration};

use splitcard_server::{
    AddressCheck, AppState, Clock, LnurlAddressCheck, SkipAddressCheck, SystemClock,
    config::Config,
    db,
    ots::{
        calendar::{HttpCalendar, HttpCalendarConfig},
        worker::{self, TimestampSettings},
    },
    outbox::{self, NostrRelayPublisher, OutboxSettings},
    router, shutdown,
};

/// How long in-flight requests and background passes get to finish after
/// SIGTERM, inside the platform's 30-second window before SIGKILL.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(20);

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    splitcard_server::install_tls_provider();
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = db::check_database_dir(&config.database_url) {
        eprintln!("startup error: {error}");
        return ExitCode::from(2);
    }
    match serve(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Resolves on SIGTERM or Ctrl-C. If no handler can be installed, never.
async fn termination_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = terminate.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(_) => {
                if tokio::signal::ctrl_c().await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
        }
    }
    #[cfg(not(unix))]
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// Variable that, set to `1`, skips the payout address lookup. Refused
/// unless `SPLITCARD_DEV` is set, so production always checks.
const SKIP_ADDRESS_CHECK: &str = "SPLITCARD_SKIP_ADDRESS_CHECK";

fn skip_address_check(config: &Config) -> Result<bool, String> {
    let skip = std::env::var(SKIP_ADDRESS_CHECK).is_ok_and(|value| value.trim() == "1");
    if skip && !config.development {
        return Err(format!("{SKIP_ADDRESS_CHECK} needs SPLITCARD_DEV=1"));
    }
    Ok(skip)
}

async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = db::connect(&config.database_url).await?;
    let publisher = NostrRelayPublisher::connect(&config.relays).await?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let (stop, until) = shutdown::channel();
    let mut workers = vec![tokio::spawn(outbox::run(
        pool.clone(),
        publisher,
        Arc::clone(&clock),
        OutboxSettings::default(),
        until.clone(),
    ))];

    let calendar = Arc::new(HttpCalendar::new(&HttpCalendarConfig::default())?);
    let stamping =
        TimestampSettings::new(config.ots_calendars.clone(), config.ots_upgrade_interval);
    workers.push(tokio::spawn(worker::run_submitter(
        pool.clone(),
        Arc::clone(&calendar),
        Arc::clone(&clock),
        stamping.clone(),
        until.clone(),
    )));
    workers.push(tokio::spawn(worker::run_upgrader(
        pool.clone(),
        calendar,
        Arc::clone(&clock),
        stamping,
        until.clone(),
    )));

    let server_pubkey = config.server_keys.public_key().to_hex();
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    let bind_addr = listener.local_addr()?;
    let addresses: Arc<dyn AddressCheck> = if skip_address_check(&config)? {
        tracing::warn!("SPLITCARD_SKIP_ADDRESS_CHECK is set: payout addresses are not looked up");
        Arc::new(SkipAddressCheck)
    } else {
        Arc::new(LnurlAddressCheck::new()?)
    };
    let state = AppState {
        db: pool.clone(),
        config: Arc::new(config),
        clock,
        addresses,
    };
    tracing::info!(%bind_addr, %server_pubkey, "Split Card server listening");

    tokio::spawn(async move {
        termination_signal().await;
        tracing::info!("shutdown requested; finishing in-flight work");
        stop.trigger();
    });
    let mut requested = until.clone();
    let mut stopped = until;
    let http = axum::serve(listener, router(state))
        .with_graceful_shutdown(async move { stopped.requested().await })
        .into_future();
    tokio::pin!(http);

    // Serve until shutdown is requested, or until the server fails. Once
    // shutdown is requested both are ready; `biased` picks shutdown, so the
    // drain below always runs.
    tokio::select! {
        biased;
        () = requested.requested() => {}
        result = &mut http => {
            result?;
            return Ok(());
        }
    }

    // No new connections are accepted now. Requests in flight run to the
    // end, each in its own transaction; workers stop after their current
    // pass; then the pool closes, checkpointing the write-ahead log.
    let drain = async {
        let result = http.await;
        for worker in workers {
            if let Err(error) = worker.await {
                tracing::error!(%error, "background worker failed");
            }
        }
        pool.close().await;
        result
    };
    let Ok(result) = tokio::time::timeout(SHUTDOWN_GRACE, drain).await else {
        tracing::warn!(
            "work still running after {} s; exiting, and SQLite rolls back any \
             transaction left open",
            SHUTDOWN_GRACE.as_secs()
        );
        return Ok(());
    };
    result?;
    tracing::info!("shut down cleanly");
    Ok(())
}
