//! Prizes and payouts: every member of the active card agrees the prize, then
//! the payer's browser pays each member and records the payment here.
//!
//! The server never pays and never fetches invoices. A payment counts only
//! when its BOLT11 invoice is for the member's exact amount, carries the
//! description hash of LNURL metadata naming the member's Lightning Address,
//! and its preimage verifies.

use std::collections::BTreeSet;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use splitcard_core::{
    card::Card,
    split::{compute_payouts, payout_addresses},
};
use splitcard_ln::{
    invoice::{DecodedInvoice, InvoiceError, Network, decode_invoice},
    lnurl::sats_to_msat,
    proof::verify_preimage,
};
use sqlx::{FromRow, SqliteConnection};

use super::{
    cards::{load_card, member_display_names, signature_jsons, stored_card, stored_events},
    conflict, forbidden, is_unique_violation, parse_json, parse_team_id, team_not_found,
    teams::{load_team, require_participant},
    unprocessable,
};
use crate::{AppState, auth::Authenticated, db::begin_write, db_time, error::ApiError};

/// Largest accepted prize: every bitcoin there will ever be, in sats.
pub const MAX_PRIZE_SATS: u64 = 2_100_000_000_000_000;

/// BOLT11 currency prefix of the only network payments are accepted on.
const MAINNET_CURRENCY: &str = "bc";

/// LNURL metadata entry types that name the payee's Lightning Address (LUD-16).
const IDENTIFIER_TYPES: [&str; 2] = ["text/identifier", "text/email"];

/// Payout phases, as the card screens name them.
pub mod phase {
    /// No prize agreed yet.
    pub const WAITING_FOR_PRIZE: &str = "waiting-for-prize";
    /// Prize agreed, nobody paid yet.
    pub const PRIZE_SET_WAITING: &str = "prize-set-waiting";
    /// At least one member paid.
    pub const PAYOUTS_IN_PROGRESS: &str = "payouts-in-progress";
}

/// Stored state of a payout item.
pub mod item_state {
    /// Not paid yet.
    pub const PENDING: &str = "pending";
    /// Paid, with a verified preimage.
    pub const PAID: &str = "paid";
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutPrize {
    prize_sats: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordPayment {
    bolt11: String,
    preimage: String,
    metadata: String,
}

#[derive(Debug, FromRow)]
struct PrizeRow {
    card_hash: String,
    prize_sats: i64,
    proposed_by: String,
    proposed_at: i64,
    agreed_at: Option<i64>,
}

#[derive(Debug, FromRow)]
struct ItemRow {
    pubkey: String,
    name: String,
    lightning_address: String,
    amount_sats: i64,
    status: String,
    payment_hash: Option<String>,
    paid_by: Option<String>,
    paid_at: Option<i64>,
}

/// A team's prize, its items and who agreed it.
#[derive(Debug)]
pub(crate) struct PayoutState {
    team_id: i64,
    prize: PrizeRow,
    /// Card members' items, in card order.
    items: Vec<ItemRow>,
    confirmed: BTreeSet<String>,
}

impl PayoutState {
    /// Hash of the card the prize is for.
    pub(crate) fn card_hash(&self) -> &str {
        &self.prize.card_hash
    }

    pub(crate) fn is_agreed(&self) -> bool {
        self.prize.agreed_at.is_some()
    }

    pub(crate) fn paid_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == item_state::PAID)
            .count()
    }

