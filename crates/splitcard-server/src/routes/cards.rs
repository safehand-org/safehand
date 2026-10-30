//! Submitting cards, collecting signatures and activating cards.

use axum::{
    Json,
    extract::{Path, Request, State},
    http::StatusCode,
};
use nostr::{
    event::{Event, EventBuilder, FinalizeEvent, Kind, Tag},
    key::PublicKey,
    nips::nip19::ToBech32,
    types::Timestamp,
};
use serde::Serialize;
use serde_json::{Value, json};
use splitcard_core::{
    card::{Card, CardV1, Payout},
    hash::{canonical_json, card_hash_hex},
    signature::{SIGNATURE_KIND, Signer, verify_signature_event},
    split::display_basis_points,
    status::{CardStatus, card_status},
    supersede::{check_supersede, verify_supersede_signature_event},
    validate::validate,
};
use sqlx::{FromRow, SqliteConnection};

use super::{
    conflict, forbidden, parse_card, parse_json, parse_team_id,
    payouts::{discard_prize, refuse_if_prize_agreed},
    profiles::{load_profile, require_profile},
    team_not_found,
    teams::{ParticipantRow, load_participants, load_team, require_participant},
    timestamps::{TimestampView, load_view},
    unprocessable,
};
use crate::{
    AppState,
    auth::{Authenticated, read_body},
    db::begin_write,
    db_time,
    error::ApiError,
    ots, outbox,
};

/// Stored lifecycle state of a card.
pub mod card_state {
    /// Collecting signatures.
    pub const DRAFT: &str = "draft";
    /// Fully signed; the team's current agreement.
    pub const ACTIVE: &str = "active";
    /// Replaced by a newer active card.
    pub const SUPERSEDED: &str = "superseded";
    /// A draft that can never activate. Computed at read time from the
    /// team's active card and never stored: the stored status stays `draft`.
    pub const STALE: &str = "stale";
    /// A draft a member declined, so it can never activate. Computed at read
    /// time from `card_declines`; the stored status stays `draft`.
    pub const DECLINED: &str = "declined";
}

/// Whether a card with stored `status` is a stale draft: its `supersedes`
/// is no longer the team's active card, or it is a first card while another
/// card is active. Signing refuses stale drafts.
pub(crate) fn is_stale(status: &str, supersedes: Option<&str>, active: Option<&str>) -> bool {
    status == card_state::DRAFT && supersedes != active
}

/// A stored card.
#[derive(Debug, FromRow)]
pub(crate) struct CardRow {
    pub(crate) hash: String,
    pub(crate) team_id: i64,
    pub(crate) canonical_json: String,
    pub(crate) supersedes: Option<String>,
    pub(crate) status: String,
    pub(crate) created_by: String,
    pub(crate) created_at: i64,
    pub(crate) activated_at: Option<i64>,
}

/// A stored card, parsed, with its members' signing status.
#[derive(Debug)]
pub(crate) struct LoadedCard {
    pub(crate) row: CardRow,
    pub(crate) card: Card,
    pub(crate) signing: SigningView,
    /// Unix seconds of the latest stored signature, if any.
    pub(crate) last_signed_at: Option<i64>,
}

/// The cards a team's screens show: its active card, and the newest draft
/// that can still activate (one whose `supersedes` is the active card).
#[derive(Debug, Default)]
pub(crate) struct TeamCards {
    pub(crate) active: Option<LoadedCard>,
    pub(crate) draft: Option<LoadedCard>,
}

pub(crate) async fn team_cards(
    conn: &mut SqliteConnection,
    team_id: i64,
    active_card_hash: Option<&str>,
) -> Result<TeamCards, ApiError> {
    let active = match active_card_hash {
        Some(hash) => load_card(conn, hash).await?,
        None => None,
    };
    let draft: Option<CardRow> = sqlx::query_as(
        "SELECT hash, team_id, canonical_json, supersedes, status, created_by, created_at, \
         activated_at FROM cards WHERE team_id = ? AND status = 'draft' AND supersedes IS ? \
         ORDER BY created_at DESC, rowid DESC LIMIT 1",
    )
    .bind(team_id)
    .bind(active_card_hash)
    .fetch_optional(&mut *conn)
    .await?;
    let mut cards = TeamCards::default();
    if let Some(row) = active {
        cards.active = Some(loaded_card(conn, row).await?);
    }
    if let Some(row) = draft {
        cards.draft = Some(loaded_card(conn, row).await?);
    }
    Ok(cards)
}

