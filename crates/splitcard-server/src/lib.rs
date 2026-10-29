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
/// Team slug rules.
pub mod slug;

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    http::{Method, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use sqlx::SqlitePool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

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

/// Shared handler state. It holds the server key, so it is not `Debug`.
#[derive(Clone)]
pub struct AppState {
    /// The database pool.
    pub db: SqlitePool,
    /// Server configuration.
    pub config: Arc<Config>,
    /// The time source.
    pub clock: Arc<dyn Clock>,
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
pub fn router(state: AppState) -> Router {
    // Auth is a header, never a cookie, so any origin may call the API.
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
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
        .route("/cards/{hash}/timestamp", get(routes::timestamps::status))
        .route(
            "/cards/{hash}/timestamp.ots",
            get(routes::timestamps::proof),
        )
        .route("/cards/{hash}/verify", get(routes::verify::card))
        .route("/teams/{id}/verify", get(routes::verify::team))
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
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
