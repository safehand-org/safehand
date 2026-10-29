//! Aggregate status of member signatures for one card.

use std::collections::BTreeSet;

use nostr::event::{Event, EventId};
use thiserror::Error;

use crate::{
    card::Card,
    error::ValidationError,
    signature::{SignatureError, verify_signature_event},
    validate::validate,
};

/// Whether a card is still awaiting signatures or fully signed.
#[derive(Debug, PartialEq, Eq)]
pub enum CardStatus {
    /// Some members are still missing signatures.
    Draft {
        /// Valid signer public keys in card member order.
        signed: Vec<String>,
        /// Missing member public keys in card member order.
        missing: Vec<String>,
    },
    /// Every card member has signed this card hash.
    Active {
        /// Valid signer public keys in card member order.
        signed: Vec<String>,
    },
}

/// Card status together with invalid events in input order.
#[derive(Debug)]
pub struct StatusReport {
    /// The draft or active status.
    pub status: CardStatus,
    /// Rejected event IDs and their verification errors, in input order.
    pub rejected: Vec<(EventId, SignatureError)>,
}

/// Errors while computing card status.
#[derive(Debug, Error)]
pub enum StatusError {
    /// The card failed Phase 1 validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// Hashing the card failed.
    #[error(transparent)]
    Hash(#[from] crate::error::HashError),
}

/// Computes card status from candidate signature events.
///
/// Invalid events are rejected in input order and do not count toward status.
///
/// # Errors
///
/// Returns an error if the card itself is invalid or cannot be hashed.
pub fn card_status(card: &Card, events: &[Event]) -> Result<StatusReport, StatusError> {
    validate(card)?;
    let mut signed_keys = BTreeSet::new();
    let mut rejected = Vec::new();
    for event in events {
        match verify_signature_event(card, event) {
            Ok(pubkey) => {
                signed_keys.insert(pubkey);
            }
            Err(error) => rejected.push((event.id, error)),
        }
    }
    let signed: Vec<String> = card
        .members
        .iter()
        .filter(|member| signed_keys.contains(&member.pubkey))
        .map(|member| member.pubkey.clone())
        .collect();
    let missing: Vec<String> = card
        .members
        .iter()
        .filter(|member| !signed_keys.contains(&member.pubkey))
        .map(|member| member.pubkey.clone())
        .collect();
    let status = if missing.is_empty() {
        CardStatus::Active { signed }
    } else {
        CardStatus::Draft { signed, missing }
    };
    Ok(StatusReport { status, rejected })
}