async fn loaded_card(conn: &mut SqliteConnection, row: CardRow) -> Result<LoadedCard, ApiError> {
    let card = stored_card(&row.canonical_json)?;
    let events = stored_events(&signature_jsons(conn, &row.hash).await?)?;
    let signing = signing_view(&card, &events)?;
    let last_signed_at =
        sqlx::query_scalar("SELECT max(received_at) FROM card_signatures WHERE card_hash = ?")
            .bind(&row.hash)
            .fetch_one(conn)
            .await?;
    Ok(LoadedCard {
        row,
        card,
        signing,
        last_signed_at,
    })
}

/// `card_status` over the signatures of the card's own members. Removed
/// members of a superseded card sign too; they count for `supersede`, not
/// for this card's member status.
fn signing_view(card: &Card, events: &[Event]) -> Result<SigningView, ApiError> {
    let member_events: Vec<Event> = events
        .iter()
        .filter(|event| card.is_member(&event.pubkey.to_hex()))
        .cloned()
        .collect();
    let report = card_status(card, &member_events)?;
    let (state, signers, missing) = match report.status {
        CardStatus::Draft { signed, missing } => ("draft", signed, missing),
        CardStatus::Active { signed } => ("active", signed, Vec::new()),
    };
    let addresses = signers
        .iter()
        .filter_map(|signer| Some((signer.pubkey.clone(), signer.lightning_address.clone()?)))
        .collect();
    let signed = signers.into_iter().map(|signer| signer.pubkey).collect();
    let rejected = report
        .rejected
        .into_iter()
        .map(|(event_id, error)| {
            json!({ "event_id": event_id.to_hex(), "error": ApiError::from(error).to_json() })
        })
        .collect();
    Ok(SigningView {
        state,
        signed,
        missing,
        rejected,
        addresses,
    })
}

/// A card in API responses.
#[derive(Debug, Serialize)]
pub struct CardView {
    /// Lowercase hex SHA-256 of the canonical JSON.
    pub hash: String,
    /// Owning team id, as a string.
    pub team_id: String,
    /// `draft`, `active` or `superseded` as stored, or `stale` for a draft
    /// that can never activate (see [`is_stale`]).
    pub status: String,
    /// The canonical card JSON, as a string so big numbers survive.
    pub card: String,
    /// Hash of the card this one replaces, if any.
    pub supersedes: Option<String>,
    /// Public key of the participant who submitted it.
    pub created_by: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds, once active.
    pub activated_at: Option<i64>,
    /// `splitcard-core`'s `card_status` report over the card's members.
    pub signing: SigningView,
    /// For a superseding card: whether `check_supersede` passes yet.
    pub supersede: Option<SupersedeView>,
    /// `OpenTimestamps` status once the card has activated, else `null`.
    pub timestamp: Option<TimestampView>,
    /// Stored signature events, each exactly as received.
    pub signatures: Vec<String>,
    /// The server's card event, once the card has activated, else `null`.
    pub card_event: Option<CardEventView>,
    /// Members in card order, with names, shares and signing state.
    pub members: Vec<MemberView>,
    /// Members who declined, in the order they did.
    pub declined: Vec<DeclineView>,
}

/// The server-signed event publishing a card.
#[derive(Debug, Serialize)]
pub struct CardEventView {
    /// The event id, to fetch it from a relay.
    pub event_id: String,
    /// The key recorded as having signed it, which may differ from the
    /// server's current key.
    pub server_pubkey: String,
}

/// The card event queued for `hash`: a server-signed outbox entry whose
/// `d` tag is the card hash.
async fn card_event_view(
    conn: &mut SqliteConnection,
    hash: &str,
) -> Result<Option<CardEventView>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_json, server_pubkey FROM relay_outbox \
         WHERE server_pubkey IS NOT NULL AND instr(event_json, ?) > 0 ORDER BY id",
    )
    .bind(hash)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().find_map(|(json, server_pubkey)| {
        let event: Event = serde_json::from_str(&json).ok()?;
        event
            .tags
            .iter()
            .any(|tag| tag.kind() == "d" && tag.content() == Some(hash))
            .then(|| CardEventView {
                event_id: event.id.to_hex(),
                server_pubkey,
            })
    }))
}

