//! Conversion of core and boundary failures into structured JS errors.
//!
//! Every match below is exhaustive with no wildcard arm, so a new core error
//! variant fails to compile until it is given a code here, in [`ERROR_CODES`]
//! and in the TypeScript `ErrorCode` union.

use serde::Serialize;
use serde_json::{Value, json};
use splitcard_core::{
    error::{HashError, PayoutError, SplitError, ValidationError},
    signature::{SignatureError, SignatureTemplateError},
    status::StatusError,
    supersede::SupersedeError,
};

/// Every error code this crate can throw, in the order of the TypeScript union.
pub const ERROR_CODES: &[&str] = &[
    // Boundary failures.
    "invalid_card_json",
    "invalid_event_json",
    "invalid_amount",
    "invalid_created_at",
    "output_serialization",
    // Keys and signing.
    "invalid_secret_key",
    "invalid_aux_rand",
    "invalid_unsigned_event",
    "event_pubkey_mismatch",
    // `ValidationError`.
    "unsupported_version",
    "empty_field",
    "invalid_member_count",
    "invalid_pubkey",
    "duplicate_pubkey",
    "empty_member_name",
    "zero_weight",
    "weight_too_large",
    "invalid_lightning_address",
    "invalid_supersedes",
    "invalid_peer_review_max_shift_points",
    // `SplitError`.
    "no_weights",
    "split_zero_weight",
    "prize_too_small",
    // `PayoutError`.
    "share_rounds_to_zero",
    // `HashError`.
    "hash_serialize",
    // `SignatureTemplateError`.
    "invalid_card_hash",
    "invalid_template_pubkey",
    "tag_construction",
    // `SignatureError`.
    "invalid_event_id",
    "invalid_signature",
    "wrong_kind",
    "missing_d_tag",
    "multiple_d_tags",
    "wrong_d_tag_hash",
    "missing_splitcard_tag",
    "multiple_splitcard_tags",
    "wrong_splitcard_tag_hash",
    "non_member",
    // `SupersedeError`.
    "invalid_old_card",
    "invalid_new_card",
    "wrong_supersedes_hash",
    "missing_new_member_signatures",
    "missing_old_member_signatures",
];

/// The `{ code, message, details }` object thrown to JavaScript.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JsError {
    /// Stable `snake_case` identifier, one of [`ERROR_CODES`].
    pub code: &'static str,
    /// The underlying error's display text.
    pub message: String,
    /// Variant fields, or `null`. Any `u64` is a decimal string.
    pub details: Value,
}

impl JsError {
    fn new(code: &'static str, message: &impl ToString, details: Value) -> Self {
        Self {
            code,
            message: message.to_string(),
            details,
        }
    }

    /// The card JSON did not parse into a card.
    pub(crate) fn card_json(error: &serde_json::Error) -> Self {
        Self::new("invalid_card_json", error, json_position(error))
    }

    /// The event JSON did not parse into a signed event or array of events.
    pub(crate) fn event_json(error: &serde_json::Error) -> Self {
        Self::new("invalid_event_json", error, json_position(error))
    }

    /// A sat amount was not a plain non-negative integer string that fits in `u64`.
    pub(crate) fn amount(value: &str) -> Self {
        Self::new(
            "invalid_amount",
            &"amount must be a non-negative integer string with no sign, spaces, \
             decimal point, exponent or leading zeros, at most 18446744073709551615",
            json!({ "value": value }),
        )
    }

    /// `created_at` was not a whole number of seconds from 0 to 4294967295.
    pub(crate) fn created_at(error: &serde_wasm_bindgen::Error) -> Self {
        Self::new("invalid_created_at", error, Value::Null)
    }

    /// A secret key was not 64 hex characters or an `nsec`, or is out of range.
    pub(crate) fn secret_key() -> Self {
        Self::new(
            "invalid_secret_key",
            &"secret key must be 64 hex characters or an nsec, and a valid secp256k1 key",
            Value::Null,
        )
    }

    /// The auxiliary randomness for a signature was not exactly 32 bytes.
    pub(crate) fn aux_rand(len: usize) -> Self {
        Self::new(
            "invalid_aux_rand",
            &"aux_rand must be exactly 32 bytes from crypto.getRandomValues",
            json!({ "length": len }),
        )
    }

    /// An unsigned event did not parse, or one of its tags is invalid.
    pub(crate) fn unsigned_event(error: &impl ToString) -> Self {
        Self::new("invalid_unsigned_event", error, Value::Null)
    }

    /// An unsigned event names a pubkey other than the signing key's.
    pub(crate) fn event_pubkey_mismatch(expected: &str, actual: &str) -> Self {
        Self::new(
            "event_pubkey_mismatch",
            &"the event's pubkey is not the signing key's",
            json!({ "expected": expected, "actual": actual }),
        )
    }

    /// A successful result could not be converted to a JS value.
    pub(crate) fn output(error: &serde_wasm_bindgen::Error) -> Self {
        Self::new("output_serialization", error, Value::Null)
    }
}

fn json_position(error: &serde_json::Error) -> Value {
    json!({ "line": error.line(), "column": error.column() })
}

