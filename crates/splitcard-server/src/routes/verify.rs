//! Verification reports: whether a card holds up.
//!
//! Every check is recomputed from stored raw data: the canonical card JSON,
//! the signature events as received, the outbox events and the `.ots` proof.
//! Stored status fields are what is being checked, not evidence. The checks
//! call `splitcard-core` (hashing, validation, signatures, status,
//! superseding) and the `.ots` parser; nothing is reimplemented.
//!
//! These endpoints only read. A mismatch is reported, never fixed.
//!
//! Relay acceptance cannot be recomputed from stored data: it is what relays
//! answered when this server published. The report lists those answers and
//! each event id, so anyone can fetch the events from the relays themselves.

use axum::{
    Json,
    extract::{Path, State},
};
use nostr::event::Event;
use serde::Serialize;
use serde_json::{Value, json};
use splitcard_core::{
    card::Card,
    hash::{canonical_json, card_hash_hex},
    signature::{SIGNATURE_KIND, verify_signature_event},
    status::{CardStatus, card_status},
    supersede::{SupersedeError, check_supersede, verify_supersede_signature_event},
    validate::validate,
};
use sqlx::{FromRow, SqliteConnection};

use super::{
    cards::{card_not_found, card_state, is_stale},
    parse_team_id, team_not_found,
    teams::load_team,
};
use crate::{
    AppState,
    error::ApiError,
    ots::format::{FileHashOp, parse_file},
};

/// Check results.
pub mod check_result {
    /// The check holds.
    pub const PASS: &str = "pass";
    /// The check cannot hold yet, but nothing contradicts it.
    pub const PENDING: &str = "pending";
    /// The stored data contradicts the check.
    pub const FAIL: &str = "fail";
    /// The check does not apply to this card.
    pub const NOT_APPLICABLE: &str = "not_applicable";
}

/// Overall verdicts.
pub mod overall {
    /// Every check passed or does not apply.
    pub const VERIFIED: &str = "verified";
    /// No check failed, and at least one is pending.
    pub const PENDING: &str = "pending";
    /// At least one check failed.
    pub const FAILED: &str = "failed";
    /// No check failed, and the card is a stale draft: nothing is wrong, and
    /// nothing more is coming.
    pub const STALE: &str = "stale";
}

/// `details.reason` of the checks a stale draft can never complete.
pub const STALE_DRAFT: &str = "stale_draft";

/// Every check, in report order.
pub const CHECK_NAMES: [&str; 8] = [
    "card_hash_matches",
    "card_valid",
    "signatures_valid",
    "all_members_signed",
    "status_consistent",
    "supersede_valid",
    "published_to_relays",
    "timestamp",
];

/// The note every timestamp check carries.
pub const TIMESTAMP_NOTE: &str = "This server does not check block headers against Bitcoin. \
     A Bitcoin attestation here means a calendar supplied one; verify it independently \
     with the OpenTimestamps client against your own Bitcoin node.";

/// A verification report for one card.
#[derive(Debug, Serialize)]
pub struct VerificationReport {
    /// The card hash being verified.
    pub card_hash: String,
    /// The stored canonical card JSON, exactly as stored.
    pub canonical_card_json: String,
    /// The card's status as `GET /cards/{hash}` shows it: the stored status,
    /// or `stale` for a draft that can never activate. Being checked, not
    /// evidence: see `status_consistent`.
    pub status: String,
    /// `failed`, `stale`, `pending` or `verified`, in that order of precedence.
    pub overall: &'static str,
    /// Every check in [`CHECK_NAMES`] order, each exactly once.
    pub checks: Vec<CheckReport>,
    /// Each stored signature event.
    pub signatures: Vec<SignatureReport>,
    /// The cards before and after this one.
    pub supersede_chain: SupersedeChain,
    /// What relays answered for each event this card was published with.
    pub relays: Vec<RelayEventReport>,
    /// The stored timestamp state and where to download the proof.
    pub timestamp: TimestampSection,
}

/// One check's outcome.
#[derive(Debug, Serialize)]
pub struct CheckReport {
    /// The check's name, from [`CHECK_NAMES`].
    pub name: &'static str,
    /// `pass`, `pending`, `fail` or `not_applicable`.
    pub result: &'static str,
    /// What was compared, and why it failed if it did.
    pub details: Value,
}

/// A stored signature event and its verification.
#[derive(Debug, Serialize)]
pub struct SignatureReport {
    /// The event JSON exactly as received.
    pub event: String,
    /// The signer's public key as stored.
    pub signer: String,
    /// The signer's name on this card (or, for a superseding card, the old
    /// card), if they are on it.
    pub member_name: Option<String>,
    /// `pass` or `fail`.
    pub result: &'static str,
    /// Why it failed, as `{ code, message, details }`.
    pub error: Option<Value>,
}

