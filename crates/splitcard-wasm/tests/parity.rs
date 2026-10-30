//! Runs the exported functions inside WASM and checks them against
//! `fixtures/parity.json`, which native `splitcard-core` generated.
//!
//! Run with `wasm-pack test --node crates/splitcard-wasm`.

#![cfg(target_arch = "wasm32")]

use serde::Serialize;
use serde_json::{Value, json};
use serde_wasm_bindgen::Serializer;
use splitcard_wasm::{
    card_hash, card_status, check_supersede, compute_payouts, display_basis_points,
    error::ERROR_CODES, keys::npub_encode, new_card, signature_template, validate_card,
    verify_signature,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

const FIXTURES: &str = include_str!("fixtures/parity.json");

fn from_js(value: JsValue) -> Value {
    serde_wasm_bindgen::from_value(value)
        .unwrap_or_else(|error| panic!("JS value must convert to JSON: {error}"))
}

fn to_js(value: &Value) -> JsValue {
    value
        .serialize(&Serializer::json_compatible())
        .unwrap_or_else(|error| panic!("JSON must convert to a JS value: {error}"))
}

fn call(function: &str, args: &[Value]) -> Result<Value, Value> {
    let text = |index: usize| {
        args[index]
            .as_str()
            .unwrap_or_else(|| panic!("{function} argument {index} must be a string"))
    };
    let result = match function {
        "card_hash" => card_hash(text(0)).map(Value::from),
        "validate_card" => validate_card(text(0)).map(|()| Value::Null),
        "compute_payouts" => compute_payouts(text(0), text(1)).map(from_js),
        "display_basis_points" => display_basis_points(text(0)).map(from_js),
        "signature_template" => signature_template(
            text(0),
            text(1),
            to_js(&args[2]),
            args.get(3).and_then(Value::as_str).map(str::to_owned),
        )
        .map(from_js),
        "verify_signature" => verify_signature(text(0), text(1)).map(from_js),
        "card_status" => card_status(text(0), text(1)).map(from_js),
        "check_supersede" => check_supersede(text(0), text(1), text(2)).map(|()| Value::Null),
        "npub_encode" => npub_encode(text(0)).map(Value::from),
        other => panic!("unknown fixture function {other}"),
    };
    result.map_err(from_js)
}

fn thrown(result: Result<Value, Value>) -> Value {
    match result {
        Ok(value) => panic!("expected a thrown error, got {value}"),
        Err(error) => error,
    }
}

/// Every thrown value is an object with a known `code`, a string `message`
/// and a `details` key (object or null).
fn assert_error_shape(error: &Value) {
    let object = error
        .as_object()
        .unwrap_or_else(|| panic!("thrown value must be an object: {error}"));
    let code = object
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(ERROR_CODES.contains(&code), "unknown code in {error}");
    assert!(
        object.get("message").is_some_and(Value::is_string),
        "{error}"
    );
    assert!(
        object
            .get("details")
            .is_some_and(|details| details.is_null() || details.is_object()),
        "{error}"
    );
    assert_eq!(object.len(), 3, "unexpected keys in {error}");
}

#[wasm_bindgen_test]
fn wasm_matches_native_fixtures() {
    let fixtures: Value = serde_json::from_str(FIXTURES)
        .unwrap_or_else(|error| panic!("fixtures must parse: {error}"));
    let cases = fixtures["cases"].as_array().map_or(&[][..], Vec::as_slice);
    assert!(!cases.is_empty(), "fixtures must contain cases");
    for case in cases {
        let name = case["name"].as_str().unwrap_or_default();
        let args = case["args"].as_array().map_or(&[][..], Vec::as_slice);
        let actual = call(case["function"].as_str().unwrap_or_default(), args);
        let expected = &case["expected"];
        if let Some(error) = expected.get("error") {
            let actual = thrown(actual);
            assert_error_shape(&actual);
            assert_eq!(&actual, error, "{name}");
        } else {
            assert_eq!(actual.as_ref(), Ok(&expected["ok"]), "{name}");
        }
    }
}

#[wasm_bindgen_test]
fn bad_amount_strings_are_rejected() {
    // The amount is parsed after the card, so pass a valid card.
    for bad in [
        "1e3", "-1", " 5", "1.0", "", "0x10", "00", "1,000", "Infinity",
    ] {
        let error = thrown(call("compute_payouts", &[json!(valid_card()), json!(bad)]));
        assert_error_shape(&error);
        assert_eq!(error["code"], "invalid_amount", "{bad:?}");
        assert_eq!(error["details"]["value"], bad);
    }
}

#[wasm_bindgen_test]
fn malformed_json_gives_boundary_codes() {
    for bad_card in ["", "null", "{", "[]", r#"{"version":1}"#] {
        let error = thrown(call("validate_card", &[json!(bad_card)]));
        assert_error_shape(&error);
        assert_eq!(error["code"], "invalid_card_json", "{bad_card:?}");
    }
    for bad_events in ["", "{", "{}", "[1]", r#"[{"id":"x"}]"#] {
        let error = thrown(call(
            "card_status",
            &[json!(valid_card()), json!(bad_events)],
        ));
        assert_error_shape(&error);
        assert_eq!(error["code"], "invalid_event_json", "{bad_events:?}");
        let error = thrown(call(
            "verify_signature",
            &[json!(valid_card()), json!(bad_events)],
        ));
        assert_eq!(error["code"], "invalid_event_json", "{bad_events:?}");
    }
}

#[wasm_bindgen_test]
fn created_at_must_be_a_whole_u32_number() {
    let hash = "37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52";
    let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    for ok in [json!(0), json!(1_700_000_000), json!(4_294_967_295_u64)] {
        let template = call(
            "signature_template",
            &[json!(hash), json!(pubkey), ok.clone()],
        )
        .unwrap_or_else(|error| panic!("{ok} must be accepted: {error}"));
        assert_eq!(template["created_at"], ok);
    }
    let bad_values = [
        to_js(&json!(-1)),
        to_js(&json!(1.5)),
        to_js(&json!(4_294_967_296_u64)),
        to_js(&json!("1700000000")),
        JsValue::NULL,
        JsValue::UNDEFINED,
        JsValue::from(f64::NAN),
    ];
    for bad in bad_values {
        let error = from_js(
            signature_template(hash, pubkey, bad.clone(), None)
                .err()
                .unwrap_or_else(|| panic!("{bad:?} must be rejected")),
        );
        assert_error_shape(&error);
        assert_eq!(error["code"], "invalid_created_at", "{bad:?}");
        assert_eq!(error["details"], Value::Null);
    }
}

#[wasm_bindgen_test]
fn new_card_builds_a_canonical_valid_v2_card() {
    let pk = |n: u8| format!("{n:064x}");
    let pubkey = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    let other = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
    let input = json!({
        "card_id": "abc",
        "event": "Hack",
        "team": "Llamas",
        "members": [
            { "pubkey": pubkey, "weight": 1, "label": "Ada" },
            { "pubkey": other, "weight": 1 },
        ],
        "leave_keep_percent": 15,
    });
    let card = new_card(to_js(&input)).unwrap_or_else(|error| panic!("{:?}", from_js(error)));
    assert_eq!(
        card,
        format!(
            r#"{{"card_id":"abc","event":"Hack","members":[{{"label":"Ada","pubkey":"{pubkey}","weight":1}},{{"pubkey":"{other}","weight":1}}],"rules":{{"leave":{{"keep_percent":15,"kind":"keep_percent"}},"peer_review":null}},"supersedes":null,"team":"Llamas","version":2}}"#
        )
    );
    assert!(validate_card(&card).is_ok());

    for bad in [
        json!({ "card_id": "abc" }),
        json!({ "card_id": "a", "event": "e", "team": "t", "members": [{ "pubkey": pk(1), "weight": 1.5 }] }),
    ] {
        let error = from_js(
            new_card(to_js(&bad))
                .err()
                .unwrap_or_else(|| panic!("{bad}")),
        );
        assert_error_shape(&error);
        assert_eq!(error["code"], "invalid_card_input", "{bad}");
    }
    let one = json!({ "card_id": "a", "event": "e", "team": "t", "members": [{ "pubkey": pubkey, "weight": 1 }] });
    let error = from_js(
        new_card(to_js(&one))
            .err()
            .unwrap_or_else(|| panic!("one member")),
    );
    assert_eq!(error["code"], "invalid_member_count");
}

fn valid_card() -> String {
    let fixtures: Value = serde_json::from_str(FIXTURES)
        .unwrap_or_else(|error| panic!("fixtures must parse: {error}"));
    fixtures["cases"][0]["args"][0]
        .as_str()
        .unwrap_or_else(|| panic!("first fixture must be the Phase 1 card"))
        .to_owned()
}
