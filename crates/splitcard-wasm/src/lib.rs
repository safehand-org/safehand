//! WebAssembly bindings for Split Card.
//!
//! A thin wrapper over `splitcard-core`: every export parses its inputs, calls
//! core, and converts the result. Cards and sat amounts cross the boundary as
//! strings so values above 2^53 - 1 never pass through JavaScript numbers.
//! Failures are thrown as `{ code, message, details }` objects; see [`error`].

use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use splitcard_core::{card::Card, hash, signature, split, status, supersede, validate::validate};
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

/// Structured errors thrown to JavaScript.
pub mod error;
/// A Nostr key held in the browser, and signing with it.
pub mod keys;

use error::JsError;

#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &str = include_str!("types.d.ts");

/// Returns the Split Card WASM package version.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Returns the lowercase hex SHA-256 hash of a card's canonical JSON.
///
/// # Errors
///
/// Throws `invalid_card_json` if the card does not parse.
#[wasm_bindgen]
pub fn card_hash(card_json: &str) -> Result<String, JsValue> {
    let run = || -> Result<String, JsError> { Ok(hash::card_hash_hex(&parse_card(card_json)?)?) };
    run().map_err(throw)
}

/// Validates a card, returning nothing if it is valid.
///
/// # Errors
///
/// Throws `invalid_card_json` or the first validation error.
#[wasm_bindgen]
pub fn validate_card(card_json: &str) -> Result<(), JsValue> {
    let run = || -> Result<(), JsError> { Ok(validate(&parse_card(card_json)?)?) };
    run().map_err(throw)
}

/// Validates a card and splits `prize_sats` (a decimal string) between its members.
///
/// Returns `[{ pubkey, amount_sats }]` in card member order, with each
/// `amount_sats` a decimal string.
///
/// # Errors
///
/// Throws `invalid_card_json`, `invalid_amount`, a validation error or a split error.
#[wasm_bindgen(unchecked_return_type = "Payout[]")]
pub fn compute_payouts(card_json: &str, prize_sats: &str) -> Result<JsValue, JsValue> {
    let run = || -> Result<Vec<Payout>, JsError> {
        let card = parse_card(card_json)?;
        let prize_sats = parse_sats(prize_sats)?;
        let payouts = split::compute_payouts(&card, prize_sats)?;
        Ok(payouts
            .into_iter()
            .map(|(pubkey, amount)| Payout {
                pubkey,
                amount_sats: amount.to_string(),
            })
            .collect())
    };
    export(run())
}

/// Validates a card and returns its members' shares in basis points, in
/// member order, summing to 10,000.
///
/// Takes the card rather than raw weights so large weights never pass through
/// JavaScript numbers.
///
/// # Errors
///
/// Throws `invalid_card_json`, a validation error or a split error.
#[wasm_bindgen(unchecked_return_type = "number[]")]
pub fn display_basis_points(card_json: &str) -> Result<JsValue, JsValue> {
    let run = || -> Result<Vec<u32>, JsError> {
        let card = parse_card(card_json)?;
        validate(&card)?;
        Ok(split::display_basis_points(&card.weights())?)
    };
    export(run())
}

/// Builds the NIP-01 unsigned event a member signs for a card hash, ready for
/// `window.nostr.signEvent`.
///
/// `created_at` is Unix seconds as a whole JS number from 0 to 4294967295.
/// Pass `lightning_address`, the signer's payout address, for a version 2
/// card; leave it out for version 1. It becomes the event's `lud16` tag.
///
/// # Errors
///
/// Throws `invalid_created_at`, `invalid_card_hash`, `invalid_template_pubkey`
/// or `invalid_template_lightning_address`.
#[wasm_bindgen(unchecked_return_type = "UnsignedSignatureEvent")]
// wasm-bindgen cannot take `Option<&str>`.
#[allow(clippy::needless_pass_by_value)]
pub fn signature_template(
    card_hash_hex: &str,
    pubkey: &str,
    #[wasm_bindgen(unchecked_param_type = "number")] created_at: JsValue,
    lightning_address: Option<String>,
) -> Result<JsValue, JsValue> {
    let run = || -> Result<UnsignedSignatureEvent, JsError> {
        let created_at: u32 = serde_wasm_bindgen::from_value(created_at)
            .map_err(|error| JsError::created_at(&error))?;
        let template = signature::unsigned_signature_template(
            card_hash_hex,
            pubkey,
            u64::from(created_at),
            lightning_address.as_deref(),
        )?;
        Ok(UnsignedSignatureEvent {
            pubkey: template.pubkey.to_hex(),
            created_at: template.created_at.as_secs(),
            kind: template.kind.as_u16(),
            tags: template
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect(),
            content: template.content,
        })
    };
    export(run())
}

/// Verifies a signed event JSON over a card and returns the signer: their
/// pubkey, and for a version 2 card the payout address they signed.
///
/// # Errors
///
/// Throws `invalid_card_json`, `invalid_event_json` or the first failed
/// signature check.
#[wasm_bindgen(unchecked_return_type = "Signer")]
pub fn verify_signature(card_json: &str, event_json: &str) -> Result<JsValue, JsValue> {
    let run = || -> Result<Signer, JsError> {
        let card = parse_card(card_json)?;
        let event =
            serde_json::from_str(event_json).map_err(|error| JsError::event_json(&error))?;
        Ok(signature::verify_signature_event(&card, &event)?.into())
    };
    export(run())
}

