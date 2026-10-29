//! NIP-98 HTTP authentication.
//!
//! Write endpoints take [`Authenticated`], which reads the body (so the
//! `payload` tag can be checked) and yields the caller's public key with it.
//! `GET /teams/{id}` and `GET /profiles/{pubkey}` take [`OptionalAuth`] to
//! decide whether to show the join code and Lightning Addresses.

use axum::{
    body::{Bytes, to_bytes},
    extract::{FromRequest, FromRequestParts, Request},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
};
use nostr::event::{Event, Tag};
use sha2::{Digest, Sha256};
use thiserror::Error;

use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{AppState, MAX_BODY_BYTES, db_time, error::ApiError};

/// NIP-98 HTTP auth event kind.
pub const AUTH_KIND: u16 = 27_235;
/// Largest allowed difference between an auth event's `created_at` and server time.
pub const MAX_CLOCK_SKEW_SECS: u64 = 60;
/// How long used auth event ids are kept. An event is accepted for at most
/// twice [`MAX_CLOCK_SKEW_SECS`] of server time, so this covers its whole life.
pub const USED_ID_RETENTION_SECS: u64 = 5 * 60;
/// Largest accepted `Authorization` header value.
const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Why a NIP-98 header was rejected. Each maps to its own `auth_*` code.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// No `Authorization` header.
    #[error("Authorization: Nostr <base64 event> header is required")]
    Missing,
    /// The header is not `Nostr <base64 event JSON>`.
    #[error("Authorization header is not Nostr <base64 event JSON>")]
    Malformed,
    /// The event id does not match its contents.
    #[error("auth event id is invalid")]
    InvalidEventId,
    /// The event's Schnorr signature is invalid.
    #[error("auth event signature is invalid")]
    InvalidSignature,
    /// The event kind is not 27235.
    #[error("auth event kind is not 27235")]
    WrongKind,
    /// `created_at` is more than 60 seconds from server time.
    #[error("auth event created_at is more than 60 seconds from server time")]
    Expired,
    /// The `u` tag is missing, repeated or not the exact request URL.
    #[error("auth event u tag does not match the request URL")]
    UrlMismatch,
    /// The `method` tag is missing, repeated or not the request method.
    #[error("auth event method tag does not match the request method")]
    MethodMismatch,
    /// The `payload` tag is missing, repeated or not the body's SHA-256.
    #[error("auth event payload tag is not the SHA-256 of the request body")]
    PayloadMismatch,
    /// The event id was already used.
    #[error("auth event was already used")]
    Replayed,
}

impl AuthError {
    /// The stable error code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Missing => "auth_missing",
            Self::Malformed => "auth_malformed",
            Self::InvalidEventId => "auth_invalid_event_id",
            Self::InvalidSignature => "auth_invalid_signature",
            Self::WrongKind => "auth_wrong_kind",
            Self::Expired => "auth_expired",
            Self::UrlMismatch => "auth_url_mismatch",
            Self::MethodMismatch => "auth_method_mismatch",
            Self::PayloadMismatch => "auth_payload_mismatch",
            Self::Replayed => "auth_replayed",
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, error.code(), error.to_string())
    }
}

/// A NIP-98-authenticated request: the signer and the exact body bytes.
#[derive(Debug)]
pub struct Authenticated {
    /// The caller's public key in lowercase hex.
    pub pubkey: String,
    /// The request body the `payload` tag was checked against.
    pub body: Bytes,
}

impl FromRequest<AppState> for Authenticated {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        let (parts, body) = request.into_parts();
        let body = read_body(body).await?;
        let header = parts.headers.get(AUTHORIZATION).ok_or(AuthError::Missing)?;
        let pubkey = authenticate(state, &parts, header.as_bytes(), &body).await?;
        Ok(Self { pubkey, body })
    }
}

/// The caller's public key if the request carries a NIP-98 header.
///
/// A header that is present but invalid is still rejected.
#[derive(Debug)]
pub struct OptionalAuth(pub Option<String>);

impl FromRequestParts<AppState> for OptionalAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match parts.headers.get(AUTHORIZATION) {
            None => Ok(Self(None)),
            Some(header) => Ok(Self(Some(
                authenticate(state, parts, header.as_bytes(), &[]).await?,
            ))),
        }
    }
}

/// Reads a request body of at most [`MAX_BODY_BYTES`].
pub(crate) async fn read_body(body: axum::body::Body) -> Result<Bytes, ApiError> {
    to_bytes(body, MAX_BODY_BYTES).await.map_err(|_| {
        ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
            "request body is larger than 64 KB or could not be read",
        )
    })
}

