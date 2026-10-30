//! Generates `tests/fixtures/parity.json` by calling `splitcard-core` natively,
//! and fails if the committed file differs.
//!
//! Regenerate with:
//!
//! ```sh
//! SPLITCARD_UPDATE_FIXTURES=1 cargo test -p splitcard-wasm --test parity_fixtures
//! ```
//!
//! Expected codes and details are written out by hand here rather than taken
//! from `splitcard_wasm::error`, so this also checks the wrapper's mapping.

#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::fmt::Display;

use nostr::{
    event::{Event, Kind, Tag, UnsignedEvent},
    key::Keys,
    types::Timestamp,
};
use secp256k1::Secp256k1;
use serde_json::{Value, json};
use splitcard_core::{
    card::{Card, CardV1, CardV2, LeaveRule, Member, MemberV2, Payout, PeerReview, Rules, RulesV2},
    hash::card_hash_hex,
    signature::{SIGNATURE_KIND, Signer, unsigned_signature_template, verify_signature_event},
    split::{compute_payouts, display_basis_points},
    status::{CardStatus, card_status},
    supersede::check_supersede,
    validate::{MAX_LABEL_CHARS, MAX_WEIGHT, validate},
};
use splitcard_wasm::error::ERROR_CODES;

const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/parity.json");
const SIGNED_AT: u64 = 1_700_000_000;
const PHASE_1_CARD_HASH: &str = "37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52";
/// The pinned event from splitcard-core's `signature_test_vector_is_pinned`.
const PHASE_2_EVENT: &str = r#"{"id":"efd8b98c288e4dc8f9e2efa9c7647a6bbdfb50208bdeab0be5321d85aa23f9e8","pubkey":"79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798","created_at":1700000000,"kind":30078,"tags":[["d","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"],["splitcard","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"]],"content":"","sig":"4aeaf8c05e75c8f2e1ea26b8057299b9d46966592a161c6e3756f7de2688599d4bd90d4c2c00492f1b125a94a0358c2620446b8acbb1b4808ff6f834e74b3ee0"}"#;

/// Codes no JS input can reach: core validates before splitting, a parsed card
/// always serializes, and core's own tags always parse.
const UNREACHABLE_CODES: &[&str] = &[
    "output_serialization",
    "no_weights",
    "split_zero_weight",
    "hash_serialize",
    "tag_construction",
];

/// Codes covered by the wasm boundary tests instead, because their messages
/// come from JS-side conversion that cannot run natively.
const BOUNDARY_ONLY_CODES: &[&str] = &[
    "invalid_created_at",
    // Keys and signing; covered by the unit tests in `src/keys.rs`.
    "invalid_secret_key",
    "invalid_aux_rand",
    "invalid_unsigned_event",
    "event_pubkey_mismatch",
];

// Inputs.

fn phase_1_card() -> Card {
    let member = |number: u8, name: &str, weight: u64, address: &str| Member {
        pubkey: format!("{number:064x}"),
        name: name.to_owned(),
        weight,
        payout: Payout::Lightning {
            address: address.to_owned(),
        },
    };
    Card::V1(CardV1 {
        version: 1,
        card_id: "btcpp-lagos-2026/team-42".to_owned(),
        event: "BTC++ Lagos 2026".to_owned(),
        team: "Team 42".to_owned(),
        members: vec![
            member(1, "Ada", 36, "ada@example.com"),
            member(2, "Bola", 35, "bola@example.org"),
            member(3, "Chidi", 29, "chidi@example.net"),
        ],
        rules: Rules {
            peer_review: Some(PeerReview {
                max_shift_points: 10,
            }),
        },
        supersedes: Some("a".repeat(64)),
    })
}

fn card_with_weights(weights: [u64; 3]) -> Card {
    let mut card = phase_1_card();
    for (member, weight) in v1(&mut card).members.iter_mut().zip(weights) {
        member.weight = weight;
    }
    card
}

/// Weights above `MAX_WEIGHT`: still parse and hash exactly, but fail validation.
fn big_weight_card() -> Card {
    card_with_weights([u64::MAX, u64::MAX - 1, 1])
}

/// The largest weights a valid card can hold.
fn max_weight_card() -> Card {
    card_with_weights([MAX_WEIGHT, MAX_WEIGHT - 1, 1])
}

fn keys(number: u8) -> Keys {
    Keys::parse(&format!("{number:064x}"))
        .unwrap_or_else(|error| panic!("fixed secret key must parse: {error}"))
}

fn member_pubkey(number: u8) -> String {
    keys(number).public_key().to_hex()
}

fn signed_card(numbers: &[u8]) -> Card {
    Card::V1(CardV1 {
        version: 1,
        card_id: "phase-3/team-1".to_owned(),
        event: "Phase 3".to_owned(),
        team: "Team 1".to_owned(),
        members: numbers
            .iter()
            .zip(1_u64..)
            .map(|(number, weight)| Member {
                pubkey: member_pubkey(*number),
                name: format!("Member {number}"),
                weight,
                payout: Payout::Lightning {
                    address: format!("member{number}@example.com"),
                },
            })
            .collect(),
        rules: Rules { peer_review: None },
        supersedes: None,
    })
}

