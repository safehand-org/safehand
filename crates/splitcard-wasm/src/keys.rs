//! A Nostr key held in the browser, and signing with it.
//!
//! The key never leaves the page: these functions take it as an argument and
//! return public values or signed events. Randomness comes from the caller,
//! so the browser's `crypto.getRandomValues` is the only source: a new key
//! is 32 random bytes checked with [`parse_secret_key`], and each signature
//! takes 32 bytes of auxiliary randomness (BIP-340).

use nostr::{
    event::{Event, EventId, Kind, Tag, Tags},
    key::{Keys, PublicKey, SecretKey},
    nips::nip19::{FromBech32, ToBech32},
    types::Timestamp,
};
use secp256k1::Secp256k1;
use serde::Deserialize;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

use crate::{error::JsError, throw};

fn keys(secret: &str) -> Result<Keys, JsError> {
    SecretKey::parse(secret.trim())
        .map(Keys::new)
        .map_err(|_| JsError::secret_key())
}

/// Checks a secret key, given as 64 hex characters or an `nsec`, and
/// returns it as lowercase hex.
///
/// # Errors
///
/// Throws `invalid_secret_key`.
#[wasm_bindgen]
pub fn parse_secret_key(secret: &str) -> Result<String, JsValue> {
    keys(secret)
        .map(|keys| keys.secret_key().to_secret_hex())
        .map_err(throw)
}

/// Returns the `nsec` backup form of a secret key.
///
/// # Errors
///
/// Throws `invalid_secret_key`.
#[wasm_bindgen]
pub fn secret_key_nsec(secret: &str) -> Result<String, JsValue> {
    let run = || -> Result<String, JsError> {
        keys(secret)?
            .secret_key()
            .to_bech32()
            .map_err(|_| JsError::secret_key())
    };
    run().map_err(throw)
}

/// Returns the public key of a secret key, as 64 lowercase hex characters.
///
/// # Errors
///
/// Throws `invalid_secret_key`.
#[wasm_bindgen]
pub fn public_key(secret: &str) -> Result<String, JsValue> {
    keys(secret)
        .map(|keys| keys.public_key().to_hex())
        .map_err(throw)
}

/// Reads a teammate's public key, given as an `npub` or 64 hex characters,
/// and returns it as lowercase hex.
///
/// # Errors
///
/// Throws `secret_key_not_public` for an `nsec`, so the page can warn that
/// it was a secret, or `invalid_public_key`.
#[wasm_bindgen]
pub fn parse_public_key(input: &str) -> Result<String, JsValue> {
    public_key_hex(input).map_err(throw)
}

pub(crate) fn public_key_hex(input: &str) -> Result<String, JsError> {
    let input = input.trim();
    if SecretKey::from_bech32(input).is_ok()
        || input
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("nsec1"))
    {
        return Err(JsError::secret_key_not_public());
    }
    let key = if input
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("npub1"))
    {
        PublicKey::from_bech32(&input.to_ascii_lowercase())
    } else {
        PublicKey::from_hex(input)
    };
    // Only a point on the curve can sign; `from_hex` checks the length alone.
    key.ok()
        .filter(|key| key.xonly().is_ok())
        .map(|key| key.to_hex())
        .ok_or_else(JsError::public_key)
}

/// Returns the `npub` form of a public key given as 64 hex characters.
///
/// # Errors
///
/// Throws `invalid_public_key`.
#[wasm_bindgen]
pub fn npub_encode(pubkey: &str) -> Result<String, JsValue> {
    encode_npub(pubkey).map_err(throw)
}

