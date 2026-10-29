//! Validation for replacing one card with another.

use std::collections::BTreeSet;

use nostr::event::Event;
use thiserror::Error;

use crate::{
    card::Card,
    error::{HashError, ValidationError},
    hash::card_hash_hex,
    signature::{SignatureError, verify_event_for_hash},
    validate::validate,
};

/// Verifies one signature event over a superseding card and returns its
/// signer public key in lowercase hex.
///
/// Like [`verify_signature_event`](crate::signature::verify_signature_event)
/// against `new`, except the signer may be a member of either card: removed
/// members of `old` must sign the new card too. The caller must pass the card
/// `new` supersedes as `old`; [`check_supersede`] checks that link.
///
/// # Errors
///
/// Returns the first failed signature invariant, or
/// [`SignatureError::NonMember`] if the signer is in neither card.
pub fn verify_supersede_signature_event(
    old: &Card,
    new: &Card,
    event: &Event,
) -> Result<String, SignatureError> {
    let new_hash = card_hash_hex(new)?;
    let pubkey = verify_event_for_hash(event, &new_hash)?;
    let is_member = |card: &Card| card.members.iter().any(|member| member.pubkey == pubkey);
    if is_member(new) || is_member(old) {
        Ok(pubkey)
    } else {
        Err(SignatureError::NonMember)
    }
}

/// Errors that prevent a card replacement from taking effect.
#[derive(Debug, Error)]
pub enum SupersedeError {
    /// The old card is invalid.
    #[error("old card is invalid: {0}")]
    InvalidOldCard(ValidationError),
    /// The new card is invalid.
    #[error("new card is invalid: {0}")]
    InvalidNewCard(ValidationError),
    /// One of the cards could not be hashed.
    #[error(transparent)]
    Hash(#[from] HashError),
    /// The new card points at the wrong previous card hash.
    #[error("new card has the wrong supersedes hash")]
    WrongSupersedesHash,
    /// New-card members missing signatures over the new card.
    #[error("new card members missing signatures: {missing:?}")]
    MissingNewMemberSignatures {
        /// Missing new-card member public keys.
        missing: Vec<String>,
    },
    /// Old-card members missing signatures over the new card.
    #[error("old card members missing signatures: {missing:?}")]
    MissingOldMemberSignatures {
        /// Missing old-card member public keys.
        missing: Vec<String>,
    },
}

/// Checks whether a new card is unanimously signed by old and new members.
///
/// Removed members are intentionally checked cryptographically against the new
/// card hash even though they are not members of the new card.
///
/// # Errors
///
/// Returns the first failed replacement rule.
pub fn check_supersede(old: &Card, new: &Card, new_events: &[Event]) -> Result<(), SupersedeError> {
    validate(old).map_err(SupersedeError::InvalidOldCard)?;
    validate(new).map_err(SupersedeError::InvalidNewCard)?;
    let old_hash = card_hash_hex(old)?;
    if new.supersedes.as_deref() != Some(old_hash.as_str()) {
        return Err(SupersedeError::WrongSupersedesHash);
    }
    let new_hash = card_hash_hex(new)?;
    let mut signers = BTreeSet::new();
    for event in new_events {
        if let Ok(pubkey) = verify_event_for_hash(event, &new_hash) {
            signers.insert(pubkey);
        }
    }
    let missing_new: Vec<String> = new
        .members
        .iter()
        .filter(|member| !signers.contains(&member.pubkey))
        .map(|member| member.pubkey.clone())
        .collect();
    if !missing_new.is_empty() {
        return Err(SupersedeError::MissingNewMemberSignatures {
            missing: missing_new,
        });
    }
    let missing_old: Vec<String> = old
        .members
        .iter()
        .filter(|member| !signers.contains(&member.pubkey))
        .map(|member| member.pubkey.clone())
        .collect();
    if !missing_old.is_empty() {
        return Err(SupersedeError::MissingOldMemberSignatures {
            missing: missing_old,
        });
    }
    Ok(())
}