/// The cards before and after this one.
#[derive(Debug, Serialize)]
pub struct SupersedeChain {
    /// The hash this card supersedes, from its canonical JSON.
    pub previous: Option<String>,
    /// The fully signed card that supersedes this one, if any.
    pub next: Option<String>,
}

/// What relays answered for one event.
#[derive(Debug, Serialize)]
pub struct RelayEventReport {
    /// The event id, to fetch the event from a relay.
    pub event_id: String,
    /// `card` for the server's card event, `signature` for a member's.
    pub kind: &'static str,
    /// The event's author.
    pub pubkey: String,
    /// For the card event: the key recorded as having signed it when it was
    /// queued. Verification checks against this, never the current key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_pubkey: Option<String>,
    /// For the card event: why the stored event is not the card's genuine
    /// card event, such as `content mismatch` or `signer mismatch`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<&'static str>,
    /// The outbox status: `pending`, `sent` (at least one relay accepted) or `failed`.
    pub status: String,
    /// Each relay's recorded answer.
    pub deliveries: Vec<DeliveryReport>,
}

/// One relay's recorded answer for one event.
#[derive(Debug, Serialize, FromRow)]
pub struct DeliveryReport {
    /// The relay URL.
    pub relay: String,
    /// `pending`, `accepted` or `failed`.
    pub status: String,
    /// Publish attempts to this relay.
    pub attempts: i64,
    /// The relay's latest error, if any.
    pub last_error: Option<String>,
}

/// The stored timestamp state.
#[derive(Debug, Serialize)]
pub struct TimestampSection {
    /// The stored status (`queued`, `pending`, `complete`, `failed`), or
    /// `null` if the card was never queued.
    pub status: Option<String>,
    /// The stored Bitcoin block height, if any.
    pub bitcoin_block_height: Option<i64>,
    /// Where to download the `.ots` proof.
    pub ots_url: String,
}

#[derive(Debug, FromRow)]
struct CardRow {
    hash: String,
    team_id: i64,
    canonical_json: String,
    status: String,
}

#[derive(Debug, FromRow)]
struct SignatureRow {
    pubkey: String,
    event_json: String,
}

#[derive(Debug, FromRow)]
struct TimestampRow {
    ots_proof: Option<Vec<u8>>,
    status: String,
    bitcoin_block_height: Option<i64>,
}

#[derive(Debug, FromRow)]
struct OutboxRow {
    id: i64,
    event_json: String,
    status: String,
    /// Set for the server's card events only.
    server_pubkey: Option<String>,
}

/// A stored card with its parsed contents and signature events.
struct Stored {
    row: CardRow,
    card: Result<Card, String>,
    signatures: Vec<(SignatureRow, Result<Event, String>)>,
}

impl Stored {
    fn events(&self) -> Vec<Event> {
        self.signatures
            .iter()
            .filter_map(|(_, event)| event.as_ref().ok().cloned())
            .collect()
    }

    fn supersedes(&self) -> Option<&str> {
        self.card.as_ref().ok()?.supersedes.as_deref()
    }
}

/// Everything a report is computed from, read in one snapshot.
struct Evidence {
    this: Stored,
    /// Every other card of the team.
    others: Vec<Stored>,
    /// The team's stored active card pointer.
    active_card_hash: Option<String>,
    timestamp: Option<TimestampRow>,
    outbox: Vec<(OutboxRow, Vec<DeliveryReport>)>,
}

// Loading.

async fn load_stored(conn: &mut SqliteConnection, row: CardRow) -> Result<Stored, sqlx::Error> {
    let signatures: Vec<SignatureRow> = sqlx::query_as(
        "SELECT pubkey, event_json FROM card_signatures WHERE card_hash = ? \
         ORDER BY received_at, rowid",
    )
    .bind(&row.hash)
    .fetch_all(conn)
    .await?;
    let card = serde_json::from_str::<Card>(&row.canonical_json).map_err(|e| e.to_string());
    let signatures = signatures
        .into_iter()
        .map(|signature| {
            let event = Event::from_json(&signature.event_json).map_err(|e| e.to_string());
            (signature, event)
        })
        .collect();
    Ok(Stored {
        row,
        card,
        signatures,
    })
}

