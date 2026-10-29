//! Nostr signature templates and verification for Split Card cards.

use nostr::{
    event::{Event, Kind, Tag, UnsignedEvent},
    key::PublicKey,
    types::Timestamp,
};
use thiserror::Error;

use crate::{card::Card, hash::card_hash_hex, validate::is_lower_hex};

/// Addressable event kind for Split Card signatures.
///
/// The final kind is undecided.
pub const SIGNATURE_KIND: u16 = 30_078;

/// Errors while constructing an unsigned signature event template.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignatureTemplateError {
    /// The supplied card hash was not lowercase hexadecimal SHA-256.
    #[error("card hash must be exactly 64 lowercase hexadecimal characters")]
    InvalidCardHash,
    /// The public key was not 64 lowercase hexadecimal characters encoding a
    /// valid secp256k1 x-only point.
    #[error("invalid Nostr public key")]
    InvalidPubkey,
    /// A required tag could not be constructed.
    #[error("could not construct signature tag")]
    TagConstruction,
}

/// Errors encountered while checking a member signature event.
#[derive(Debug, Error)]
pub enum SignatureError {
    /// The event's ID does not match its serialized fields.
    #[error("event ID is invalid")]
    InvalidEventId,
    /// The event's Schnorr signature is invalid.
    #[error("event Schnorr signature is invalid")]
    InvalidSignature,
    /// The event kind is not the Split Card signature kind.
    #[error("event kind is not {SIGNATURE_KIND}")]
    WrongKind,
    /// The event has no `d` tag.
    #[error("event is missing its d tag")]
    MissingDTag,
    /// The event has more than one `d` tag.
    #[error("event has multiple d tags")]
    MultipleDTags,
    /// The event's `d` tag does not identify this card hash.
    #[error("event d tag has the wrong card hash")]
    WrongDTagHash,
    /// The event has no `splitcard` tag.
    #[error("event is missing its splitcard tag")]
    MissingSplitcardTag,
    /// The event has more than one `splitcard` tag.
    #[error("event has multiple splitcard tags")]
    MultipleSplitcardTags,
    /// The event's `splitcard` tag does not identify this card hash.
    #[error("event splitcard tag has the wrong card hash")]
    WrongSplitcardTagHash,
    /// The event signer is not a member of this card.
    #[error("event signer is not a member of this card")]
    NonMember,
    /// The card hash could not be computed.
    #[error("could not hash card: {0}")]
    CardHash(#[from] crate::error::HashError),
}

/// Builds the unsigned Nostr event members sign for a card hash.
///
/// # Errors
///
/// Returns an error if the hash or public key is malformed.
pub fn unsigned_signature_template(
    card_hash_hex: &str,
    pubkey: &str,
    created_at: u64,
) -> Result<UnsignedEvent, SignatureTemplateError> {
    if !is_lower_hex(card_hash_hex, 64) {
        return Err(SignatureTemplateError::InvalidCardHash);
    }
    // `PublicKey::from_hex` only decodes bytes, so `xonly` checks the curve point.
    let public_key = PublicKey::from_hex(pubkey)
        .ok()
        .filter(|key| is_lower_hex(pubkey, 64) && key.xonly().is_ok())
        .ok_or(SignatureTemplateError::InvalidPubkey)?;
    let d_tag =
        Tag::parse(["d", card_hash_hex]).map_err(|_| SignatureTemplateError::TagConstruction)?;
    let splitcard_tag = Tag::parse(["splitcard", card_hash_hex])
        .map_err(|_| SignatureTemplateError::TagConstruction)?;
    Ok(UnsignedEvent::new(
        public_key,
        Timestamp::from_secs(created_at),
        Kind::from_u16(SIGNATURE_KIND),
        [d_tag, splitcard_tag],
        "",
    ))
}

/// Verifies a signature event and returns its member public key in lowercase hex.
///
/// Checks the event ID, Schnorr signature, kind, exactly one matching `d` tag,
/// exactly one matching `splitcard` tag, and card membership in that order.
///
/// # Errors
///
/// Returns the first failed signature invariant.
pub fn verify_signature_event(card: &Card, event: &Event) -> Result<String, SignatureError> {
    let card_hash = card_hash_hex(card)?;
    verify_event_for_hash(event, &card_hash)?;
    let pubkey = event.pubkey.to_hex();
    if !card.members.iter().any(|member| member.pubkey == pubkey) {
        return Err(SignatureError::NonMember);
    }
    Ok(pubkey)
}

/// Runs every check except card membership, so supersede can verify removed members.
pub(crate) fn verify_event_for_hash(
    event: &Event,
    card_hash: &str,
) -> Result<String, SignatureError> {
    if !event.verify_id() {
        return Err(SignatureError::InvalidEventId);
    }
    if !event.verify_signature() {
        return Err(SignatureError::InvalidSignature);
    }
    if event.kind.as_u16() != SIGNATURE_KIND {
        return Err(SignatureError::WrongKind);
    }
    let d_tags: Vec<&Tag> = event.tags.iter().filter(|tag| tag.kind() == "d").collect();
    if d_tags.is_empty() {
        return Err(SignatureError::MissingDTag);
    }
    if d_tags.len() != 1 {
        return Err(SignatureError::MultipleDTags);
    }
    if d_tags[0].content() != Some(card_hash) {
        return Err(SignatureError::WrongDTagHash);
    }
    let splitcard_tags: Vec<&Tag> = event
        .tags
        .iter()
        .filter(|tag| tag.kind() == "splitcard")
        .collect();
    if splitcard_tags.is_empty() {
        return Err(SignatureError::MissingSplitcardTag);
    }
    if splitcard_tags.len() != 1 {
        return Err(SignatureError::MultipleSplitcardTags);
    }
    if splitcard_tags[0].content() != Some(card_hash) {
        return Err(SignatureError::WrongSplitcardTagHash);
    }
    Ok(event.pubkey.to_hex())
}
