//! HTTP API for Split Card: member profiles, teams, cards, signatures, and
//! prizes with preimage-proved payouts.
//!
//! All card logic (validation, hashing, signature checks, status and
//! superseding) comes from `splitcard-core`; this crate only stores cards and
//! decides when to call it. Activated cards are queued in `relay_outbox` and
//! published by [`outbox::run`], and queued in `card_timestamps` and stamped
//! by the [`ots::worker`] loops, all in the background.

#![deny(clippy::float_arithmetic)]

/// NIP-98 HTTP authentication.
pub mod auth;
/// Environment configuration.
pub mod config;
/// SQLite connection and migrations.
pub mod db;
/// JSON error responses.
pub mod error;
/// OpenTimestamps proofs for active cards.
pub mod ots;
/// Relay publishing from `relay_outbox`.
pub mod outbox;
/// Request handlers.
pub mod routes;
/// Graceful shutdown for background loops.
pub mod shutdown;
/// Team slug rules.
pub mod slug;

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    response::IntoResponse,
    routing::{get, post, put},
};
use splitcard_ln::{
    address::parse_address,
    client::{ClientConfig, ClientError, LnClient},
};
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;

use crate::{config::Config, error::ApiError};

/// Largest accepted request body.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// A source of the current Unix time, so tests can control it.
pub trait Clock: Send + Sync + 'static {
    /// Seconds since the Unix epoch.
    fn now_unix(&self) -> u64;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs())
    }
}

/// The future an [`AddressCheck`] returns.
pub type AddressCheckFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

/// Checks that a Lightning Address answers, before a member signs it as
/// their payout address, so a typo is caught before money is involved.
pub trait AddressCheck: Send + Sync + 'static {
    /// `Ok` if `address` serves an LNURL-pay request; `Err` holds the
    /// provider's reason.
    fn check<'a>(&'a self, address: &'a str) -> AddressCheckFuture<'a>;
}

/// Most time an address check may take.
pub const ADDRESS_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Fetches the address's LNURL-pay request with `splitcard-ln`, under its
/// HTTPS and public-address rules.
#[derive(Debug, Clone)]
pub struct LnurlAddressCheck {
    client: LnClient,
}

impl LnurlAddressCheck {
    /// A checker that gives up after [`ADDRESS_CHECK_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Build`] if the HTTP client cannot be built.
    pub fn new() -> Result<Self, ClientError> {
        let client = LnClient::new(ClientConfig {
            timeout: ADDRESS_CHECK_TIMEOUT,
            ..ClientConfig::default()
        })?;
        Ok(Self { client })
    }
}

impl AddressCheck for LnurlAddressCheck {
    fn check<'a>(&'a self, address: &'a str) -> AddressCheckFuture<'a> {
        Box::pin(async move {
            let address = parse_address(address).map_err(|error| error.to_string())?;
            self.client
                .fetch_pay_request(&address)
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}

/// Accepts every address without a lookup. Only for local development with
/// made-up addresses; `main` allows it only with `SPLITCARD_DEV=1`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SkipAddressCheck;

impl AddressCheck for SkipAddressCheck {
    fn check<'a>(&'a self, _address: &'a str) -> AddressCheckFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

/// Shared handler state. It holds the server key, so it is not `Debug`.
#[derive(Clone)]
pub struct AppState {
    /// The database pool.
    pub db: SqlitePool,
    /// Server configuration.
    pub config: Arc<Config>,
    /// The time source.
    pub clock: Arc<dyn Clock>,
    /// Checks payout addresses before version 2 signatures are accepted.
    pub addresses: Arc<dyn AddressCheck>,
}

/// Installs `ring` as the process-wide rustls provider, for relay websockets.
///
/// Dependencies enable both `ring` and `aws-lc-rs`, so rustls cannot pick one
/// itself and panics on the first TLS connection that relies on the default.
/// Call once at startup; later calls keep the provider already installed.
pub fn install_tls_provider() {
    // An error only means a provider is already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Builds the API router.
///
/// It sends no CORS headers: browsers reach the API only through the
/// website's own origin, which proxies `/api` here.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/me", get(routes::me::get))
        .route("/me/profile", put(routes::profiles::put_mine))
        .route("/profiles/{pubkey}", get(routes::profiles::get))
        .route("/invites/{slug}", get(routes::invites::get))
        .route("/teams", post(routes::teams::create))
        .route("/teams/by-slug/{slug}", get(routes::teams::get_by_slug))
        .route("/teams/{id}", get(routes::teams::get))
        .route("/teams/{id}/join", post(routes::teams::join))
        .route("/teams/{id}/submission", put(routes::teams::set_submission))
        .route("/teams/{id}/cards", post(routes::cards::submit))
        .route("/teams/{id}/card", get(routes::cards::active))
        .route("/teams/{id}/prize", put(routes::payouts::put_prize))
        .route("/teams/{id}/payout", get(routes::payouts::get))
        .route(
            "/teams/{id}/payout/items/{index}/payment",
            post(routes::payouts::record_payment),
        )
        .route("/cards/{hash}", get(routes::cards::get))
        .route("/cards/{hash}/signatures", post(routes::cards::sign))
        .route("/cards/{hash}/decline", post(routes::cards::decline))
        .route("/cards/{hash}/timestamp", get(routes::timestamps::status))
        .route(
            "/cards/{hash}/timestamp.ots",
            get(routes::timestamps::proof),
        )
        .route("/cards/{hash}/verify", get(routes::verify::card))
        .route("/teams/{id}/verify", get(routes::verify::team))
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    ([("content-type", "application/json")], r#"{"status":"ok"}"#)
}

async fn not_found() -> ApiError {
    ApiError::not_found("not_found", "no such route")
}

/// Converts Unix seconds to the SQLite integer column type.
pub(crate) fn db_time(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};

    use super::health;

    #[tokio::test]
    async fn health_route_returns_ok() {
        let response = health().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_route_returns_status_body() {
        let response = health().await.into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health response body should be readable");
        assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
    }
}
