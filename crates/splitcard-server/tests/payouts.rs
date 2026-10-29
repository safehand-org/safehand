//! Prizes agreed by every card member, and payments recorded with a paid
//! invoice and its preimage.

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use bitcoin::{
    hashes::{Hash, sha256},
    secp256k1::{Secp256k1, SecretKey},
};
use common::{ALICE, BOB, CHIDI, DAYO, EVE, NOW, TestApp, app, card, first_card, person, pubkey};
use lightning_invoice::{Currency, InvoiceBuilder, PaymentSecret, RawBolt11Invoice};
use serde_json::{Value, json};

/// Alice, Bob and Chidi with equal weights on an active card, so a prize of
/// 300,000 sats is 100,000 each.
async fn equal_card(app: &TestApp) -> String {
    let team_id = app.alice_bob_chidi().await;
    let hash = app
        .submit_ok(&team_id, &card(&[(ALICE, 1), (BOB, 1), (CHIDI, 1)], None))
        .await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }
    team_id
}

async fn put_prize(app: &TestApp, number: u8, team_id: &str, sats: &str) -> (StatusCode, Value) {
    let body = json!({ "prize_sats": sats }).to_string();
    app.authed(
        number,
        Method::PUT,
        &format!("/teams/{team_id}/prize"),
        &body,
    )
    .await
}

