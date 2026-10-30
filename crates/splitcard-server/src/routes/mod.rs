//! Request handlers, grouped by resource.

/// `/teams/{id}/cards`, `/teams/{id}/card` and `/cards/...`.
pub mod cards;
/// `/invites/{slug}`.
pub mod invites;
/// `/me`.
pub mod me;
/// `/teams/{id}/prize` and `/teams/{id}/payout...`.
pub mod payouts;
/// `/me/profile` and `/profiles/{pubkey}`.
pub mod profiles;
/// `/teams` and `/teams/...`.
pub mod teams;
/// `/cards/{hash}/timestamp` and `/cards/{hash}/timestamp.ots`.
pub mod timestamps;
/// `/cards/{hash}/verify` and `/teams/{id}/verify`.
pub mod verify;

use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use splitcard_core::{card::Card, validate::is_lightning_address};

use crate::error::ApiError;

/// Parses a JSON body, reporting failures under `code`. Bodies holding a
/// wallet connection string are refused first; see [`reject_wallet_secrets`].
pub(crate) fn parse_json<T: DeserializeOwned>(
    body: &[u8],
    code: &'static str,
) -> Result<T, ApiError> {
    reject_wallet_secrets(body)?;
    reject_secret_keys(body)?;
    serde_json::from_slice(body).map_err(|error| ApiError::invalid_json(code, &error))
}

/// Parses a card body with [`Card::from_json`], reporting failures as
/// `invalid_card_json`. Wallet secrets are refused first, as in [`parse_json`].
pub(crate) fn parse_card(body: &[u8]) -> Result<Card, ApiError> {
    reject_wallet_secrets(body)?;
    reject_secret_keys(body)?;
    let text = std::str::from_utf8(body).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_card_json",
            error.to_string(),
        )
    })?;
    Card::from_json(text).map_err(|error| ApiError::invalid_json("invalid_card_json", &error))
}

/// URI schemes of NIP-47 (Nostr Wallet Connect) connection strings, which
/// carry the wallet's secret.
const WALLET_CONNECT_SCHEMES: [&str; 2] = ["nostr+walletconnect:", "nostrwalletconnect:"];

/// Refuses a body containing an NWC connection string with
/// `wallet_secret_rejected` (400).
///
/// Wallet credentials never reach this server: no endpoint accepts
/// one, so it is never stored, and the error never echoes it. The raw bytes
/// are checked, and every JSON string and key after unescaping, so `\u` escapes
/// do not slip past.
pub(crate) fn reject_wallet_secrets(body: &[u8]) -> Result<(), ApiError> {
    let raw = String::from_utf8_lossy(body);
    let mut found = has_wallet_connect_uri(&raw);
    if !found && let Ok(value) = serde_json::from_slice::<Value>(body) {
        found = json_has(&value, has_wallet_connect_uri);
    }
    if found {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "wallet_secret_rejected",
            "request bodies must never contain a wallet connection string; \
             NWC credentials are never sent to this server",
        ));
    }
    Ok(())
}

fn has_wallet_connect_uri(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    WALLET_CONNECT_SCHEMES
        .iter()
        .any(|scheme| lower.contains(scheme))
}

/// Refuses a body containing anything that looks like a Nostr secret key
/// (`nsec1` and 58 bech32 characters, in either case) with
/// `secret_key_rejected` (400).
///
/// Secret keys stay in the browser: no endpoint takes one, and the error
/// never echoes it. As with wallet secrets, the raw bytes are checked and
/// every JSON string and key after unescaping.
pub(crate) fn reject_secret_keys(body: &[u8]) -> Result<(), ApiError> {
    let raw = String::from_utf8_lossy(body);
    let mut found = has_nsec(&raw);
    if !found && let Ok(value) = serde_json::from_slice::<Value>(body) {
        found = json_has(&value, has_nsec);
    }
    if found {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "secret_key_rejected",
            "request bodies must never contain a Nostr secret key; \
             secret keys never leave your device",
        ));
    }
    Ok(())
}

/// Characters of a bech32 data part.
const BECH32_CHARSET: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// Whether `text` holds `nsec1` followed by the 58 data characters of an
/// encoded 32-byte key.
fn has_nsec(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    lower.match_indices("nsec1").any(|(start, prefix)| {
        let data = &bytes[start + prefix.len()..];
        data.len() >= 58 && data[..58].iter().all(|byte| BECH32_CHARSET.contains(byte))
    })
}

fn json_has(value: &Value, matches: fn(&str) -> bool) -> bool {
    match value {
        Value::String(text) => matches(text),
        Value::Array(items) => items.iter().any(|item| json_has(item, matches)),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| matches(key) || json_has(value, matches)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Parses a `{id}` path segment; anything that is not an id is an unknown team.
pub(crate) fn parse_team_id(id: &str) -> Result<i64, ApiError> {
    id.parse().map_err(|_| team_not_found())
}

pub(crate) fn team_not_found() -> ApiError {
    ApiError::not_found("team_not_found", "no team with this id")
}

pub(crate) fn forbidden(code: &'static str, message: &str) -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, code, message)
}

pub(crate) fn unprocessable(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

pub(crate) fn conflict(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::CONFLICT, code, message)
}

/// Rejects an empty text field with the WASM `empty_field` code.
pub(crate) fn require_text(field: &'static str, value: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(
            unprocessable("empty_field", format!("{field} must not be empty"))
                .with_details(json!({ "field": field })),
        );
    }
    Ok(())
}

/// Rejects anything `splitcard-core` would not accept on a card.
pub(crate) fn require_lightning_address(value: &str) -> Result<(), ApiError> {
    if !is_lightning_address(value) {
        return Err(unprocessable(
            "invalid_lightning_address",
            "lightning_address is not a lowercase LUD-16 Lightning Address",
        )
        .with_details(json!({ "field": "lightning_address" })));
    }
    Ok(())
}

/// Whether a database error is a UNIQUE or PRIMARY KEY violation.
pub(crate) fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(error) if error.is_unique_violation())
}
