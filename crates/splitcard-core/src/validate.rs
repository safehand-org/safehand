//! Card validation rules.

use std::collections::BTreeSet;

use crate::{
    card::{Card, Payout},
    error::ValidationError,
};

/// Largest allowed member weight: 2^53 - 1, the largest integer a JavaScript
/// number holds exactly, so cards built in JavaScript cannot lose precision.
pub const MAX_WEIGHT: u64 = 9_007_199_254_740_991;

/// Validates every invariant required for a version-one card.
///
/// # Errors
///
/// Returns the first violated card invariant in schema order.
pub fn validate(card: &Card) -> Result<(), ValidationError> {
    if card.version != 1 {
        return Err(ValidationError::UnsupportedVersion {
            version: card.version,
        });
    }
    for (field, value) in [
        ("card_id", card.card_id.as_str()),
        ("event", card.event.as_str()),
        ("team", card.team.as_str()),
    ] {
        if value.is_empty() {
            return Err(ValidationError::EmptyField { field });
        }
    }
    if !(2..=20).contains(&card.members.len()) {
        return Err(ValidationError::InvalidMemberCount {
            count: card.members.len(),
        });
    }

    let mut pubkeys = BTreeSet::new();
    for (index, member) in card.members.iter().enumerate() {
        if !is_lower_hex(&member.pubkey, 64) {
            return Err(ValidationError::InvalidPubkey { index });
        }
        if !pubkeys.insert(member.pubkey.as_str()) {
            return Err(ValidationError::DuplicatePubkey { index });
        }
        if member.name.is_empty() {
            return Err(ValidationError::EmptyMemberName { index });
        }
        if member.weight == 0 {
            return Err(ValidationError::ZeroWeight { index });
        }
        if member.weight > MAX_WEIGHT {
            return Err(ValidationError::WeightTooLarge {
                index,
                weight: member.weight,
            });
        }
        let Payout::Lightning { address } = &member.payout;
        if !is_lightning_address(address) {
            return Err(ValidationError::InvalidLightningAddress { index });
        }
    }

    if let Some(supersedes) = &card.supersedes
        && !is_lower_hex(supersedes, 64)
    {
        return Err(ValidationError::InvalidSupersedes);
    }
    if let Some(peer_review) = &card.rules.peer_review
        && !(1..=50).contains(&peer_review.max_shift_points)
    {
        return Err(ValidationError::InvalidPeerReviewMaxShiftPoints);
    }
    Ok(())
}

pub(crate) fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// LUD-16 Lightning Address, already lowercase: `username@domain`, with a
/// username of `a-z 0-9 - _ . +` and a domain of at least two labels, each
/// 1–63 characters of `a-z 0-9 -` not starting or ending with `-`, at most 253
/// bytes in total, and a non-numeric top-level label.
///
/// This must accept exactly what `splitcard-ln`'s `parse_address` accepts
/// unchanged, so a valid card cannot fail at payout.
#[must_use]
pub fn is_lightning_address(address: &str) -> bool {
    let mut parts = address.split('@');
    let (Some(username), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let username_ok = !username.is_empty()
        && username.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.+".contains(&byte)
        });
    let labels: Vec<&str> = domain.split('.').collect();
    let labels_ok = labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    });
    let numeric_tld = labels
        .last()
        .is_some_and(|tld| tld.bytes().all(|byte| byte.is_ascii_digit()));
    username_ok && domain.len() <= 253 && labels.len() >= 2 && labels_ok && !numeric_tld
}
