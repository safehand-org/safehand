//! Tests for the pure modules: addresses, LNURL, invoices and proofs.

mod common;

use common::{CREATED_AT, EXPIRY_SECS, METADATA, NOW, PREIMAGE, Spec, invoice, payment_hash};
use lightning_invoice::Currency;
use serde_json::json;
use splitcard_ln::{
    address::{AddressError, parse_address, well_known_url},
    invoice::{InvoiceError, Network, check_invoice, decode_invoice},
    lnurl::{
        LnurlError, Settlement, build_callback_url, check_amount, parse_invoice_response,
        parse_pay_request, parse_verify_response, parse_verify_url, sats_to_msat,
    },
    proof::{ProofError, verify_preimage},
};
use url::Url;

// Addresses.

#[test]
fn valid_addresses_parse() {
    for (input, username, domain) in [
        ("alice@example.com", "alice", "example.com"),
        (
            "a.b-c_d+e@pay.example.co.uk",
            "a.b-c_d+e",
            "pay.example.co.uk",
        ),
        ("0x7@xn--bcher-kva.example", "0x7", "xn--bcher-kva.example"),
        ("bob@sub-1.example.io", "bob", "sub-1.example.io"),
    ] {
        let address =
            parse_address(input).unwrap_or_else(|error| panic!("{input} must parse: {error}"));
        assert_eq!(address.username(), username);
        assert_eq!(address.domain(), domain);
        assert_eq!(address.to_string(), input);
    }
}

#[test]
fn uppercase_addresses_are_lowercased() {
    let address = parse_address("Alice.Smith@Example.COM")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(address.username(), "alice.smith");
    assert_eq!(address.domain(), "example.com");
    assert_eq!(
        well_known_url(&address).as_str(),
        "https://example.com/.well-known/lnurlp/alice.smith"
    );
}

#[test]
fn well_known_url_follows_lud16() {
    let address = parse_address("pay+tips@wallet.example.org")
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(
        well_known_url(&address).as_str(),
        "https://wallet.example.org/.well-known/lnurlp/pay+tips"
    );
}

#[test]
fn invalid_addresses_are_rejected() {
    let long_domain = format!("{}.com", "a".repeat(250));
    let long_label = format!("{}.com", "a".repeat(64));
    let cases: Vec<(&str, AddressError)> = vec![
        ("alice.example.com", AddressError::MissingAt),
        ("alice@bob@example.com", AddressError::MultipleAt),
        ("@example.com", AddressError::EmptyUsername),
        (
            "al ice@example.com",
            AddressError::InvalidUsernameCharacter { character: ' ' },
        ),
        (
            "alice!@example.com",
            AddressError::InvalidUsernameCharacter { character: '!' },
        ),
        (
            "alicé@example.com",
            AddressError::InvalidUsernameCharacter { character: 'é' },
        ),
        ("alice@", AddressError::EmptyDomain),
        (
            "alice@exa_mple.com",
            AddressError::InvalidDomainLabel {
                label: "exa_mple".to_owned(),
            },
        ),
        (
            "alice@-example.com",
            AddressError::InvalidDomainLabel {
                label: "-example".to_owned(),
            },
        ),
        (
            "alice@example-.com",
            AddressError::InvalidDomainLabel {
                label: "example-".to_owned(),
            },
        ),
        (
            "alice@example..com",
            AddressError::InvalidDomainLabel {
                label: String::new(),
            },
        ),
        (
            "alice@example.com:8080",
            AddressError::InvalidDomainLabel {
                label: "com:8080".to_owned(),
            },
        ),
        ("alice@localhost", AddressError::SingleLabelDomain),
        ("alice@127.0.0.1", AddressError::NumericTopLevelDomain),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_address(input), Err(expected), "{input}");
    }
    assert_eq!(
        parse_address(&format!("alice@{long_domain}")),
        Err(AddressError::DomainTooLong { length: 254 })
    );
    assert_eq!(
        parse_address(&format!("alice@{long_label}")),
        Err(AddressError::InvalidDomainLabel {
            label: "a".repeat(64)
        })
    );
}

// LNURL.

fn pay_request_json(callback: &str) -> String {
    common::pay_request_body(callback, 1_000, 1_000_000_000)
}