async fn put_prize_ok(app: &TestApp, number: u8, team_id: &str, sats: &str) -> Value {
    let (status, body) = put_prize(app, number, team_id, sats).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Alice, Bob and Chidi agree 300,000 sats on the equal card.
async fn agreed(app: &TestApp) -> String {
    let team_id = equal_card(app).await;
    for number in [ALICE, BOB, CHIDI] {
        put_prize_ok(app, number, &team_id, "300000").await;
    }
    team_id
}

async fn payout(app: &TestApp, number: u8, team_id: &str) -> (StatusCode, Value) {
    app.authed(number, Method::GET, &format!("/teams/{team_id}/payout"), "")
        .await
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str) {
    let (actual, body) = response;
    assert_eq!(*actual, status, "{body}");
    assert_eq!(body["code"], code, "{body}");
}

/// LNURL metadata naming `addresses`, as a LUD-16 server would send it.
fn metadata(addresses: &[&str]) -> String {
    let mut entries = vec![json!(["text/plain", "Pay a Split Card member"])];
    entries.extend(
        addresses
            .iter()
            .map(|address| json!(["text/identifier", address])),
    );
    Value::Array(entries).to_string()
}

fn metadata_for(number: u8) -> String {
    metadata(&[person(number).1])
}

fn preimage(seed: u8) -> [u8; 32] {
    [seed; 32]
}

enum Description<'a> {
    Hash(&'a str),
    Plain,
}

/// A signed BOLT11 invoice paying `preimage(seed)`'s hash.
fn invoice(
    currency: Currency,
    amount_msat: Option<u64>,
    description: &Description<'_>,
    seed: u8,
) -> String {
    let payment_hash = sha256::Hash::hash(&preimage(seed));
    let finish = |raw: RawBolt11Invoice| {
        let secp = Secp256k1::new();
        let key = SecretKey::from_slice(&[7; 32]).expect("valid key");
        raw.sign::<_, ()>(|message| Ok(secp.sign_ecdsa_recoverable(message, &key)))
            .expect("signs")
            .to_string()
    };
    macro_rules! build {
        ($builder:expr) => {{
            let mut builder = $builder
                .payment_hash(payment_hash)
                .payment_secret(PaymentSecret([42; 32]))
                .duration_since_epoch(Duration::from_secs(NOW))
                .min_final_cltv_expiry_delta(144)
                .expiry_time(Duration::from_secs(600));
            if let Some(amount) = amount_msat {
                builder = builder.amount_milli_satoshis(amount);
            }
            finish(builder.build_raw().expect("builds"))
        }};
    }
    match description {
        Description::Hash(metadata) => build!(
            InvoiceBuilder::new(currency).description_hash(sha256::Hash::hash(metadata.as_bytes()))
        ),
        Description::Plain => {
            build!(InvoiceBuilder::new(currency).description("Split Card".to_owned()))
        }
    }
}

/// A paid mainnet invoice for 100,000 sats to `number`.
fn share_invoice(number: u8, seed: u8) -> String {
    invoice(
        Currency::Bitcoin,
        Some(100_000_000),
        &Description::Hash(&metadata_for(number)),
        seed,
    )
}

async fn record(
    app: &TestApp,
    payer: u8,
    team_id: &str,
    index: &str,
    bolt11: &str,
    preimage_hex: &str,
    metadata: &str,
) -> (StatusCode, Value) {
    let body =
        json!({ "bolt11": bolt11, "preimage": preimage_hex, "metadata": metadata }).to_string();
    app.authed(
        payer,
        Method::POST,
        &format!("/teams/{team_id}/payout/items/{index}/payment"),
        &body,
    )
    .await
}

async fn pay(app: &TestApp, payer: u8, team_id: &str, index: usize, to: u8, seed: u8) -> Value {
    let (status, body) = record(
        app,
        payer,
        team_id,
        &index.to_string(),
        &share_invoice(to, seed),
        &hex::encode(preimage(seed)),
        &metadata_for(to),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body
}

#[tokio::test]
async fn every_card_member_must_agree_the_prize() {
    let app = app().await;
    let (team_id, hash) = app.active_first_card().await;

    let proposed = put_prize_ok(&app, ALICE, &team_id, "100001").await;
    let (alice, bob, chidi) = (pubkey(ALICE), pubkey(BOB), pubkey(CHIDI));
    assert_eq!(
        proposed,
        json!({
            "team_id": team_id,
            "card_hash": hash,
            "prize_sats": "100001",
            "phase": "waiting-for-prize",
            "proposed_by": alice,
            "proposed_at": NOW,
            "agreed_at": null,
            "confirmed": [alice],
            "missing": [bob, chidi],
            "paid_count": 0,
            "items": [
                { "index": 0, "pubkey": alice, "name": "Alice",
                  "lightning_address": "alice@blink.sv", "amount_sats": "36001",
                  "status": "pending", "payment_hash": null, "paid_by": null, "paid_at": null },
                { "index": 1, "pubkey": bob, "name": "Bob",
                  "lightning_address": "bob@getalby.com", "amount_sats": "35000",
                  "status": "pending", "payment_hash": null, "paid_by": null, "paid_at": null },
                { "index": 2, "pubkey": chidi, "name": "Chidi",
                  "lightning_address": "chidi@walletofsatoshi.com", "amount_sats": "29000",
                  "status": "pending", "payment_hash": null, "paid_by": null, "paid_at": null },
            ],
        })
    );

    // Agreeing twice changes nothing.
    put_prize_ok(&app, ALICE, &team_id, "100001").await;
    app.clock.advance(5);
    let two = put_prize_ok(&app, BOB, &team_id, "100001").await;
    assert_eq!(two["confirmed"], json!([alice, bob]));
    assert_eq!(two["agreed_at"], Value::Null);

    app.clock.advance(5);
    let all = put_prize_ok(&app, CHIDI, &team_id, "100001").await;
    assert_eq!(all["agreed_at"], NOW + 10);
    assert_eq!(all["phase"], "prize-set-waiting");
    assert_eq!(all["missing"], json!([]));
    assert_eq!(payout(&app, BOB, &team_id).await.1, all);

    // An agreed prize is fixed.
    for sats in ["100001", "5000"] {
        let response = put_prize(&app, ALICE, &team_id, sats).await;
        assert_error(&response, StatusCode::CONFLICT, "prize_already_agreed");
        assert_eq!(response.1["details"]["prize_sats"], "100001");
    }
}

#[tokio::test]
async fn a_different_amount_starts_the_agreement_over() {
    let app = app().await;
    let (team_id, _) = app.active_first_card().await;
    put_prize_ok(&app, ALICE, &team_id, "1000").await;
    put_prize_ok(&app, CHIDI, &team_id, "1000").await;
    app.clock.advance(5);

    let counter = put_prize_ok(&app, BOB, &team_id, "2000").await;
    assert_eq!(counter["prize_sats"], "2000");
    assert_eq!(counter["proposed_by"], pubkey(BOB));
    assert_eq!(counter["proposed_at"], NOW + 5);
    assert_eq!(counter["confirmed"], json!([pubkey(BOB)]));
    let amounts: Vec<&str> = counter["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["amount_sats"].as_str().expect("amount"))
        .collect();
    assert_eq!(amounts, ["720", "700", "580"]);
}

#[tokio::test]
async fn proposing_needs_an_active_card_a_member_and_a_valid_amount() {
    let app = app().await;
    let team_id = app.alice_bob_chidi().await;
    assert_error(
        &put_prize(&app, ALICE, &team_id, "1000").await,
        StatusCode::CONFLICT,
        "no_active_card",
    );
    let hash = app.submit_ok(&team_id, &first_card()).await;
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&hash, signer).await;
    }

    // Dayo joined after the card was signed, so he is not on it.
    let (_, team) = app
        .authed(ALICE, Method::GET, &format!("/teams/{team_id}"), "")
        .await;
    let code = team["join_code"].as_str().expect("code").to_owned();
    let (status, _) = app.join(&team_id, &code, DAYO).await;
    assert_eq!(status, StatusCode::OK);
    assert_error(
        &put_prize(&app, DAYO, &team_id, "1000").await,
        StatusCode::FORBIDDEN,
        "not_a_card_member",
    );
    app.profile_ok(EVE).await;
    assert_error(
        &put_prize(&app, EVE, &team_id, "1000").await,
        StatusCode::FORBIDDEN,
        "not_a_card_member",
    );
    assert_error(
        &put_prize(&app, ALICE, "999", "1000").await,
        StatusCode::NOT_FOUND,
        "team_not_found",
    );

    for sats in [
        "0",
        "-1",
        "01",
        "1.5",
        "",
        " 1000",
        "1e3",
        "abc",
        "2100000000000001",
        "99999999999999999999",
    ] {
        let response = put_prize(&app, ALICE, &team_id, sats).await;
        assert_error(
            &response,
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_prize_sats",
        );
    }
    put_prize_ok(&app, ALICE, &team_id, "2100000000000000").await;
    let number = json!({ "prize_sats": 1000 }).to_string();
    assert_error(
        &app.authed(
            ALICE,
            Method::PUT,
            &format!("/teams/{team_id}/prize"),
            &number,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "invalid_request_json",
    );
    let response = put_prize(&app, ALICE, &team_id, "2").await;
    assert_error(
        &response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "prize_too_small",
    );
}

#[tokio::test]
async fn only_participants_see_the_payout() {
    let app = app().await;
    let (team_id, _) = app.active_first_card().await;
    assert_error(
        &payout(&app, BOB, &team_id).await,
        StatusCode::NOT_FOUND,
        "payout_not_found",
    );
    put_prize_ok(&app, ALICE, &team_id, "1000").await;
    let (status, body) = payout(&app, CHIDI, &team_id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_error(
        &payout(&app, EVE, &team_id).await,
        StatusCode::FORBIDDEN,
        "not_a_participant",
    );
    assert_error(
        &app.get(&format!("/teams/{team_id}/payout")).await,
        StatusCode::UNAUTHORIZED,
        "auth_missing",
    );
}

#[tokio::test]
async fn a_payment_proved_by_its_preimage_marks_the_item_paid() {
    let app = app().await;
    let team_id = agreed(&app).await;
    app.clock.advance(60);

    let paid = pay(&app, ALICE, &team_id, 1, BOB, 1).await;
    let item = &paid["items"][1];
    assert_eq!(item["status"], "paid");
    assert_eq!(
        item["payment_hash"],
        hex::encode(sha256::Hash::hash(&preimage(1)).to_byte_array())
    );
    assert_eq!(item["paid_by"], pubkey(ALICE));
    assert_eq!(item["paid_at"], NOW + 60);
    assert_eq!(paid["paid_count"], 1);
    assert_eq!(paid["phase"], "payouts-in-progress");
    assert_eq!(paid["items"][0]["status"], "pending");

    // The same payment again is accepted once; another one is refused.
    let (status, again) = record(
        &app,
        BOB,
        &team_id,
        "1",
        &share_invoice(BOB, 1),
        &hex::encode(preimage(1)),
        &metadata_for(BOB),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again, paid);
    let other = record(
        &app,
        ALICE,
        &team_id,
        "1",
        &share_invoice(BOB, 2),
        &hex::encode(preimage(2)),
        &metadata_for(BOB),
    )
    .await;
    assert_error(&other, StatusCode::CONFLICT, "item_already_paid");

    // One payment never pays two members.
    let both = metadata(&[person(BOB).1, person(CHIDI).1]);
    let bolt11 = invoice(
        Currency::Bitcoin,
        Some(100_000_000),
        &Description::Hash(&both),
        3,
    );
    let first = record(
        &app,
        ALICE,
        &team_id,
        "1",
        &bolt11,
        &hex::encode(preimage(3)),
        &both,
    )
    .await;
    assert_error(&first, StatusCode::CONFLICT, "item_already_paid");
    pay(&app, ALICE, &team_id, 0, ALICE, 4).await;
    let reused = invoice(
        Currency::Bitcoin,
        Some(100_000_000),
        &Description::Hash(&both),
        1,
    );
    let response = record(
        &app,
        ALICE,
        &team_id,
        "2",
        &reused,
        &hex::encode(preimage(1)),
        &both,
    )
    .await;
    assert_error(&response, StatusCode::CONFLICT, "payment_already_used");

    let done = pay(&app, CHIDI, &team_id, 2, CHIDI, 5).await;
    assert_eq!(done["paid_count"], 3);
    assert_eq!(done["phase"], "payouts-in-progress");
}

/// Payments to Bob's item that must be refused: the invoice, preimage and
/// metadata, and the error code and `details.reason` expected.
type Rejection = (String, String, String, &'static str, Option<&'static str>);

fn rejections() -> Vec<Rejection> {
    let bob = metadata_for(BOB);
    let good_preimage = hex::encode(preimage(1));
    vec![
        (
            invoice(
                Currency::Bitcoin,
                Some(99_999_000),
                &Description::Hash(&bob),
                1,
            ),
            good_preimage.clone(),
            bob.clone(),
            "invalid_invoice",
            Some("wrong_amount"),
        ),
        (
            invoice(Currency::Bitcoin, None, &Description::Hash(&bob), 1),
            good_preimage.clone(),
            bob.clone(),
            "invalid_invoice",
            Some("missing_amount"),
        ),
        (
            invoice(Currency::Bitcoin, Some(100_000_000), &Description::Plain, 1),
            good_preimage.clone(),
            bob.clone(),
            "invalid_invoice",
            Some("missing_description_hash"),
        ),
        (
            share_invoice(BOB, 1),
            good_preimage.clone(),
            metadata(&["bob@getalby.com", "x@y.z"]),
            "invalid_invoice",
            Some("wrong_description_hash"),
        ),
        (
            share_invoice(CHIDI, 1),
            good_preimage.clone(),
            metadata_for(CHIDI),
            "metadata_wrong_address",
            None,
        ),
        (
            invoice(
                Currency::Signet,
                Some(100_000_000),
                &Description::Hash(&bob),
                1,
            ),
            good_preimage.clone(),
            bob.clone(),
            "invalid_invoice",
            Some("wrong_network"),
        ),
        (
            "lnbc1garbage".to_owned(),
            good_preimage.clone(),
            bob.clone(),
            "invalid_invoice",
            Some("unparseable"),
        ),
        (
            share_invoice(BOB, 1),
            hex::encode(preimage(2)),
            bob.clone(),
            "preimage_mismatch",
            None,
        ),
        (
            share_invoice(BOB, 1),
            "g".repeat(64),
            bob.clone(),
            "invalid_preimage_hex",
            None,
        ),
    ]
}

#[tokio::test]
async fn payments_that_do_not_prove_the_member_was_paid_are_refused() {
    let app = app().await;
    let team_id = agreed(&app).await;
    let good_preimage = hex::encode(preimage(1));
    let cases = rejections();
    for (bolt11, preimage_hex, metadata, code, reason) in cases {
        let response = record(
            &app,
            ALICE,
            &team_id,
            "1",
            &bolt11,
            &preimage_hex,
            &metadata,
        )
        .await;
        assert_error(&response, StatusCode::UNPROCESSABLE_ENTITY, code);
        if let Some(reason) = reason {
            assert_eq!(response.1["details"]["reason"], reason, "{}", response.1);
        }
    }
    let (_, state) = payout(&app, BOB, &team_id).await;
    assert_eq!(state["paid_count"], 0);

    // Mixed-case identifiers in the metadata still name the address.
    let shouty = metadata(&["Bob@GetAlby.com"]);
    let bolt11 = invoice(
        Currency::Bitcoin,
        Some(100_000_000),
        &Description::Hash(&shouty),
        1,
    );
    let (status, body) = record(&app, ALICE, &team_id, "1", &bolt11, &good_preimage, &shouty).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn payments_need_an_agreed_prize_a_card_member_and_a_real_item() {
    let app = app().await;
    let team_id = equal_card(&app).await;
    let bob = metadata_for(BOB);
    let good = hex::encode(preimage(1));
    let attempt = |payer: u8, index: &'static str| {
        let (team_id, bob, good) = (team_id.clone(), bob.clone(), good.clone());
        let app = &app;
        async move {
            record(
                app,
                payer,
                &team_id,
                index,
                &share_invoice(BOB, 1),
                &good,
                &bob,
            )
            .await
        }
    };
    assert_error(
        &attempt(ALICE, "1").await,
        StatusCode::NOT_FOUND,
        "payout_not_found",
    );
    put_prize_ok(&app, ALICE, &team_id, "300000").await;
    assert_error(
        &attempt(ALICE, "1").await,
        StatusCode::CONFLICT,
        "prize_not_agreed",
    );
    for number in [BOB, CHIDI] {
        put_prize_ok(&app, number, &team_id, "300000").await;
    }
    app.profile_ok(EVE).await;
    assert_error(
        &attempt(EVE, "1").await,
        StatusCode::FORBIDDEN,
        "not_a_card_member",
    );
    for index in ["3", "x", "-1"] {
        assert_error(
            &attempt(ALICE, index).await,
            StatusCode::NOT_FOUND,
            "payout_item_not_found",
        );
    }
    let (status, _) = attempt(CHIDI, "1").await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn an_agreed_prize_stops_the_card_being_replaced() {
    let app = app().await;
    let (team_id, active) = app.active_first_card().await;
    let replacement = card(&[(ALICE, 50), (BOB, 50)], Some(&active));
    let early = app.submit_ok(&team_id, &replacement).await;
    for number in [ALICE, BOB, CHIDI] {
        put_prize_ok(&app, number, &team_id, "1000").await;
    }

    let later = card(&[(ALICE, 60), (BOB, 40)], Some(&active));
    let response = app.submit_card(ALICE, &team_id, &later).await;
    assert_error(&response, StatusCode::CONFLICT, "prize_agreed");
    // A replacement drafted before the agreement can no longer be signed.
    let response = app
        .sign(&early, &common::signature(ALICE, &early, app.now()))
        .await;
    assert_error(&response, StatusCode::CONFLICT, "prize_agreed");
    let (_, team) = app.get(&format!("/teams/{team_id}")).await;
    assert_eq!(team["active_card_hash"], active.as_str());
}

#[tokio::test]
async fn a_proposal_goes_with_the_card_it_was_for() {
    let app = app().await;
    let (team_id, active) = app.active_first_card().await;
    put_prize_ok(&app, ALICE, &team_id, "1000").await;
    let replacement = app
        .submit_ok(&team_id, &card(&[(ALICE, 50), (BOB, 50)], Some(&active)))
        .await;
    // Superseding needs the removed member's signature too.
    for signer in [ALICE, BOB, CHIDI] {
        app.sign_ok(&replacement, signer).await;
    }
    assert_error(
        &payout(&app, ALICE, &team_id).await,
        StatusCode::NOT_FOUND,
        "payout_not_found",
    );
    let proposed = put_prize_ok(&app, BOB, &team_id, "1000").await;
    assert_eq!(proposed["card_hash"], replacement.as_str());
    assert_eq!(proposed["items"].as_array().map(Vec::len), Some(2));
}

#[tokio::test]
async fn me_shows_the_payout_phase_and_what_each_member_received() {
    let app = app().await;
    let team_id = equal_card(&app).await;
    let me = |number: u8| {
        let app = &app;
        async move {
            let (status, body) = app.authed(number, Method::GET, "/me", "").await;
            assert_eq!(status, StatusCode::OK, "{body}");
            body
        }
    };
    let team = me(BOB).await["teams"][0].clone();
    assert_eq!(team["phase"], "waiting-for-prize");
    put_prize_ok(&app, ALICE, &team_id, "300000").await;
    assert_eq!(me(BOB).await["teams"][0]["phase"], "waiting-for-prize");
    put_prize_ok(&app, BOB, &team_id, "300000").await;
    app.clock.advance(30);
    put_prize_ok(&app, CHIDI, &team_id, "300000").await;
    let bob = me(BOB).await;
    assert_eq!(bob["teams"][0]["list_status"], "locked");
    assert_eq!(bob["teams"][0]["phase"], "prize-set-waiting");
    assert_eq!(bob["teams"][0]["updated_at"], NOW + 30);

    app.clock.advance(30);
    pay(&app, ALICE, &team_id, 1, BOB, 1).await;
    let bob = me(BOB).await;
    let team = &bob["teams"][0];
    assert_eq!(team["list_status"], "paid-out");
    assert_eq!(team["phase"], "payouts-in-progress");
    assert_eq!(team["you_received_sats"], "100000");
    assert_eq!(team["updated_at"], NOW + 60);
    assert_eq!(
        bob["summary"],
        json!({ "received_sats": "100000", "paid_team_count": 1 })
    );
    let alice = me(ALICE).await;
    assert_eq!(alice["teams"][0]["list_status"], "paid-out");
    assert_eq!(alice["teams"][0]["you_received_sats"], Value::Null);
    assert_eq!(
        alice["summary"],
        json!({ "received_sats": "0", "paid_team_count": 0 })
    );
}