async fn load_evidence(
    conn: &mut SqliteConnection,
    hash: &str,
) -> Result<Option<Evidence>, sqlx::Error> {
    let Some(row) = sqlx::query_as::<_, CardRow>(
        "SELECT hash, team_id, canonical_json, status FROM cards WHERE hash = ?",
    )
    .bind(hash)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let team_id = row.team_id;
    let this = load_stored(conn, row).await?;
    let other_rows: Vec<CardRow> = sqlx::query_as(
        "SELECT hash, team_id, canonical_json, status FROM cards \
         WHERE team_id = ? AND hash != ? ORDER BY created_at, rowid",
    )
    .bind(team_id)
    .bind(hash)
    .fetch_all(&mut *conn)
    .await?;
    let mut others = Vec::with_capacity(other_rows.len());
    for row in other_rows {
        others.push(load_stored(conn, row).await?);
    }
    let timestamp = sqlx::query_as(
        "SELECT ots_proof, status, bitcoin_block_height FROM card_timestamps \
         WHERE card_hash = ?",
    )
    .bind(hash)
    .fetch_optional(&mut *conn)
    .await?;
    let outbox = load_outbox(conn, &this).await?;
    let active_card_hash: Option<String> =
        sqlx::query_scalar("SELECT active_card_hash FROM teams WHERE id = ?")
            .bind(team_id)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
    Ok(Some(Evidence {
        this,
        others,
        active_card_hash,
        timestamp,
        outbox,
    }))
}

/// Outbox entries that mention the card hash or a signature event id, with
/// their deliveries. Which entries are this card's is decided later from the
/// parsed events.
async fn load_outbox(
    conn: &mut SqliteConnection,
    this: &Stored,
) -> Result<Vec<(OutboxRow, Vec<DeliveryReport>)>, sqlx::Error> {
    let mut needles = vec![this.row.hash.clone()];
    needles.extend(
        this.signatures
            .iter()
            .filter_map(|(_, event)| event.as_ref().ok().map(|event| event.id.to_hex())),
    );
    let mut rows: Vec<OutboxRow> = Vec::new();
    for needle in needles {
        let found: Vec<OutboxRow> = sqlx::query_as(
            "SELECT id, event_json, status, server_pubkey FROM relay_outbox \
             WHERE instr(event_json, ?) > 0 ORDER BY id",
        )
        .bind(needle)
        .fetch_all(&mut *conn)
        .await?;
        for row in found {
            if !rows.iter().any(|known| known.id == row.id) {
                rows.push(row);
            }
        }
    }
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let deliveries = sqlx::query_as(
            "SELECT relay, status, attempts, last_error FROM relay_deliveries \
             WHERE outbox_id = ? ORDER BY relay",
        )
        .bind(row.id)
        .fetch_all(&mut *conn)
        .await?;
        entries.push((row, deliveries));
    }
    Ok(entries)
}

// Shared helpers.

fn check(name: &'static str, result: &'static str, details: Value) -> CheckReport {
    CheckReport {
        name,
        result,
        details,
    }
}

fn error_json(error: impl Into<ApiError>) -> Value {
    error.into().to_json()
}

fn unparsed_card(name: &'static str, error: &str) -> CheckReport {
    check(
        name,
        check_result::FAIL,
        json!({ "reason": "the stored canonical JSON is not a card", "error": error }),
    )
}

/// The card `card` supersedes, among `others`.
fn previous_card<'a>(card: &Card, others: &'a [Stored]) -> Option<&'a Stored> {
    let previous = card.supersedes.as_deref()?;
    others.iter().find(|other| other.row.hash == previous)
}

/// Whether a card is fully signed: every member for a first card, or what
/// `check_supersede` requires for a superseding one.
fn fully_signed(stored: &Stored, others: &[&Stored]) -> bool {
    let Ok(card) = &stored.card else {
        return false;
    };
    let events = stored.events();
    match card.supersedes.as_deref() {
        None => matches!(
            card_status(card, &events).map(|report| report.status),
            Ok(CardStatus::Active { .. })
        ),
        Some(previous) => others
            .iter()
            .find(|other| other.row.hash == previous)
            .and_then(|old| old.card.as_ref().ok())
            .is_some_and(|old| check_supersede(old, card, &events).is_ok()),
    }
}

/// What the signatures and supersede chain imply the card's status is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Implied {
    Draft,
    Active,
    Superseded,
    /// Fully signed, but so is another card replacing the same predecessor.
    /// Signing refuses drafts that no longer follow the active card, so this
    /// never arises through the API and is always inconsistent.
    Competing,
}