fn url(text: &str) -> Url {
    Url::parse(text).unwrap_or_else(|error| panic!("test URL must parse: {error}"))
}

#[test]
fn pay_request_parses_and_keeps_metadata_exactly() {
    let request = parse_pay_request(&pay_request_json("https://pay.example.com/cb"), false)
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(request.callback.as_str(), "https://pay.example.com/cb");
    assert_eq!(request.min_sendable_msat, 1_000);
    assert_eq!(request.max_sendable_msat, 1_000_000_000);
    assert_eq!(request.metadata, METADATA);
}

#[test]
fn callback_keeps_existing_query_parameters() {
    let callback = url("https://pay.example.com/cb?user=alice&tag=x%20y");
    assert_eq!(
        build_callback_url(&callback, 21_000).as_str(),
        "https://pay.example.com/cb?user=alice&tag=x+y&amount=21000"
    );
    let bare = url("https://pay.example.com/cb");
    assert_eq!(
        build_callback_url(&bare, 1).as_str(),
        "https://pay.example.com/cb?amount=1"
    );
    let stale = url("https://pay.example.com/cb?amount=5&user=bob");
    assert_eq!(
        build_callback_url(&stale, 7).as_str(),
        "https://pay.example.com/cb?user=bob&amount=7"
    );
}

#[test]
fn error_response_carries_the_reason() {
    let body = r#"{"status":"ERROR","reason":"user not found"}"#;
    assert_eq!(
        parse_pay_request(body, false),
        Err(LnurlError::Service {
            reason: "user not found".to_owned()
        })
    );
    assert_eq!(
        parse_invoice_response(body),
        Err(LnurlError::Service {
            reason: "user not found".to_owned()
        })
    );
}

#[test]
fn wrong_tag_is_rejected() {
    let body =
        pay_request_json("https://pay.example.com/cb").replace("payRequest", "withdrawRequest");
    assert_eq!(
        parse_pay_request(&body, false),
        Err(LnurlError::WrongTag {
            found: "withdrawRequest".to_owned()
        })
    );
}

#[test]
fn non_https_callback_is_rejected() {
    assert_eq!(
        parse_pay_request(&pay_request_json("http://pay.example.com/cb"), false),
        Err(LnurlError::InsecureCallback {
            scheme: "http".to_owned()
        })
    );
    assert!(parse_pay_request(&pay_request_json("http://127.0.0.1:9/cb"), true).is_ok());
}

#[test]
fn malformed_pay_requests_are_rejected() {
    assert!(matches!(
        parse_pay_request("not json", false),
        Err(LnurlError::InvalidJson { .. })
    ));
    let missing =
        r#"{"tag":"payRequest","callback":"https://a.example/cb","maxSendable":5,"metadata":"[]"}"#;
    assert_eq!(
        parse_pay_request(missing, false),
        Err(LnurlError::MissingField {
            field: "minSendable"
        })
    );
    let fractional = common::pay_request_body("https://a.example/cb", 1, 5)
        .replace("\"minSendable\":1", "\"minSendable\":1.5");
    assert_eq!(
        parse_pay_request(&fractional, false),
        Err(LnurlError::InvalidField {
            field: "minSendable"
        })
    );
    let inverted = common::pay_request_body("https://a.example/cb", 10, 5);
    assert_eq!(
        parse_pay_request(&inverted, false),
        Err(LnurlError::InvalidSendableRange {
            min_sendable_msat: 10,
            max_sendable_msat: 5
        })
    );
}

#[test]
fn amount_outside_sendable_range_is_rejected() {
    let request = parse_pay_request(&pay_request_json("https://pay.example.com/cb"), false)
        .unwrap_or_else(|error| panic!("must parse: {error}"));
    assert_eq!(check_amount(&request, 1_000), Ok(()));
    assert_eq!(check_amount(&request, 1_000_000_000), Ok(()));
    assert_eq!(
        check_amount(&request, 999),
        Err(LnurlError::AmountBelowMinimum {
            amount_msat: 999,
            min_sendable_msat: 1_000
        })
    );
    assert_eq!(
        check_amount(&request, 1_000_000_001),
        Err(LnurlError::AmountAboveMaximum {
            amount_msat: 1_000_000_001,
            max_sendable_msat: 1_000_000_000
        })
    );
}

