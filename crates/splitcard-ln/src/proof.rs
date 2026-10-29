//! Proof of payment: a preimage whose SHA-256 is the invoice's payment hash.

use sha2::{Digest, Sha256};
use thiserror::Error;

/// A reason a preimage does not prove payment.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProofError {
    /// The payment hash is not 64 lowercase hex characters.
    #[error("payment hash must be 64 lowercase hexadecimal characters")]
    InvalidPaymentHash,
    /// The preimage is not 64 characters long.
    #[error("preimage must be 64 characters, got {length}")]
    InvalidPreimageLength {
        /// The preimage's length in characters.
        length: usize,
    },
    /// The preimage has a character other than `0-9 a-f`.
    #[error("preimage must be lowercase hexadecimal")]
    InvalidPreimageHex,
    /// `sha256(preimage)` is not the payment hash.
    #[error("preimage does not match the payment hash")]
    PreimageMismatch,
}

/// Verifies that `sha256(preimage)` equals the payment hash.
///
/// # Errors
///
/// Returns [`ProofError`] if either input is malformed or they do not match.
pub fn verify_preimage(payment_hash_hex: &str, preimage_hex: &str) -> Result<(), ProofError> {
    let payment_hash =
        decode_lower_hex_32(payment_hash_hex).ok_or(ProofError::InvalidPaymentHash)?;
    let length = preimage_hex.chars().count();
    if length != 64 {
        return Err(ProofError::InvalidPreimageLength { length });
    }
    let preimage = decode_lower_hex_32(preimage_hex).ok_or(ProofError::InvalidPreimageHex)?;
    let digest: [u8; 32] = Sha256::digest(preimage).into();
    if digest != payment_hash {
        return Err(ProofError::PreimageMismatch);
    }
    Ok(())
}

/// Decodes exactly 64 lowercase hex characters; `hex` alone also accepts uppercase.
fn decode_lower_hex_32(value: &str) -> Option<[u8; 32]> {
    let lowercase = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !lowercase {
        return None;
    }
    let mut bytes = [0_u8; 32];
    hex::decode_to_slice(value, &mut bytes).ok()?;
    Some(bytes)
}