impl Implied {
    fn allows(self, stored: &str) -> bool {
        match self {
            Self::Draft => stored == card_state::DRAFT,
            Self::Active => stored == card_state::ACTIVE,
            Self::Superseded => stored == card_state::SUPERSEDED,
            Self::Competing => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Draft => card_state::DRAFT,
            Self::Active => card_state::ACTIVE,
            Self::Superseded => card_state::SUPERSEDED,
            Self::Competing => "competing",
        }
    }
}

/// The chain around the card, from the parsed cards.
struct Chain<'a> {
    implied: Implied,
    /// The fully signed card superseding this one, first by creation.
    next: Option<&'a Stored>,
    /// Other fully signed cards replacing the same predecessor.
    competing: Vec<&'a str>,
    /// A draft that can never activate, as `GET /cards/{hash}` reports it.
    stale: bool,
}

fn chain(evidence: &Evidence) -> Chain<'_> {
    let all: Vec<&Stored> = std::iter::once(&evidence.this)
        .chain(&evidence.others)
        .collect();
    let this = &evidence.this;
    let next = evidence.others.iter().find(|other| {
        other.supersedes() == Some(this.row.hash.as_str()) && fully_signed(other, &all)
    });
    let signed = fully_signed(this, &all);
    let competing: Vec<&str> = if signed {
        evidence
            .others
            .iter()
            .filter(|other| other.supersedes() == this.supersedes() && fully_signed(other, &all))
            .map(|other| other.row.hash.as_str())
            .collect()
    } else {
        Vec::new()
    };
    let implied = if !signed {
        Implied::Draft
    } else if next.is_some() {
        Implied::Superseded
    } else if !competing.is_empty() {
        Implied::Competing
    } else {
        Implied::Active
    };
    let stale = is_stale(
        &this.row.status,
        this.supersedes(),
        evidence.active_card_hash.as_deref(),
    );
    Chain {
        implied,
        next,
        competing,
        stale,
    }
}

// Checks.

fn card_hash_matches(evidence: &Evidence, hash: &str) -> CheckReport {
    const NAME: &str = "card_hash_matches";
    let card = match &evidence.this.card {
        Ok(card) => card,
        Err(error) => return unparsed_card(NAME, error),
    };
    let (computed, canonical) = match (card_hash_hex(card), canonical_json(card)) {
        (Ok(computed), Ok(canonical)) => (computed, canonical),
        (Err(error), _) | (_, Err(error)) => {
            return check(
                NAME,
                check_result::FAIL,
                json!({ "error": error_json(error) }),
            );
        }
    };
    let is_canonical = canonical == evidence.this.row.canonical_json;
    let pass = is_canonical && computed == hash;
    check(
        NAME,
        if pass {
            check_result::PASS
        } else {
            check_result::FAIL
        },
        json!({
            "expected": hash,
            "computed": computed,
            "stored_json_is_canonical": is_canonical,
        }),
    )
}

fn card_valid(evidence: &Evidence) -> CheckReport {
    const NAME: &str = "card_valid";
    match &evidence.this.card {
        Err(error) => unparsed_card(NAME, error),
        Ok(card) => match validate(card) {
            Ok(()) => check(NAME, check_result::PASS, Value::Null),
            Err(error) => check(
                NAME,
                check_result::FAIL,
                json!({ "error": error_json(error) }),
            ),
        },
    }
}

/// Verifies each stored signature event against the card, or for a
/// superseding card against the old and new cards. `Ok` holds the signer.
fn verify_events(evidence: &Evidence) -> Vec<Result<String, Value>> {
    let this = &evidence.this;
    let old = this
        .card
        .as_ref()
        .ok()
        .and_then(|card| previous_card(card, &evidence.others))
        .and_then(|old| old.card.as_ref().ok());
    this.signatures
        .iter()
        .map(|(row, event)| {
            let card = this.card.as_ref().map_err(
                |error| json!({ "code": "card_unparseable", "message": error, "details": null }),
            )?;
            let event = event.as_ref().map_err(
                |error| json!({ "code": "invalid_event_json", "message": error, "details": null }),
            )?;
            let signer = match (card.supersedes.is_some(), old) {
                (true, Some(old)) => verify_supersede_signature_event(old, card, event),
                _ => verify_signature_event(card, event),
            }
            .map_err(error_json)?;
            if signer != row.pubkey {
                return Err(json!({
                    "code": "stored_signer_mismatch",
                    "message": "the event's signer is not the stored signer",
                    "details": { "stored": row.pubkey, "event": signer },
                }));
            }
            Ok(signer)
        })
        .collect()
}

