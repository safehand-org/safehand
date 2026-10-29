//! Startup configuration fails fast with a clear message.

mod common;

use std::{collections::HashMap, sync::Mutex};

use common::{SERVER_SECRET_HEX, env};
use splitcard_server::config::{Config, ConfigError};

fn load(env: &HashMap<&'static str, String>) -> Result<Config, ConfigError> {
    Config::from_lookup(|name| env.get(name).cloned())
}

fn with(name: &'static str, value: &str) -> HashMap<&'static str, String> {
    let mut env = env();
    env.insert(name, value.to_owned());
    env
}

#[test]
fn valid_environment_loads_with_defaults() {
    let config = load(&env()).expect("valid");
    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:3000");
    assert_eq!(config.public_base_url, "https://splitcard.test");
    assert_eq!(config.relays.len(), 2);
    assert_eq!(
        load(&with("BIND_ADDR", "0.0.0.0:8080"))
            .expect("valid")
            .bind_addr
            .port(),
        8080
    );
}

#[test]
fn every_required_variable_is_reported_when_missing() {
    for name in [
        "DATABASE_URL",
        "PUBLIC_BASE_URL",
        "RELAYS",
        "SERVER_SECRET_KEY",
    ] {
        let mut env = env();
        env.remove(name);
        assert_eq!(load(&env).err(), Some(ConfigError::Missing { name }));
        assert_eq!(
            load(&with(name, "  ")).err(),
            Some(ConfigError::Missing { name })
        );
    }
}

#[test]
fn organizer_pubkeys_is_no_longer_read() {
    let asked = Mutex::new(Vec::new());
    let mut env = env();
    env.insert("ORGANIZER_PUBKEYS", "not a key".to_owned());
    Config::from_lookup(|name| {
        asked.lock().expect("lock").push(name.to_owned());
        env.get(name).cloned()
    })
    .expect("loads whatever ORGANIZER_PUBKEYS holds");
    let asked = asked.into_inner().expect("lock");
    assert!(asked.contains(&"RELAYS".to_owned()), "{asked:?}");
    assert!(
        !asked.contains(&"ORGANIZER_PUBKEYS".to_owned()),
        "{asked:?}"
    );
}

#[test]
fn malformed_variables_are_rejected() {
    for (name, value) in [
        ("BIND_ADDR", "localhost"),
        ("PUBLIC_BASE_URL", "splitcard.test"),
        ("PUBLIC_BASE_URL", "ftp://splitcard.test"),
        ("PUBLIC_BASE_URL", "https://splitcard.test/?x=1"),
        ("RELAYS", "ws://relay.test"),
        ("RELAYS", "wss://relay.one.test,https://relay.two.test"),
        ("SERVER_SECRET_KEY", "not-a-key"),
    ] {
        match load(&with(name, value)) {
            Err(ConfigError::Invalid { name: reported, .. }) => {
                assert_eq!(reported, name, "{value}");
            }
            other => panic!("{name}={value:?} should be invalid, got {other:?}"),
        }
    }
}

#[test]
fn bad_secret_key_error_does_not_echo_the_value() {
    let almost = format!("{}zz", &SERVER_SECRET_HEX[..62]);
    let message = load(&with("SERVER_SECRET_KEY", &almost))
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert_eq!(
        message,
        "SERVER_SECRET_KEY is invalid: expected a hex or nsec secret key"
    );
}

#[test]
fn ots_settings_default_and_parse() {
    let config = load(&env()).expect("valid");
    let calendars: Vec<&str> = config.ots_calendars.iter().map(url::Url::as_str).collect();
    assert_eq!(
        calendars,
        [
            "https://a.pool.opentimestamps.org/",
            "https://b.pool.opentimestamps.org/",
            "https://a.pool.eternitywall.com/",
            "https://ots.btc.catallaxy.com/",
        ]
    );
    assert_eq!(config.ots_upgrade_interval.as_secs(), 600);

    let config = load(&with("OTS_CALENDARS", "https://cal.example")).expect("valid");
    assert_eq!(config.ots_calendars.len(), 1);
    let config = load(&with("OTS_UPGRADE_INTERVAL_SECS", "60")).expect("valid");
    assert_eq!(config.ots_upgrade_interval.as_secs(), 60);
}

#[test]
fn malformed_ots_settings_are_rejected() {
    for (name, value) in [
        ("OTS_CALENDARS", "http://a.pool.opentimestamps.org"),
        ("OTS_CALENDARS", "https://a.pool.opentimestamps.org,"),
        ("OTS_CALENDARS", "https://a.pool.opentimestamps.org/?x=1"),
        ("OTS_CALENDARS", "https://user:pw@a.pool.opentimestamps.org"),
        ("OTS_UPGRADE_INTERVAL_SECS", "0"),
        ("OTS_UPGRADE_INTERVAL_SECS", "-5"),
        ("OTS_UPGRADE_INTERVAL_SECS", "10m"),
    ] {
        match load(&with(name, value)) {
            Err(ConfigError::Invalid { name: reported, .. }) => {
                assert_eq!(reported, name, "{value}");
            }
            other => panic!("{name}={value:?} should be invalid, got {other:?}"),
        }
    }
}