fn tag(parts: &[&str]) -> Tag {
    Tag::parse(parts.iter().copied()).unwrap_or_else(|error| panic!("tag must parse: {error}"))
}

/// Signs with all-zero BIP-340 aux randomness so the fixtures are reproducible.
fn sign(key_number: u8, unsigned: UnsignedEvent) -> Event {
    let id = unsigned.compute_id();
    let sig = keys(key_number).sign_schnorr_with_aux_rand(
        &Secp256k1::signing_only(),
        id.as_bytes(),
        &[0; 32],
    );
    unsigned
        .add_signature(sig)
        .unwrap_or_else(|error| panic!("event must sign: {error}"))
}

fn event(key_number: u8, kind: u16, tags: Vec<Tag>) -> Event {
    let unsigned = UnsignedEvent::new(
        keys(key_number).public_key(),
        Timestamp::from_secs(SIGNED_AT),
        Kind::from_u16(kind),
        tags,
        "I agree to this split.",
    );
    sign(key_number, unsigned)
}

fn signature(card: &Card, key_number: u8) -> Event {
    let hash = hash(card);
    event(
        key_number,
        SIGNATURE_KIND,
        vec![tag(&["d", &hash]), tag(&["splitcard", &hash])],
    )
}

/// The version 1 card inside `card`, for cases that change one field.
fn v1(card: &mut Card) -> &mut CardV1 {
    match card {
        Card::V1(card) => card,
        Card::V2(_) => panic!("expected a version 1 card"),
    }
}

fn hash(card: &Card) -> String {
    card_hash_hex(card).unwrap_or_else(|error| panic!("card must hash: {error}"))
}

fn card_json(card: &Card) -> String {
    serde_json::to_string(card).unwrap_or_else(|error| panic!("card must serialize: {error}"))
}

fn event_json(event: &Event) -> String {
    serde_json::to_string(event).unwrap_or_else(|error| panic!("event must serialize: {error}"))
}

fn events_json(events: &[Event]) -> String {
    serde_json::to_string(events).unwrap_or_else(|error| panic!("events must serialize: {error}"))
}

fn pubkeys(numbers: &[u8]) -> Vec<String> {
    numbers
        .iter()
        .map(|number| member_pubkey(*number))
        .collect()
}

// Expected outputs.

struct Cases(Vec<Value>);

// Callers pass temporaries; `json!` serializes them by reference.
#[allow(clippy::needless_pass_by_value)]
impl Cases {
    fn ok(&mut self, name: &str, function: &str, args: Value, expected: Value) {
        self.0.push(json!({
            "name": name,
            "function": function,
            "args": args,
            "expected": { "ok": expected },
        }));
    }

    fn error(
        &mut self,
        name: &str,
        function: &str,
        args: Value,
        code: &str,
        message: impl Display,
        details: Value,
    ) {
        self.0.push(json!({
            "name": name,
            "function": function,
            "args": args,
            "expected": { "error": { "code": code, "message": message.to_string(), "details": details } },
        }));
    }
}

fn core_err<T, E: Display>(result: Result<T, E>) -> E {
    match result {
        Ok(_) => panic!("core call was expected to fail"),
        Err(error) => error,
    }
}

fn card_json_error(json: &str) -> serde_json::Error {
    core_err(Card::from_json(json))
}

fn json_position(error: &serde_json::Error) -> Value {
    json!({ "line": error.line(), "column": error.column() })
}

fn payouts(card: &Card, prize_sats: u64) -> Value {
    let payouts = compute_payouts(card, prize_sats)
        .unwrap_or_else(|error| panic!("payouts must compute: {error}"));
    payouts
        .into_iter()
        .map(|(pubkey, amount)| json!({ "pubkey": pubkey, "amount_sats": amount.to_string() }))
        .collect()
}

fn basis_points(card: &Card) -> Value {
    json!(
        display_basis_points(&card.weights()).unwrap_or_else(|error| panic!("must split: {error}"))
    )
}

fn signer_json(signer: &Signer) -> Value {
    json!({ "pubkey": signer.pubkey, "lightning_address": signer.lightning_address })
}

fn signers_json(signers: &[Signer]) -> Value {
    signers.iter().map(signer_json).collect()
}

fn status(card: &Card, events: &[Event], rejected_codes: &[&str]) -> Value {
    let report = card_status(card, events).unwrap_or_else(|error| panic!("status: {error}"));
    assert_eq!(report.rejected.len(), rejected_codes.len());
    let rejected: Vec<Value> = report
        .rejected
        .iter()
        .zip(rejected_codes)
        .map(|((id, error), code)| {
            json!({ "event_id": id.to_hex(), "code": code, "message": error.to_string() })
        })
        .collect();
    match report.status {
        CardStatus::Draft { signed, missing } => {
            json!({ "status": "draft", "signed": signers_json(&signed), "missing": missing, "rejected": rejected })
        }
        CardStatus::Active { signed } => {
            json!({ "status": "active", "signed": signers_json(&signed), "rejected": rejected })
        }
    }
}

