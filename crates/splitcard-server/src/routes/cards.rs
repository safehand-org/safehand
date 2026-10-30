//! Submitting cards, collecting signatures and activating cards.

use axum::{
    Json,
    extract::{Path, Request, State},
    http::StatusCode,
};
use nostr::{
    event::{Event, EventBuilder, FinalizeEvent, Kind, Tag},
    types::Timestamp,
};
use serde::Serialize;
use serde_json::{Value, json};
use splitcard_core::{
    card::{Card, CardV1, Payout},
    hash::{canonical_json, card_hash_hex},
    signature::{SIGNATURE_KIND, verify_signature_event},
    status::{CardStatus, card_status},
    supersede::{check_supersede, verify_supersede_signature_event},
    validate::validate,
};
use sqlx::{FromRow, SqliteConnection};

use super::{
    conflict, parse_card, parse_json, parse_team_id,
    payouts::{discard_prize, refuse_if_prize_agreed},
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

async fn signature_jsons(
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

fn stored_events(jsons: &[String]) -> Result<Vec<Event>, ApiError> {
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
    let status = if is_stale(
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
    })
}

/// `POST /teams/{id}/cards`: a participant submits a card as raw JSON.
///
/// The card must pass `splitcard-core` validation, name this team's event
/// label and team name, list only participants with their profile's current
/// name and Lightning Address, and supersede exactly the active card (or
/// nothing, if there is none). The canonical JSON is stored as a draft.
/// Submitting a stored card again returns it with 200. Profiles are checked
/// only here: a later profile change does not affect a submitted card.
///
/// # Errors
///
/// `invalid_card_json` (400); `team_not_found` (404); `not_a_participant`
/// (403); a core validation code, `wrong_event`, `wrong_team`,
/// `member_not_participant`, `member_name_mismatch` or
/// `member_address_mismatch` (422); `unexpected_supersedes`,
/// `missing_supersedes`, `wrong_supersedes_hash`, or `prize_agreed` for a
/// replacement once the team agreed a prize (409).
pub async fn submit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<(StatusCode, Json<CardView>), ApiError> {
    let team_id = parse_team_id(&id)?;
    let card = parse_card(&auth.body)?;
    validate(&card)?;
    let Card::V1(v1) = &card else {
        return Err(unprocessable(
            "unsupported_version",
            "version 2 cards are not accepted yet",
        )
        .with_details(json!({ "version": card.version() })));
    };
    let hash = card_hash_hex(&card)?;
    let canonical = canonical_json(&card)?;

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
    let participants = load_participants(&mut tx, team_id).await?;
    check_v1_members(v1, &participants)?;
    if card.supersedes().is_some() {
        refuse_if_prize_agreed(&mut tx, team_id).await?;
    }
    match (team.active_card_hash.as_deref(), card.supersedes()) {
        (None, Some(_)) => {
            return Err(conflict(
                "unexpected_supersedes",
                "the team has no active card, so supersedes must be null",
            ));
        }
        (Some(active), None) => {
            return Err(conflict(
                "missing_supersedes",
                "the team has an active card, so supersedes must be its hash",
            )
            .with_details(json!({ "active_card_hash": active })));
        }
        (Some(active), Some(supersedes)) if supersedes != active => {
            return Err(conflict(
                "wrong_supersedes_hash",
                "supersedes is not the team's active card hash",
            )
            .with_details(json!({ "active_card_hash": active })));
        }
        _ => {}
    }
    sqlx::query(
        "INSERT INTO cards (hash, team_id, canonical_json, supersedes, status, created_by, \
         created_at) VALUES (?, ?, ?, ?, 'draft', ?, ?)",
    )
    .bind(&hash)
    .bind(team_id)
    .bind(&canonical)
    .bind(card.supersedes())
    .bind(&auth.pubkey)
    .bind(db_time(state.clock.now_unix()))
    .execute(&mut *tx)
    .await?;
    let row = load_card(&mut tx, &hash)
        .await?
        .ok_or_else(card_not_found)?;
    let view = card_view(&mut tx, row).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(view)))
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
        if *address != participant.lightning_address {
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
/// # Errors
///
/// `invalid_event_json` (400), `card_not_found` (404), a core signature code
/// such as `invalid_signature` or `non_member` (422), `card_superseded` (409),
/// `card_stale` (409) for a draft whose `supersedes` is no longer the team's
/// active card, so two fully signed cards never compete for one place, and
/// `prize_agreed` (409) for a replacement card once the team agreed a prize.
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
    let pubkey = match &old {
        None => verify_signature_event(&card, &event)?,
        Some(old) => verify_supersede_signature_event(old, &card, &event)?,
    }
    .pubkey;

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
    if card.supersedes().is_some() {
        refuse_if_prize_agreed(&mut tx, row.team_id).await?;
    }
    let now = state.clock.now_unix();
    let stored = sqlx::query(
        "INSERT INTO card_signatures (card_hash, pubkey, event_json, received_at) \
         VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(&hash)
    .bind(&pubkey)
    .bind(&event_json)
    .bind(db_time(now))
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
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
