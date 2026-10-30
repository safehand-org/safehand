//! Nostr signature templates and verification for Split Card cards.
//!
//! A signature event over a version 2 card carries exactly one
//! `["lud16", "<lightning address>"]` tag: the signer's payout address,
//! signed by their own key so nobody else can change where their share goes.
//! A signature over a version 1 card must not carry one, because its payout
//! addresses are in the card.

use nostr::{
    event::{Event, Kind, Tag, UnsignedEvent},
    key::PublicKey,
    types::Timestamp,
};
use thiserror::Error;

use crate::{
    card::Card,
    hash::card_hash_hex,
    validate::{is_lightning_address, is_lower_hex},
};

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
    /// The Lightning Address was not a lowercase LUD-16 address.
    #[error("lightning address must be a lowercase LUD-16 address")]
    InvalidLightningAddress,
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
    /// A signature over a version 2 card has no `lud16` tag.
    #[error("event is missing its lud16 tag")]
    MissingLud16Tag,
    /// A signature over a version 2 card has more than one `lud16` tag.
    #[error("event has multiple lud16 tags")]
    MultipleLud16Tags,
    /// The `lud16` tag is not a lowercase LUD-16 Lightning Address.
    #[error("event lud16 tag is not a lowercase LUD-16 Lightning Address")]
    InvalidLud16,
    /// A signature over a version 1 card has a `lud16` tag; version 1 payout
    /// addresses are in the card.
    #[error("event has a lud16 tag, but version 1 cards hold payout addresses in the card")]
    UnexpectedLud16Tag,
    /// The event signer is not a member of this card.
    #[error("event signer is not a member of this card")]
    NonMember,
    /// The card hash could not be computed.
    #[error("could not hash card: {0}")]
    CardHash(#[from] crate::error::HashError),
}

/// A verified signer of a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    /// The signer's public key in lowercase hex.
    pub pubkey: String,
    /// The payout address from the event's `lud16` tag: always present for a
    /// version 2 card, `None` for version 1.
    pub lightning_address: Option<String>,
}

/// Builds the unsigned Nostr event members sign for a card hash.
///
/// Pass `lightning_address` (the signer's payout address, as a `lud16` tag)
/// for a version 2 card, and `None` for version 1. The template only knows
/// the hash, so [`verify_signature_event`] is what enforces this per version.
///
/// # Errors
///
/// Returns an error if the hash, public key or Lightning Address is malformed.
pub fn unsigned_signature_template(
    card_hash_hex: &str,
    pubkey: &str,
    created_at: u64,
    lightning_address: Option<&str>,
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
    let mut tags = vec![d_tag, splitcard_tag];
    if let Some(address) = lightning_address {
        if !is_lightning_address(address) {
            return Err(SignatureTemplateError::InvalidLightningAddress);
        }
        tags.push(
            Tag::parse(["lud16", address]).map_err(|_| SignatureTemplateError::TagConstruction)?,
        );
    }
    Ok(UnsignedEvent::new(
        public_key,
        Timestamp::from_secs(created_at),
        Kind::from_u16(SIGNATURE_KIND),
        tags,
        "",
    ))
}

/// Verifies a signature event and returns its signer and payout address.
///
/// Checks the event ID, Schnorr signature, kind, exactly one matching `d` tag,
/// exactly one matching `splitcard` tag, the `lud16` rule for the card's
/// version, and card membership in that order.
///
/// # Errors
///
/// Returns the first failed signature invariant.
pub fn verify_signature_event(card: &Card, event: &Event) -> Result<Signer, SignatureError> {
    let card_hash = card_hash_hex(card)?;
    verify_event_for_hash(event, &card_hash)?;
    let lightning_address = signed_lightning_address(card, event)?;
    let pubkey = event.pubkey.to_hex();
    if !card.is_member(&pubkey) {
        return Err(SignatureError::NonMember);
    }
    Ok(Signer {
        pubkey,
        lightning_address,
    })
}

/// Applies the `lud16` rule for `card`'s version: exactly one valid tag for
/// version 2, which is returned; none for version 1.
pub(crate) fn signed_lightning_address(
    card: &Card,
    event: &Event,
) -> Result<Option<String>, SignatureError> {
    let lud16_tags: Vec<&Tag> = event
        .tags
        .iter()
        .filter(|tag| tag.kind() == "lud16")
        .collect();
    match card {
        Card::V1(_) if lud16_tags.is_empty() => Ok(None),
        Card::V1(_) => Err(SignatureError::UnexpectedLud16Tag),
        Card::V2(_) => match lud16_tags.as_slice() {
            [] => Err(SignatureError::MissingLud16Tag),
            [tag] => tag
                .content()
                .filter(|address| is_lightning_address(address) && tag.as_slice().len() == 2)
                .map(|address| Some(address.to_owned()))
                .ok_or(SignatureError::InvalidLud16),
            _ => Err(SignatureError::MultipleLud16Tags),
        },
    }
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
