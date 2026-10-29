//! Deterministic canonical JSON and card hashing.

use sha2::{Digest, Sha256};

use crate::{card::Card, error::HashError};

/// Serializes a card as compact canonical JSON with lexicographically sorted object keys.
///
/// `serde_json` uses a sorted map by default. The workspace must not enable its
/// `preserve_order` feature, which would make input order observable.
///
/// # Errors
///
/// Returns [`HashError::Serialize`] if JSON serialization fails.
pub fn canonical_json(card: &Card) -> Result<String, HashError> {
    let value = serde_json::to_value(card)?;
    serde_json::to_string(&value).map_err(HashError::from)
}

/// Calculates the SHA-256 hash of a card's canonical JSON.
///
/// # Errors
///
/// Returns [`HashError::Serialize`] if canonical JSON serialization fails.
pub fn card_hash(card: &Card) -> Result<[u8; 32], HashError> {
    let canonical = canonical_json(card)?;
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    Ok(hash)
}

/// Calculates the lowercase hexadecimal SHA-256 hash of a card's canonical JSON.
///
/// # Errors
///
/// Returns [`HashError::Serialize`] if canonical JSON serialization fails.
pub fn card_hash_hex(card: &Card) -> Result<String, HashError> {
    let hash = card_hash(card)?;
    Ok(hex_encode(&hash))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
