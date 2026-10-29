//! Lightning Address parsing (LUD-16).

use std::fmt;

use thiserror::Error;
use url::Url;

/// Longest allowed domain name, in bytes.
const MAX_DOMAIN_LEN: usize = 253;
/// Longest allowed domain label, in bytes.
const MAX_LABEL_LEN: usize = 63;

/// A parsed, lowercased Lightning Address such as `alice@example.com`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightningAddress {
    username: String,
    domain: String,
    well_known: Url,
}

impl LightningAddress {
    /// The lowercased username, before the `@`.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The lowercased domain, after the `@`.
    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }
}

impl fmt::Display for LightningAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.username, self.domain)
    }
}

/// A string that is not a valid LUD-16 Lightning Address.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AddressError {
    /// There is no `@`.
    #[error("Lightning Address must contain an @")]
    MissingAt,
    /// There is more than one `@`.
    #[error("Lightning Address must contain exactly one @")]
    MultipleAt,
    /// Nothing comes before the `@`.
    #[error("Lightning Address username is empty")]
    EmptyUsername,
    /// The username has a character outside `a-z 0-9 - _ . +`.
    #[error("Lightning Address username contains invalid character {character:?}")]
    InvalidUsernameCharacter {
        /// The first offending character.
        character: char,
    },
    /// Nothing comes after the `@`.
    #[error("Lightning Address domain is empty")]
    EmptyDomain,
    /// The domain is longer than 253 bytes.
    #[error("Lightning Address domain is {length} bytes, longer than 253")]
    DomainTooLong {
        /// The domain's length in bytes.
        length: usize,
    },
    /// A domain label is empty, longer than 63 bytes, has a character outside
    /// `a-z 0-9 -`, or starts or ends with `-`.
    #[error("Lightning Address domain label {label:?} is invalid")]
    InvalidDomainLabel {
        /// The offending label.
        label: String,
    },
    /// The domain has a single label, such as `localhost`.
    #[error("Lightning Address domain must have at least two labels")]
    SingleLabelDomain,
    /// The top-level label is all digits, as in an IPv4 address.
    #[error("Lightning Address domain cannot be a numeric address")]
    NumericTopLevelDomain,
    /// The well-known LNURL-pay URL could not be built.
    #[error("could not build the LNURL-pay URL for this address")]
    UrlConstruction,
}

/// Parses a LUD-16 Lightning Address, lowercasing it.
///
/// # Errors
///
/// Returns the first [`AddressError`] found, checking the `@`, then the
/// username, then the domain.
pub fn parse_address(input: &str) -> Result<LightningAddress, AddressError> {
    let lowered = input.to_ascii_lowercase();
    let mut parts = lowered.split('@');
    let (Some(username), Some(domain)) = (parts.next(), parts.next()) else {
        return Err(AddressError::MissingAt);
    };
    if parts.next().is_some() {
        return Err(AddressError::MultipleAt);
    }
    check_username(username)?;
    check_domain(domain)?;
    let well_known = Url::parse(&format!("https://{domain}/.well-known/lnurlp/{username}"))
        .map_err(|_| AddressError::UrlConstruction)?;
    Ok(LightningAddress {
        username: username.to_owned(),
        domain: domain.to_owned(),
        well_known,
    })
}

/// Returns the LUD-16 pay-request URL, `https://<domain>/.well-known/lnurlp/<username>`.
#[must_use]
pub fn well_known_url(address: &LightningAddress) -> Url {
    address.well_known.clone()
}

fn check_username(username: &str) -> Result<(), AddressError> {
    if username.is_empty() {
        return Err(AddressError::EmptyUsername);
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.+".contains(c);
    match username.chars().find(|c| !allowed(*c)) {
        Some(character) => Err(AddressError::InvalidUsernameCharacter { character }),
        None => Ok(()),
    }
}

fn check_domain(domain: &str) -> Result<(), AddressError> {
    if domain.is_empty() {
        return Err(AddressError::EmptyDomain);
    }
    if domain.len() > MAX_DOMAIN_LEN {
        return Err(AddressError::DomainTooLong {
            length: domain.len(),
        });
    }
    let labels: Vec<&str> = domain.split('.').collect();
    for label in &labels {
        let valid = !label.is_empty()
            && label.len() <= MAX_LABEL_LEN
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !valid {
            return Err(AddressError::InvalidDomainLabel {
                label: (*label).to_owned(),
            });
        }
    }
    if labels.len() < 2 {
        return Err(AddressError::SingleLabelDomain);
    }
    if labels
        .last()
        .is_some_and(|tld| tld.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(AddressError::NumericTopLevelDomain);
    }
    Ok(())
}
