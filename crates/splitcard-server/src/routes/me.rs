//! The signed-in member's own view: their profile and their teams.

use axum::{Json, extract::State};
use serde::Serialize;
use splitcard_core::{card::Card, split::display_basis_points};
use sqlx::SqliteConnection;

use super::{
    cards::{LoadedCard, TeamCards, team_cards},
    profiles::{ProfileView, load_profile},
    teams::{TeamRow, load_participants, load_team},
};
use crate::{AppState, auth::Authenticated, error::ApiError};

/// What a team's row on the cards list shows.
pub mod list_status {
    /// No active card yet.
    pub const DRAFT: &str = "draft";
    /// An active card: everyone signed.
    pub const LOCKED: &str = "locked";
}

/// The only payout phase while the server records no prizes.
pub const WAITING_FOR_PRIZE: &str = "waiting-for-prize";

/// `GET /me` response.
#[derive(Debug, Serialize)]
pub struct MeView {
    /// The caller's public key in lowercase hex.
    pub pubkey: String,
    /// The caller's profile, or `null` before `PUT /me/profile`.
    pub profile: Option<ProfileView>,
    /// Teams the caller joined, most recently updated first.
    pub teams: Vec<TeamSummary>,
    /// The newest card waiting for the caller's signature, if any.
    pub needs_signature: Option<NeedsSignature>,
}

/// One of the caller's teams.
#[derive(Debug, Serialize)]
pub struct TeamSummary {
    /// Team id, as a string.
    pub team_id: String,
    /// Team slug.
    pub slug: String,
    /// Team name.
    pub name: String,
    /// Event label.
    pub event_label: String,
    /// `draft` until the team has an active card, then `locked`.
    pub list_status: &'static str,
    /// `waiting-for-prize` when locked, else `null`.
    pub phase: Option<&'static str>,
    /// The card the row describes: the active card, else the newest draft
    /// that can still activate, else `null`.
    pub card_hash: Option<String>,
    /// Members of that card who signed, or `null` without a card.
    pub signed_count: Option<usize>,
    /// Members on that card, or `null` without a card.
    pub member_count: Option<usize>,
    /// People who joined the team.
    pub participant_count: usize,
    /// Whether the caller is on the team's open draft and has not signed it.
    pub you_must_sign: bool,
    /// Unix seconds the caller joined.
    pub joined_at: i64,
    /// Unix seconds of the latest change: team created, someone joined, a
    /// card submitted, signed or activated.
    pub updated_at: i64,
}

/// A card waiting for the caller's signature.
#[derive(Debug, Serialize)]
pub struct NeedsSignature {
    /// Team id, as a string.
    pub team_id: String,
    /// Team slug.
    pub slug: String,
    /// Team name.
    pub name: String,
    /// The card to sign.
    pub card_hash: String,
    /// Profile name of the participant who submitted the card.
    pub inviter_name: String,
    /// The caller's share in basis points (10,000 is the whole prize), as
    /// `splitcard-core`'s `display_basis_points` gives it.
    pub share_basis_points: u32,
    /// Members on the card, the caller included.
    pub member_count: usize,
    /// Unix seconds the card was submitted.
    pub invited_at: i64,
    /// The team's join code, for the invite link.
    pub join_code: String,
}

/// The caller's share of `card` in basis points, if they are a member.
pub(crate) fn share_basis_points(card: &Card, pubkey: &str) -> Result<Option<u32>, ApiError> {
    let weights: Vec<u64> = card.members.iter().map(|member| member.weight).collect();
    let shares = display_basis_points(&weights)?;
    Ok(card
        .members
        .iter()
        .zip(shares)
        .find(|(member, _)| member.pubkey == pubkey)
        .map(|(_, share)| share))
}

/// Whether `pubkey` is on `card` and still has to sign it.
pub(crate) fn must_sign(card: &LoadedCard, pubkey: &str) -> bool {
    card.signing.missing.iter().any(|missing| missing == pubkey)
}

