//! BOLT11 invoice checks against an LNURL-pay request.

use std::fmt;

use lightning_invoice::{
    Bolt11Invoice, Bolt11InvoiceDescriptionRef, Bolt11SemanticError, Currency, ParseOrSemanticError,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The Lightning network invoices must be for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Network {
    /// Bitcoin mainnet (`lnbc`).
    #[default]
    Mainnet,
    /// Signet (`lntbs`), for demos.
    Signet,
    /// Regtest (`lnbcrt`), for demos.
    Regtest,
}

impl Network {
    fn currency(self) -> Currency {
        match self {
            Self::Mainnet => Currency::Bitcoin,
            Self::Signet => Currency::Signet,
            Self::Regtest => Currency::Regtest,
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Mainnet => "mainnet",
            Self::Signet => "signet",
            Self::Regtest => "regtest",
        })
    }
}

/// An invoice that passed every check in [`check_invoice`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedInvoice {
    /// The BOLT11 string, for the payer's wallet.
    pub bolt11: String,
    /// The payment hash, 64 lowercase hex characters.
    pub payment_hash: String,
    /// The invoice amount, in millisats.
    pub amount_msat: u64,
    /// Unix time in seconds at which the invoice expires (timestamp plus expiry).
    pub expires_at_unix: u64,
}

/// A BOLT11 invoice whose signature verifies, decoded without checking it
/// against a pay request or the clock. For proving past payments, whose
/// invoices have usually expired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedInvoice {
    /// The payment hash, 64 lowercase hex characters.
    pub payment_hash: String,
    /// The invoice amount in millisats, if it has one.
    pub amount_msat: Option<u64>,
    /// The currency prefix, such as `bc` for mainnet.
    pub currency: String,
    /// Unix time in seconds at which the invoice expires (timestamp plus expiry).
    pub expires_at_unix: u64,
    /// The description hash in lowercase hex, or `None` for an invoice with a
    /// plain description.
    pub description_hash: Option<String>,
}

/// A reason an invoice is not acceptable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvoiceError {
    /// The string is not a well-formed BOLT11 invoice.
    #[error("invoice is not valid BOLT11: {message}")]
    Unparseable {
        /// The parser's message.
        message: String,
    },
    /// The invoice's signature does not verify.
    #[error("invoice signature is invalid")]
    InvalidSignature,
    /// The invoice is for a different network.
    #[error("invoice is for network {found}, expected {expected}")]
    WrongNetwork {
        /// The configured network.
        expected: Network,
        /// The invoice's currency prefix.
        found: String,
    },
    /// The invoice has no amount.
    #[error("invoice has no amount")]
    MissingAmount,
    /// The invoice amount is not the expected amount.
    #[error("invoice is for {found_msat} msat, expected {expected_msat} msat")]
    WrongAmount {
        /// The amount requested, in millisats.
        expected_msat: u64,
        /// The invoice's amount, in millisats.
        found_msat: u64,
    },
    /// The invoice has a plain description instead of a description hash.
    #[error("invoice has no description hash")]
    MissingDescriptionHash,
    /// The description hash is not the SHA-256 of the LNURL metadata.
    #[error("invoice description hash does not match the LNURL metadata")]
    WrongDescriptionHash,
    /// The invoice has expired.
    #[error("invoice expired at {expires_at_unix}, now is {now_unix}")]
    Expired {
        /// When the invoice expired, in Unix seconds.
        expires_at_unix: u64,
        /// The time it was checked at, in Unix seconds.
        now_unix: u64,
    },
}

/// Checks a BOLT11 invoice returned by an LNURL-pay callback.
///
/// In order: it parses and its signature verifies, it is for `network`, its
/// amount is exactly `expected_msat`, its description hash is
/// `sha256(metadata)`, and it has not expired at `now_unix`. An invoice counts
/// as expired from the second its timestamp plus expiry is reached.
///
/// # Errors
///
/// Returns the first failed check.
pub fn check_invoice(
    bolt11: &str,
    expected_msat: u64,
    metadata: &str,
    network: Network,
    now_unix: u64,
) -> Result<CheckedInvoice, InvoiceError> {
    let invoice = parse_bolt11(bolt11)?;

    if invoice.currency() != network.currency() {
        return Err(InvoiceError::WrongNetwork {
            expected: network,
            found: invoice.currency().to_string(),
        });
    }

    let amount_msat = invoice
        .amount_milli_satoshis()
        .ok_or(InvoiceError::MissingAmount)?;
    if amount_msat != expected_msat {
        return Err(InvoiceError::WrongAmount {
            expected_msat,
            found_msat: amount_msat,
        });
    }

    let Bolt11InvoiceDescriptionRef::Hash(description_hash) = invoice.description() else {
        return Err(InvoiceError::MissingDescriptionHash);
    };
    let expected_hash: [u8; 32] = Sha256::digest(metadata.as_bytes()).into();
    let actual_hash: &[u8; 32] = description_hash.0.as_ref();
    if actual_hash != &expected_hash {
        return Err(InvoiceError::WrongDescriptionHash);
    }

    let expires_at_unix = expiry(&invoice);
    if now_unix >= expires_at_unix {
        return Err(InvoiceError::Expired {
            expires_at_unix,
            now_unix,
        });
    }

    let payment_hash: &[u8; 32] = invoice.payment_hash().as_ref();
    Ok(CheckedInvoice {
        bolt11: bolt11.to_owned(),
        payment_hash: hex::encode(payment_hash),
        amount_msat,
        expires_at_unix,
    })
}

/// Decodes a BOLT11 invoice and verifies its signature, with no other checks.
///
/// # Errors
///
/// [`InvoiceError::Unparseable`] or [`InvoiceError::InvalidSignature`].
pub fn decode_invoice(bolt11: &str) -> Result<DecodedInvoice, InvoiceError> {
    let invoice = parse_bolt11(bolt11)?;
    let payment_hash: &[u8; 32] = invoice.payment_hash().as_ref();
    Ok(DecodedInvoice {
        payment_hash: hex::encode(payment_hash),
        amount_msat: invoice.amount_milli_satoshis(),
        currency: invoice.currency().to_string(),
        expires_at_unix: expiry(&invoice),
        description_hash: match invoice.description() {
            Bolt11InvoiceDescriptionRef::Hash(hash) => {
                let hash: &[u8; 32] = hash.0.as_ref();
                Some(hex::encode(hash))
            }
            Bolt11InvoiceDescriptionRef::Direct(_) => None,
        },
    })
}

/// Parses an invoice; parsing also verifies its signature.
fn parse_bolt11(bolt11: &str) -> Result<Bolt11Invoice, InvoiceError> {
    bolt11.parse().map_err(|error| match error {
        ParseOrSemanticError::SemanticError(Bolt11SemanticError::InvalidSignature) => {
            InvoiceError::InvalidSignature
        }
        other => InvoiceError::Unparseable {
            message: other.to_string(),
        },
    })
}

/// The invoice's timestamp plus its expiry, in Unix seconds.
fn expiry(invoice: &Bolt11Invoice) -> u64 {
    invoice
        .duration_since_epoch()
        .as_secs()
        .saturating_add(invoice.expiry_time().as_secs())
}