fn signatures_valid(evidence: &Evidence, results: &[Result<String, Value>]) -> CheckReport {
    let events: Vec<Value> = evidence
        .this
        .signatures
        .iter()
        .zip(results)
        .map(|((row, event), result)| {
            json!({
                "event_id": event.as_ref().ok().map(|event| event.id.to_hex()),
                "pubkey": row.pubkey,
                "result": if result.is_ok() { check_result::PASS } else { check_result::FAIL },
                "error": result.as_ref().err(),
            })
        })
        .collect();
    let pass = results.iter().all(Result::is_ok);
    check(
        "signatures_valid",
        if pass {
            check_result::PASS
        } else {
            check_result::FAIL
        },
        json!({ "events": events }),
    )
}

fn member_list(card: &Card, pubkeys: &[String]) -> Vec<Value> {
    pubkeys
        .iter()
        .map(|pubkey| {
            let name = card
                .members
                .iter()
                .find(|member| &member.pubkey == pubkey)
                .map(|member| member.name.clone());
            json!({ "pubkey": pubkey, "name": name })
        })
        .collect()
}

fn all_members_signed(evidence: &Evidence) -> CheckReport {
    const NAME: &str = "all_members_signed";
    let card = match &evidence.this.card {
        Ok(card) => card,
        Err(error) => return unparsed_card(NAME, error),
    };
    match card_status(card, &evidence.this.events()) {
        Ok(report) => match report.status {
            CardStatus::Active { signed } => check(
                NAME,
                check_result::PASS,
                json!({ "signed": member_list(card, &signed) }),
            ),
            CardStatus::Draft { signed, missing } => check(
                NAME,
                check_result::PENDING,
                json!({
                    "signed": member_list(card, &signed),
                    "missing": member_list(card, &missing),
                }),
            ),
        },
        Err(error) => check(
            NAME,
            check_result::FAIL,
            json!({ "error": error_json(error) }),
        ),
    }
}

fn status_consistent(evidence: &Evidence, chain: &Chain<'_>) -> CheckReport {
    const NAME: &str = "status_consistent";
    if let Err(error) = &evidence.this.card {
        return unparsed_card(NAME, error);
    }
    let stored = evidence.this.row.status.as_str();
    let mut details = json!({ "stored": stored, "implied": chain.implied.name() });
    if chain.stale {
        // Still a draft as far as the evidence goes; it just can never activate.
        details["stale"] = json!(true);
    }
    if chain.implied == Implied::Competing {
        details["competing_with"] = json!(chain.competing);
    }
    check(
        NAME,
        if chain.implied.allows(stored) {
            check_result::PASS
        } else {
            check_result::FAIL
        },
        details,
    )
}

fn supersede_valid(evidence: &Evidence, chain: &Chain<'_>) -> CheckReport {
    const NAME: &str = "supersede_valid";
    let card = match &evidence.this.card {
        Ok(card) => card,
        Err(error) => return unparsed_card(NAME, error),
    };
    let Some(previous) = card.supersedes.as_deref() else {
        return check(NAME, check_result::NOT_APPLICABLE, Value::Null);
    };
    let Some(old) = previous_card(card, &evidence.others) else {
        return check(
            NAME,
            check_result::FAIL,
            json!({ "previous": previous, "reason": "the superseded card is not stored" }),
        );
    };
    let old_card = match &old.card {
        Ok(old_card) => old_card,
        Err(error) => {
            return check(
                NAME,
                check_result::FAIL,
                json!({ "previous": previous, "reason": "the superseded card is not a card", "error": error }),
            );
        }
    };
    match check_supersede(old_card, card, &evidence.this.events()) {
        Ok(()) => check(NAME, check_result::PASS, json!({ "previous": previous })),
        // Every rule but the signature count held. A stale draft can never
        // collect the rest; all_members_signed reports that instead.
        Err(
            error @ (SupersedeError::MissingNewMemberSignatures { .. }
            | SupersedeError::MissingOldMemberSignatures { .. }),
        ) if chain.stale => check(
            NAME,
            check_result::PASS,
            json!({ "previous": previous, "signatures": STALE_DRAFT, "error": error_json(error) }),
        ),
        Err(
            error @ (SupersedeError::MissingNewMemberSignatures { .. }
            | SupersedeError::MissingOldMemberSignatures { .. }),
        ) => check(
            NAME,
            check_result::PENDING,
            json!({ "previous": previous, "error": error_json(error) }),
        ),
        Err(error) => check(
            NAME,
            check_result::FAIL,
            json!({ "previous": previous, "error": error_json(error) }),
        ),
    }
}