/// A `card_status` report.
#[derive(Debug, Serialize)]
pub struct SigningView {
    /// `draft` until every card member has signed, then `active`.
    pub state: &'static str,
    /// Members who signed, in card order.
    pub signed: Vec<String>,
    /// Members still to sign, in card order.
    pub missing: Vec<String>,
    /// Events `card_status` rejected, with their error codes.
    pub rejected: Vec<Value>,
    /// `(pubkey, address)` each version 2 signer signed, in card order.
    #[serde(skip)]
    pub(crate) addresses: Vec<(String, String)>,
}

/// One member of a card, for the card screens.
#[derive(Debug, Serialize)]
pub struct MemberView {
    /// Nostr public key in lowercase hex.
    pub pubkey: String,
    /// The same key as an `npub`.
    pub npub: String,
    /// The card's name for them (a version 1 `name` or version 2 `label`),
    /// else their profile name, else `null`.
    pub name: Option<String>,
    /// Share weight, as a string so big numbers survive.
    pub weight: String,
    /// Share in basis points from `splitcard-core`'s `display_basis_points`.
    pub share_basis_points: u32,
    /// Whether they signed this card.
    pub signed: bool,
    /// Whether they declined this card.
    pub declined: bool,
    /// Where their share is paid: the card's address for version 1, the
    /// address they signed for version 2 (`null` until they sign). Public,
    /// like the signatures it comes from, which are published to relays.
    pub lightning_address: Option<String>,
}

/// A member who declined a draft.
#[derive(Debug, Serialize)]
pub struct DeclineView {
    /// Their public key in lowercase hex.
    pub pubkey: String,
    /// Their name, as in [`MemberView::name`].
    pub name: Option<String>,
    /// Unix seconds.
    pub declined_at: i64,
}

/// Progress of a superseding card.
#[derive(Debug, Serialize)]
pub struct SupersedeView {
    /// Whether old and new members have all signed the new card.
    pub ready: bool,
    /// Why not, as `{ code, message, details }`, while not ready.
    pub error: Option<Value>,
}

pub(crate) async fn load_card(
    conn: &mut SqliteConnection,
    hash: &str,
) -> Result<Option<CardRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT hash, team_id, canonical_json, supersedes, status, created_by, created_at, \
         activated_at FROM cards WHERE hash = ?",
    )
    .bind(hash)
    .fetch_optional(conn)
    .await
}

pub(crate) fn card_not_found() -> ApiError {
    ApiError::not_found("card_not_found", "no card with this hash")
}

fn card_superseded() -> ApiError {
    conflict("card_superseded", "this card has been superseded")
}

fn card_declined() -> ApiError {
    conflict(
        "card_declined",
        "a member declined this draft, so it can never activate; make a new card",
    )
}

fn card_stale() -> ApiError {
    conflict(
        "card_stale",
        "this draft no longer follows the team's active card, so it can never activate",
    )
}

/// Parses JSON this server stored; failure means the database is corrupt.
pub(crate) fn stored_card(json: &str) -> Result<Card, ApiError> {
    Card::from_json(json).map_err(|error| ApiError::internal(&error))
}

pub(crate) async fn signature_jsons(
    conn: &mut SqliteConnection,
    hash: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT event_json FROM card_signatures WHERE card_hash = ? ORDER BY received_at, rowid",
    )
    .bind(hash)
    .fetch_all(conn)
    .await
}

pub(crate) fn stored_events(jsons: &[String]) -> Result<Vec<Event>, ApiError> {
    jsons
        .iter()
        .map(|json| serde_json::from_str(json).map_err(|error| ApiError::internal(&error)))
        .collect()
}

