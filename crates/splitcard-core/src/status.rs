//! Aggregate status of member signatures for one card.

use std::collections::BTreeMap;

use nostr::event::{Event, EventId};
use thiserror::Error;

use crate::{
    card::Card,
    error::ValidationError,
    signature::{SignatureError, Signer, verify_signature_event},
    validate::validate,
};

/// Whether a card is still awaiting signatures or fully signed.
#[derive(Debug, PartialEq, Eq)]
pub enum CardStatus {
    /// Some members are still missing signatures.
    Draft {
        /// Valid signers in card member order, each with the payout address
        /// they signed (version 2 cards only).
        signed: Vec<Signer>,
        /// Missing member public keys in card member order.
        missing: Vec<String>,
    },
    /// Every card member has signed this card hash.
    Active {
        /// Valid signers in card member order, each with the payout address
        /// they signed (version 2 cards only).
        signed: Vec<Signer>,
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
/// If one member has several valid events, the first in input order counts,
/// so its payout address is the one reported.
///
/// # Errors
///
/// Returns an error if the card itself is invalid or cannot be hashed.
pub fn card_status(card: &Card, events: &[Event]) -> Result<StatusReport, StatusError> {
    validate(card)?;
    let mut signers = BTreeMap::new();
    let mut rejected = Vec::new();
    for event in events {
        match verify_signature_event(card, event) {
            Ok(signer) => {
                signers.entry(signer.pubkey.clone()).or_insert(signer);
            }
            Err(error) => rejected.push((event.id, error)),
        }
    }
    let pubkeys = card.pubkeys();
    let signed: Vec<Signer> = pubkeys
        .iter()
        .filter_map(|pubkey| signers.remove(*pubkey))
        .collect();
    let missing: Vec<String> = pubkeys
        .into_iter()
        .filter(|pubkey| !signed.iter().any(|signer| signer.pubkey == *pubkey))
        .map(str::to_owned)
        .collect();
    let status = if missing.is_empty() {
        CardStatus::Active { signed }
    } else {
        CardStatus::Draft { signed, missing }
    };
    Ok(StatusReport { status, rejected })
}