fn card_hash_cases(cases: &mut Cases) {
    let card = phase_1_card();
    assert_eq!(hash(&card), PHASE_1_CARD_HASH);
    cases.ok(
        "card_hash: pinned Phase 1 card",
        "card_hash",
        json!([card_json(&card)]),
        json!(PHASE_1_CARD_HASH),
    );
    cases.ok(
        "card_hash: weights above 2^53 hash exactly (hashing does not validate)",
        "card_hash",
        json!([card_json(&big_weight_card())]),
        json!(hash(&big_weight_card())),
    );
    let error = card_json_error("{");
    cases.error(
        "card_hash: malformed card JSON",
        "card_hash",
        json!(["{"]),
        "invalid_card_json",
        &error,
        json_position(&error),
    );
}

fn validate_cases(cases: &mut Cases) {
    let ok = phase_1_card();
    cases.ok(
        "validate_card: valid card",
        "validate_card",
        json!([card_json(&ok)]),
        Value::Null,
    );

    let mut invalid: Vec<(&str, Card, Value)> = Vec::new();
    let mut card = phase_1_card();
    // Version 2 JSON parses as a version 2 card, so use 3 here.
    v1(&mut card).version = 3;
    invalid.push(("unsupported_version", card, json!({ "version": 3 })));
    let mut card = phase_1_card();
    v1(&mut card).team.clear();
    invalid.push(("empty_field", card, json!({ "field": "team" })));
    let mut card = phase_1_card();
    v1(&mut card).members.truncate(1);
    invalid.push(("invalid_member_count", card, json!({ "count": 1 })));
    let mut card = phase_1_card();
    v1(&mut card).members[1].pubkey = "A".repeat(64);
    invalid.push(("invalid_pubkey", card, json!({ "index": 1 })));
    let mut card = phase_1_card();
    let first = v1(&mut card).members[0].pubkey.clone();
    v1(&mut card).members[2].pubkey = first;
    invalid.push(("duplicate_pubkey", card, json!({ "index": 2 })));
    let mut card = phase_1_card();
    v1(&mut card).members[0].name.clear();
    invalid.push(("empty_member_name", card, json!({ "index": 0 })));
    let mut card = phase_1_card();
    v1(&mut card).members[1].weight = 0;
    invalid.push(("zero_weight", card, json!({ "index": 1 })));
    let mut card = phase_1_card();
    v1(&mut card).members[2].payout = Payout::Lightning {
        address: "not-an-address".to_owned(),
    };
    invalid.push(("invalid_lightning_address", card, json!({ "index": 2 })));
    let mut card = phase_1_card();
    v1(&mut card).supersedes = Some("xyz".to_owned());
    invalid.push(("invalid_supersedes", card, Value::Null));
    let mut card = phase_1_card();
    v1(&mut card).rules.peer_review = Some(PeerReview {
        max_shift_points: 51,
    });
    invalid.push(("invalid_peer_review_max_shift_points", card, Value::Null));

    for (code, card, details) in invalid {
        let error = core_err(validate(&card));
        cases.error(
            &format!("validate_card: {code}"),
            "validate_card",
            json!([card_json(&card)]),
            code,
            error,
            details,
        );
    }

    // Accepted by core before LUD-16 validation, but rejected at payout.
    let mut card = phase_1_card();
    v1(&mut card).members[0].payout = Payout::Lightning {
        address: "Alice!@Blink.sv".to_owned(),
    };
    let error = core_err(validate(&card));
    cases.error(
        "validate_card: non-LUD-16 address Alice!@Blink.sv",
        "validate_card",
        json!([card_json(&card)]),
        "invalid_lightning_address",
        error,
        json!({ "index": 0 }),
    );

    let unknown_field = r#"{"version":1,"surprise":true}"#;
    let error = card_json_error(unknown_field);
    cases.error(
        "validate_card: unknown field is invalid card JSON",
        "validate_card",
        json!([unknown_field]),
        "invalid_card_json",
        &error,
        json_position(&error),
    );
}