/// The card this card supersedes, if any.
async fn old_card(conn: &mut SqliteConnection, card: &Card) -> Result<Option<Card>, ApiError> {
    let Some(old_hash) = card.supersedes() else {
        return Ok(None);
    };
    let row = load_card(conn, old_hash)
        .await?
        .ok_or_else(|| ApiError::internal(&"superseded card is missing"))?;
    Ok(Some(stored_card(&row.canonical_json)?))
}

/// Who declined the card, in the order they did.
pub(crate) async fn load_declines(
    conn: &mut SqliteConnection,
    hash: &str,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT pubkey, declined_at FROM card_declines WHERE card_hash = ? \
         ORDER BY declined_at, rowid",
    )
    .bind(hash)
    .fetch_all(conn)
    .await
}

/// Each member's name: the card's, else their profile's, else `None`.
pub(crate) async fn member_display_names(
    conn: &mut SqliteConnection,
    card: &Card,
) -> Result<Vec<Option<String>>, sqlx::Error> {
    let mut names = Vec::with_capacity(card.member_count());
    for (pubkey, name) in card.pubkeys().into_iter().zip(card.member_names()) {
        names.push(match name {
            Some(name) => Some(name.to_owned()),
            None => load_profile(conn, pubkey)
                .await?
                .map(|profile| profile.name),
        });
    }
    Ok(names)
}

/// Each member's payout address: from the card for version 1, from their
/// signature for version 2 (`None` until they sign).
pub(crate) fn member_addresses(card: &Card, signing: &SigningView) -> Vec<Option<String>> {
    match card {
        Card::V1(card) => card
            .members
            .iter()
            .map(|member| {
                let Payout::Lightning { address } = &member.payout;
                Some(address.clone())
            })
            .collect(),
        Card::V2(card) => card
            .members
            .iter()
            .map(|member| {
                signing
                    .addresses
                    .iter()
                    .find(|(pubkey, _)| *pubkey == member.pubkey)
                    .map(|(_, address)| address.clone())
            })
            .collect(),
    }
}

async fn member_views(
    conn: &mut SqliteConnection,
    card: &Card,
    signing: &SigningView,
    declines: &[(String, i64)],
) -> Result<(Vec<MemberView>, Vec<DeclineView>), ApiError> {
    let weights = card.weights();
    let shares = display_basis_points(&weights)?;
    let names = member_display_names(conn, card).await?;
    let addresses = member_addresses(card, signing);
    let mut members = Vec::with_capacity(names.len());
    for ((((pubkey, weight), share), name), lightning_address) in card
        .pubkeys()
        .into_iter()
        .zip(weights)
        .zip(shares)
        .zip(names)
        .zip(addresses)
    {
        let npub = PublicKey::from_hex(pubkey)
            .ok()
            .and_then(|key| key.to_bech32().ok())
            .unwrap_or_default();
        members.push(MemberView {
            pubkey: pubkey.to_owned(),
            npub,
            name,
            weight: weight.to_string(),
            share_basis_points: share,
            signed: signing.signed.iter().any(|signed| signed == pubkey),
            declined: declines.iter().any(|(declined, _)| declined == pubkey),
            lightning_address,
        });
    }
    let decline_views = declines
        .iter()
        .map(|(pubkey, declined_at)| DeclineView {
            pubkey: pubkey.clone(),
            name: members
                .iter()
                .find(|member| member.pubkey == *pubkey)
                .and_then(|member| member.name.clone()),
            declined_at: *declined_at,
        })
        .collect();
    Ok((members, decline_views))
}