fn encode_npub(pubkey: &str) -> Result<String, JsError> {
    PublicKey::from_hex(pubkey)
        .ok()
        .filter(|key| key.xonly().is_ok())
        .and_then(|key| key.to_bech32().ok())
        .ok_or_else(JsError::public_key)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsignedEvent {
    #[serde(default)]
    pubkey: Option<String>,
    created_at: u64,
    kind: u16,
    tags: Vec<Vec<String>>,
    content: String,
}

/// Signs a NIP-01 event and returns it as JSON.
///
/// `unsigned_json` is `{ pubkey?, created_at, kind, tags, content }`, such as
/// the output of `signature_template` or a NIP-98 event. `aux_rand` is 32
/// bytes from `crypto.getRandomValues`.
///
/// # Errors
///
/// Throws `invalid_secret_key`, `invalid_aux_rand`, `invalid_unsigned_event`
/// or `event_pubkey_mismatch`.
#[wasm_bindgen(unchecked_return_type = "SignedEventJson")]
pub fn sign_event(
    secret: &str,
    #[wasm_bindgen(unchecked_param_type = "string")] unsigned_json: &str,
    aux_rand: &[u8],
) -> Result<String, JsValue> {
    signed_event_json(secret, unsigned_json, aux_rand).map_err(throw)
}

pub(crate) fn signed_event_json(
    secret: &str,
    unsigned_json: &str,
    aux_rand: &[u8],
) -> Result<String, JsError> {
    let keys = keys(secret)?;
    let aux: &[u8; 32] = aux_rand
        .try_into()
        .map_err(|_| JsError::aux_rand(aux_rand.len()))?;
    let unsigned: UnsignedEvent =
        serde_json::from_str(unsigned_json).map_err(|error| JsError::unsigned_event(&error))?;
    let public_key = keys.public_key();
    if let Some(pubkey) = &unsigned.pubkey
        && *pubkey != public_key.to_hex()
    {
        return Err(JsError::event_pubkey_mismatch(&public_key.to_hex(), pubkey));
    }
    let tags = unsigned
        .tags
        .into_iter()
        .map(Tag::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| JsError::unsigned_event(&error))?;
    let tags = Tags::from_list(tags);
    let created_at = Timestamp::from_secs(unsigned.created_at);
    let kind = Kind::from_u16(unsigned.kind);
    let id = EventId::compute(&public_key, &created_at, &kind, &tags, &unsigned.content);
    let secp = Secp256k1::signing_only();
    let sig = keys.sign_schnorr_with_aux_rand(&secp, id.as_bytes(), aux);
    let event = Event::new(
        id,
        public_key,
        created_at,
        kind,
        tags,
        unsigned.content,
        sig,
    );
    Ok(event.as_json())
}

#[cfg(test)]
mod tests {
    use nostr::event::Event;
    use serde_json::json;

    use super::{keys, public_key_hex, signed_event_json};

    const SECRET: &str = "0000000000000000000000000000000000000000000000000000000000000003";
    const AUX: [u8; 32] = [7; 32];
    const GENERATOR_PUBKEY: &str =
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const GENERATOR_NPUB: &str = "npub10xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqpkge6d";

    #[test]
    fn secret_keys_parse_from_hex_or_nsec_and_round_trip() {
        let keys = keys(SECRET).expect("valid");
        let nsec = nostr::nips::nip19::ToBech32::to_bech32(keys.secret_key()).expect("nsec");
        assert!(nsec.starts_with("nsec1"));
        assert_eq!(
            super::keys(&nsec).expect("nsec parses").public_key(),
            keys.public_key()
        );
        for bad in ["", "zz", &"0".repeat(64), &"f".repeat(64), "nsec1abc"] {
            assert_eq!(
                super::keys(bad).err().map(|e| e.code),
                Some("invalid_secret_key")
            );
        }
    }

    #[test]
    fn public_keys_parse_from_npub_or_hex_and_nsecs_are_refused() {
        let keys = keys(SECRET).expect("valid");
        let hex = keys.public_key().to_hex();
        let npub = nostr::nips::nip19::ToBech32::to_bech32(&keys.public_key()).expect("npub");
        assert_eq!(public_key_hex(&npub).ok(), Some(hex.clone()));
        assert_eq!(
            public_key_hex(&format!(" {} ", npub.to_uppercase())).ok(),
            Some(hex.clone())
        );
        assert_eq!(public_key_hex(&hex.to_uppercase()).ok(), Some(hex));
        let nsec = nostr::nips::nip19::ToBech32::to_bech32(keys.secret_key()).expect("nsec");
        for secret in [nsec.as_str(), "nsec1abc", "NSEC1xyz"] {
            assert_eq!(
                public_key_hex(secret).err().map(|e| e.code),
                Some("secret_key_not_public"),
                "{secret}"
            );
        }
        for bad in ["", "npub1abc", "zz", &"f".repeat(64)] {
            assert_eq!(
                public_key_hex(bad).err().map(|e| e.code),
                Some("invalid_public_key"),
                "{bad}"
            );
        }
    }

    #[test]
    fn signed_events_verify_and_keep_their_fields() {
        let unsigned = json!({
            "created_at": 1_700_000_000,
            "kind": 27_235,
            "tags": [["u", "https://example.com/api/me"], ["method", "GET"]],
            "content": "",
        })
        .to_string();
        let signed = signed_event_json(SECRET, &unsigned, &AUX).expect("signs");
        let event: Event = serde_json::from_str(&signed).expect("an event");
        event.verify().expect("valid id and signature");
        assert_eq!(event.pubkey, keys(SECRET).expect("keys").public_key());
        assert_eq!(event.kind.as_u16(), 27_235);
        assert_eq!(event.created_at.as_secs(), 1_700_000_000);
        assert_eq!(event.tags.len(), 2);
    }

    #[test]
    fn bad_inputs_are_rejected_with_their_codes() {
        let unsigned = json!({ "created_at": 1, "kind": 1, "tags": [], "content": "" });
        let code = |secret: &str, unsigned: &str, aux: &[u8]| {
            signed_event_json(secret, unsigned, aux)
                .err()
                .map(|e| e.code)
        };
        let ok = unsigned.to_string();
        assert_eq!(code("nope", &ok, &AUX), Some("invalid_secret_key"));
        assert_eq!(code(SECRET, &ok, &[0; 31]), Some("invalid_aux_rand"));
        assert_eq!(code(SECRET, "{}", &AUX), Some("invalid_unsigned_event"));
        let empty_tag = json!({ "created_at": 1, "kind": 1, "tags": [[]], "content": "" });
        assert_eq!(
            code(SECRET, &empty_tag.to_string(), &AUX),
            Some("invalid_unsigned_event")
        );
        let other = json!({
            "pubkey": "f".repeat(64), "created_at": 1, "kind": 1, "tags": [], "content": ""
        });
        assert_eq!(
            code(SECRET, &other.to_string(), &AUX),
            Some("event_pubkey_mismatch")
        );
        let mine = json!({
            "pubkey": keys(SECRET).expect("keys").public_key().to_hex(),
            "created_at": 1, "kind": 1, "tags": [], "content": ""
        });
        assert!(signed_event_json(SECRET, &mine.to_string(), &AUX).is_ok());
    }

    #[test]
    fn npub_encode_known_pubkey() {
        assert_eq!(
            super::encode_npub(GENERATOR_PUBKEY).expect("npub"),
            GENERATOR_NPUB
        );
    }

    #[test]
    fn npub_encode_rejects_invalid_hex() {
        for bad in ["", "zz", &"0".repeat(63), &"f".repeat(64)] {
            assert_eq!(
                super::encode_npub(bad).err().map(|error| error.code),
                Some("invalid_public_key"),
                "{bad:?}"
            );
        }
    }
}
