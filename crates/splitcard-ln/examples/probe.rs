//! Resolves a real Lightning Address to a checked mainnet invoice, with the
//! default (secure) client config. It never pays anything.
//!
//! ```sh
//! cargo run -p splitcard-ln --example probe -- alice@blink.sv 1000
//! ```

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use splitcard_ln::{
    address::{parse_address, well_known_url},
    client::{ClientConfig, LnClient},
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, amount_sats] = args.as_slice() else {
        eprintln!("usage: probe <lightning-address> <amount-sats>");
        return ExitCode::from(2);
    };
    let Ok(amount_sats) = amount_sats.parse::<u64>() else {
        eprintln!("amount must be a whole number of sats, got {amount_sats:?}");
        return ExitCode::from(2);
    };
    match probe(address, amount_sats).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn probe(address: &str, amount_sats: u64) -> Result<(), Box<dyn std::error::Error>> {
    // The library never reads the clock; callers pass the current time in.
    let now_unix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let address = parse_address(address)?;
    println!("address      {address}");
    println!("pay request  {}", well_known_url(&address));

    let client = LnClient::new(ClientConfig::default())?;
    let pay_request = client.fetch_pay_request(&address).await?;
    println!("callback     {}", pay_request.callback);
    println!(
        "sendable     {} to {} msat",
        pay_request.min_sendable_msat, pay_request.max_sendable_msat
    );

    let invoice = client
        .request_invoice(&pay_request, amount_sats, now_unix)
        .await?;
    println!("amount       {} msat", invoice.amount_msat);
    println!("payment hash {}", invoice.payment_hash);
    println!(
        "expires in   {} s",
        invoice.expires_at_unix.saturating_sub(now_unix)
    );
    println!("invoice      {}", invoice.bolt11);
    println!("all checks passed");
    Ok(())
}