fn payout_cases(cases: &mut Cases) {
    let card = phase_1_card();
    let expected = payouts(&card, 100_001);
    assert_eq!(
        expected,
        json!([
            { "pubkey": card.pubkeys()[0], "amount_sats": "36001" },
            { "pubkey": card.pubkeys()[1], "amount_sats": "35000" },
            { "pubkey": card.pubkeys()[2], "amount_sats": "29000" },
        ])
    );
    cases.ok(
        "compute_payouts: 100,001 sats at 36/35/29",
        "compute_payouts",
        json!([card_json(&card), "100001"]),
        expected,
    );

    // Weights above MAX_WEIGHT are rejected before any split.
    let big = big_weight_card();
    let prize = 2_100_000_000_000_000_u64;
    let error = core_err(compute_payouts(&big, prize));
    cases.error(
        "compute_payouts: 21M BTC at [u64::MAX, u64::MAX - 1, 1]",
        "compute_payouts",
        json!([card_json(&big), prize.to_string()]),
        "weight_too_large",
        error,
        json!({ "index": 0, "weight": u64::MAX.to_string() }),
    );

    // At the largest valid weights, 21M BTC leaves the weight-1 member 0 sats.
    let max = max_weight_card();
    let error = core_err(compute_payouts(&max, prize));
    cases.error(
        "compute_payouts: 21M BTC at [MAX_WEIGHT, MAX_WEIGHT - 1, 1]",
        "compute_payouts",
        json!([card_json(&max), prize.to_string()]),
        "share_rounds_to_zero",
        error,
        json!({ "index": 2, "pubkey": max.pubkeys()[2] }),
    );

    // 21M BTC is below 2^53; a u64::MAX prize proves amounts above 2^53 survive.
    let expected = payouts(&max, u64::MAX);
    let above_max_safe = expected
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|payout| payout["amount_sats"].as_str()?.parse::<u64>().ok())
        .any(|amount| amount > MAX_WEIGHT);
    assert!(
        above_max_safe,
        "the u64::MAX prize must produce amounts above 2^53"
    );
    cases.ok(
        "compute_payouts: u64::MAX prize at [MAX_WEIGHT, MAX_WEIGHT - 1, 1]",
        "compute_payouts",
        json!([card_json(&max), u64::MAX.to_string()]),
        expected,
    );

    let error = core_err(compute_payouts(&card, 2));
    cases.error(
        "compute_payouts: prize_too_small",
        "compute_payouts",
        json!([card_json(&card), "2"]),
        "prize_too_small",
        error,
        json!({ "prize_sats": "2", "member_count": 3 }),
    );

    let mut zero = phase_1_card();
    v1(&mut zero).members[0].weight = 0;
    let error = core_err(compute_payouts(&zero, 100));
    cases.error(
        "compute_payouts: validates the card first",
        "compute_payouts",
        json!([card_json(&zero), "100"]),
        "zero_weight",
        error,
        json!({ "index": 0 }),
    );

    for bad in [
        "1e3",
        "-1",
        " 5",
        "5 ",
        "1.0",
        "+5",
        "",
        "007",
        "18446744073709551616",
    ] {
        cases.error(
            &format!("compute_payouts: invalid_amount {bad:?}"),
            "compute_payouts",
            json!([card_json(&card), bad]),
            "invalid_amount",
            "amount must be a non-negative integer string with no sign, spaces, \
             decimal point, exponent or leading zeros, at most 18446744073709551615",
            json!({ "value": bad }),
        );
    }
}

fn basis_point_cases(cases: &mut Cases) {
    let card = phase_1_card();
    let expected = basis_points(&card);
    assert_eq!(expected, json!([3600, 3500, 2900]));
    cases.ok(
        "display_basis_points: 36/35/29",
        "display_basis_points",
        json!([card_json(&card)]),
        expected,
    );
    let max = max_weight_card();
    cases.ok(
        "display_basis_points: MAX_WEIGHT weights",
        "display_basis_points",
        json!([card_json(&max)]),
        basis_points(&max),
    );
    let big = big_weight_card();
    let error = core_err(validate(&big));
    cases.error(
        "display_basis_points: weight_too_large",
        "display_basis_points",
        json!([card_json(&big)]),
        "weight_too_large",
        error,
        json!({ "index": 0, "weight": u64::MAX.to_string() }),
    );
    let mut empty_name = phase_1_card();
    v1(&mut empty_name).members[1].name.clear();
    let error = core_err(validate(&empty_name));
    cases.error(
        "display_basis_points: validates the card first",
        "display_basis_points",
        json!([card_json(&empty_name)]),
        "empty_member_name",
        error,
        json!({ "index": 1 }),
    );
}

fn template_cases(cases: &mut Cases) {
    let pubkey = member_pubkey(1);
    let template = unsigned_signature_template(PHASE_1_CARD_HASH, &pubkey, SIGNED_AT, None)
        .unwrap_or_else(|error| panic!("template must build: {error}"));
    let pinned = event_json(&sign(1, template.clone()));
    assert_eq!(pinned, PHASE_2_EVENT);
    cases.ok(
        "signature_template: pinned Phase 2 vector",
        "signature_template",
        json!([PHASE_1_CARD_HASH, pubkey, SIGNED_AT]),
        json!({
            "pubkey": template.pubkey.to_hex(),
            "created_at": template.created_at.as_secs(),
            "kind": template.kind.as_u16(),
            "tags": template.tags.iter().map(|tag| tag.as_slice().to_vec()).collect::<Vec<_>>(),
            "content": template.content,
        }),
    );
    let error = core_err(unsigned_signature_template("ABC", &pubkey, SIGNED_AT, None));
    cases.error(
        "signature_template: invalid_card_hash",
        "signature_template",
        json!(["ABC", pubkey, SIGNED_AT]),
        "invalid_card_hash",
        error,
        Value::Null,
    );
    let off_curve = "f".repeat(64);
    let error = core_err(unsigned_signature_template(
        PHASE_1_CARD_HASH,
        &off_curve,
        SIGNED_AT,
        None,
    ));
    cases.error(
        "signature_template: invalid_template_pubkey",
        "signature_template",
        json!([PHASE_1_CARD_HASH, off_curve, SIGNED_AT]),
        "invalid_template_pubkey",
        error,
        Value::Null,
    );
}

