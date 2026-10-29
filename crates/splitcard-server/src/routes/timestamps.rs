//! OpenTimestamps status and proof downloads.
//!
//! `complete` means a calendar supplied a Bitcoin block attestation. This
//! server does not check block headers against Bitcoin; verify independently
//! with `ots verify <hash>.json.ots`, with the canonical card JSON saved as
//! `<hash>.json`.

use axum::{
    Json,
    extract::{Path, State},
    http::header,
    response::IntoResponse,
};
use serde::Serialize;
use sqlx::{FromRow, SqliteConnection};

use crate::{AppState, error::ApiError};

/// A card's stamping status in API responses.
#[derive(Debug, Serialize, FromRow)]
pub struct TimestampView {
    /// `queued`, `pending`, `complete` (a calendar supplied a Bitcoin
    /// attestation, not checked by this server) or `failed`.
    pub status: String,
    /// How many calendars accepted the card hash.
    pub calendars_accepted: i64,
    /// Height of the Bitcoin block a calendar attested to, once complete.
    pub bitcoin_block_height: Option<i64>,
    /// Unix seconds when the first calendar accepted.
    pub submitted_at: Option<i64>,
    /// Unix seconds when a Bitcoin attestation arrived.
    pub completed_at: Option<i64>,
    /// The latest calendar error, if any.
    pub last_error: Option<String>,
}

/// The stamping status of a card, or `None` if it was never activated.
pub(crate) async fn load_view(
    conn: &mut SqliteConnection,
    card_hash: &str,
) -> Result<Option<TimestampView>, sqlx::Error> {
    sqlx::query_as(
        "SELECT status, calendars_accepted, bitcoin_block_height, submitted_at, completed_at, \
         last_error FROM card_timestamps WHERE card_hash = ?",
    )
    .bind(card_hash)
    .fetch_optional(conn)
    .await
}

/// `GET /cards/{hash}/timestamp`: the card's stamping status.
///
/// # Errors
///
/// `timestamp_not_found` (404) if the card was never activated.
pub async fn status(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<TimestampView>, ApiError> {
    let mut conn = state.db.acquire().await?;
    load_view(&mut conn, &hash)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("timestamp_not_found", "this card was never activated"))
}

/// `GET /cards/{hash}/timestamp.ots`: the detached proof for the canonical
/// card JSON, as `<hash>.json.ots`.
///
/// # Errors
///
/// `proof_not_available` (404) until at least one calendar has accepted.
pub async fn proof(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let row: Option<(String, Vec<u8>)> = sqlx::query_as(
        "SELECT card_hash, ots_proof FROM card_timestamps \
         WHERE card_hash = ? AND ots_proof IS NOT NULL",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await?;
    // The filename comes from the stored hash, which is always lowercase hex.
    let (card_hash, proof) = row.ok_or_else(|| {
        ApiError::not_found(
            "proof_not_available",
            "no calendar has accepted this card's hash yet",
        )
    })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{card_hash}.json.ots\""),
            ),
        ],
        proof,
    ))
}