    pub(crate) fn phase(&self) -> &'static str {
        if !self.is_agreed() {
            phase::WAITING_FOR_PRIZE
        } else if self.paid_count() == 0 {
            phase::PRIZE_SET_WAITING
        } else {
            phase::PAYOUTS_IN_PROGRESS
        }
    }

    /// Unix seconds of the latest proposal, agreement or payment.
    pub(crate) fn latest_change(&self) -> i64 {
        self.items
            .iter()
            .filter_map(|item| item.paid_at)
            .chain(self.prize.agreed_at)
            .fold(self.prize.proposed_at, i64::max)
    }

    /// The sats `pubkey` has been paid, if their item is paid.
    pub(crate) fn received_sats(&self, pubkey: &str) -> Option<u64> {
        self.items
            .iter()
            .find(|item| item.pubkey == pubkey && item.status == item_state::PAID)
            .and_then(|item| u64::try_from(item.amount_sats).ok())
    }

    fn is_member(&self, pubkey: &str) -> bool {
        self.items.iter().any(|item| item.pubkey == pubkey)
    }

    fn view(self) -> PayoutView {
        let phase = self.phase();
        let paid_count = self.paid_count();
        let (confirmed, missing) = self
            .items
            .iter()
            .map(|item| item.pubkey.clone())
            .partition(|pubkey| self.confirmed.contains(pubkey));
        PayoutView {
            team_id: self.team_id.to_string(),
            card_hash: self.prize.card_hash,
            prize_sats: self.prize.prize_sats.to_string(),
            phase,
            proposed_by: self.prize.proposed_by,
            proposed_at: self.prize.proposed_at,
            agreed_at: self.prize.agreed_at,
            confirmed,
            missing,
            paid_count,
            items: self
                .items
                .into_iter()
                .enumerate()
                .map(|(index, item)| PayoutItemView {
                    index,
                    pubkey: item.pubkey,
                    name: item.name,
                    lightning_address: item.lightning_address,
                    amount_sats: item.amount_sats.to_string(),
                    status: item.status,
                    payment_hash: item.payment_hash,
                    paid_by: item.paid_by,
                    paid_at: item.paid_at,
                })
                .collect(),
        }
    }
}

/// A team's prize and payouts in API responses.
#[derive(Debug, Serialize)]
pub struct PayoutView {
    /// Team id, as a string.
    pub team_id: String,
    /// The active card the prize is split by.
    pub card_hash: String,
    /// The prize, in sats, as a string.
    pub prize_sats: String,
    /// One of the [`phase`] values.
    pub phase: &'static str,
    /// Public key of the member who proposed this amount.
    pub proposed_by: String,
    /// Unix seconds.
    pub proposed_at: i64,
    /// Unix seconds once every card member agreed, else `null`.
    pub agreed_at: Option<i64>,
    /// Card members who agreed this amount, in card order.
    pub confirmed: Vec<String>,
    /// Card members still to agree, in card order.
    pub missing: Vec<String>,
    /// Items paid so far.
    pub paid_count: usize,
    /// Each card member's share, in card order.
    pub items: Vec<PayoutItemView>,
}

/// One member's share of the prize.
#[derive(Debug, Serialize)]
pub struct PayoutItemView {
    /// The member's index on the card.
    pub index: usize,
    /// Nostr public key in lowercase hex.
    pub pubkey: String,
    /// Name on the card.
    pub name: String,
    /// Lightning Address on the card; the payment must go here.
    pub lightning_address: String,
    /// The member's share in sats, as a string, from `compute_payouts`.
    pub amount_sats: String,
    /// `pending` or `paid`.
    pub status: String,
    /// The paid invoice's payment hash, once paid.
    pub payment_hash: Option<String>,
    /// Public key of the member who recorded the payment, once paid.
    pub paid_by: Option<String>,
    /// Unix seconds, once paid.
    pub paid_at: Option<i64>,
}