fn verify_cases(cases: &mut Cases) {
    // The pinned event signs the Phase 1 card, whose members are placeholder
    // keys, so it passes every cryptographic and tag check, then fails membership.
    let phase_1 = phase_1_card();
    let pinned: Event = serde_json::from_str(PHASE_2_EVENT)
        .unwrap_or_else(|error| panic!("pinned event must parse: {error}"));
    let error = core_err(verify_signature_event(&phase_1, &pinned));
    cases.error(
        "verify_signature: pinned Phase 2 event against the Phase 1 card",
        "verify_signature",
        json!([card_json(&phase_1), PHASE_2_EVENT]),
        "non_member",
        error,
        Value::Null,
    );

    let card = signed_card(&[1, 2, 3]);
    let card_hash = hash(&card);
    let valid = signature(&card, 2);
    let signer = verify_signature_event(&card, &valid)
        .unwrap_or_else(|error| panic!("valid event must verify: {error}"));
    assert_eq!(signer.pubkey, member_pubkey(2));
    cases.ok(
        "verify_signature: valid member signature",
        "verify_signature",
        json!([card_json(&card), event_json(&valid)]),
        signer_json(&signer),
    );

    let d = |hash: &str| tag(&["d", hash]);
    let splitcard = |hash: &str| tag(&["splitcard", hash]);
    let other = "b".repeat(64);
    let mut wrong_id = valid.clone();
    wrong_id.content.push('!');
    let mut bad_sig = valid.clone();
    bad_sig.sig = signature(&card, 3).sig;
    let failures: Vec<(&str, Event)> = vec![
        ("invalid_event_id", wrong_id),
        ("invalid_signature", bad_sig),
        (
            "wrong_kind",
            event(2, 1, vec![d(&card_hash), splitcard(&card_hash)]),
        ),
        (
            "missing_d_tag",
            event(2, SIGNATURE_KIND, vec![splitcard(&card_hash)]),
        ),
        (
            "multiple_d_tags",
            event(
                2,
                SIGNATURE_KIND,
                vec![d(&card_hash), d(&card_hash), splitcard(&card_hash)],
            ),
        ),
        (
            "wrong_d_tag_hash",
            event(2, SIGNATURE_KIND, vec![d(&other), splitcard(&card_hash)]),
        ),
        (
            "missing_splitcard_tag",
            event(2, SIGNATURE_KIND, vec![d(&card_hash)]),
        ),
        (
            "multiple_splitcard_tags",
            event(
                2,
                SIGNATURE_KIND,
                vec![d(&card_hash), splitcard(&card_hash), splitcard(&card_hash)],
            ),
        ),
        (
            "wrong_splitcard_tag_hash",
            event(2, SIGNATURE_KIND, vec![d(&card_hash), splitcard(&other)]),
        ),
        ("non_member", signature(&card, 4)),
    ];
    for (code, event) in failures {
        let error = core_err(verify_signature_event(&card, &event));
        cases.error(
            &format!("verify_signature: {code}"),
            "verify_signature",
            json!([card_json(&card), event_json(&event)]),
            code,
            error,
            Value::Null,
        );
    }

    let malformed = r#"{"id":"not an event"}"#;
    let error = core_err(serde_json::from_str::<Event>(malformed));
    cases.error(
        "verify_signature: malformed event JSON",
        "verify_signature",
        json!([card_json(&card), malformed]),
        "invalid_event_json",
        &error,
        json_position(&error),
    );
}

fn status_cases(cases: &mut Cases) {
    let card = signed_card(&[1, 2, 3]);
    let outsider = signature(&card, 4);
    let draft = [signature(&card, 1), outsider.clone()];
    let expected = status(&card, &draft, &["non_member"]);
    assert_eq!(expected["missing"], json!(pubkeys(&[2, 3])));
    cases.ok(
        "card_status: draft with a rejected non-member",
        "card_status",
        json!([card_json(&card), events_json(&draft)]),
        expected,
    );

    let card_hash = hash(&card);
    let wrong_kind = event(
        3,
        1,
        vec![tag(&["d", &card_hash]), tag(&["splitcard", &card_hash])],
    );
    let active = [
        signature(&card, 3),
        wrong_kind,
        signature(&card, 1),
        signature(&card, 1),
        signature(&card, 2),
        outsider,
    ];
    let expected = status(&card, &active, &["wrong_kind", "non_member"]);
    assert_eq!(expected["status"], "active");
    cases.ok(
        "card_status: active with rejected events in input order",
        "card_status",
        json!([card_json(&card), events_json(&active)]),
        expected,
    );

    let mut one_member = card.clone();
    v1(&mut one_member).members.truncate(1);
    let error = core_err(card_status(&one_member, &[]));
    cases.error(
        "card_status: validates the card first",
        "card_status",
        json!([card_json(&one_member), "[]"]),
        "invalid_member_count",
        error,
        json!({ "count": 1 }),
    );

    let not_array = event_json(&signature(&card, 1));
    let error = core_err(serde_json::from_str::<Vec<Event>>(&not_array));
    cases.error(
        "card_status: events must be a JSON array",
        "card_status",
        json!([card_json(&card), not_array]),
        "invalid_event_json",
        &error,
        json_position(&error),
    );
}