/// The card event and every signature event, matched to outbox entries.
/// Why `event` is not the genuine card event for `hash`: each a separate
/// reason, from the event's own id, signature and content, and the key
/// recorded as its signer. The server's current key plays no part.
fn card_event_problems(
    event: &Event,
    hash: &str,
    canonical_json: &str,
    recorded_signer: &str,
) -> Vec<&'static str> {
    let tag_ok = |name: &str| {
        let mut tags = event.tags.iter().filter(|tag| tag.kind() == name);
        matches!((tags.next(), tags.next()), (Some(tag), None) if tag.content() == Some(hash))
    };
    let mut problems = Vec::new();
    if !event.verify_id() {
        problems.push("invalid event id");
    }
    if !event.verify_signature() {
        problems.push("invalid signature");
    }
    if event.kind.as_u16() != SIGNATURE_KIND {
        problems.push("wrong kind");
    }
    if !tag_ok("d") {
        problems.push("d tag mismatch");
    }
    if !tag_ok("splitcard") {
        problems.push("splitcard tag mismatch");
    }
    if event.content != canonical_json {
        problems.push("content mismatch");
    }
    if event.pubkey.to_hex() != recorded_signer {
        problems.push("signer mismatch");
    }
    problems
}

fn relay_events(evidence: &Evidence) -> Vec<(RelayEventReport, bool)> {
    let hash = evidence.this.row.hash.as_str();
    let canonical_json = evidence.this.row.canonical_json.as_str();
    let entries: Vec<(&OutboxRow, &Vec<DeliveryReport>, Event)> = evidence
        .outbox
        .iter()
        .filter_map(|(row, deliveries)| {
            Event::from_json(&row.event_json)
                .ok()
                .map(|event| (row, deliveries, event))
        })
        .collect();
    let report = |kind: &'static str, event_id: String, pubkey: String| {
        let entry = entries
            .iter()
            .find(|(_, _, event)| event.id.to_hex() == event_id);
        let found = entry.is_some();
        (
            RelayEventReport {
                event_id,
                kind,
                pubkey,
                server_pubkey: None,
                problems: Vec::new(),
                status: entry.map_or_else(|| "not_queued".to_owned(), |e| e.0.status.clone()),
                deliveries: entry.map_or_else(Vec::new, |(_, deliveries, _)| {
                    deliveries
                        .iter()
                        .map(|d| DeliveryReport {
                            relay: d.relay.clone(),
                            status: d.status.clone(),
                            attempts: d.attempts,
                            last_error: d.last_error.clone(),
                        })
                        .collect()
                }),
            },
            found,
        )
    };
    let mut events = Vec::new();
    // The server's card event for this card: queued with a recorded signer,
    // and naming the card in a tag or carrying its JSON, so that tampering
    // with either still finds it and reports what is wrong.
    let card_event = entries.iter().find_map(|(row, _, event)| {
        let signer = row.server_pubkey.as_deref()?;
        let names_card = event.tags.iter().any(|tag| {
            (tag.kind() == "d" || tag.kind() == "splitcard") && tag.content() == Some(hash)
        });
        (names_card || event.content == canonical_json).then_some((signer, event))
    });
    match card_event {
        Some((signer, event)) => {
            let (mut report, found) = report("card", event.id.to_hex(), event.pubkey.to_hex());
            report.server_pubkey = Some(signer.to_owned());
            report.problems = card_event_problems(event, hash, canonical_json, signer);
            events.push((report, found));
        }
        None => events.push((
            RelayEventReport {
                event_id: String::new(),
                kind: "card",
                pubkey: String::new(),
                server_pubkey: None,
                problems: Vec::new(),
                status: "not_queued".to_owned(),
                deliveries: Vec::new(),
            },
            false,
        )),
    }
    for (row, event) in &evidence.this.signatures {
        match event {
            Ok(event) => events.push(report("signature", event.id.to_hex(), row.pubkey.clone())),
            Err(_) => events.push((
                RelayEventReport {
                    event_id: String::new(),
                    kind: "signature",
                    pubkey: row.pubkey.clone(),
                    server_pubkey: None,
                    problems: Vec::new(),
                    status: "not_queued".to_owned(),
                    deliveries: Vec::new(),
                },
                false,
            )),
        }
    }
    events
}