impl From<ValidationError> for JsError {
    fn from(error: ValidationError) -> Self {
        let (code, details) = match &error {
            ValidationError::UnsupportedVersion { version } => {
                ("unsupported_version", json!({ "version": version }))
            }
            ValidationError::EmptyField { field } => ("empty_field", json!({ "field": field })),
            ValidationError::InvalidMemberCount { count } => {
                ("invalid_member_count", json!({ "count": count }))
            }
            ValidationError::InvalidPubkey { index } => {
                ("invalid_pubkey", json!({ "index": index }))
            }
            ValidationError::DuplicatePubkey { index } => {
                ("duplicate_pubkey", json!({ "index": index }))
            }
            ValidationError::EmptyMemberName { index } => {
                ("empty_member_name", json!({ "index": index }))
            }
            ValidationError::ZeroWeight { index } => ("zero_weight", json!({ "index": index })),
            ValidationError::WeightTooLarge { index, weight } => (
                "weight_too_large",
                json!({ "index": index, "weight": weight.to_string() }),
            ),
            ValidationError::InvalidLightningAddress { index } => {
                ("invalid_lightning_address", json!({ "index": index }))
            }
            ValidationError::InvalidSupersedes => ("invalid_supersedes", Value::Null),
            ValidationError::InvalidPeerReviewMaxShiftPoints => {
                ("invalid_peer_review_max_shift_points", Value::Null)
            }
        };
        Self::new(code, &error, details)
    }
}

impl From<SplitError> for JsError {
    fn from(error: SplitError) -> Self {
        let (code, details) = match &error {
            SplitError::NoWeights => ("no_weights", Value::Null),
            SplitError::ZeroWeight { index } => ("split_zero_weight", json!({ "index": index })),
            SplitError::PrizeTooSmall {
                prize_sats,
                member_count,
            } => (
                "prize_too_small",
                json!({ "prize_sats": prize_sats.to_string(), "member_count": member_count }),
            ),
        };
        Self::new(code, &error, details)
    }
}

impl From<PayoutError> for JsError {
    fn from(error: PayoutError) -> Self {
        let message = error.to_string();
        match error {
            PayoutError::Validation(inner) => inner.into(),
            PayoutError::Split(inner) => inner.into(),
            PayoutError::ShareRoundsToZero { index, pubkey } => Self::new(
                "share_rounds_to_zero",
                &message,
                json!({ "index": index, "pubkey": pubkey }),
            ),
        }
    }
}

impl From<HashError> for JsError {
    fn from(error: HashError) -> Self {
        match &error {
            HashError::Serialize(_) => Self::new("hash_serialize", &error, Value::Null),
        }
    }
}

impl From<SignatureTemplateError> for JsError {
    fn from(error: SignatureTemplateError) -> Self {
        let code = match &error {
            SignatureTemplateError::InvalidCardHash => "invalid_card_hash",
            SignatureTemplateError::InvalidPubkey => "invalid_template_pubkey",
            SignatureTemplateError::TagConstruction => "tag_construction",
        };
        Self::new(code, &error, Value::Null)
    }
}

impl From<SignatureError> for JsError {
    fn from(error: SignatureError) -> Self {
        let message = error.to_string();
        let code = match error {
            SignatureError::InvalidEventId => "invalid_event_id",
            SignatureError::InvalidSignature => "invalid_signature",
            SignatureError::WrongKind => "wrong_kind",
            SignatureError::MissingDTag => "missing_d_tag",
            SignatureError::MultipleDTags => "multiple_d_tags",
            SignatureError::WrongDTagHash => "wrong_d_tag_hash",
            SignatureError::MissingSplitcardTag => "missing_splitcard_tag",
            SignatureError::MultipleSplitcardTags => "multiple_splitcard_tags",
            SignatureError::WrongSplitcardTagHash => "wrong_splitcard_tag_hash",
            SignatureError::NonMember => "non_member",
            SignatureError::CardHash(inner) => return inner.into(),
        };
        Self::new(code, &message, Value::Null)
    }
}

impl From<StatusError> for JsError {
    fn from(error: StatusError) -> Self {
        match error {
            StatusError::Validation(inner) => inner.into(),
            StatusError::Hash(inner) => inner.into(),
        }
    }
}

impl From<SupersedeError> for JsError {
    fn from(error: SupersedeError) -> Self {
        let message = error.to_string();
        let (code, details) = match error {
            SupersedeError::InvalidOldCard(inner) => ("invalid_old_card", cause(inner)),
            SupersedeError::InvalidNewCard(inner) => ("invalid_new_card", cause(inner)),
            SupersedeError::Hash(inner) => return inner.into(),
            SupersedeError::WrongSupersedesHash => ("wrong_supersedes_hash", Value::Null),
            SupersedeError::MissingNewMemberSignatures { missing } => (
                "missing_new_member_signatures",
                json!({ "missing": missing }),
            ),
            SupersedeError::MissingOldMemberSignatures { missing } => (
                "missing_old_member_signatures",
                json!({ "missing": missing }),
            ),
        };
        Self::new(code, &message, details)
    }
}

/// Nests the validation error that made a card invalid as `details.cause`.
fn cause(inner: ValidationError) -> Value {
    let inner = JsError::from(inner);
    json!({ "cause": { "code": inner.code, "message": inner.message, "details": inner.details } })
}
