//! Public invite previews for the join screen.

use axum::{
    Json,
    extract::{Path, RawQuery, State},
};
use serde::Serialize;
use splitcard_core::{
    card::{Card, Payout},
    split::display_basis_points,
};

use super::{
    cards::{LoadedCard, team_cards},
    me::must_sign,
    profiles::load_profile,
    teams::{join_code_matches, load_participants, load_team_by_slug, team_slug_not_found},
};
use crate::{AppState, auth::OptionalAuth, error::ApiError};

/// Where the viewer stands, which picks the join screen's first step.
pub mod viewer_state {
    /// No NIP-98 header.
    pub const ANONYMOUS: &str = "anonymous";
    /// Signed in without a profile.
    pub const NEEDS_PROFILE: &str = "needs_profile";
    /// Has a profile, has not joined.
    pub const NEEDS_JOIN: &str = "needs_join";
    /// Joined, on the open draft, not signed yet.
    pub const MISSING_SIGNATURE: &str = "missing_signature";
    /// Joined and signed the open draft; others still have to sign.
    pub const SIGNED: &str = "signed";
    /// Joined, nothing to sign, and the team has an active card.
    pub const LOCKED: &str = "locked";
    /// Joined, with no card to sign yet.
    pub const JOINED: &str = "joined";
}

/// `GET /invites/{slug}` response.
#[derive(Debug, Serialize)]
pub struct InviteView {
    /// Team id, as a string, for `POST /teams/{id}/join`.
    pub team_id: String,
    /// Team slug.
    pub slug: String,
    /// Team name.
    pub name: String,
    /// Event label.
    pub event_label: String,
    /// Profile name of the team's first participant, who created it.
    pub created_by_name: String,
    /// Whether the `join` query parameter is the team's join code.
    pub join_code_ok: bool,
    /// One of the [`viewer_state`] values.
    pub viewer: &'static str,
    /// People who joined the team.
    pub participant_count: usize,
    /// The card to show: the newest draft that can still activate, else the
    /// active card, else `null`.
    pub card: Option<InviteCard>,
}

/// A card as the join screen shows it.
#[derive(Debug, Serialize)]
pub struct InviteCard {
    /// Card hash; signature events sign over it.
    pub hash: String,
    /// `draft` or `active`.
    pub status: String,
    /// Profile name of the participant who submitted it.
    pub created_by_name: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds, once active.
    pub activated_at: Option<i64>,
    /// Members in card order.
    pub members: Vec<InviteMember>,
    /// Members who signed.
    pub signed_count: usize,
}

/// A card member on an invite.
#[derive(Debug, Serialize)]
pub struct InviteMember {
    /// Nostr public key in lowercase hex.
    pub pubkey: String,
    /// Name on the card.
    pub name: String,
    /// Share weight, as a string.
    pub weight: String,
    /// Share in basis points (10,000 is the whole prize).
    pub share_basis_points: u32,
    /// Whether they signed this card.
    pub signed: bool,
    /// The card's Lightning Address for this member, shown only to the
    /// member themselves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lightning_address: Option<String>,
}

/// The `join` value from a raw query string, if present.
fn join_param(query: Option<&str>) -> Option<String> {
    url::form_urlencoded::parse(query?.as_bytes())
        .find(|(key, _)| key == "join")
        .map(|(_, value)| value.into_owned())
}

fn invite_card(
    loaded: &LoadedCard,
    created_by_name: String,
    viewer: Option<&str>,
) -> Result<InviteCard, ApiError> {
    // Invites only exist for version 1 cards, whose addresses are in the card.
    let Card::V1(card) = &loaded.card else {
        return Err(ApiError::internal(&"invite for a version 2 card"));
    };
    let weights: Vec<u64> = card.members.iter().map(|m| m.weight).collect();
    let shares = display_basis_points(&weights)?;
    let members = card
        .members
        .iter()
        .zip(shares)
        .map(|(member, share)| {
            let Payout::Lightning { address } = &member.payout;
            InviteMember {
                pubkey: member.pubkey.clone(),
                name: member.name.clone(),
                weight: member.weight.to_string(),
                share_basis_points: share,
                signed: loaded.signing.signed.contains(&member.pubkey),
                lightning_address: (viewer == Some(member.pubkey.as_str()))
                    .then(|| address.clone()),
            }
        })
        .collect();
    Ok(InviteCard {
        hash: loaded.row.hash.clone(),
        status: loaded.row.status.clone(),
        created_by_name,
        created_at: loaded.row.created_at,
        activated_at: loaded.row.activated_at,
        members,
        signed_count: loaded.signing.signed.len(),
    })
}

/// `GET /invites/{slug}?join={join_code}`: a preview of the team and its
/// card for the join screen, and where the viewer stands.
///
/// Readable without the join code; `join_code_ok` says whether the query
/// presented the right one, and the stored code is never echoed. Lightning
/// Addresses are left out except the viewer's own.
///
/// # Errors
///
/// `team_not_found` (404), or an `auth_*` code (401) for a bad header.
pub async fn get(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    RawQuery(query): RawQuery,
    OptionalAuth(viewer): OptionalAuth,
) -> Result<Json<InviteView>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let team = load_team_by_slug(&mut conn, &slug)
        .await?
        .ok_or_else(team_slug_not_found)?;
    let join_code_ok =
        join_param(query.as_deref()).is_some_and(|code| join_code_matches(&team, &code));
    let participants = load_participants(&mut conn, team.id).await?;
    let created_by_name = participants
        .first()
        .map(|creator| creator.name.clone())
        .ok_or_else(|| ApiError::internal(&"team without participants"))?;
    let cards = team_cards(&mut conn, team.id, team.active_card_hash.as_deref()).await?;
    let shown = cards.draft.as_ref().or(cards.active.as_ref());

    let viewer_state = match viewer.as_deref() {
        None => viewer_state::ANONYMOUS,
        Some(viewer) if load_profile(&mut conn, viewer).await?.is_none() => {
            viewer_state::NEEDS_PROFILE
        }
        Some(viewer) if !participants.iter().any(|p| p.pubkey == viewer) => {
            viewer_state::NEEDS_JOIN
        }
        Some(viewer) => match &cards.draft {
            Some(draft) if must_sign(draft, viewer) => viewer_state::MISSING_SIGNATURE,
            Some(draft) if draft.signing.signed.iter().any(|s| s == viewer) => viewer_state::SIGNED,
            _ if cards.active.is_some() => viewer_state::LOCKED,
            _ => viewer_state::JOINED,
        },
    };

    let card = match shown {
        Some(loaded) => {
            let submitter = load_profile(&mut conn, &loaded.row.created_by)
                .await?
                .ok_or_else(|| ApiError::internal(&"card submitter has no profile"))?;
            Some(invite_card(loaded, submitter.name, viewer.as_deref())?)
        }
        None => None,
    };
    Ok(Json(InviteView {
        team_id: team.id.to_string(),
        slug: team.slug,
        name: team.name,
        event_label: team.event_label,
        created_by_name,
        join_code_ok,
        viewer: viewer_state,
        participant_count: participants.len(),
        card,
    }))
}