fn published_to_relays(chain: &Chain<'_>, events: &[(RelayEventReport, bool)]) -> CheckReport {
    const NAME: &str = "published_to_relays";
    let note = "Relay answers as recorded when this server published; fetch the event ids \
                from any relay to confirm.";
    if chain.implied == Implied::Draft {
        return check(
            NAME,
            check_result::NOT_APPLICABLE,
            json!({ "reason": "a card is published only once fully signed" }),
        );
    }
    let accepted = |report: &RelayEventReport| {
        report
            .deliveries
            .iter()
            .any(|delivery| delivery.status == "accepted")
    };
    // An entry failed before per-relay deliveries existed has none, so its
    // outbox status is the only record that publishing gave up.
    let gave_up = |report: &RelayEventReport| {
        !accepted(report)
            && (report.status == "failed"
                || (!report.deliveries.is_empty()
                    && report
                        .deliveries
                        .iter()
                        .all(|delivery| delivery.status == "failed")))
    };
    let missing = events.iter().any(|(_, found)| !found);
    let forged = events.iter().any(|(report, _)| !report.problems.is_empty());
    let result = if missing || forged || events.iter().any(|(report, _)| gave_up(report)) {
        check_result::FAIL
    } else if events.iter().all(|(report, _)| accepted(report)) {
        check_result::PASS
    } else {
        check_result::PENDING
    };
    let events: Vec<Value> = events
        .iter()
        .map(|(report, found)| {
            let mut event = json!({
                "event_id": report.event_id,
                "kind": report.kind,
                "queued": found,
                "accepted": accepted(report),
                "relays": report.deliveries,
            });
            if let Some(signer) = &report.server_pubkey {
                event["server_pubkey"] = json!(signer);
                event["problems"] = json!(report.problems);
            }
            event
        })
        .collect();
    check(NAME, result, json!({ "events": events, "note": note }))
}

fn timestamp(state: &AppState, evidence: &Evidence, chain: &Chain<'_>) -> CheckReport {
    const NAME: &str = "timestamp";
    let hash = &evidence.this.row.hash;
    let base = json!({
        "note": TIMESTAMP_NOTE,
        "ots_url": ots_url(state, hash),
        "verify_command": format!("ots verify {hash}.json.ots"),
        "verify_steps": format!(
            "Save canonical_card_json byte for byte as {hash}.json, download the proof as \
             {hash}.json.ots next to it, then run: ots verify {hash}.json.ots"
        ),
    });
    let with = |result: &'static str, extra: Value| {
        let mut details = base.clone();
        if let (Value::Object(details), Value::Object(extra)) = (&mut details, extra) {
            details.extend(extra);
        }
        check(NAME, result, details)
    };
    let proof = evidence
        .timestamp
        .as_ref()
        .and_then(|row| row.ots_proof.as_deref());
    let Some(proof) = proof else {
        return match (&evidence.timestamp, chain.implied) {
            (None, Implied::Draft) => check(
                NAME,
                check_result::NOT_APPLICABLE,
                json!({ "reason": "a card is timestamped only once fully signed" }),
            ),
            (Some(row), _) if row.status == "queued" => with(
                check_result::PENDING,
                json!({ "reason": "queued; no calendar has accepted the hash yet" }),
            ),
            _ => with(
                check_result::FAIL,
                json!({ "reason": "no proof is stored" }),
            ),
        };
    };
    let file = match parse_file(proof) {
        Ok(file) => file,
        Err(error) => {
            return with(
                check_result::FAIL,
                json!({ "reason": "the proof does not parse", "error": error.to_string() }),
            );
        }
    };
    let digest = hex::encode(&file.digest);
    if file.file_hash_op != FileHashOp::Sha256 || digest != *hash {
        return with(
            check_result::FAIL,
            json!({ "reason": "the proof is for a different digest", "proof_digest": digest }),
        );
    }
    let pending = file
        .timestamp
        .pending_attestations(&file.digest)
        .map(|found| found.into_iter().map(|p| p.uri).collect::<Vec<_>>())
        .unwrap_or_default();
    match file.timestamp.bitcoin_heights().into_iter().min() {
        Some(height) => with(
            check_result::PASS,
            json!({ "proof_digest": digest, "bitcoin_block_height": height, "pending_calendars": pending }),
        ),
        None if !pending.is_empty() => with(
            check_result::PENDING,
            json!({ "proof_digest": digest, "pending_calendars": pending }),
        ),
        None => with(
            check_result::FAIL,
            json!({ "reason": "the proof has no Bitcoin or pending attestation", "proof_digest": digest }),
        ),
    }
}

fn ots_url(state: &AppState, hash: &str) -> String {
    format!(
        "{}/cards/{hash}/timestamp.ots",
        state.config.public_base_url
    )
}

// The report.

