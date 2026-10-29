//! Server configuration from environment variables.

use std::{fmt, net::SocketAddr, time::Duration};

use nostr::key::{Keys, PublicKey, SecretKey};
use thiserror::Error;
use url::Url;

/// Address used when `BIND_ADDR` is unset.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
/// Calendars used when `OTS_CALENDARS` is unset: the reference client's
/// default aggregators.
pub const DEFAULT_OTS_CALENDARS: [&str; 4] = [
    "https://a.pool.opentimestamps.org",
    "https://b.pool.opentimestamps.org",
    "https://a.pool.eternitywall.com",
    "https://ots.btc.catallaxy.com",
];
/// Upgrade interval used when `OTS_UPGRADE_INTERVAL_SECS` is unset.
pub const DEFAULT_OTS_UPGRADE_INTERVAL_SECS: u64 = 600;

/// Validated server configuration.
///
/// `Debug` never prints the server secret key.
#[derive(Clone)]
pub struct Config {
    /// `BIND_ADDR`: address the HTTP server listens on.
    pub bind_addr: SocketAddr,
    /// `DATABASE_URL`: an SQLite file path or `sqlite:` URL.
    pub database_url: String,
    /// `PUBLIC_BASE_URL` without a trailing slash; NIP-98 `u` tags must equal
    /// this plus the request path and query.
    pub public_base_url: String,
    /// `RELAYS`: `wss://` relays card events are published to.
    pub relays: Vec<Url>,
    /// `SERVER_SECRET_KEY`: the key card events are signed with.
    pub server_keys: Keys,
    /// `OTS_CALENDARS`: HTTPS `OpenTimestamps` calendars card hashes are
    /// submitted to.
    pub ots_calendars: Vec<Url>,
    /// `OTS_UPGRADE_INTERVAL_SECS`: pause between attempts to upgrade
    /// pending proofs.
    pub ots_upgrade_interval: Duration,
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("database_url", &self.database_url)
            .field("public_base_url", &self.public_base_url)
            .field("relays", &self.relays)
            .field("server_pubkey", &self.server_keys.public_key().to_hex())
            .field("ots_calendars", &self.ots_calendars)
            .field("ots_upgrade_interval", &self.ots_upgrade_interval)
            .finish_non_exhaustive()
    }
}

/// A missing or malformed environment variable.
///
/// Messages never include the value of `SERVER_SECRET_KEY`.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// A required variable is unset or empty.
    #[error("{name} is required")]
    Missing {
        /// Variable name.
        name: &'static str,
    },
    /// A variable is set but malformed.
    #[error("{name} is invalid: {reason}")]
    Invalid {
        /// Variable name.
        name: &'static str,
        /// What is wrong with it.
        reason: String,
    },
}

impl Config {
    /// Reads the configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns the first missing or malformed variable.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads the configuration through `lookup`, which returns a variable's
    /// value by name.
    ///
    /// # Errors
    ///
    /// Returns the first missing or malformed variable.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |name: &'static str| {
            lookup(name)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let require = |name: &'static str| get(name).ok_or(ConfigError::Missing { name });

        let bind_addr = get("BIND_ADDR")
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned())
            .parse()
            .map_err(|_| invalid("BIND_ADDR", "expected host:port, like 127.0.0.1:3000"))?;
        let database_url = require("DATABASE_URL")?;
        let public_base_url = parse_base_url(&require("PUBLIC_BASE_URL")?)?;
        let relays = parse_relays(&require("RELAYS")?)?;
        let server_keys = SecretKey::parse(&require("SERVER_SECRET_KEY")?)
            .map(Keys::new)
            .map_err(|_| invalid("SERVER_SECRET_KEY", "expected a hex or nsec secret key"))?;
        let ots_calendars = parse_calendars(
            &get("OTS_CALENDARS").unwrap_or_else(|| DEFAULT_OTS_CALENDARS.join(",")),
        )?;
        let ots_upgrade_interval = match get("OTS_UPGRADE_INTERVAL_SECS") {
            None => Duration::from_secs(DEFAULT_OTS_UPGRADE_INTERVAL_SECS),
            Some(value) => value
                .parse::<u64>()
                .ok()
                .filter(|secs| *secs > 0)
                .map(Duration::from_secs)
                .ok_or_else(|| {
                    invalid(
                        "OTS_UPGRADE_INTERVAL_SECS",
                        "expected a positive whole number of seconds",
                    )
                })?,
        };
        Ok(Self {
            bind_addr,
            database_url,
            public_base_url,
            relays,
            server_keys,
            ots_calendars,
            ots_upgrade_interval,
        })
    }
}

fn invalid(name: &'static str, reason: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        name,
        reason: reason.into(),
    }
}

fn parse_base_url(value: &str) -> Result<String, ConfigError> {
    let url = Url::parse(value).map_err(|error| invalid("PUBLIC_BASE_URL", error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid("PUBLIC_BASE_URL", "scheme must be http or https"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid(
            "PUBLIC_BASE_URL",
            "must not have a query or fragment",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn parse_relays(value: &str) -> Result<Vec<Url>, ConfigError> {
    value
        .split(',')
        .map(str::trim)
        .map(|relay| {
            Url::parse(relay)
                .ok()
                .filter(|url| url.scheme() == "wss" && url.host().is_some())
                .ok_or_else(|| invalid("RELAYS", format!("{relay:?} is not a wss:// URL")))
        })
        .collect()
}

fn parse_calendars(value: &str) -> Result<Vec<Url>, ConfigError> {
    value
        .split(',')
        .map(str::trim)
        .map(|calendar| {
            Url::parse(calendar)
                .ok()
                .filter(|url| {
                    url.scheme() == "https"
                        && url.host().is_some()
                        && url.query().is_none()
                        && url.fragment().is_none()
                        && url.username().is_empty()
                        && url.password().is_none()
                })
                .ok_or_else(|| {
                    invalid(
                        "OTS_CALENDARS",
                        format!("{calendar:?} is not an https:// URL without query or credentials"),
                    )
                })
        })
        .collect()
}

/// Whether `key` is 64 lowercase hex characters encoding a valid x-only point.
pub(crate) fn is_pubkey(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        && PublicKey::from_hex(key).is_ok_and(|key| key.xonly().is_ok())
}
