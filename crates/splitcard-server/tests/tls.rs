//! Relay websockets build their TLS config from rustls's process-wide
//! provider, which must be installed explicitly because dependencies enable
//! two of them.
//!
//! This is the only test in its binary: the provider is process-wide.

#[test]
fn tls_client_configs_build_once_the_provider_is_installed() {
    // The panic the relay connection hit before the fix.
    let without = std::panic::catch_unwind(rustls::ClientConfig::builder);
    assert!(without.is_err(), "rustls picked a provider on its own");

    splitcard_server::install_tls_provider();
    splitcard_server::install_tls_provider();
    let _config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
}
