//! Member profiles: the name and Lightning Address a member uses on every
//! team and card, and an optional bio.
//!
//! A new member signs up with a name only. The Lightning Address is needed
//! before they create or join a team ([`require_payable_profile`]).

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};

use serde_json::json;

use super::{conflict, parse_json, require_lightning_address, require_text, unprocessable};
use crate::{
    AppState,
    auth::{Authenticated, OptionalAuth},
    config::is_pubkey,
    db::begin_write,
    db_time,
    error::ApiError,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutProfile {
    name: String,
    /// Absent or null keeps the stored address.
    #[serde(default)]
    lightning_address: Option<String>,
    /// Absent or null keeps the stored bio; an empty string clears it.
    #[serde(default)]
    bio: Option<String>,
}

/// Longest bio accepted, in characters.
pub const MAX_BIO_CHARS: usize = 280;

/// A stored profile.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct ProfileRow {
    pub(crate) pubkey: String,
    pub(crate) name: String,
    pub(crate) lightning_address: Option<String>,
    pub(crate) bio: Option<String>,
    pub(crate) updated_at: i64,
}

/// A profile in API responses.
#[derive(Debug, Serialize)]
pub struct ProfileView {
    /// Nostr public key in lowercase hex.
    pub pubkey: String,
    /// Display name; cards must use it.
    pub name: String,
    /// Lightning Address; cards must use it. Shown only to the member and to
    /// members of a team they share, and absent until the member sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightning_address: Option<String>,
    /// Optional bio, shown to everyone; absent when not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    /// Unix seconds of the last change, shown with the address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl ProfileView {
    pub(crate) fn new(row: ProfileRow, show_private: bool) -> Self {
        Self {
            pubkey: row.pubkey,
            name: row.name,
            lightning_address: row.lightning_address.filter(|_| show_private),
            bio: row.bio,
            updated_at: show_private.then_some(row.updated_at),
        }
    }
}

pub(crate) async fn load_profile(
    conn: &mut SqliteConnection,
    pubkey: &str,
) -> Result<Option<ProfileRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT pubkey, name, lightning_address, bio, updated_at FROM profiles WHERE pubkey = ?",
    )
    .bind(pubkey)
    .fetch_optional(conn)
    .await
}

/// Fails with `profile_required` (409) unless `pubkey` has a profile.
pub(crate) async fn require_profile(
    conn: &mut SqliteConnection,
    pubkey: &str,
) -> Result<ProfileRow, ApiError> {
    load_profile(conn, pubkey).await?.ok_or_else(|| {
        conflict(
            "profile_required",
            "set your name and Lightning Address with PUT /me/profile first",
        )
    })
}

/// Fails with `profile_required` (409) unless `pubkey` has a profile, or
/// `lightning_address_required` (409) unless that profile has a Lightning
/// Address. Creating or joining a team needs both: cards pay members there.
pub(crate) async fn require_payable_profile(
    conn: &mut SqliteConnection,
    pubkey: &str,
) -> Result<ProfileRow, ApiError> {
    let profile = require_profile(conn, pubkey).await?;
    if profile.lightning_address.is_none() {
        return Err(conflict(
            "lightning_address_required",
            "add a Lightning Address with PUT /me/profile first",
        ));
    }
    Ok(profile)
}

async fn share_a_team(
    conn: &mut SqliteConnection,
    left: &str,
    right: &str,
) -> Result<bool, sqlx::Error> {
    let shared: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM participants a JOIN participants b ON a.team_id = b.team_id \
         WHERE a.pubkey = ? AND b.pubkey = ? LIMIT 1",
    )
    .bind(left)
    .bind(right)
    .fetch_optional(conn)
    .await?;
    Ok(shared.is_some())
}

/// `PUT /me/profile`: the signed-in member sets `{ name, lightning_address?,
/// bio? }` on their own profile, creating it if needed. An absent or null
/// `lightning_address` or `bio` keeps the stored value, so the address is
/// never cleared once set; an empty `bio` clears it. Allowed at any time;
/// cards already submitted keep the details they were submitted with.
///
/// # Errors
///
/// `invalid_request_json` (400), `empty_field`, `invalid_lightning_address`
/// or `bio_too_long` (422).
pub async fn put_mine(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<Json<ProfileView>, ApiError> {
    let request: PutProfile = parse_json(&auth.body, "invalid_request_json")?;
    require_text("name", &request.name)?;
    if let Some(address) = &request.lightning_address {
        require_lightning_address(address)?;
    }
    if let Some(bio) = &request.bio
        && bio.chars().count() > MAX_BIO_CHARS
    {
        return Err(unprocessable(
            "bio_too_long",
            format!("bio must be at most {MAX_BIO_CHARS} characters"),
        )
        .with_details(json!({ "field": "bio", "max_chars": MAX_BIO_CHARS })));
    }
    let bio_given = request.bio.is_some();
    let bio = request.bio.filter(|bio| !bio.is_empty());
    let mut tx = begin_write(&state.db).await?;
    sqlx::query(
        "INSERT INTO profiles (pubkey, name, lightning_address, bio, updated_at) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT (pubkey) DO UPDATE SET name = excluded.name, \
         lightning_address = coalesce(excluded.lightning_address, profiles.lightning_address), \
         bio = CASE WHEN ? THEN excluded.bio ELSE profiles.bio END, \
         updated_at = excluded.updated_at",
    )
    .bind(&auth.pubkey)
    .bind(&request.name)
    .bind(&request.lightning_address)
    .bind(&bio)
    .bind(db_time(state.clock.now_unix()))
    .bind(bio_given)
    .execute(&mut *tx)
    .await?;
    let profile = require_profile(&mut tx, &auth.pubkey).await?;
    tx.commit().await?;
    Ok(Json(ProfileView::new(profile, true)))
}

/// `GET /profiles/{pubkey}`: the member's name. The Lightning Address is
/// shown only when the caller sends a valid NIP-98 header as that member or
/// as a member of a team they share.
///
/// # Errors
///
/// `profile_not_found` (404), or an `auth_*` code (401) for a bad header.
pub async fn get(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
    OptionalAuth(viewer): OptionalAuth,
) -> Result<Json<ProfileView>, ApiError> {
    let not_found = || ApiError::not_found("profile_not_found", "no profile for this public key");
    if !is_pubkey(&pubkey) {
        return Err(not_found());
    }
    let mut conn = state.db.acquire().await?;
    let profile = load_profile(&mut conn, &pubkey)
        .await?
        .ok_or_else(not_found)?;
    let show_private = match viewer.as_deref() {
        None => false,
        Some(viewer) if viewer == pubkey => true,
        Some(viewer) => share_a_team(&mut conn, viewer, &pubkey).await?,
    };
    Ok(Json(ProfileView::new(profile, show_private)))
}