fn supersede_cases(cases: &mut Cases) {
    let old = signed_card(&[1, 2, 3]);
    let mut new = signed_card(&[1, 2, 4]);
    v1(&mut new).supersedes = Some(hash(&old));
    let all: Vec<Event> = [1, 2, 3, 4].map(|number| signature(&new, number)).to_vec();
    assert!(check_supersede(&old, &new, &all).is_ok());
    cases.ok(
        "check_supersede: valid replacement",
        "check_supersede",
        json!([card_json(&old), card_json(&new), events_json(&all)]),
        Value::Null,
    );

    let mut wrong = new.clone();
    v1(&mut wrong).supersedes = Some("c".repeat(64));
    let wrong_events: Vec<Event> = [1, 2, 3, 4]
        .map(|number| signature(&wrong, number))
        .to_vec();
    let error = core_err(check_supersede(&old, &wrong, &wrong_events));
    cases.error(
        "check_supersede: wrong_supersedes_hash",
        "check_supersede",
        json!([
            card_json(&old),
            card_json(&wrong),
            events_json(&wrong_events)
        ]),
        "wrong_supersedes_hash",
        error,
        Value::Null,
    );

    let without_new: Vec<Event> = [1, 2, 3].map(|number| signature(&new, number)).to_vec();
    let error = core_err(check_supersede(&old, &new, &without_new));
    cases.error(
        "check_supersede: new member did not sign",
        "check_supersede",
        json!([card_json(&old), card_json(&new), events_json(&without_new)]),
        "missing_new_member_signatures",
        error,
        json!({ "missing": pubkeys(&[4]) }),
    );

    let without_removed: Vec<Event> = [1, 2, 4].map(|number| signature(&new, number)).to_vec();
    let error = core_err(check_supersede(&old, &new, &without_removed));
    cases.error(
        "check_supersede: removed member did not sign",
        "check_supersede",
        json!([
            card_json(&old),
            card_json(&new),
            events_json(&without_removed)
        ]),
        "missing_old_member_signatures",
        error,
        json!({ "missing": pubkeys(&[3]) }),
    );

    let mut bad_old = old.clone();
    v1(&mut bad_old).version = 7;
    let cause = core_err(validate(&bad_old));
    let error = core_err(check_supersede(&bad_old, &new, &all));
    cases.error(
        "check_supersede: invalid_old_card",
        "check_supersede",
        json!([card_json(&bad_old), card_json(&new), events_json(&all)]),
        "invalid_old_card",
        error,
        json!({ "cause": { "code": "unsupported_version", "message": cause.to_string(), "details": { "version": 7 } } }),
    );

    let mut bad_new = new.clone();
    v1(&mut bad_new).card_id.clear();
    let cause = core_err(validate(&bad_new));
    let error = core_err(check_supersede(&old, &bad_new, &all));
    cases.error(
        "check_supersede: invalid_new_card",
        "check_supersede",
        json!([card_json(&old), card_json(&bad_new), events_json(&all)]),
        "invalid_new_card",
        error,
        json!({ "cause": { "code": "empty_field", "message": cause.to_string(), "details": { "field": "card_id" } } }),
    );

    let error = card_json_error("[]");
    cases.error(
        "check_supersede: malformed old card JSON",
        "check_supersede",
        json!(["[]", card_json(&new), events_json(&all)]),
        "invalid_card_json",
        &error,
        json_position(&error),
    );
}

// Version 2 cards: members by pubkey, payout addresses in signatures.

fn v2_card(numbers: &[u8], weights: &[u64]) -> Card {
    Card::V2(CardV2 {
        version: 2,
        card_id: "lagos-2026/llamas".to_owned(),
        event: "Lagos 2026".to_owned(),
        team: "Llamas".to_owned(),
        members: numbers
            .iter()
            .zip(weights)
            .map(|(number, weight)| MemberV2 {
                pubkey: member_pubkey(*number),
                weight: *weight,
                label: (*number != 3).then(|| format!("Member {number}")),
            })
            .collect(),
        rules: RulesV2 {
            peer_review: None,
            leave: LeaveRule::KeepPercent { keep_percent: 15 },
        },
        supersedes: None,
    })
}

/// The version 2 card inside `card`, for cases that change one field.
fn v2(card: &mut Card) -> &mut CardV2 {
    match card {
        Card::V2(card) => card,
        Card::V1(_) => panic!("expected a version 2 card"),
    }
}

fn lud16(number: u8) -> String {
    format!("member{number}@example.com")
}