/// Computes a card's signing status from a JSON array of signed events.
///
/// Invalid events are listed in `rejected` in input order and do not count.
///
/// # Errors
///
/// Throws `invalid_card_json`, `invalid_event_json` or a validation error.
#[wasm_bindgen(unchecked_return_type = "CardStatus")]
pub fn card_status(card_json: &str, events_json: &str) -> Result<JsValue, JsValue> {
    let run = || -> Result<CardStatus, JsError> {
        let card = parse_card(card_json)?;
        let events: Vec<_> =
            serde_json::from_str(events_json).map_err(|error| JsError::event_json(&error))?;
        let report = status::card_status(&card, &events)?;
        let rejected = report
            .rejected
            .into_iter()
            .map(|(event_id, error)| {
                let error = JsError::from(error);
                RejectedEvent {
                    event_id: event_id.to_hex(),
                    code: error.code,
                    message: error.message,
                }
            })
            .collect();
        let signers =
            |signed: Vec<signature::Signer>| signed.into_iter().map(Signer::from).collect();
        Ok(match report.status {
            status::CardStatus::Draft { signed, missing } => CardStatus::Draft {
                signed: signers(signed),
                missing,
                rejected,
            },
            status::CardStatus::Active { signed } => CardStatus::Active {
                signed: signers(signed),
                rejected,
            },
        })
    };
    export(run())
}

/// Checks that a new card validly replaces an old one, given a JSON array of
/// signed events over the new card.
///
/// # Errors
///
/// Throws `invalid_card_json`, `invalid_event_json` or the first failed
/// replacement rule.
#[wasm_bindgen]
pub fn check_supersede(
    old_card_json: &str,
    new_card_json: &str,
    events_json: &str,
) -> Result<(), JsValue> {
    let run = || -> Result<(), JsError> {
        let old = parse_card(old_card_json)?;
        let new = parse_card(new_card_json)?;
        let events: Vec<_> =
            serde_json::from_str(events_json).map_err(|error| JsError::event_json(&error))?;
        Ok(supersede::check_supersede(&old, &new, &events)?)
    };
    run().map_err(throw)
}

#[derive(Serialize)]
struct Payout {
    pubkey: String,
    amount_sats: String,
}

#[derive(Serialize)]
struct UnsignedSignatureEvent {
    pubkey: String,
    created_at: u64,
    kind: u16,
    tags: Vec<Vec<String>>,
    content: String,
}

#[derive(Serialize)]
struct Signer {
    pubkey: String,
    lightning_address: Option<String>,
}

impl From<signature::Signer> for Signer {
    fn from(signer: signature::Signer) -> Self {
        Self {
            pubkey: signer.pubkey,
            lightning_address: signer.lightning_address,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum CardStatus {
    Draft {
        signed: Vec<Signer>,
        missing: Vec<String>,
        rejected: Vec<RejectedEvent>,
    },
    Active {
        signed: Vec<Signer>,
        rejected: Vec<RejectedEvent>,
    },
}

#[derive(Serialize)]
struct RejectedEvent {
    event_id: String,
    code: &'static str,
    message: String,
}

fn parse_card(card_json: &str) -> Result<Card, JsError> {
    Card::from_json(card_json).map_err(|error| JsError::card_json(&error))
}

/// Accepts only canonical decimal integers: digits, no leading zeros, fits in `u64`.
fn parse_sats(value: &str) -> Result<u64, JsError> {
    let canonical = !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'));
    if !canonical {
        return Err(JsError::amount(value));
    }
    value.parse().map_err(|_| JsError::amount(value))
}

fn export<T: Serialize>(result: Result<T, JsError>) -> Result<JsValue, JsValue> {
    let value = result.map_err(throw)?;
    value
        .serialize(&Serializer::json_compatible())
        .map_err(|error| throw(JsError::output(&error)))
}

fn throw(error: JsError) -> JsValue {
    // `JsError` holds only strings, small integers and null, so this cannot
    // fail in practice; the string fallback avoids a panic path.
    match error.serialize(&Serializer::json_compatible()) {
        Ok(value) => value,
        Err(_) => JsValue::from(error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::{error::ERROR_CODES, parse_sats};

    // The `typescript_custom_section` const is consumed by the macro, so read the file again.
    const TS_TYPES: &str = include_str!("types.d.ts");

    #[test]
    fn typescript_error_code_union_matches_error_codes() {
        let union = TS_TYPES
            .split("export type ErrorCode =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .unwrap_or_else(|| panic!("types.d.ts must declare ErrorCode"));
        let declared: Vec<&str> = union
            .split('|')
            .map(|member| member.trim().trim_matches('"'))
            .filter(|member| !member.is_empty())
            .collect();
        assert_eq!(declared, ERROR_CODES);
    }

    #[test]
    fn error_codes_are_unique_snake_case() {
        let mut seen = std::collections::BTreeSet::new();
        for code in ERROR_CODES {
            assert!(seen.insert(code), "duplicate code {code}");
            assert!(
                code.starts_with(|c: char| c.is_ascii_lowercase())
                    && code.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    }),
                "{code} is not snake_case"
            );
        }
    }

    #[test]
    fn sats_parse_only_plain_integers() {
        assert_eq!(parse_sats("0").ok(), Some(0));
        assert_eq!(parse_sats("100001").ok(), Some(100_001));
        assert_eq!(parse_sats("18446744073709551615").ok(), Some(u64::MAX));
        for bad in [
            "",
            "1e3",
            "-1",
            "+1",
            " 5",
            "5 ",
            "1.0",
            "1_000",
            "0x10",
            "007",
            "١",
            "18446744073709551616",
        ] {
            assert!(
                parse_sats(bad).is_err_and(|error| error.code == "invalid_amount"),
                "{bad:?} must be rejected"
            );
        }
    }
}
