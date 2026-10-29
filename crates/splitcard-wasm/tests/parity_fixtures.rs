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
    card::{Card, Member, Payout, PeerReview, Rules},
    hash::card_hash_hex,
    signature::{SIGNATURE_KIND, unsigned_signature_template, verify_signature_event},
    split::{compute_payouts, display_basis_points},
    status::{CardStatus, card_status},
    supersede::check_supersede,
    validate::{MAX_WEIGHT, validate},
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
    Card {
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
    }
}

fn card_with_weights(weights: [u64; 3]) -> Card {
    let mut card = phase_1_card();
    for (member, weight) in card.members.iter_mut().zip(weights) {
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
    Card {
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
    }
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
    core_err(serde_json::from_str::<Card>(json))
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
    let weights: Vec<u64> = card.members.iter().map(|member| member.weight).collect();
    json!(display_basis_points(&weights).unwrap_or_else(|error| panic!("must split: {error}")))
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
            json!({ "status": "draft", "signed": signed, "missing": missing, "rejected": rejected })
        }
        CardStatus::Active { signed } => {
            json!({ "status": "active", "signed": signed, "rejected": rejected })
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
    card.version = 2;
    invalid.push(("unsupported_version", card, json!({ "version": 2 })));
    let mut card = phase_1_card();
    card.team.clear();
    invalid.push(("empty_field", card, json!({ "field": "team" })));
    let mut card = phase_1_card();
    card.members.truncate(1);
    invalid.push(("invalid_member_count", card, json!({ "count": 1 })));
    let mut card = phase_1_card();
    card.members[1].pubkey = "A".repeat(64);
    invalid.push(("invalid_pubkey", card, json!({ "index": 1 })));
    let mut card = phase_1_card();
    let first = card.members[0].pubkey.clone();
    card.members[2].pubkey = first;
    invalid.push(("duplicate_pubkey", card, json!({ "index": 2 })));
    let mut card = phase_1_card();
    card.members[0].name.clear();
    invalid.push(("empty_member_name", card, json!({ "index": 0 })));
    let mut card = phase_1_card();
    card.members[1].weight = 0;
    invalid.push(("zero_weight", card, json!({ "index": 1 })));
    let mut card = phase_1_card();
    card.members[2].payout = Payout::Lightning {
        address: "not-an-address".to_owned(),
    };
    invalid.push(("invalid_lightning_address", card, json!({ "index": 2 })));
    let mut card = phase_1_card();
    card.supersedes = Some("xyz".to_owned());
    invalid.push(("invalid_supersedes", card, Value::Null));
    let mut card = phase_1_card();
    card.rules.peer_review = Some(PeerReview {
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
    card.members[0].payout = Payout::Lightning {
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
            { "pubkey": card.members[0].pubkey, "amount_sats": "36001" },
            { "pubkey": card.members[1].pubkey, "amount_sats": "35000" },
            { "pubkey": card.members[2].pubkey, "amount_sats": "29000" },
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
        json!({ "index": 2, "pubkey": max.members[2].pubkey }),
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
    zero.members[0].weight = 0;
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
    empty_name.members[1].name.clear();
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
    let template = unsigned_signature_template(PHASE_1_CARD_HASH, &pubkey, SIGNED_AT)
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
    let error = core_err(unsigned_signature_template("ABC", &pubkey, SIGNED_AT));
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
    assert_eq!(signer, member_pubkey(2));
    cases.ok(
        "verify_signature: valid member signature",
        "verify_signature",
        json!([card_json(&card), event_json(&valid)]),
        json!(signer),
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
    one_member.members.truncate(1);
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
    new.supersedes = Some(hash(&old));
    let all: Vec<Event> = [1, 2, 3, 4].map(|number| signature(&new, number)).to_vec();
    assert!(check_supersede(&old, &new, &all).is_ok());
    cases.ok(
        "check_supersede: valid replacement",
        "check_supersede",
        json!([card_json(&old), card_json(&new), events_json(&all)]),
        Value::Null,
    );

    let mut wrong = new.clone();
    wrong.supersedes = Some("c".repeat(64));
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
    bad_old.version = 7;
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
    bad_new.card_id.clear();
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
