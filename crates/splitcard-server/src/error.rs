//! JSON error responses.
//!
//! Every error is `{ code, message, details }`, the shape `splitcard-wasm`
//! throws. Core errors get exactly the WASM codes and details; the matches
//! below are exhaustive with no wildcard arm, so a new core variant fails to
//! compile until it is given a code.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};
use splitcard_core::{
    error::{HashError, PayoutAddressError, PayoutError, SplitError, ValidationError},
    signature::SignatureError,
    status::StatusError,
    supersede::SupersedeError,
};
use splitcard_ln::proof::ProofError;

/// An error response.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    /// HTTP status.
    pub status: StatusCode,
    /// Stable `snake_case` identifier.
    pub code: &'static str,
    /// Human-readable description.
    pub message: String,
    /// Variant fields, or `null`. Any `u64` is a decimal string.
    pub details: Value,
}

#[derive(Serialize)]
struct Body<'a> {
    code: &'a str,
    message: &'a str,
    details: &'a Value,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Body {
            code: self.code,
            message: &self.message,
            details: &self.details,
        };
        (self.status, Json(body)).into_response()
    }
}

impl ApiError {
    /// An error with `null` details.
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    /// The `{ code, message, details }` object, for nesting in a response.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({ "code": self.code, "message": self.message, "details": self.details })
    }

    /// Replaces the details.
    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    /// 404 with the given code.
    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    /// The request body did not parse as JSON of the expected shape.
    #[must_use]
    pub fn invalid_json(code: &'static str, error: &serde_json::Error) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, error.to_string())
            .with_details(json!({ "line": error.line(), "column": error.column() }))
    }

    /// An unexpected failure. The cause is logged, not returned.
    pub fn internal(cause: &dyn std::fmt::Display) -> Self {
        tracing::error!(%cause, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "internal server error",
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal(&error)
    }
}

impl From<ValidationError> for ApiError {
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
            ValidationError::InvalidMemberLabel { index } => {
                ("invalid_member_label", json!({ "index": index }))
            }
            ValidationError::InvalidSupersedes => ("invalid_supersedes", Value::Null),
            ValidationError::InvalidPeerReviewMaxShiftPoints => {
                ("invalid_peer_review_max_shift_points", Value::Null)
            }
            ValidationError::InvalidLeaveKeepPercent { keep_percent } => (
                "invalid_leave_keep_percent",
                json!({ "keep_percent": keep_percent }),
            ),
        };
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, error.to_string()).with_details(details)
    }
}

impl From<HashError> for ApiError {
    fn from(error: HashError) -> Self {
        match &error {
            HashError::Serialize(_) => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "hash_serialize",
                error.to_string(),
            ),
        }
    }
}

impl From<SignatureError> for ApiError {
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
            SignatureError::MissingLud16Tag => "missing_lud16_tag",
            SignatureError::MultipleLud16Tags => "multiple_lud16_tags",
            SignatureError::InvalidLud16 => "invalid_lud16",
            SignatureError::UnexpectedLud16Tag => "unexpected_lud16_tag",
            SignatureError::NonMember => "non_member",
            SignatureError::CardHash(inner) => return inner.into(),
        };
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }
}

impl From<StatusError> for ApiError {
    fn from(error: StatusError) -> Self {
        match error {
            StatusError::Validation(inner) => inner.into(),
            StatusError::Hash(inner) => inner.into(),
        }
    }
}

impl From<SupersedeError> for ApiError {
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
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message).with_details(details)
    }
}

impl From<SplitError> for ApiError {
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
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, error.to_string()).with_details(details)
    }
}

impl From<PayoutError> for ApiError {
    fn from(error: PayoutError) -> Self {
        let message = error.to_string();
        match error {
            PayoutError::Validation(inner) => inner.into(),
            PayoutError::Split(inner) => inner.into(),
            PayoutError::ShareRoundsToZero { index, pubkey } => Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "share_rounds_to_zero",
                message,
            )
            .with_details(json!({ "index": index, "pubkey": pubkey })),
        }
    }
}

impl From<PayoutAddressError> for ApiError {
    fn from(error: PayoutAddressError) -> Self {
        match error {
            PayoutAddressError::Validation(inner) => inner.into(),
            // Payout addresses are only paired for fully signed cards.
            missing @ PayoutAddressError::MissingSignature { .. } => Self::internal(&missing),
        }
    }
}

impl From<ProofError> for ApiError {
    fn from(error: ProofError) -> Self {
        let message = error.to_string();
        let (code, details) = match error {
            ProofError::InvalidPaymentHash => ("invalid_payment_hash", Value::Null),
            ProofError::InvalidPreimageLength { length } => {
                ("invalid_preimage_length", json!({ "length": length }))
            }
            ProofError::InvalidPreimageHex => ("invalid_preimage_hex", Value::Null),
            ProofError::PreimageMismatch => ("preimage_mismatch", Value::Null),
        };
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message).with_details(details)
    }
}

/// Nests the validation error that made a card invalid as `details.cause`.
fn cause(inner: ValidationError) -> Value {
    let inner = ApiError::from(inner);
    json!({ "cause": { "code": inner.code, "message": inner.message, "details": inner.details } })
}