async fn card_view(conn: &mut SqliteConnection, row: CardRow) -> Result<CardView, ApiError> {
    let card = stored_card(&row.canonical_json)?;
    let signatures = signature_jsons(conn, &row.hash).await?;
    let events = stored_events(&signatures)?;
    let signing = signing_view(&card, &events)?;
    let supersede =
        old_card(conn, &card)
            .await?
            .map(|old| match check_supersede(&old, &card, &events) {
                Ok(()) => SupersedeView {
                    ready: true,
                    error: None,
                },
                Err(error) => SupersedeView {
                    ready: false,
                    error: Some(ApiError::from(error).to_json()),
                },
            });
    let timestamp = load_view(conn, &row.hash).await?;
    let card_event = card_event_view(conn, &row.hash).await?;
    let team = load_team(conn, row.team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let declines = load_declines(conn, &row.hash).await?;
    let (members, decline_views) = member_views(conn, &card, &signing, &declines).await?;
    let status = if row.status == card_state::DRAFT && !declines.is_empty() {
        card_state::DECLINED.to_owned()
    } else if is_stale(
        &row.status,
        row.supersedes.as_deref(),
        team.active_card_hash.as_deref(),
    ) {
        card_state::STALE.to_owned()
    } else {
        row.status
    };
    Ok(CardView {
        hash: row.hash,
        team_id: row.team_id.to_string(),
        status,
        card: row.canonical_json,
        supersedes: row.supersedes,
        created_by: row.created_by,
        created_at: row.created_at,
        activated_at: row.activated_at,
        signing,
        supersede,
        timestamp,
        signatures,
        card_event,
        members,
        declined: decline_views,
    })
}

/// `POST /teams/{id}/cards`: a participant submits a card as raw JSON.
///
/// The card must pass `splitcard-core` validation, name this team's event
/// label and team name, and supersede exactly the active card (or nothing,
/// if there is none). A version 1 card lists only participants with their
/// profile's current name and Lightning Address. A version 2 card must list
/// the submitter; its other members need no account, since each proves their
/// key and attaches their payout address when they sign. The canonical JSON
/// is stored as a draft, and its members are indexed so each can find it.
/// Submitting a stored card again returns it with 200.
///
/// # Errors
///
/// `invalid_card_json` or `secret_key_rejected` (400); `team_not_found`
/// (404); `not_a_participant` (403); a core validation code, `wrong_event`,
/// `wrong_team`, `submitter_not_member`, `member_not_participant`,
/// `member_name_mismatch` or `member_address_mismatch` (422);
/// `unexpected_supersedes`, `missing_supersedes`, `wrong_supersedes_hash`,
/// or `prize_agreed` for a replacement once the team agreed a prize (409).
pub async fn submit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<(StatusCode, Json<CardView>), ApiError> {
    let team_id = parse_team_id(&id)?;
    let card = parse_card(&auth.body)?;
    validate(&card)?;
    let hash = card_hash_hex(&card)?;

    let mut tx = begin_write(&state.db).await?;
    let team = load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    require_participant(&mut tx, team_id, &auth.pubkey).await?;
    if card.event() != team.event_label {
        return Err(
            unprocessable("wrong_event", "card event is not this team's event label")
                .with_details(json!({ "expected": team.event_label })),
        );
    }
    if card.team() != team.name {
        return Err(
            unprocessable("wrong_team", "card team is not this team's name")
                .with_details(json!({ "expected": team.name })),
        );
    }
    if let Some(existing) = load_card(&mut tx, &hash).await? {
        let view = card_view(&mut tx, existing).await?;
        return Ok((StatusCode::OK, Json(view)));
    }
    match &card {
        Card::V1(v1) => check_v1_members(v1, &load_participants(&mut tx, team_id).await?)?,
        Card::V2(_) if !card.is_member(&auth.pubkey) => {
            return Err(unprocessable(
                "submitter_not_member",
                "a version 2 card must list the person submitting it",
            ));
        }
        Card::V2(_) => {}
    }
    if card.supersedes().is_some() {
        refuse_if_prize_agreed(&mut tx, team_id).await?;
    }
    check_supersedes(team.active_card_hash.as_deref(), card.supersedes())?;
    insert_card(
        &mut tx,
        &card,
        &hash,
        team_id,
        &auth.pubkey,
        state.clock.now_unix(),
    )
    .await?;
    let row = load_card(&mut tx, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    let view = card_view(&mut tx, row).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
}

/// A card supersedes exactly the team's active card, or nothing without one.
fn check_supersedes(active: Option<&str>, supersedes: Option<&str>) -> Result<(), ApiError> {
    match (active, supersedes) {
        (None, Some(_)) => Err(conflict(
            "unexpected_supersedes",
            "the team has no active card, so supersedes must be null",
        )),
        (Some(active), None) => Err(conflict(
            "missing_supersedes",
            "the team has an active card, so supersedes must be its hash",
        )
        .with_details(json!({ "active_card_hash": active }))),
        (Some(active), Some(supersedes)) if supersedes != active => Err(conflict(
            "wrong_supersedes_hash",
            "supersedes is not the team's active card hash",
        )
        .with_details(json!({ "active_card_hash": active }))),
        _ => Ok(()),
    }
}

/// Stores a new draft and indexes its members.
async fn insert_card(
    conn: &mut SqliteConnection,
    card: &Card,
    hash: &str,
    team_id: i64,
    created_by: &str,
    now: u64,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO cards (hash, team_id, canonical_json, supersedes, status, created_by, \
         created_at) VALUES (?, ?, ?, ?, 'draft', ?, ?)",
    )
    .bind(hash)
    .bind(team_id)
    .bind(canonical_json(card)?)
    .bind(card.supersedes())
    .bind(created_by)
    .bind(db_time(now))
    .execute(&mut *conn)
    .await?;
    let members = card
        .pubkeys()
        .into_iter()
        .zip(card.member_names())
        .zip(card.weights())
        .enumerate();
    for (index, ((pubkey, label), weight)) in members {
        sqlx::query(
            "INSERT INTO card_members (card_hash, member_index, pubkey, label, weight) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(hash)
        .bind(i64::try_from(index).map_err(|error| ApiError::internal(&error))?)
        .bind(pubkey)
        .bind(label)
        // Validated weights are at most 2^53 - 1.
        .bind(i64::try_from(weight).map_err(|error| ApiError::internal(&error))?)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Version 1 members must be participants, named and paid as in their
/// profiles when the card is submitted.
fn check_v1_members(card: &CardV1, participants: &[ParticipantRow]) -> Result<(), ApiError> {
    for (index, member) in card.members.iter().enumerate() {
        let details = json!({ "index": index, "pubkey": member.pubkey });
        let Some(participant) = participants.iter().find(|p| p.pubkey == member.pubkey) else {
            return Err(unprocessable(
                "member_not_participant",
                format!("member {index} has not joined this team"),
            )
            .with_details(details));
        };
        if member.name != participant.name {
            return Err(unprocessable(
                "member_name_mismatch",
                format!("member {index} name differs from their profile"),
            )
            .with_details(details));
        }
        let Payout::Lightning { address } = &member.payout;
        if participant.lightning_address.as_ref() != Some(address) {
            return Err(unprocessable(
                "member_address_mismatch",
                format!("member {index} Lightning Address differs from their profile"),
            )
            .with_details(details));
        }
    }
    Ok(())
}

/// `GET /cards/{hash}`: the canonical card JSON with its signing status.
///
/// # Errors
///
/// `card_not_found` (404).
pub async fn get(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<CardView>, ApiError> {
    let mut conn = state.db.acquire().await?;
    let row = load_card(&mut conn, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    Ok(Json(card_view(&mut conn, row).await?))
}

/// `GET /teams/{id}/card`: the team's active card.
///
/// # Errors
///
/// `team_not_found` or `no_active_card` (404).
pub async fn active(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CardView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let mut conn = state.db.acquire().await?;
    let team = load_team(&mut conn, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let no_active = || ApiError::not_found("no_active_card", "the team has no active card");
    let hash = team.active_card_hash.ok_or_else(no_active)?;
    let row = load_card(&mut conn, &hash).await?.ok_or_else(no_active)?;
    Ok(Json(card_view(&mut conn, row).await?))
}

/// `POST /cards/{hash}/signatures`: submits a member's signed signature event
/// as the raw event JSON. The signed event is the authentication.
///
/// The event is checked with `splitcard-core`: against the card's members, or
/// for a superseding card against the old and new cards' members. A valid new
/// signature is stored (201) and may activate the card; a second valid
/// signature from the same member is ignored (200).
///
/// On a version 2 card the event carries the signer's payout address. The
/// signer needs a profile, the address must answer as a Lightning Address,
/// it is stored with the signature, and a card member who signs joins the
/// team.
///
/// # Errors
///
/// `invalid_event_json` or `secret_key_rejected` (400), `card_not_found`
/// (404), a core signature code such as `invalid_signature`,
/// `missing_lud16_tag` or `non_member` (422), `lightning_address_unreachable`
/// (422), `profile_required` (409), `card_superseded` (409), `card_declined`
/// (409), `card_stale` (409) for a draft whose `supersedes` is no longer the
/// team's active card, so two fully signed cards never compete for one place,
/// and `prize_agreed` (409) for a replacement card once the team agreed a
/// prize.
pub async fn sign(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    request: Request,
) -> Result<(StatusCode, Json<CardView>), ApiError> {
    let body = read_body(request.into_body()).await?;
    let event: Event = parse_json(&body, "invalid_event_json")?;
    // `parse_json` accepted it, so the body is UTF-8.
    let event_json = String::from_utf8(body.to_vec()).map_err(|error| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_event_json",
            error.to_string(),
        )
    })?;

    // Card contents never change, so verify before taking the write lock.
    let (card, old) = {
        let mut conn = state.db.acquire().await?;
        let row = load_card(&mut conn, &hash)
            .await?
            .ok_or_else(card_not_found)?;
        if row.status == card_state::SUPERSEDED {
            return Err(card_superseded());
        }
        let card = stored_card(&row.canonical_json)?;
        let old = old_card(&mut conn, &card).await?;
        (card, old)
    };
    let signer = match &old {
        None => verify_signature_event(&card, &event)?,
        Some(old) => verify_supersede_signature_event(old, &card, &event)?,
    };
    if let Some(address) = &signer.lightning_address {
        let mut conn = state.db.acquire().await?;
        require_profile(&mut conn, &signer.pubkey).await?;
        drop(conn);
        check_address(&state, address).await?;
    }
    let Signer {
        pubkey,
        lightning_address,
    } = signer;

    // Everything below re-reads state inside one write transaction, so
    // concurrent signatures are applied one at a time.
    let mut tx = begin_write(&state.db).await?;
    let row = load_card(&mut tx, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    if row.status == card_state::SUPERSEDED {
        return Err(card_superseded());
    }
    let team = load_team(&mut tx, row.team_id)
        .await?
        .ok_or_else(team_not_found)?;
    if is_stale(
        &row.status,
        card.supersedes(),
        team.active_card_hash.as_deref(),
    ) {
        return Err(card_stale());
    }
    if row.status == card_state::DRAFT && !load_declines(&mut tx, &hash).await?.is_empty() {
        return Err(card_declined());
    }
    if card.supersedes().is_some() {
        refuse_if_prize_agreed(&mut tx, row.team_id).await?;
    }
    let now = state.clock.now_unix();
    let stored = sqlx::query(
        "INSERT INTO card_signatures (card_hash, pubkey, event_json, received_at, \
         lightning_address) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(&hash)
    .bind(&pubkey)
    .bind(&event_json)
    .bind(db_time(now))
    .bind(&lightning_address)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if stored && matches!(card, Card::V2(_)) && card.is_member(&pubkey) {
        sqlx::query(
            "INSERT INTO participants (team_id, pubkey, joined_at) VALUES (?, ?, ?) \
             ON CONFLICT DO NOTHING",
        )
        .bind(row.team_id)
        .bind(&pubkey)
        .bind(db_time(now))
        .execute(&mut *tx)
        .await?;
    }
    if stored && row.status == card_state::DRAFT {
        activate_if_ready(&mut tx, &state, &row, &card, old.as_ref(), now).await?;
    }
    let row = load_card(&mut tx, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    let view = card_view(&mut tx, row).await?;
    tx.commit().await?;
    let status = if stored {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(view)))
}

/// Looks up a payout address before it is signed, so a typo fails here
/// rather than at payout.
async fn check_address(state: &AppState, address: &str) -> Result<(), ApiError> {
    state.addresses.check(address).await.map_err(|reason| {
        unprocessable(
            "lightning_address_unreachable",
            format!("{address} did not answer as a Lightning Address"),
        )
        .with_details(json!({ "lightning_address": address, "reason": reason }))
    })
}

/// `POST /cards/{hash}/decline`: a member listed on a draft who has not
/// signed it declines it. A declined draft can never activate; the creator
/// makes a new card. Declining again changes nothing.
///
/// # Errors
///
/// An `auth_*` code (401), `card_not_found` (404), `not_a_card_member`
/// (403), `card_not_draft` or `already_signed` (409).
pub async fn decline(
    State(state): State<AppState>,
    Path(hash): Path<String>,
    auth: Authenticated,
) -> Result<Json<CardView>, ApiError> {
    let mut tx = begin_write(&state.db).await?;
    let row = load_card(&mut tx, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    if row.status != card_state::DRAFT {
        return Err(conflict("card_not_draft", "only a draft can be declined"));
    }
    let card = stored_card(&row.canonical_json)?;
    if !card.is_member(&auth.pubkey) {
        return Err(forbidden(
            "not_a_card_member",
            "only members listed on the card can decline it",
        ));
    }
    let signed: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM card_signatures WHERE card_hash = ? AND pubkey = ?")
            .bind(&hash)
            .bind(&auth.pubkey)
            .fetch_optional(&mut *tx)
            .await?;
    if signed.is_some() {
        return Err(conflict(
            "already_signed",
            "you signed this card, so you cannot decline it",
        ));
    }
    sqlx::query(
        "INSERT INTO card_declines (card_hash, pubkey, declined_at) VALUES (?, ?, ?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&hash)
    .bind(&auth.pubkey)
    .bind(db_time(state.clock.now_unix()))
    .execute(&mut *tx)
    .await?;
    let view = card_view(&mut tx, row).await?;
    tx.commit().await?;
    Ok(Json(view))
}

/// Activates a draft card once `splitcard-core` says it is fully signed and
/// it still follows the team's current active card.
async fn activate_if_ready(
    conn: &mut SqliteConnection,
    state: &AppState,
    row: &CardRow,
    card: &Card,
    old: Option<&Card>,
    now: u64,
) -> Result<(), ApiError> {
    let team = load_team(conn, row.team_id)
        .await?
        .ok_or_else(team_not_found)?;
    // A draft written against an earlier active card can never activate.
    if team.active_card_hash.as_deref() != card.supersedes() {
        return Ok(());
    }
    let signatures = signature_jsons(conn, &row.hash).await?;
    let events = stored_events(&signatures)?;
    let ready = match old {
        None => matches!(
            card_status(card, &events)?.status,
            CardStatus::Active { .. }
        ),
        Some(old) => check_supersede(old, card, &events).is_ok(),
    };
    if !ready {
        return Ok(());
    }

    sqlx::query(
        "UPDATE cards SET status = 'active', activated_at = ? WHERE hash = ? AND status = 'draft'",
    )
    .bind(db_time(now))
    .bind(&row.hash)
    .execute(&mut *conn)
    .await?;
    if let Some(old_hash) = card.supersedes() {
        // A prize proposed for the old card was never agreed (agreeing one
        // stops the card from being replaced), so it goes with that card.
        discard_prize(conn, row.team_id).await?;
        sqlx::query("UPDATE cards SET status = 'superseded' WHERE hash = ? AND status = 'active'")
            .bind(old_hash)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE teams SET active_card_hash = ? WHERE id = ?")
        .bind(&row.hash)
        .bind(row.team_id)
        .execute(&mut *conn)
        .await?;

    for signature in &signatures {
        outbox::enqueue(conn, signature, None, now).await?;
    }
    let card_event = server_card_event(state, &row.hash, &row.canonical_json, now)?;
    let signer = card_event.pubkey.to_hex();
    outbox::enqueue(conn, &card_event.as_json(), Some(&signer), now).await?;

    // Queue the OpenTimestamps job in this transaction; the workers do the rest.
    ots::worker::enqueue(conn, &row.hash, now).await?;
    Ok(())
}

/// The server-signed event publishing a card: the canonical JSON as content,
/// tagged `d` and `splitcard` with the card hash, in the signature kind.
fn server_card_event(
    state: &AppState,
    hash: &str,
    canonical_json: &str,
    now: u64,
) -> Result<Event, ApiError> {
    let tag = |name: &str| Tag::parse([name, hash]).map_err(|error| ApiError::internal(&error));
    EventBuilder::new(Kind::from_u16(SIGNATURE_KIND), canonical_json)
        .tags([tag("d")?, tag("splitcard")?])
        .custom_created_at(Timestamp::from_secs(now))
        .finalize(&state.config.server_keys)
        .map_err(|error| ApiError::internal(&error))
}