fn signature_reports(
    evidence: &Evidence,
    results: Vec<Result<String, Value>>,
) -> Vec<SignatureReport> {
    let this = &evidence.this;
    let card = this.card.as_ref().ok();
    let old = card
        .and_then(|card| previous_card(card, &evidence.others))
        .and_then(|old| old.card.as_ref().ok());
    this.signatures
        .iter()
        .zip(results)
        .map(|((row, _), result)| {
            let member_name = card
                .into_iter()
                .chain(old)
                .flat_map(|card| &card.members)
                .find(|member| member.pubkey == row.pubkey)
                .map(|member| member.name.clone());
            SignatureReport {
                event: row.event_json.clone(),
                signer: row.pubkey.clone(),
                member_name,
                result: if result.is_ok() {
                    check_result::PASS
                } else {
                    check_result::FAIL
                },
                error: result.err(),
            }
        })
        .collect()
}

fn build_report(state: &AppState, evidence: &Evidence) -> VerificationReport {
    let hash = evidence.this.row.hash.clone();
    let chain = chain(evidence);
    let signature_results = verify_events(evidence);
    let relay_events = relay_events(evidence);
    // Checks a stale draft can never complete; integrity checks still run.
    let unless_stale = |name: &'static str, run: &dyn Fn() -> CheckReport| {
        if chain.stale {
            check(
                name,
                check_result::NOT_APPLICABLE,
                json!({ "reason": STALE_DRAFT }),
            )
        } else {
            run()
        }
    };
    let checks = vec![
        card_hash_matches(evidence, &hash),
        card_valid(evidence),
        signatures_valid(evidence, &signature_results),
        unless_stale("all_members_signed", &|| all_members_signed(evidence)),
        status_consistent(evidence, &chain),
        supersede_valid(evidence, &chain),
        unless_stale("published_to_relays", &|| {
            published_to_relays(&chain, &relay_events)
        }),
        unless_stale("timestamp", &|| timestamp(state, evidence, &chain)),
    ];
    // Tampering always wins, even on a stale draft.
    let overall = if checks.iter().any(|c| c.result == check_result::FAIL) {
        overall::FAILED
    } else if chain.stale {
        overall::STALE
    } else if checks.iter().any(|c| c.result == check_result::PENDING) {
        overall::PENDING
    } else {
        overall::VERIFIED
    };
    let next = chain.next.map(|next| next.row.hash.clone());
    let status = if chain.stale {
        card_state::STALE.to_owned()
    } else {
        evidence.this.row.status.clone()
    };
    VerificationReport {
        card_hash: hash.clone(),
        canonical_card_json: evidence.this.row.canonical_json.clone(),
        status,
        overall,
        checks,
        signatures: signature_reports(evidence, signature_results),
        supersede_chain: SupersedeChain {
            previous: evidence.this.supersedes().map(str::to_owned),
            next,
        },
        relays: relay_events.into_iter().map(|(report, _)| report).collect(),
        timestamp: TimestampSection {
            status: evidence.timestamp.as_ref().map(|row| row.status.clone()),
            bitcoin_block_height: evidence
                .timestamp
                .as_ref()
                .and_then(|row| row.bitcoin_block_height),
            ots_url: ots_url(state, &hash),
        },
    }
}

/// Builds the report for `hash` in one read-only snapshot.
async fn report_for(state: &AppState, hash: &str) -> Result<VerificationReport, ApiError> {
    // A deferred transaction for one consistent snapshot; it only reads.
    let mut tx = state.db.begin().await?;
    let evidence = load_evidence(&mut tx, hash)
        .await?
        .ok_or_else(card_not_found)?;
    let report = build_report(state, &evidence);
    tx.rollback().await?;
    Ok(report)
}

/// `GET /cards/{hash}/verify`: the verification report for a card. Read-only.
///
/// # Errors
///
/// `card_not_found` (404).
pub async fn card(
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<VerificationReport>, ApiError> {
    Ok(Json(report_for(&state, &hash).await?))
}

/// `GET /teams/{id}/verify`: the verification report for the team's active
/// card. Read-only.
///
/// # Errors
///
/// `team_not_found` or `no_active_card` (404).
pub async fn team(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<VerificationReport>, ApiError> {
    let team_id = parse_team_id(&id)?;
    let hash = {
        let mut conn = state.db.acquire().await?;
        load_team(&mut conn, team_id)
            .await?
            .ok_or_else(team_not_found)?
            .active_card_hash
            .ok_or_else(|| ApiError::not_found("no_active_card", "the team has no active card"))?
    };
    Ok(Json(report_for(&state, &hash).await?))
}