#[test]
fn msat_conversion_overflow_is_an_error() {
    assert_eq!(sats_to_msat(0), Ok(0));
    assert_eq!(sats_to_msat(100_001), Ok(100_001_000));
    assert_eq!(sats_to_msat(u64::MAX / 1_000), Ok(u64::MAX / 1_000 * 1_000));
    assert_eq!(
        sats_to_msat(u64::MAX / 1_000 + 1),
        Err(LnurlError::AmountOverflow {
            sats: u64::MAX / 1_000 + 1
        })
    );
    assert_eq!(
        sats_to_msat(u64::MAX),
        Err(LnurlError::AmountOverflow { sats: u64::MAX })
    );
}

// Invoices.

#[test]
fn valid_invoice_passes_with_hash_amount_and_expiry() {
    let bolt11 = invoice(&Spec::new(21_000_000));
    let checked = check_invoice(&bolt11, 21_000_000, METADATA, Network::Mainnet, NOW)
        .unwrap_or_else(|error| panic!("must pass: {error}"));
    assert_eq!(checked.bolt11, bolt11);
    assert_eq!(checked.payment_hash, hex::encode(payment_hash()));
    assert_eq!(checked.amount_msat, 21_000_000);
    assert_eq!(checked.expires_at_unix, CREATED_AT + EXPIRY_SECS);
}

#[test]
fn wrong_amount_is_rejected() {
    let bolt11 = invoice(&Spec::new(21_000_001));
    assert_eq!(
        check_invoice(&bolt11, 21_000_000, METADATA, Network::Mainnet, NOW),
        Err(InvoiceError::WrongAmount {
            expected_msat: 21_000_000,
            found_msat: 21_000_001
        })
    );
}

#[test]
fn missing_amount_is_rejected() {
    let mut spec = Spec::new(0);
    spec.amount_msat = None;
    assert_eq!(
        check_invoice(&invoice(&spec), 21_000_000, METADATA, Network::Mainnet, NOW),
        Err(InvoiceError::MissingAmount)
    );
}

#[test]
fn wrong_network_is_rejected() {
    let mut spec = Spec::new(21_000_000);
    spec.currency = Currency::Signet;
    let signet = invoice(&spec);
    assert!(matches!(
        check_invoice(&signet, 21_000_000, METADATA, Network::Mainnet, NOW),
        Err(InvoiceError::WrongNetwork {
            expected: Network::Mainnet,
            ..
        })
    ));
    assert!(check_invoice(&signet, 21_000_000, METADATA, Network::Signet, NOW).is_ok());
    let mainnet = invoice(&Spec::new(21_000_000));
    assert!(matches!(
        check_invoice(&mainnet, 21_000_000, METADATA, Network::Regtest, NOW),
        Err(InvoiceError::WrongNetwork {
            expected: Network::Regtest,
            ..
        })
    ));
}

#[test]
fn wrong_description_hash_is_rejected() {
    let mut spec = Spec::new(21_000_000);
    spec.metadata = r#"[["text/plain","something else"]]"#.to_owned();
    assert_eq!(
        check_invoice(&invoice(&spec), 21_000_000, METADATA, Network::Mainnet, NOW),
        Err(InvoiceError::WrongDescriptionHash)
    );
}

#[test]
fn expired_invoice_is_rejected() {
    let bolt11 = invoice(&Spec::new(21_000_000));
    let expires_at_unix = CREATED_AT + EXPIRY_SECS;
    assert!(
        check_invoice(
            &bolt11,
            21_000_000,
            METADATA,
            Network::Mainnet,
            expires_at_unix - 1
        )
        .is_ok()
    );
    assert_eq!(
        check_invoice(
            &bolt11,
            21_000_000,
            METADATA,
            Network::Mainnet,
            expires_at_unix
        ),
        Err(InvoiceError::Expired {
            expires_at_unix,
            now_unix: expires_at_unix
        })
    );
}

#[test]
fn bad_signature_is_rejected() {
    let mut spec = Spec::new(21_000_000);
    // Declares key 2 as the payee but is signed by key 1.
    spec.payee = Some(2);
    assert_eq!(
        check_invoice(&invoice(&spec), 21_000_000, METADATA, Network::Mainnet, NOW),
        Err(InvoiceError::InvalidSignature)
    );
}

