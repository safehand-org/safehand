//! LNURL-pay responses and callback URLs (LUD-06).

use serde_json::{Map, Value};
use thiserror::Error;
use url::Url;

/// A validated LNURL-pay request (the `payRequest` response).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayRequest {
    /// Where to request an invoice; HTTPS unless insecure hosts are allowed.
    pub callback: Url,
    /// Smallest payable amount, in millisats.
    pub min_sendable_msat: u64,
    /// Largest payable amount, in millisats.
    pub max_sendable_msat: u64,
    /// The `metadata` string exactly as received; invoices must commit to its SHA-256.
    pub metadata: String,
}

/// A failure in an LNURL-pay response or amount.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LnurlError {
    /// The body is not a JSON object.
    #[error("LNURL response is not a JSON object: {message}")]
    InvalidJson {
        /// The JSON parser's message.
        message: String,
    },
    /// The service answered `{ "status": "ERROR", "reason": ... }`.
    #[error("LNURL service returned an error: {reason}")]
    Service {
        /// The service's `reason`, or empty if it gave none.
        reason: String,
    },
    /// The `tag` is not `payRequest`.
    #[error("LNURL tag is {found:?}, not \"payRequest\"")]
    WrongTag {
        /// The tag received, or empty if missing.
        found: String,
    },
    /// A required field is missing.
    #[error("LNURL response is missing {field}")]
    MissingField {
        /// The missing field's name.
        field: &'static str,
    },
    /// A field has the wrong JSON type, or a number is not a non-negative integer.
    #[error("LNURL response field {field} has the wrong type")]
    InvalidField {
        /// The offending field's name.
        field: &'static str,
    },
    /// The callback is not a valid URL.
    #[error("LNURL callback is not a valid URL")]
    InvalidCallback,
    /// The callback is not HTTPS.
    #[error("LNURL callback must use https, not {scheme}")]
    InsecureCallback {
        /// The callback's scheme.
        scheme: String,
    },
    /// `minSendable` is greater than `maxSendable`.
    #[error("LNURL minSendable {min_sendable_msat} exceeds maxSendable {max_sendable_msat}")]
    InvalidSendableRange {
        /// `minSendable`, in millisats.
        min_sendable_msat: u64,
        /// `maxSendable`, in millisats.
        max_sendable_msat: u64,
    },
    /// The amount in sats does not fit in millisats.
    #[error("{sats} sats overflows when converted to millisats")]
    AmountOverflow {
        /// The amount in sats.
        sats: u64,
    },
    /// The amount is below `minSendable`.
    #[error("amount {amount_msat} msat is below the minimum of {min_sendable_msat} msat")]
    AmountBelowMinimum {
        /// The requested amount, in millisats.
        amount_msat: u64,
        /// `minSendable`, in millisats.
        min_sendable_msat: u64,
    },
    /// The amount is above `maxSendable`.
    #[error("amount {amount_msat} msat is above the maximum of {max_sendable_msat} msat")]
    AmountAboveMaximum {
        /// The requested amount, in millisats.
        amount_msat: u64,
        /// `maxSendable`, in millisats.
        max_sendable_msat: u64,
    },
}

/// Parses a LUD-06 pay-request response body.
///
/// `allow_insecure` permits a non-HTTPS callback; it exists for tests and
/// local demos and must be `false` in production.
///
/// # Errors
///
/// Returns [`LnurlError::Service`] for an error response, otherwise the first
/// invalid field.
pub fn parse_pay_request(body: &str, allow_insecure: bool) -> Result<PayRequest, LnurlError> {
    let object = parse_object(body)?;
    let tag = object
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if tag != "payRequest" {
        return Err(LnurlError::WrongTag {
            found: tag.to_owned(),
        });
    }
    let callback = string_field(&object, "callback")?;
    let callback = Url::parse(callback).map_err(|_| LnurlError::InvalidCallback)?;
    if callback.scheme() != "https" && !allow_insecure {
        return Err(LnurlError::InsecureCallback {
            scheme: callback.scheme().to_owned(),
        });
    }
    let min_sendable_msat = u64_field(&object, "minSendable")?;
    let max_sendable_msat = u64_field(&object, "maxSendable")?;
    if min_sendable_msat > max_sendable_msat {
        return Err(LnurlError::InvalidSendableRange {
            min_sendable_msat,
            max_sendable_msat,
        });
    }
    let metadata = string_field(&object, "metadata")?.to_owned();
    Ok(PayRequest {
        callback,
        min_sendable_msat,
        max_sendable_msat,
        metadata,
    })
}

/// Parses the callback's response and returns the BOLT11 invoice in `pr`.
///
/// # Errors
///
/// Returns [`LnurlError::Service`] for an error response, or a missing or
/// mistyped `pr`.
pub fn parse_invoice_response(body: &str) -> Result<String, LnurlError> {
    let object = parse_object(body)?;
    Ok(string_field(&object, "pr")?.to_owned())
}