pub(crate) async fn load_payout(
    conn: &mut SqliteConnection,
    team_id: i64,
) -> Result<Option<PayoutState>, sqlx::Error> {
    let prize: Option<PrizeRow> = sqlx::query_as(
        "SELECT card_hash, prize_sats, proposed_by, proposed_at, agreed_at FROM prizes \
         WHERE team_id = ?",
    )
    .bind(team_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(prize) = prize else {
        return Ok(None);
    };
    let items = sqlx::query_as(
        "SELECT pubkey, name, lightning_address, amount_sats, status, payment_hash, paid_by, \
         paid_at FROM payout_items WHERE team_id = ? ORDER BY member_index",
    )
    .bind(team_id)
    .fetch_all(&mut *conn)
    .await?;
    let confirmed: Vec<String> =
        sqlx::query_scalar("SELECT pubkey FROM prize_confirmations WHERE team_id = ?")
            .bind(team_id)
            .fetch_all(conn)
            .await?;
    Ok(Some(PayoutState {
        team_id,
        prize,
        items,
        confirmed: confirmed.into_iter().collect(),
    }))
}

/// Deletes the team's prize, confirmations and items.
pub(crate) async fn discard_prize(
    conn: &mut SqliteConnection,
    team_id: i64,
) -> Result<(), sqlx::Error> {
    for table in ["payout_items", "prize_confirmations", "prizes"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE team_id = ?"
        )))
        .bind(team_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Fails with `prize_agreed` (409) once the team's prize is agreed: the card
/// it is split by can no longer be replaced.
pub(crate) async fn refuse_if_prize_agreed(
    conn: &mut SqliteConnection,
    team_id: i64,
) -> Result<(), ApiError> {
    let agreed: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM prizes WHERE team_id = ? AND agreed_at IS NOT NULL")
            .bind(team_id)
            .fetch_optional(conn)
            .await?;
    if agreed.is_some() {
        return Err(conflict(
            "prize_agreed",
            "the team agreed a prize for its active card, so the card can no longer be replaced",
        ));
    }
    Ok(())
}

fn payout_not_found() -> ApiError {
    ApiError::not_found("payout_not_found", "the team has no prize yet")
}

fn not_a_card_member() -> ApiError {
    forbidden(
        "not_a_card_member",
        "only members of the team's active card can do this",
    )
}

/// Parses a decimal sats string: digits only, no leading zero, 1 to
/// [`MAX_PRIZE_SATS`].
fn parse_prize_sats(value: &str) -> Result<u64, ApiError> {
    let invalid = || {
        unprocessable(
            "invalid_prize_sats",
            format!(
                "prize_sats must be a whole number of sats from 1 to {MAX_PRIZE_SATS}, as a string"
            ),
        )
        .with_details(json!({ "field": "prize_sats" }))
    };
    let digits = !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && !value.starts_with('0');
    if !digits {
        return Err(invalid());
    }
    match value.parse::<u64>() {
        Ok(sats) if sats <= MAX_PRIZE_SATS => Ok(sats),
        _ => Err(invalid()),
    }
}

fn db_sats(sats: u64) -> Result<i64, ApiError> {
    i64::try_from(sats).map_err(|error| ApiError::internal(&error))
}

/// The team's active card, or `no_active_card` (409).
async fn active_card(
    conn: &mut SqliteConnection,
    active_card_hash: Option<&str>,
) -> Result<(String, Card), ApiError> {
    let no_active = || {
        conflict(
            "no_active_card",
            "the team has no active card, so there is nothing to split",
        )
    };
    let hash = active_card_hash.ok_or_else(no_active)?;
    let row = load_card(conn, hash).await?.ok_or_else(no_active)?;
    Ok((row.hash, stored_card(&row.canonical_json)?))
}

/// `PUT /teams/{id}/prize`: a member of the active card agrees the prize as
/// `{ prize_sats }`, a decimal string.
///
/// The same amount as the open proposal adds the caller's agreement; a
/// different amount replaces the proposal and starts the agreements over
/// with the caller's. Once every card member agreed one amount it is fixed,
/// and the card can no longer be replaced. Items come from
/// `splitcard-core`'s `compute_payouts`.
///
/// # Errors
///
/// `invalid_request_json` (400), `team_not_found` (404), `not_a_card_member`
/// (403), `invalid_prize_sats`, `prize_too_small` or `share_rounds_to_zero`
/// (422), `no_active_card` or `prize_already_agreed` (409).
pub async fn put_prize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<Json<PayoutView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let request: PutPrize = parse_json(&auth.body, "invalid_request_json")?;
    let prize_sats = parse_prize_sats(&request.prize_sats)?;
    let now = db_time(state.clock.now_unix());

    let mut tx = begin_write(&state.db).await?;
    let team = load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let (card_hash, card) = active_card(&mut tx, team.active_card_hash.as_deref()).await?;
    if !card.is_member(&auth.pubkey) {
        return Err(not_a_card_member());
    }
    let amounts = compute_payouts(&card, prize_sats)?;
    let events = stored_events(&signature_jsons(&mut tx, &card_hash).await?)?;
    let addresses = payout_addresses(&card, &events)?;
    let names = member_display_names(&mut tx, &card).await?;

    let existing = load_payout(&mut tx, team_id).await?;
    if let Some(existing) = &existing
        && existing.is_agreed()
    {
        return Err(conflict(
            "prize_already_agreed",
            "every card member agreed the prize; it can no longer change",
        )
        .with_details(json!({ "prize_sats": existing.prize.prize_sats.to_string() })));
    }
    let same_proposal = existing.as_ref().is_some_and(|existing| {
        existing.prize.card_hash == card_hash
            && u64::try_from(existing.prize.prize_sats) == Ok(prize_sats)
    });
    if !same_proposal {
        discard_prize(&mut tx, team_id).await?;
        sqlx::query(
            "INSERT INTO prizes (team_id, card_hash, prize_sats, proposed_by, proposed_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(team_id)
        .bind(&card_hash)
        .bind(db_sats(prize_sats)?)
        .bind(&auth.pubkey)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for (index, (((pubkey, amount), (_, address)), name)) in
            amounts.iter().zip(&addresses).zip(&names).enumerate()
        {
            sqlx::query(
                "INSERT INTO payout_items (team_id, member_index, pubkey, name, \
                 lightning_address, amount_sats, status) VALUES (?, ?, ?, ?, ?, ?, 'pending')",
            )
            .bind(team_id)
            .bind(i64::try_from(index).map_err(|error| ApiError::internal(&error))?)
            .bind(pubkey)
            .bind(name.as_deref().unwrap_or_default())
            .bind(address)
            .bind(db_sats(*amount)?)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "INSERT INTO prize_confirmations (team_id, pubkey, confirmed_at) VALUES (?, ?, ?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(team_id)
    .bind(&auth.pubkey)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let payout = load_payout(&mut tx, team_id)
        .await?
        .ok_or_else(payout_not_found)?;
    if payout
        .items
        .iter()
        .all(|item| payout.confirmed.contains(&item.pubkey))
    {
        sqlx::query("UPDATE prizes SET agreed_at = ? WHERE team_id = ?")
            .bind(now)
            .bind(team_id)
            .execute(&mut *tx)
            .await?;
    }
    let payout = load_payout(&mut tx, team_id)
        .await?
        .ok_or_else(payout_not_found)?;
    tx.commit().await?;
    Ok(Json(payout.view()))
}

/// `GET /teams/{id}/payout`: the team's prize and each member's payout, for
/// its participants.
///
/// # Errors
///
/// `team_not_found` or `payout_not_found` (404), `not_a_participant` (403),
/// or an `auth_*` code (401).
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: Authenticated,
) -> Result<Json<PayoutView>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let mut conn = state.db.acquire().await?;
    load_team(&mut conn, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    require_participant(&mut conn, team_id, &auth.pubkey).await?;
    let payout = load_payout(&mut conn, team_id)
        .await?
        .ok_or_else(payout_not_found)?;
    Ok(Json(payout.view()))
}

fn invalid_invoice(error: &InvoiceError) -> ApiError {
    let reason = match error {
        InvoiceError::Unparseable { .. } => "unparseable",
        InvoiceError::InvalidSignature => "invalid_signature",
        InvoiceError::WrongNetwork { .. } => "wrong_network",
        InvoiceError::MissingAmount => "missing_amount",
        InvoiceError::WrongAmount { .. } => "wrong_amount",
        InvoiceError::MissingDescriptionHash => "missing_description_hash",
        InvoiceError::WrongDescriptionHash => "wrong_description_hash",
        InvoiceError::Expired { .. } => "expired",
    };
    unprocessable("invalid_invoice", error.to_string()).with_details(json!({ "reason": reason }))
}

/// Whether LNURL `metadata` has a `text/identifier` or `text/email` entry
/// that is `address`.
fn metadata_names(metadata: &str, address: &str) -> bool {
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(metadata) else {
        return false;
    };
    entries.iter().any(|entry| {
        let Some([Value::String(kind), Value::String(value), ..]) =
            entry.as_array().map(Vec::as_slice)
        else {
            return false;
        };
        IDENTIFIER_TYPES.contains(&kind.as_str()) && value.to_ascii_lowercase() == address
    })
}

/// Checks a paid invoice against an item: mainnet, the exact amount, and a
/// description hash committing to metadata that names the item's address.
fn check_paid_invoice(
    invoice: &DecodedInvoice,
    amount_sats: u64,
    metadata: &str,
    address: &str,
) -> Result<(), ApiError> {
    if invoice.currency != MAINNET_CURRENCY {
        return Err(invalid_invoice(&InvoiceError::WrongNetwork {
            expected: Network::Mainnet,
            found: invoice.currency.clone(),
        }));
    }
    let expected_msat = sats_to_msat(amount_sats).map_err(|error| ApiError::internal(&error))?;
    let found_msat = invoice
        .amount_msat
        .ok_or_else(|| invalid_invoice(&InvoiceError::MissingAmount))?;
    if found_msat != expected_msat {
        return Err(invalid_invoice(&InvoiceError::WrongAmount {
            expected_msat,
            found_msat,
        })
        .with_details(json!({
            "reason": "wrong_amount",
            "expected_msat": expected_msat.to_string(),
            "found_msat": found_msat.to_string(),
        })));
    }
    let description_hash = invoice
        .description_hash
        .as_deref()
        .ok_or_else(|| invalid_invoice(&InvoiceError::MissingDescriptionHash))?;
    if hex::encode(Sha256::digest(metadata.as_bytes())) != description_hash {
        return Err(invalid_invoice(&InvoiceError::WrongDescriptionHash));
    }
    if !metadata_names(metadata, address) {
        return Err(unprocessable(
            "metadata_wrong_address",
            "the LNURL metadata does not name the member's Lightning Address",
        )
        .with_details(json!({ "lightning_address": address })));
    }
    Ok(())
}

/// `POST /teams/{id}/payout/items/{index}/payment`: a member of the card
/// records that item `index` was paid, as `{ bolt11, preimage, metadata }`:
/// the paid invoice, its preimage and the LNURL-pay metadata of the member's
/// Lightning Address.
///
/// The prize must be agreed. The invoice must be a mainnet invoice for the
/// item's exact amount whose description hash is `sha256(metadata)`, the
/// metadata must name the item's Lightning Address, and the preimage must
/// hash to the payment hash. Recording the same payment again returns 200.
///
/// # Errors
///
/// `invalid_request_json` (400); `team_not_found`, `payout_not_found` or
/// `payout_item_not_found` (404); `not_a_card_member` (403);
/// `invalid_invoice`, `metadata_wrong_address` or a preimage code such as
/// `preimage_mismatch` (422); `prize_not_agreed`, `item_already_paid` or
/// `payment_already_used` (409).
pub async fn record_payment(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, String)>,
    auth: Authenticated,
) -> Result<(StatusCode, Json<PayoutView>), ApiError> {
    let team_id = parse_team_id(&id)?;
    let request: RecordPayment = parse_json(&auth.body, "invalid_request_json")?;
    let item_not_found =
        || ApiError::not_found("payout_item_not_found", "no payout item with this index");
    let index: usize = index.parse().map_err(|_| item_not_found())?;
    let invoice = decode_invoice(&request.bolt11).map_err(|error| invalid_invoice(&error))?;

    let mut tx = begin_write(&state.db).await?;
    load_team(&mut tx, team_id)
        .await?
        .ok_or_else(team_not_found)?;
    let payout = load_payout(&mut tx, team_id)
        .await?
        .ok_or_else(payout_not_found)?;
    if !payout.is_member(&auth.pubkey) {
        return Err(not_a_card_member());
    }
    if !payout.is_agreed() {
        return Err(conflict(
            "prize_not_agreed",
            "every card member must agree the prize before payments are recorded",
        ));
    }
    let item = payout.items.get(index).ok_or_else(item_not_found)?;
    if item.status == item_state::PAID {
        if item.payment_hash.as_deref() == Some(invoice.payment_hash.as_str()) {
            tx.commit().await?;
            return Ok((StatusCode::OK, Json(payout.view())));
        }
        return Err(conflict(
            "item_already_paid",
            "this member's share is already recorded as paid by another payment",
        ));
    }
    let amount_sats =
        u64::try_from(item.amount_sats).map_err(|error| ApiError::internal(&error))?;
    check_paid_invoice(
        &invoice,
        amount_sats,
        &request.metadata,
        &item.lightning_address,
    )?;
    verify_preimage(&invoice.payment_hash, &request.preimage)?;

    let updated = sqlx::query(
        "UPDATE payout_items SET status = 'paid', bolt11 = ?, payment_hash = ?, preimage = ?, \
         paid_by = ?, paid_at = ? WHERE team_id = ? AND member_index = ? AND status = 'pending'",
    )
    .bind(&request.bolt11)
    .bind(&invoice.payment_hash)
    .bind(&request.preimage)
    .bind(&auth.pubkey)
    .bind(db_time(state.clock.now_unix()))
    .bind(team_id)
    .bind(i64::try_from(index).map_err(|error| ApiError::internal(&error))?)
    .execute(&mut *tx)
    .await;
    match updated {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(conflict(
                "payment_already_used",
                "this payment is already recorded for another member",
            ));
        }
        Err(error) => return Err(error.into()),
    }
    let payout = load_payout(&mut tx, team_id)
        .await?
        .ok_or_else(payout_not_found)?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(payout.view())))
}
