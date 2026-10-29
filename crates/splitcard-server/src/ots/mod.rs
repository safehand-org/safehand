//! OpenTimestamps proofs for active cards.
//!
//! A card's hash is the SHA-256 of its canonical JSON, so a proof stamping
//! that hash is a standard detached `.ots` file for the canonical JSON: save
//! the JSON as `<hash>.json` and run `ots verify <hash>.json.ots`.
//!
//! [`format`] is the pure file format, [`calendar`] the only network code,
//! and [`worker`] the background submit and upgrade loops.

/// The only module that talks to calendars.
pub mod calendar;
/// The `.ots` binary format, with no I/O.
pub mod format;
/// Background submit and upgrade workers.
pub mod worker;
