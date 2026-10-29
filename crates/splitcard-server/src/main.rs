//! Runs the Split Card HTTP API and its background workers.

use std::{process::ExitCode, sync::Arc};

use splitcard_server::{
    AppState, Clock, SystemClock,
    config::Config,
    db,
    ots::{
        calendar::{HttpCalendar, HttpCalendarConfig},
        worker::{self, TimestampSettings},
    },
    outbox::{self, NostrRelayPublisher, OutboxSettings},
    router,
};

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
    match serve(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let pool = db::connect(&config.database_url).await?;
    let publisher = NostrRelayPublisher::connect(&config.relays).await?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    tokio::spawn(outbox::run(
        pool.clone(),
        publisher,
        Arc::clone(&clock),
        OutboxSettings::default(),
    ));

    let calendar = Arc::new(HttpCalendar::new(&HttpCalendarConfig::default())?);
    let stamping =
        TimestampSettings::new(config.ots_calendars.clone(), config.ots_upgrade_interval);
    tokio::spawn(worker::run_submitter(
        pool.clone(),
        Arc::clone(&calendar),
        Arc::clone(&clock),
        stamping.clone(),
    ));
    tokio::spawn(worker::run_upgrader(
        pool.clone(),
        calendar,
        Arc::clone(&clock),
        stamping,
    ));

    let bind_addr = config.bind_addr;
    let server_pubkey = config.server_keys.public_key().to_hex();
    let state = AppState {
        db: pool,
        config: Arc::new(config),
        clock,
    };
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(%bind_addr, %server_pubkey, "Split Card server listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            // If the signal handler cannot be installed, run until killed.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
        .await?;
    Ok(())
}
