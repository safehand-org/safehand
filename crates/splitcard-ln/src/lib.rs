//! Lightning payouts for Split Card: turns Lightning Addresses into checked
//! invoices for exact amounts, and verifies payment preimages.
//!
//! This crate never pays anything and never holds wallet credentials. It does
//! not depend on `splitcard-core`; it works on plain `(id, address, amount)`
//! inputs. Everything except [`client`] and [`preflight`] is pure: no I/O, and
//! the current time is always passed in as `now_unix`.

/// Lightning Address parsing (LUD-16).
pub mod address;
/// The only module that does HTTP, with server-side request forgery protection.
pub mod client;
/// BOLT11 invoice checks.
pub mod invoice;
/// LNURL-pay responses and callback URLs (LUD-06).
pub mod lnurl;
/// Resolving every member's invoice concurrently before a payout.
pub mod preflight;
/// Payment proof by preimage.
pub mod proof;
