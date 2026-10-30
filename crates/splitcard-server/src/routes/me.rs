//! The signed-in member's own view: their profile and their teams.

use axum::{Json, extract::State};
use serde::Serialize;
use splitcard_core::{card::Card, split::display_basis_points};
use sqlx::SqliteConnection;

use super::{
    cards::{LoadedCard, TeamCards, load_card, stored_card, team_cards},
    payouts::{PayoutState, load_payout, phase},
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
    /// An active card with at least one member paid.
    pub const PAID_OUT: &str = "paid-out";
}

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
    /// Every draft listing the caller that they have not signed or declined
    /// and that can still activate, newest first. Includes teams the caller
    /// has not joined: a version 2 card lists members by key alone.
    pub pending_cards: Vec<PendingCard>,
    /// Totals over the caller's teams.
    pub summary: Summary,
}

/// Totals for the cards screen.
#[derive(Debug, Serialize)]
pub struct Summary {
    /// Sats paid to the caller across all teams, as a string.
    pub received_sats: String,
    /// Teams whose payout to the caller is recorded as paid.
    pub paid_team_count: usize,
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
    /// `draft` until the team has an active card, then `locked`, then
    /// `paid-out` once any member is paid.
    pub list_status: &'static str,
    /// With an active card, the payout phase: `waiting-for-prize`,
    /// `prize-set-waiting` or `payouts-in-progress`. Else `null`.
    pub phase: Option<&'static str>,
    /// Sats paid to the caller for this team, as a string, else `null`.
    pub you_received_sats: Option<String>,
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
    /// card submitted, signed or activated, a prize proposed or agreed, or a
    /// member paid.
    pub updated_at: i64,
}

/// A draft that lists the caller and waits for their signature.
#[derive(Debug, Serialize)]
pub struct PendingCard {
    /// Team id, as a string.
    pub team_id: String,
    /// Team slug.
    pub slug: String,
    /// Team name.
    pub name: String,
    /// The card to sign.
    pub card_hash: String,
    /// Profile name of the member who created the card.
    pub creator_name: String,
    /// The caller's share in basis points, from `display_basis_points`.
    pub share_basis_points: u32,
    /// Unix seconds the card was created.
    pub created_at: i64,
}

/// Drafts listing `pubkey` that it has not signed or declined, and that no
/// one declined and can still activate, newest first.
async fn pending_cards(
    conn: &mut SqliteConnection,
    pubkey: &str,
) -> Result<Vec<PendingCard>, ApiError> {
    let hashes: Vec<String> = sqlx::query_scalar(
        "SELECT cards.hash FROM card_members \
         JOIN cards ON cards.hash = card_members.card_hash \
         JOIN teams ON teams.id = cards.team_id \
         WHERE card_members.pubkey = ? AND cards.status = 'draft' \
         AND cards.supersedes IS teams.active_card_hash \
         AND NOT EXISTS (SELECT 1 FROM card_signatures s \
             WHERE s.card_hash = cards.hash AND s.pubkey = card_members.pubkey) \
         AND NOT EXISTS (SELECT 1 FROM card_declines d WHERE d.card_hash = cards.hash) \
         ORDER BY cards.created_at DESC, cards.rowid DESC",
    )
    .bind(pubkey)
    .fetch_all(&mut *conn)
    .await?;
    let mut pending = Vec::with_capacity(hashes.len());
    for hash in hashes {
        let row = load_card(conn, &hash)
            .await?
            .ok_or_else(|| ApiError::internal(&"indexed card is missing"))?;
        let card = stored_card(&row.canonical_json)?;
        let team = load_team(conn, row.team_id)
            .await?
            .ok_or_else(|| ApiError::internal(&"card of a missing team"))?;
        let creator = load_profile(conn, &row.created_by)
            .await?
            .ok_or_else(|| ApiError::internal(&"card submitter has no profile"))?;
        let share = share_basis_points(&card, pubkey)?
            .ok_or_else(|| ApiError::internal(&"indexed member is not on the card"))?;
        pending.push(PendingCard {
            team_id: team.id.to_string(),
            slug: team.slug,
            name: team.name,
            card_hash: row.hash,
            creator_name: creator.name,
            share_basis_points: share,
            created_at: row.created_at,
        });
    }
    Ok(pending)
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
    let shares = display_basis_points(&card.weights())?;
    Ok(card
        .pubkeys()
        .into_iter()
        .zip(shares)
        .find(|(member, _)| *member == pubkey)
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
    payout: Option<&PayoutState>,
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
    // A prize is only ever for the active card.
    let payout = payout.filter(|payout| {
        cards
            .active
            .as_ref()
            .is_some_and(|active| active.row.hash == payout.card_hash())
    });
    if let Some(payout) = payout {
        updated_at = updated_at.max(payout.latest_change());
    }
    let locked = cards.active.is_some();
    let paid_out = payout.is_some_and(|payout| payout.paid_count() > 0);
    Ok(TeamSummary {
        team_id: team.id.to_string(),
        slug: team.slug.clone(),
        name: team.name.clone(),
        event_label: team.event_label.clone(),
        list_status: if paid_out {
            list_status::PAID_OUT
        } else if locked {
            list_status::LOCKED
        } else {
            list_status::DRAFT
        },
        phase: locked.then(|| payout.map_or(phase::WAITING_FOR_PRIZE, PayoutState::phase)),
        you_received_sats: payout
            .and_then(|payout| payout.received_sats(viewer))
            .map(|sats| sats.to_string()),
        card_hash: shown.map(|card| card.row.hash.clone()),
        signed_count: shown.map(|card| card.signing.signed.len()),
        member_count: shown.map(|card| card.card.member_count()),
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
        member_count: draft.card.member_count(),
        invited_at: draft.row.created_at,
        join_code: team.join_code.clone(),
    }))
}

/// `GET /me`: the caller's profile, a summary of each team they joined, and
/// what they have been paid, for the home and cards screens.
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
    let (mut received_sats, mut paid_team_count) = (0_u64, 0_usize);
    for (team_id, joined_at) in joined {
        let team = load_team(&mut conn, team_id)
            .await?
            .ok_or_else(|| ApiError::internal(&"participant of a missing team"))?;
        let cards = team_cards(&mut conn, team.id, team.active_card_hash.as_deref()).await?;
        let payout = load_payout(&mut conn, team.id).await?;
        if let Some(sats) = payout
            .as_ref()
            .and_then(|payout| payout.received_sats(&auth.pubkey))
        {
            received_sats = received_sats.saturating_add(sats);
            paid_team_count += 1;
        }
        teams.push(
            summarize(
                &mut conn,
                &team,
                &cards,
                payout.as_ref(),
                &auth.pubkey,
                joined_at,
            )
            .await?,
        );
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
    let pending_cards = pending_cards(&mut conn, &auth.pubkey).await?;
    Ok(Json(MeView {
        pubkey: auth.pubkey,
        profile,
        teams,
        needs_signature,
        pending_cards,
        summary: Summary {
            received_sats: received_sats.to_string(),
            paid_team_count,
        },
    }))
}
