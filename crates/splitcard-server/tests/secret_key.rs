//! The server secret key never appears in logs, responses or `Debug` output.
//!
//! This is the only test in its binary: tracing caches per-callsite interest
//! process-wide, so a parallel test without a subscriber could hide events.

mod common;

use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use common::{ALICE, BOB, CHIDI, SERVER_SECRET_HEX, app, first_card};
use nostr::key::{Keys, SecretKey};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn server_secret_key_never_appears_in_logs_or_responses() {
    let logs = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    let hash = app.submit_ok(&team_id, &first_card()).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }
    app.get(&format!("/cards/{hash}")).await;
    app.get(&format!("/teams/{team_id}/card")).await;
    app.sign(&hash, "not json").await;

    let secret = SecretKey::parse(SERVER_SECRET_HEX).expect("secret");
    let nsec = nostr::nips::nip19::ToBech32::to_bech32(&secret).expect("nsec");
    let forbidden = [SERVER_SECRET_HEX.to_owned(), nsec];

    let logs = String::from_utf8(logs.0.lock().expect("lock").clone()).expect("UTF-8 logs");
    assert!(
        logs.contains("/teams/"),
        "request logs were captured: {logs}"
    );
    let debug = format!("{:?}", app.state.config);
    assert!(debug.contains(&Keys::new(secret).public_key().to_hex()));
    let bodies = app.bodies.lock().expect("lock").join("\n");
    for text in [&logs, &debug, &bodies] {
        for secret in &forbidden {
            assert!(!text.contains(secret.as_str()));
        }
    }
}