/// A version 2 signature with the given `lud16` tags.
fn v2_signature_with(card: &Card, key_number: u8, lud16_tags: &[&str]) -> Event {
    let hash = hash(card);
    let mut tags = vec![tag(&["d", &hash]), tag(&["splitcard", &hash])];
    tags.extend(lud16_tags.iter().map(|address| tag(&["lud16", address])));
    event(key_number, SIGNATURE_KIND, tags)
}

fn v2_signature(card: &Card, key_number: u8) -> Event {
    v2_signature_with(card, key_number, &[&lud16(key_number)])
}

fn v2_cases(cases: &mut Cases) {
    let card = v2_card(&[1, 2, 3], &[333, 333, 334]);
    let json = card_json(&card);
    assert!(json.contains(r#""leave":{"kind":"keep_percent","keep_percent":15}"#));
    assert!(!json.contains("label\":null"), "member 3 has no label key");
    cases.ok(
        "card_hash: version 2 card",
        "card_hash",
        json!([json]),
        json!(hash(&card)),
    );
    cases.ok(
        "validate_card: valid version 2 card",
        "validate_card",
        json!([json]),
        Value::Null,
    );
    cases.ok(
        "display_basis_points: version 2 at 333/333/334",
        "display_basis_points",
        json!([json]),
        basis_points(&card),
    );
    let equal = v2_card(&[1, 2, 3], &[1, 1, 1]);
    let expected = basis_points(&equal);
    assert_eq!(expected, json!([3334, 3333, 3333]));
    cases.ok(
        "display_basis_points: version 2 equal split",
        "display_basis_points",
        json!([card_json(&equal)]),
        expected,
    );
    cases.ok(
        "compute_payouts: version 2 at 333/333/334",
        "compute_payouts",
        json!([json, "100000"]),
        payouts(&card, 100_000),
    );

    let mut long_label = card.clone();
    v2(&mut long_label).members[0].label = Some("x".repeat(MAX_LABEL_CHARS + 1));
    let mut bad_percent = card.clone();
    v2(&mut bad_percent).rules.leave = LeaveRule::KeepPercent { keep_percent: 100 };
    let mut v2_version_3 = card.clone();
    v2(&mut v2_version_3).version = 3;
    for (code, invalid, details) in [
        ("invalid_member_label", long_label, json!({ "index": 0 })),
        (
            "invalid_leave_keep_percent",
            bad_percent,
            json!({ "keep_percent": 100 }),
        ),
    ] {
        let error = core_err(validate(&invalid));
        cases.error(
            &format!("validate_card: version 2 {code}"),
            "validate_card",
            json!([card_json(&invalid)]),
            code,
            error,
            details,
        );
    }
    // A version 2 card with a version 1 member field.
    let with_name = json.replacen(r#""weight":333"#, r#""weight":333,"name":"Ann""#, 1);
    let error = card_json_error(&with_name);
    cases.error(
        "validate_card: version 2 member with a version 1 name field",
        "validate_card",
        json!([with_name]),
        "invalid_card_json",
        &error,
        json_position(&error),
    );
    // Only "version": 2 selects the version 2 schema.
    let error = card_json_error(&card_json(&v2_version_3));
    cases.error(
        "validate_card: version 2 shape under version 3",
        "validate_card",
        json!([card_json(&v2_version_3)]),
        "invalid_card_json",
        &error,
        json_position(&error),
    );
}

fn v2_signature_cases(cases: &mut Cases) {
    let card = v2_card(&[1, 2, 3], &[333, 333, 334]);
    let json = card_json(&card);

    // Templates.
    let card_hash = hash(&card);
    let pubkey = member_pubkey(1);
    let template = unsigned_signature_template(&card_hash, &pubkey, SIGNED_AT, Some(&lud16(1)))
        .unwrap_or_else(|error| panic!("template must build: {error}"));
    cases.ok(
        "signature_template: version 2 with a lud16 tag",
        "signature_template",
        json!([card_hash, pubkey, SIGNED_AT, lud16(1)]),
        json!({
            "pubkey": template.pubkey.to_hex(),
            "created_at": template.created_at.as_secs(),
            "kind": template.kind.as_u16(),
            "tags": template.tags.iter().map(|tag| tag.as_slice().to_vec()).collect::<Vec<_>>(),
            "content": template.content,
        }),
    );
    let error = core_err(unsigned_signature_template(
        &card_hash,
        &pubkey,
        SIGNED_AT,
        Some("Member1@Example.com"),
    ));
    cases.error(
        "signature_template: invalid_template_lightning_address",
        "signature_template",
        json!([card_hash, pubkey, SIGNED_AT, "Member1@Example.com"]),
        "invalid_template_lightning_address",
        error,
        Value::Null,
    );

    // Verification.
    let valid = v2_signature(&card, 2);
    let signer = verify_signature_event(&card, &valid)
        .unwrap_or_else(|error| panic!("must verify: {error}"));
    assert_eq!(signer.lightning_address, Some(lud16(2)));
    cases.ok(
        "verify_signature: version 2 returns the signed address",
        "verify_signature",
        json!([json, event_json(&valid)]),
        signer_json(&signer),
    );
    for (code, event) in [
        ("missing_lud16_tag", v2_signature_with(&card, 2, &[])),
        (
            "multiple_lud16_tags",
            v2_signature_with(&card, 2, &[&lud16(2), "other@example.com"]),
        ),
        (
            "invalid_lud16",
            v2_signature_with(&card, 2, &["Member2@Example.com"]),
        ),
    ] {
        let error = core_err(verify_signature_event(&card, &event));
        cases.error(
            &format!("verify_signature: version 2 {code}"),
            "verify_signature",
            json!([json, event_json(&event)]),
            code,
            error,
            Value::Null,
        );
    }
    let v1 = signed_card(&[1, 2, 3]);
    let with_tag = v2_signature_with(&v1, 2, &[&lud16(2)]);
    let error = core_err(verify_signature_event(&v1, &with_tag));
    cases.error(
        "verify_signature: unexpected_lud16_tag on a version 1 card",
        "verify_signature",
        json!([card_json(&v1), event_json(&with_tag)]),
        "unexpected_lud16_tag",
        error,
        Value::Null,
    );
}

fn v2_status_cases(cases: &mut Cases) {
    let card = v2_card(&[1, 2, 3], &[333, 333, 334]);
    let json = card_json(&card);

    // Status.
    let draft = [
        v2_signature(&card, 3),
        v2_signature_with(&card, 2, &[]),
        v2_signature(&card, 1),
    ];
    let expected = status(&card, &draft, &["missing_lud16_tag"]);
    assert_eq!(expected["missing"], json!(pubkeys(&[2])));
    cases.ok(
        "card_status: version 2 draft reports each signed address",
        "card_status",
        json!([json, events_json(&draft)]),
        expected,
    );

    // Superseding a version 1 card with a version 2 card.
    let old = signed_card(&[1, 2, 3]);
    let mut new = v2_card(&[1, 2, 4], &[1, 1, 1]);
    v2(&mut new).supersedes = Some(hash(&old));
    let all: Vec<Event> = [1, 2, 3, 4]
        .map(|number| v2_signature(&new, number))
        .to_vec();
    assert!(check_supersede(&old, &new, &all).is_ok());
    cases.ok(
        "check_supersede: version 2 replaces version 1",
        "check_supersede",
        json!([card_json(&old), card_json(&new), events_json(&all)]),
        Value::Null,
    );
    let mut removed_without_tag = all.clone();
    removed_without_tag[2] = v2_signature_with(&new, 3, &[]);
    let error = core_err(check_supersede(&old, &new, &removed_without_tag));
    cases.error(
        "check_supersede: removed member's signature needs a lud16 tag on a version 2 card",
        "check_supersede",
        json!([
            card_json(&old),
            card_json(&new),
            events_json(&removed_without_tag)
        ]),
        "missing_old_member_signatures",
        error,
        json!({ "missing": pubkeys(&[3]) }),
    );
}

fn generate() -> Value {
    let mut cases = Cases(Vec::new());
    card_hash_cases(&mut cases);
    validate_cases(&mut cases);
    payout_cases(&mut cases);
    basis_point_cases(&mut cases);
    template_cases(&mut cases);
    verify_cases(&mut cases);
    status_cases(&mut cases);
    supersede_cases(&mut cases);
    v2_cases(&mut cases);
    v2_signature_cases(&mut cases);
    v2_status_cases(&mut cases);
    json!({
        "generated_by": "cargo test -p splitcard-wasm --test parity_fixtures (set SPLITCARD_UPDATE_FIXTURES=1 to rewrite)",
        "cases": cases.0,
    })
}

#[test]
fn committed_fixtures_match_native_core() {
    let generated = generate();
    let mut text = serde_json::to_string_pretty(&generated)
        .unwrap_or_else(|error| panic!("fixtures must serialize: {error}"));
    text.push('\n');
    if std::env::var_os("SPLITCARD_UPDATE_FIXTURES").is_some() {
        std::fs::write(FIXTURE_PATH, &text)
            .unwrap_or_else(|error| panic!("could not write {FIXTURE_PATH}: {error}"));
    }
    let committed = std::fs::read_to_string(FIXTURE_PATH).unwrap_or_default();
    assert!(
        committed == text,
        "{FIXTURE_PATH} is stale; regenerate with \
         SPLITCARD_UPDATE_FIXTURES=1 cargo test -p splitcard-wasm --test parity_fixtures"
    );
}

#[test]
fn fixtures_cover_every_reachable_error_code() {
    let generated = generate();
    let mut covered: BTreeSet<&str> = generated["cases"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|case| case["expected"]["error"]["code"].as_str())
        .collect();
    covered.extend(BOUNDARY_ONLY_CODES);
    covered.extend(UNREACHABLE_CODES);
    let all: BTreeSet<&str> = ERROR_CODES.iter().copied().collect();
    assert_eq!(covered, all);
}
