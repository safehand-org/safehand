//! Shared helpers: test invoices signed with a fixed key, and mock LNURL bodies.

#![allow(dead_code)]

use std::time::Duration;

use bitcoin::hashes::{Hash, sha256};
use bitcoin::secp256k1::{PublicKey, Secp256k1, SecretKey};
use lightning_invoice::{Currency, InvoiceBuilder, PaymentSecret};
use serde_json::json;

/// LNURL metadata every test invoice commits to.
pub const METADATA: &str =
    r#"[["text/plain","Split Card payout"],["text/identifier","alice@example.com"]]"#;
/// Invoice creation time used throughout.
pub const CREATED_AT: u64 = 1_700_000_000;
/// Invoice expiry used throughout, in seconds.
pub const EXPIRY_SECS: u64 = 600;
/// A time at which test invoices are valid.
pub const NOW: u64 = CREATED_AT + 60;
/// Preimage of every test invoice's payment hash. Its hex has letters, so
/// uppercasing it changes it.
pub const PREIMAGE: [u8; 32] = [0xab; 32];

pub fn secret_key(byte: u8) -> SecretKey {
    SecretKey::from_slice(&[byte; 32]).unwrap_or_else(|error| panic!("fixed key: {error}"))
}

pub fn payment_hash() -> [u8; 32] {
    sha256::Hash::hash(&PREIMAGE).to_byte_array()
}

/// What to put in a test invoice.
pub struct Spec {
    pub currency: Currency,
    pub amount_msat: Option<u64>,
    pub metadata: String,
    pub created_at: u64,
    /// Sign with this key byte.
    pub signer: u8,
    /// Declare this key byte as the payee; a mismatch with `signer` breaks the signature.
    pub payee: Option<u8>,
}

impl Spec {
    pub fn new(amount_msat: u64) -> Self {
        Self {
            currency: Currency::Bitcoin,
            amount_msat: Some(amount_msat),
            metadata: METADATA.to_owned(),
            created_at: CREATED_AT,
            signer: 1,
            payee: None,
        }
    }
}

/// Builds and signs a BOLT11 invoice with the builder and a fixed key.
pub fn invoice(spec: &Spec) -> String {
    let secp = Secp256k1::new();
    let mut builder = InvoiceBuilder::new(spec.currency.clone())
        .description_hash(sha256::Hash::hash(spec.metadata.as_bytes()))
        .payment_hash(sha256::Hash::from_byte_array(payment_hash()))
        .payment_secret(PaymentSecret([42; 32]))
        .duration_since_epoch(Duration::from_secs(spec.created_at))
        .min_final_cltv_expiry_delta(144)
        .expiry_time(Duration::from_secs(EXPIRY_SECS));
    if let Some(amount_msat) = spec.amount_msat {
        builder = builder.amount_milli_satoshis(amount_msat);
    }
    if let Some(payee) = spec.payee {
        builder = builder.payee_pub_key(PublicKey::from_secret_key(&secp, &secret_key(payee)));
    }
    let raw = builder
        .build_raw()
        .unwrap_or_else(|error| panic!("test invoice must build: {error:?}"));
    let signer = secret_key(spec.signer);
    raw.sign::<_, ()>(|message| Ok(secp.sign_ecdsa_recoverable(message, &signer)))
        .unwrap_or_else(|()| panic!("test invoice must sign"))
        .to_string()
}

/// A LUD-06 pay-request body.
pub fn pay_request_body(callback: &str, min_msat: u64, max_msat: u64) -> String {
    json!({
        "tag": "payRequest",
        "callback": callback,
        "minSendable": min_msat,
        "maxSendable": max_msat,
        "metadata": METADATA,
    })
    .to_string()
}

/// A LUD-06 callback body carrying an invoice.
pub fn invoice_body(bolt11: &str) -> String {
    json!({ "pr": bolt11, "routes": [] }).to_string()
}