#[test]
fn garbage_is_unparseable() {
    for bad in ["", "lnbc1", "not an invoice"] {
        assert!(matches!(
            check_invoice(bad, 1, METADATA, Network::Mainnet, NOW),
            Err(InvoiceError::Unparseable { .. })
        ));
    }
}

/// "Now send $24 for an entire list of things (hashed)", copied from the
/// BOLT 11 specification's Examples section (11-payment-encoding.md, lightning/bolts).
const BOLT11_SPEC_HASHED: &str = "lnbc20m1pvjluezsp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygspp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqhp58yjmdan79s6qqdhdzgynm4zwqd5d7xmw5fk98klysy043l2ahrqs9qrsgq7ea976txfraylvgzuxs8kgcw23ezlrszfnh8r6qtfpr6cxga50aj6txm9rxrydzd06dfeawfk6swupvz4erwnyutnjq7x39ymw6j38gp7ynn44";
/// The same invoice on testnet with a fallback address, from the same section.
const BOLT11_SPEC_TESTNET: &str = "lntb20m1pvjluezsp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygshp58yjmdan79s6qqdhdzgynm4zwqd5d7xmw5fk98klysy043l2ahrqspp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqfpp3x9et2e20v6pu37c5d9vax37wxq72un989qrsgqdj545axuxtnfemtpwkc45hx9d2ft7x04mt8q7y6t0k2dge9e7h8kpy9p34ytyslj3yu569aalz2xdk8xkd7ltxqld94u8h2esmsmacgpghe9k8";
/// The description whose SHA-256 the spec vectors commit to.
const BOLT11_SPEC_DESCRIPTION: &str = "One piece of chocolate cake, one icecream cone, one pickle, one slice of swiss cheese, one slice of salami, one lollypop, one piece of cherry pie, one sausage, one cupcake, and one slice of watermelon";
/// Timestamp `pvjluez` from the spec breakdown; no `x` field, so expiry is 3600 s.
const BOLT11_SPEC_TIMESTAMP: u64 = 1_496_314_658;

#[test]
fn bolt11_spec_vector_passes() {
    // 20 milli-bitcoin = 2,000,000 sats = 2,000,000,000 msat.
    let checked = check_invoice(
        BOLT11_SPEC_HASHED,
        2_000_000_000,
        BOLT11_SPEC_DESCRIPTION,
        Network::Mainnet,
        BOLT11_SPEC_TIMESTAMP + 60,
    )
    .unwrap_or_else(|error| panic!("spec vector must pass: {error}"));
    assert_eq!(
        checked.payment_hash,
        "0001020304050607080900010203040506070809000102030405060708090102"
    );
    assert_eq!(checked.amount_msat, 2_000_000_000);
    assert_eq!(checked.expires_at_unix, BOLT11_SPEC_TIMESTAMP + 3_600);
}

#[test]
fn bolt11_spec_vectors_fail_the_right_checks() {
    assert!(matches!(
        check_invoice(
            BOLT11_SPEC_TESTNET,
            2_000_000_000,
            BOLT11_SPEC_DESCRIPTION,
            Network::Mainnet,
            BOLT11_SPEC_TIMESTAMP + 60
        ),
        Err(InvoiceError::WrongNetwork { .. })
    ));
    assert_eq!(
        check_invoice(
            BOLT11_SPEC_HASHED,
            2_000_000_000,
            BOLT11_SPEC_DESCRIPTION,
            Network::Mainnet,
            BOLT11_SPEC_TIMESTAMP + 3_600
        ),
        Err(InvoiceError::Expired {
            expires_at_unix: BOLT11_SPEC_TIMESTAMP + 3_600,
            now_unix: BOLT11_SPEC_TIMESTAMP + 3_600
        })
    );
}

// Proof.

#[test]
fn correct_preimage_passes() {
    assert_eq!(
        verify_preimage(&hex::encode(payment_hash()), &hex::encode(PREIMAGE)),
        Ok(())
    );
}

