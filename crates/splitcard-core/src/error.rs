//! Typed errors returned by Split Card core operations.

use thiserror::Error;

/// Failure while constructing canonical JSON or hashing a card.
#[derive(Debug, Error)]
pub enum HashError {
    /// The card could not be converted into JSON.
    #[error("could not serialize card as JSON: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// A rule violation in a card.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The schema version is unsupported.
    #[error("unsupported card version {version}")]
    UnsupportedVersion {
        /// Received schema version.
        version: u32,
    },
    /// A required card text field was empty.
    #[error("{field} must not be empty")]
    EmptyField {
        /// The empty field's name.
        field: &'static str,
    },
    /// The member count was outside the allowed range.
    #[error("member count {count} is outside the allowed range of 2 to 20")]
    InvalidMemberCount {
        /// Received member count.
        count: usize,
    },
    /// A member Nostr public key was malformed.
    #[error("member {index} has an invalid Nostr public key")]
    InvalidPubkey {
        /// Index of the malformed member.
        index: usize,
    },
    /// Two members used the same Nostr public key.
    #[error("member {index} duplicates a prior Nostr public key")]
    DuplicatePubkey {
        /// Index of the duplicate member.
        index: usize,
    },
    /// A member name was empty.
    #[error("member {index} name must not be empty")]
    EmptyMemberName {
        /// Index of the member.
        index: usize,
    },
    /// A member weight was zero.
    #[error("member {index} weight must be greater than zero")]
    ZeroWeight {
        /// Index of the member.
        index: usize,
    },
    /// A member weight exceeded [`MAX_WEIGHT`](crate::validate::MAX_WEIGHT).
    #[error(
        "member {index} weight {weight} exceeds the maximum of {max}",
        max = crate::validate::MAX_WEIGHT
    )]
    WeightTooLarge {
        /// Index of the member.
        index: usize,
        /// The rejected weight.
        weight: u64,
    },
    /// A member Lightning Address was malformed.
    #[error("member {index} has an invalid Lightning Address")]
    InvalidLightningAddress {
        /// Index of the member.
        index: usize,
    },
    /// A version 2 member label was empty, too long or held a control character.
    #[error(
        "member {index} label must be 1 to {max} characters with no control characters",
        max = crate::validate::MAX_LABEL_CHARS
    )]
    InvalidMemberLabel {
        /// Index of the member.
        index: usize,
    },
    /// The superseded-card hash was malformed.
    #[error("supersedes must be a lowercase hexadecimal SHA-256 hash")]
    InvalidSupersedes,
    /// A peer-review maximum was outside the allowed range.
    #[error("peer review max_shift_points must be between 1 and 50")]
    InvalidPeerReviewMaxShiftPoints,
    /// A `keep_percent` leave rule was outside 1 to 99.
    #[error("leave keep_percent {keep_percent} must be between 1 and 99")]
    InvalidLeaveKeepPercent {
        /// The rejected percentage.
        keep_percent: u32,
    },
}

/// Failure while calculating a weighted split.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SplitError {
    /// No weights were supplied.
    #[error("at least one weight is required")]
    NoWeights,
    /// A weight was zero.
    #[error("weight at index {index} must be greater than zero")]
    ZeroWeight {
        /// Index of the zero weight.
        index: usize,
    },
    /// The prize has fewer sats than recipients.
    #[error("prize of {prize_sats} sats is smaller than recipient count {member_count}")]
    PrizeTooSmall {
        /// Prize size in sats.
        prize_sats: u64,
        /// Number of recipients.
        member_count: usize,
    },
}

/// Failure while calculating payouts for a card.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PayoutError {
    /// The card failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// The card's weights could not be split.
    #[error(transparent)]
    Split(#[from] SplitError),
    /// A member's share of the prize rounded down to zero sats.
    #[error("member {index} ({pubkey}) would receive 0 sats")]
    ShareRoundsToZero {
        /// Index of the first member whose share is zero.
        index: usize,
        /// That member's public key.
        pubkey: String,
    },
}

/// Failure while pairing members with their payout addresses.
#[derive(Debug, Error)]
pub enum PayoutAddressError {
    /// The card failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A version 2 member has no valid signature, so no signed address.
    #[error("member {index} ({pubkey}) has not signed, so has no payout address")]
    MissingSignature {
        /// Index of the first member without a valid signature.
        index: usize,
        /// That member's public key.
        pubkey: String,
    },
}
