//! Makes NIP-98 headers for testing a running server, such as through the
//! website's `/api` proxy. Never use it with a real member's key.
//!
//! ```sh
//! key=$(cargo run -q -p splitcard-server --example nip98 -- keygen)
//! NIP98_SECRET_KEY=$key cargo run -q -p splitcard-server --example nip98 -- pubkey
//! NIP98_SECRET_KEY=$key cargo run -q -p splitcard-server --example nip98 -- \
//!     sign PUT https://example.com/api/me/profile body.json
//! ```
//!
//! `sign` prints `Nostr <base64 event>` for the method, the exact URL and the
//! raw bytes of the body file (no file for an empty body).

use std::process::ExitCode;

use base64::Engine as _;
use nostr::{
    event::{EventBuilder, FinalizeEvent, Kind, Tag},
    key::{Keys, SecretKey},
};
use sha2::{Digest, Sha256};

const USAGE: &str = "usage: nip98 keygen | nip98 pubkey | nip98 sign <METHOD> <URL> [BODY_FILE]
(pubkey and sign read the hex secret key from NIP98_SECRET_KEY)";

fn keys() -> Result<Keys, String> {
    let secret =
        std::env::var("NIP98_SECRET_KEY").map_err(|_| "NIP98_SECRET_KEY is not set".to_owned())?;
    SecretKey::parse(secret.trim())
        .map(Keys::new)
        .map_err(|_| "NIP98_SECRET_KEY is not a valid secret key".to_owned())
}

fn sign(method: &str, url: &str, body_file: Option<&String>) -> Result<String, String> {
    let body = match body_file {
        Some(path) => std::fs::read(path).map_err(|error| format!("{path}: {error}"))?,
        None => Vec::new(),
    };
    let mut tags = vec![["u", url], ["method", method]];
    let payload = hex::encode(Sha256::digest(&body));
    if !body.is_empty() {
        tags.push(["payload", &payload]);
    }
    let tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let event = EventBuilder::new(Kind::from_u16(27_235), "")
        .tags(tags)
        .finalize(&keys()?)
        .map_err(|error| error.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(event.as_json());
    Ok(format!("Nostr {encoded}"))
}

fn run(args: &[String]) -> Result<String, String> {
    match args {
        [command] if command == "keygen" => Ok(SecretKey::generate().to_secret_hex()),
        [command] if command == "pubkey" => Ok(keys()?.public_key().to_hex()),
        [command, method, url, rest @ ..] if command == "sign" && rest.len() <= 1 => {
            sign(method, url, rest.first())
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
