//! Pure data types and calculations for Split Card.

#![deny(clippy::float_arithmetic)]

/// Card schema types will live here.
pub mod card;
/// Shared error types will live here.
pub mod error;
/// Deterministic hashing utilities will live here.
pub mod hash;
/// Nostr member-signature templates and verification.
pub mod signature;
/// Prize-split calculations will live here.
pub mod split;
/// Aggregate card signing status.
pub mod status;
/// Rules for replacing an earlier card.
pub mod supersede;
/// Card validation rules.
pub mod validate;

#[cfg(test)]
mod tests;