#[test]
fn bad_preimages_fail() {
    let hash = hex::encode(payment_hash());
    assert_eq!(
        verify_preimage(&hash, &hex::encode([8_u8; 32])),
        Err(ProofError::PreimageMismatch)
    );
    assert_eq!(
        verify_preimage(&hash, &"zz".repeat(32)),
        Err(ProofError::InvalidPreimageHex)
    );
    assert_eq!(
        verify_preimage(&hash, &hex::encode(PREIMAGE).to_uppercase()),
        Err(ProofError::InvalidPreimageHex)
    );
    assert_eq!(
        verify_preimage(&hash, &hex::encode([7_u8; 31])),
        Err(ProofError::InvalidPreimageLength { length: 62 })
    );
    assert_eq!(
        verify_preimage(&hash, &hex::encode([7_u8; 33])),
        Err(ProofError::InvalidPreimageLength { length: 66 })
    );
    assert_eq!(
        verify_preimage(&hash.to_uppercase(), &hex::encode(PREIMAGE)),
        Err(ProofError::InvalidPaymentHash)
    );
}

#[test]
fn verify_url_is_optional_and_must_be_https() {
    let body = |verify: serde_json::Value| json!({ "pr": "lnbc1", "verify": verify }).to_string();
    assert_eq!(
        parse_verify_url(&body(json!("https://pay.example/verify/abc")), false),
        Some(url("https://pay.example/verify/abc"))
    );
    assert_eq!(
        parse_verify_url(&json!({ "pr": "lnbc1" }).to_string(), false),
        None
    );
    assert_eq!(parse_verify_url(&body(json!(7)), false), None);
    assert_eq!(parse_verify_url(&body(json!("not a url")), false), None);
    assert_eq!(
        parse_verify_url(&body(json!("http://pay.example/v")), false),
        None
    );
    assert_eq!(
        parse_verify_url(&body(json!("http://pay.example/v")), true),
        Some(url("http://pay.example/v"))
    );
}

#[test]
fn verify_responses_parse() {
    let settled = json!({ "status": "OK", "settled": true, "preimage": "ab", "pr": "lnbc1" });
    assert_eq!(
        parse_verify_response(&settled.to_string()),
        Ok(Settlement {
            settled: true,
            preimage: Some("ab".to_owned()),
            bolt11: "lnbc1".to_owned(),
        })
    );
    let unsettled = json!({ "status": "OK", "settled": false, "preimage": null, "pr": "lnbc1" });
    assert_eq!(
        parse_verify_response(&unsettled.to_string()).map(|s| (s.settled, s.preimage)),
        Ok((false, None))
    );
    let error = json!({ "status": "ERROR", "reason": "Not found" });
    assert_eq!(
        parse_verify_response(&error.to_string()),
        Err(LnurlError::Service {
            reason: "Not found".to_owned()
        })
    );
    assert_eq!(
        parse_verify_response(&json!({ "pr": "lnbc1" }).to_string()),
        Err(LnurlError::MissingField { field: "settled" })
    );
    assert_eq!(
        parse_verify_response(&json!({ "settled": "yes", "pr": "x" }).to_string()),
        Err(LnurlError::InvalidField { field: "settled" })
    );
    assert_eq!(
        parse_verify_response(&json!({ "settled": true, "preimage": 1, "pr": "x" }).to_string()),
        Err(LnurlError::InvalidField { field: "preimage" })
    );
}

#[test]
fn decoding_skips_pay_request_and_clock_checks_but_not_the_signature() {
    let bolt11 = invoice(&Spec::new(21_000));
    let decoded = decode_invoice(&bolt11).expect("decodes");
    assert_eq!(decoded.payment_hash, hex::encode(payment_hash()));
    assert_eq!(decoded.amount_msat, Some(21_000));
    assert_eq!(decoded.currency, "bc");
    assert_eq!(decoded.expires_at_unix, CREATED_AT + EXPIRY_SECS);

    let no_amount = invoice(&Spec {
        amount_msat: None,
        ..Spec::new(0)
    });
    assert_eq!(decode_invoice(&no_amount).map(|d| d.amount_msat), Ok(None));
    let forged = invoice(&Spec {
        payee: Some(2),
        ..Spec::new(21_000)
    });
    assert_eq!(decode_invoice(&forged), Err(InvoiceError::InvalidSignature));
    assert!(matches!(
        decode_invoice("lnbc1garbage"),
        Err(InvoiceError::Unparseable { .. })
    ));
}