/// Returns the LUD-21 `verify` URL from a callback response, if it has a
/// usable one.
///
/// The URL is optional in LUD-21, so a missing, mistyped, unparseable or (unless
/// `allow_insecure`) non-HTTPS value gives `None` instead of failing the invoice.
#[must_use]
pub fn parse_verify_url(body: &str, allow_insecure: bool) -> Option<Url> {
    let object = parse_object(body).ok()?;
    let url = Url::parse(object.get("verify")?.as_str()?).ok()?;
    (url.scheme() == "https" || allow_insecure).then_some(url)
}

/// What a LUD-21 `verify` URL reports about an invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settlement {
    /// Whether the provider says the invoice was paid.
    pub settled: bool,
    /// The preimage the provider returned, if any. It proves nothing until
    /// checked with [`verify_preimage`](crate::proof::verify_preimage).
    pub preimage: Option<String>,
    /// The invoice the answer is about, as the provider gave it.
    pub bolt11: String,
}

/// Parses a LUD-21 verify response, `{ "status": "OK", "settled", "preimage", "pr" }`.
///
/// # Errors
///
/// Returns [`LnurlError::Service`] for an error response, or a missing or
/// mistyped `settled`, `preimage` or `pr`.
pub fn parse_verify_response(body: &str) -> Result<Settlement, LnurlError> {
    let object = parse_object(body)?;
    let settled = object
        .get("settled")
        .ok_or(LnurlError::MissingField { field: "settled" })?
        .as_bool()
        .ok_or(LnurlError::InvalidField { field: "settled" })?;
    let preimage = match object.get("preimage") {
        None | Some(Value::Null) => None,
        Some(Value::String(preimage)) => Some(preimage.clone()),
        Some(_) => return Err(LnurlError::InvalidField { field: "preimage" }),
    };
    let bolt11 = string_field(&object, "pr")?.to_owned();
    Ok(Settlement {
        settled,
        preimage,
        bolt11,
    })
}

/// Converts sats to millisats.
///
/// # Errors
///
/// Returns [`LnurlError::AmountOverflow`] if the result does not fit in a `u64`.
pub fn sats_to_msat(sats: u64) -> Result<u64, LnurlError> {
    sats.checked_mul(1_000)
        .ok_or(LnurlError::AmountOverflow { sats })
}

/// Checks that an amount lies within `[minSendable, maxSendable]`.
///
/// # Errors
///
/// Returns [`LnurlError::AmountBelowMinimum`] or [`LnurlError::AmountAboveMaximum`].
pub fn check_amount(pay_request: &PayRequest, amount_msat: u64) -> Result<(), LnurlError> {
    if amount_msat < pay_request.min_sendable_msat {
        return Err(LnurlError::AmountBelowMinimum {
            amount_msat,
            min_sendable_msat: pay_request.min_sendable_msat,
        });
    }
    if amount_msat > pay_request.max_sendable_msat {
        return Err(LnurlError::AmountAboveMaximum {
            amount_msat,
            max_sendable_msat: pay_request.max_sendable_msat,
        });
    }
    Ok(())
}

/// Adds `amount=<amount_msat>` to the callback, keeping its other query
/// parameters and replacing any existing `amount`.
#[must_use]
pub fn build_callback_url(callback: &Url, amount_msat: u64) -> Url {
    let kept: Vec<(String, String)> = callback
        .query_pairs()
        .filter(|(key, _)| key != "amount")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let mut url = callback.clone();
    url.query_pairs_mut()
        .clear()
        .extend_pairs(kept)
        .append_pair("amount", &amount_msat.to_string());
    url
}

/// Parses a JSON object, turning an LNURL error response into [`LnurlError::Service`].
fn parse_object(body: &str) -> Result<Map<String, Value>, LnurlError> {
    let value: Value = serde_json::from_str(body).map_err(|error| LnurlError::InvalidJson {
        message: error.to_string(),
    })?;
    let Value::Object(object) = value else {
        return Err(LnurlError::InvalidJson {
            message: "expected a JSON object".to_owned(),
        });
    };
    if object
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("ERROR"))
    {
        let reason = object
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Err(LnurlError::Service {
            reason: reason.to_owned(),
        });
    }
    Ok(object)
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, LnurlError> {
    object
        .get(field)
        .ok_or(LnurlError::MissingField { field })?
        .as_str()
        .ok_or(LnurlError::InvalidField { field })
}

fn u64_field(object: &Map<String, Value>, field: &'static str) -> Result<u64, LnurlError> {
    object
        .get(field)
        .ok_or(LnurlError::MissingField { field })?
        .as_u64()
        .ok_or(LnurlError::InvalidField { field })
}