async fn summarize(
    conn: &mut SqliteConnection,
    team: &TeamRow,
    cards: &TeamCards,
    viewer: &str,
    joined_at: i64,
) -> Result<TeamSummary, ApiError> {
    let participants = load_participants(conn, team.id).await?;
    let shown = cards.active.as_ref().or(cards.draft.as_ref());
    let mut updated_at = team.created_at;
    updated_at = participants
        .iter()
        .map(|participant| participant.joined_at)
        .fold(updated_at, i64::max);
    for card in [&cards.active, &cards.draft].into_iter().flatten() {
        updated_at = updated_at.max(card.row.created_at);
        updated_at = updated_at.max(card.row.activated_at.unwrap_or_default());
        updated_at = updated_at.max(card.last_signed_at.unwrap_or_default());
    }
    let locked = cards.active.is_some();
    Ok(TeamSummary {
        team_id: team.id.to_string(),
        slug: team.slug.clone(),
        name: team.name.clone(),
        event_label: team.event_label.clone(),
        list_status: if locked {
            list_status::LOCKED
        } else {
            list_status::DRAFT
        },
        phase: locked.then_some(WAITING_FOR_PRIZE),
        card_hash: shown.map(|card| card.row.hash.clone()),
        signed_count: shown.map(|card| card.signing.signed.len()),
        member_count: shown.map(|card| card.card.members.len()),
        participant_count: participants.len(),
        you_must_sign: cards
            .draft
            .as_ref()
            .is_some_and(|draft| must_sign(draft, viewer)),
        joined_at,
        updated_at,
    })
}

async fn needs_signature(
    conn: &mut SqliteConnection,
    team: &TeamRow,
    draft: &LoadedCard,
    viewer: &str,
) -> Result<Option<NeedsSignature>, ApiError> {
    let Some(share) = share_basis_points(&draft.card, viewer)? else {
        return Ok(None);
    };
    let inviter = load_profile(conn, &draft.row.created_by)
        .await?
        .ok_or_else(|| ApiError::internal(&"card submitter has no profile"))?;
    Ok(Some(NeedsSignature {
        team_id: team.id.to_string(),
        slug: team.slug.clone(),
        name: team.name.clone(),
        card_hash: draft.row.hash.clone(),
        inviter_name: inviter.name,
        share_basis_points: share,
        member_count: draft.card.members.len(),
        invited_at: draft.row.created_at,
        join_code: team.join_code.clone(),
    }))
}

/// `GET /me`: the caller's profile and a summary of each team they joined,
/// for the home and cards screens.
///
/// # Errors
///
/// An `auth_*` code (401).
pub async fn get(
    State(state): State<AppState>,
    auth: Authenticated,
) -> Result<Json<MeView>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let profile = load_profile(&mut conn, &auth.pubkey)
        .await?
        .map(|row| ProfileView::new(row, true));
    let joined: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT team_id, joined_at FROM participants WHERE pubkey = ? ORDER BY joined_at, rowid",
    )
    .bind(&auth.pubkey)
    .fetch_all(&mut *conn)
    .await?;

    let mut teams = Vec::with_capacity(joined.len());
    let mut asks: Vec<NeedsSignature> = Vec::new();
    for (team_id, joined_at) in joined {
        let team = load_team(&mut conn, team_id)
            .await?
            .ok_or_else(|| ApiError::internal(&"participant of a missing team"))?;
        let cards = team_cards(&mut conn, team.id, team.active_card_hash.as_deref()).await?;
        teams.push(summarize(&mut conn, &team, &cards, &auth.pubkey, joined_at).await?);
        if let Some(draft) = cards
            .draft
            .as_ref()
            .filter(|draft| must_sign(draft, &auth.pubkey))
            && let Some(ask) = needs_signature(&mut conn, &team, draft, &auth.pubkey).await?
        {
            asks.push(ask);
        }
    }
    // Newest first; the sort is stable, so ties keep join order.
    teams.sort_by_key(|team| std::cmp::Reverse(team.updated_at));
    let needs_signature = asks.into_iter().max_by_key(|ask| ask.invited_at);
    Ok(Json(MeView {
        pubkey: auth.pubkey,
        profile,
        teams,
        needs_signature,
    }))
}