async fn authenticate(
    state: &AppState,
    parts: &Parts,
    header: &[u8],
    body: &[u8],
) -> Result<String, ApiError> {
    let path = parts
        .uri
        .path_and_query()
        .map_or_else(|| parts.uri.path(), |path| path.as_str());
    let url = format!("{}{path}", state.config.public_base_url);
    let now = state.clock.now_unix();
    let event = verify_header(header, &url, parts.method.as_str(), body, now)?;
    mark_used(state, &event.id.to_hex(), now).await?;
    Ok(event.pubkey.to_hex())
}

/// Checks a NIP-98 `Authorization` header value against a request, in the
/// order of [`AuthError`]'s variants. Replay is checked separately.
///
/// # Errors
///
/// Returns the first failed check.
pub fn verify_header(
    header: &[u8],
    url: &str,
    method: &str,
    body: &[u8],
    now_unix: u64,
) -> Result<Event, AuthError> {
    if header.len() > MAX_HEADER_BYTES {
        return Err(AuthError::Malformed);
    }
    let encoded = std::str::from_utf8(header)
        .ok()
        .and_then(|header| header.strip_prefix("Nostr "))
        .ok_or(AuthError::Malformed)?;
    let json = decode_base64(encoded).ok_or(AuthError::Malformed)?;
    let event = Event::from_json(json).map_err(|_| AuthError::Malformed)?;
    if !event.verify_id() {
        return Err(AuthError::InvalidEventId);
    }
    if !event.verify_signature() {
        return Err(AuthError::InvalidSignature);
    }
    if event.kind.as_u16() != AUTH_KIND {
        return Err(AuthError::WrongKind);
    }
    if event.created_at.as_secs().abs_diff(now_unix) > MAX_CLOCK_SKEW_SECS {
        return Err(AuthError::Expired);
    }
    if single_tag(&event, "u") != Some(url) {
        return Err(AuthError::UrlMismatch);
    }
    if single_tag(&event, "method") != Some(method) {
        return Err(AuthError::MethodMismatch);
    }
    let has_payload_tag = event.tags.iter().any(|tag| tag.kind() == "payload");
    if !body.is_empty() || has_payload_tag {
        let body_hash = hex::encode(Sha256::digest(body));
        if single_tag(&event, "payload") != Some(body_hash.as_str()) {
            return Err(AuthError::PayloadMismatch);
        }
    }
    Ok(event)
}

/// Decodes RFC 4648 standard Base64 with canonical `=` padding.
///
/// Other alphabets, whitespace, missing or wrong padding and non-zero
/// trailing bits are rejected, so every accepted input has exactly one
/// decoding.
fn decode_base64(input: &str) -> Option<Vec<u8>> {
    STANDARD.decode(input).ok()
}

/// The value of the only tag named `name`, or `None` if there are zero or several.
fn single_tag<'a>(event: &'a Event, name: &str) -> Option<&'a str> {
    let mut tags = event.tags.iter().filter(|tag| tag.kind() == name);
    match (tags.next(), tags.next()) {
        (Some(tag), None) => Tag::content(tag),
        _ => None,
    }
}

/// Records an auth event id, failing if it was already used, and forgets ids
/// older than [`USED_ID_RETENTION_SECS`].
async fn mark_used(state: &AppState, event_id: &str, now: u64) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM used_auth_events WHERE seen_at < ?")
        .bind(db_time(now.saturating_sub(USED_ID_RETENTION_SECS)))
        .execute(&state.db)
        .await?;
    let inserted = sqlx::query(
        "INSERT INTO used_auth_events (event_id, seen_at) VALUES (?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(event_id)
    .bind(db_time(now))
    .execute(&state.db)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(AuthError::Replayed.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decode_base64;

    #[test]
    fn decodes_rfc_4648_vectors() {
        for (encoded, decoded) in [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ] {
            assert_eq!(
                decode_base64(encoded).as_deref(),
                Some(decoded.as_bytes()),
                "{encoded}"
            );
        }
        assert_eq!(decode_base64("+/+/"), Some(vec![0xfb, 0xff, 0xbf]));
    }

    #[test]
    fn rejects_malformed_input() {
        for encoded in [
            "Z", "Zg=", "Zg===", "Z===", "Zm9v=", "Zm 9v", "Zm9v\n", "-_-_", "Zh==", "Zm9=",
            "=Zm9", "Zm=v", // Padding is strict: unpadded input is rejected.
            "Zg", "Zm8",
        ] {
            assert_eq!(decode_base64(encoded), None, "{encoded:?}");
        }
    }
}
