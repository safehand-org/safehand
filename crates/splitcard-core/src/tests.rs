use nostr::{
    event::{Event, EventId, Kind, Tag, UnsignedEvent},
    key::Keys,
    types::Timestamp,
};
use proptest::prelude::*;
use secp256k1::Secp256k1;

use crate::{
    card::{Card, CardV1, Member, Payout, PeerReview, Rules},
    error::{PayoutError, SplitError, ValidationError},
    hash::card_hash_hex,
    signature::{
        SIGNATURE_KIND, SignatureError, SignatureTemplateError, Signer,
        unsigned_signature_template, verify_signature_event,
    },
    split::{compute_payouts, display_basis_points, split},
    status::{CardStatus, StatusError, StatusReport, card_status},
    supersede::{SupersedeError, check_supersede, verify_supersede_signature_event},
    validate::{MAX_WEIGHT, validate},
};

fn pubkey(number: u8) -> String {
    format!("{number:064x}")
}

fn fixture_card() -> Card {
    Card::V1(CardV1 {
        version: 1,
        card_id: "btcpp-lagos-2026/team-42".to_owned(),
        event: "BTC++ Lagos 2026".to_owned(),
        team: "Team 42".to_owned(),
        members: vec![
            Member {
                pubkey: pubkey(1),
                name: "Ada".to_owned(),
                weight: 36,
                payout: Payout::Lightning {
                    address: "ada@example.com".to_owned(),
                },
            },
            Member {
                pubkey: pubkey(2),
                name: "Bola".to_owned(),
                weight: 35,
                payout: Payout::Lightning {
                    address: "bola@example.org".to_owned(),
                },
            },
            Member {
                pubkey: pubkey(3),
                name: "Chidi".to_owned(),
                weight: 29,
                payout: Payout::Lightning {
                    address: "chidi@example.net".to_owned(),
                },
            },
        ],
        rules: Rules {
            peer_review: Some(PeerReview {
                max_shift_points: 10,
            }),
        },
        supersedes: Some("a".repeat(64)),
    })
}

/// The version 1 card inside `card`, for tests that change one field.
fn v1(card: &mut Card) -> &mut CardV1 {
    match card {
        Card::V1(card) => card,
        Card::V2(_) => panic!("expected a version 1 card"),
    }
}

fn hash_hex(card: &Card) -> String {
    card_hash_hex(card).unwrap_or_else(|error| panic!("fixture must hash: {error}"))
}

#[test]
fn canonical_json_sorts_object_keys() {
    let value: serde_json::Value = serde_json::from_str(r#"{"z":1,"a":{"y":2,"b":3}}"#)
        .unwrap_or_else(|error| panic!("test JSON must parse: {error}"));
    let canonical = serde_json::to_string(&value)
        .unwrap_or_else(|error| panic!("test JSON must serialize: {error}"));
    assert_eq!(canonical, r#"{"a":{"b":3,"y":2},"z":1}"#);
}

#[test]
fn fixture_card_has_pinned_hash() {
    assert_eq!(
        hash_hex(&fixture_card()),
        "37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"
    );
}

#[test]
fn reordered_json_and_whitespace_preserve_hash() {
    let parsed = Card::from_json(
        r#"
        {
          "team": "Team 42",
          "rules": { "peer_review": { "max_shift_points": 10 } },
          "members": [
            { "weight": 36, "payout": { "lightning": { "address": "ada@example.com" } }, "name": "Ada", "pubkey": "0000000000000000000000000000000000000000000000000000000000000001" },
            { "name": "Bola", "pubkey": "0000000000000000000000000000000000000000000000000000000000000002", "payout": { "lightning": { "address": "bola@example.org" } }, "weight": 35 },
            { "payout": { "lightning": { "address": "chidi@example.net" } }, "weight": 29, "pubkey": "0000000000000000000000000000000000000000000000000000000000000003", "name": "Chidi" }
          ],
          "supersedes": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "event": "BTC++ Lagos 2026",
          "version": 1,
          "card_id": "btcpp-lagos-2026/team-42"
        }
        "#,
    )
    .unwrap_or_else(|error| panic!("fixture JSON must parse: {error}"));

    assert_eq!(hash_hex(&parsed), hash_hex(&fixture_card()));
}

#[test]
fn changing_any_card_field_changes_hash() {
    let original = fixture_card();
    let original_hash = hash_hex(&original);
    let mut changes = Vec::new();

    let mut card = original.clone();
    v1(&mut card).version = 2;
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).card_id.push('x');
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).event.push('x');
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).team.push('x');
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).members[0].pubkey = pubkey(9);
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).members[0].name.push('x');
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).members[0].weight += 1;
    changes.push(card);
    let mut card = original.clone();
    let Payout::Lightning { address } = &mut v1(&mut card).members[0].payout;
    address.push('x');
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).rules.peer_review = None;
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card)
        .rules
        .peer_review
        .as_mut()
        .unwrap_or_else(|| panic!("fixture has peer review"))
        .max_shift_points += 1;
    changes.push(card);
    let mut card = original.clone();
    v1(&mut card).supersedes = None;
    changes.push(card);

    for changed in changes {
        assert_ne!(hash_hex(&changed), original_hash);
    }
}

#[test]
fn validation_accepts_fixture() {
    assert_eq!(validate(&fixture_card()), Ok(()));
}

#[test]
fn every_validation_error_variant_is_reachable() {
    let mut card = fixture_card();
    v1(&mut card).version = 2;
    assert!(matches!(
        validate(&card),
        Err(ValidationError::UnsupportedVersion { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).card_id.clear();
    assert!(matches!(
        validate(&card),
        Err(ValidationError::EmptyField { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).members.truncate(1);
    assert!(matches!(
        validate(&card),
        Err(ValidationError::InvalidMemberCount { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).members[0].pubkey = "A".repeat(64);
    assert!(matches!(
        validate(&card),
        Err(ValidationError::InvalidPubkey { .. })
    ));

    let mut card = fixture_card();
    let first = v1(&mut card).members[0].pubkey.clone();
    v1(&mut card).members[1].pubkey = first;
    assert!(matches!(
        validate(&card),
        Err(ValidationError::DuplicatePubkey { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).members[0].name.clear();
    assert!(matches!(
        validate(&card),
        Err(ValidationError::EmptyMemberName { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).members[0].weight = 0;
    assert!(matches!(
        validate(&card),
        Err(ValidationError::ZeroWeight { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).members[0].weight = MAX_WEIGHT + 1;
    assert!(matches!(
        validate(&card),
        Err(ValidationError::WeightTooLarge { .. })
    ));

    let mut card = fixture_card();
    let Payout::Lightning { address } = &mut v1(&mut card).members[0].payout;
    address.clear();
    assert!(matches!(
        validate(&card),
        Err(ValidationError::InvalidLightningAddress { .. })
    ));

    let mut card = fixture_card();
    v1(&mut card).supersedes = Some("A".repeat(64));
    assert!(matches!(
        validate(&card),
        Err(ValidationError::InvalidSupersedes)
    ));

    let mut card = fixture_card();
    v1(&mut card).rules.peer_review = Some(PeerReview {
        max_shift_points: 0,
    });
    assert!(matches!(
        validate(&card),
        Err(ValidationError::InvalidPeerReviewMaxShiftPoints)
    ));
}

#[test]
fn split_matches_spec_examples() {
    assert_eq!(
        split(100_001, &[36, 35, 29]),
        Ok(vec![36_001, 35_000, 29_000])
    );
    assert_eq!(split(100, &[1, 1, 1]), Ok(vec![34, 33, 33]));
    assert_eq!(split(4, &[1, 1, 1]), Ok(vec![2, 1, 1]));
}

#[test]
fn split_errors_are_reachable() {
    assert_eq!(split(1, &[]), Err(SplitError::NoWeights));
    assert_eq!(split(2, &[1, 0]), Err(SplitError::ZeroWeight { index: 1 }));
    assert_eq!(
        split(2, &[1, 1, 1]),
        Err(SplitError::PrizeTooSmall {
            prize_sats: 2,
            member_count: 3,
        })
    );
}

#[test]
fn split_handles_maximum_weights_without_overflow() {
    let amounts = split(2_100_000_000_000_000, &[u64::MAX, u64::MAX - 1, 1])
        .unwrap_or_else(|error| panic!("split must succeed: {error}"));
    assert_eq!(
        amounts
            .iter()
            .map(|amount| u128::from(*amount))
            .sum::<u128>(),
        2_100_000_000_000_000
    );
}

#[test]
fn display_basis_points_is_exact() {
    assert_eq!(
        display_basis_points(&[1, 1, 1]),
        Ok(vec![3_334, 3_333, 3_333])
    );
}

#[test]
fn compute_payouts_validates_and_preserves_member_order() {
    let card = fixture_card();
    assert_eq!(
        compute_payouts(&card, 100),
        Ok(vec![(pubkey(1), 36), (pubkey(2), 35), (pubkey(3), 29)])
    );
}

fn card_with_weights(weights: &[u64]) -> Card {
    let mut card = fixture_card();
    v1(&mut card).members = weights
        .iter()
        .zip(1_u8..)
        .map(|(weight, number)| Member {
            pubkey: pubkey(number),
            name: format!("Member {number}"),
            weight: *weight,
            payout: Payout::Lightning {
                address: format!("member{number}@example.com"),
            },
        })
        .collect();
    card
}

fn card_with_address(address: &str) -> Card {
    let mut card = fixture_card();
    v1(&mut card).members[1].payout = Payout::Lightning {
        address: address.to_owned(),
    };
    card
}

#[test]
fn lud16_lightning_addresses_are_accepted() {
    let long_label = format!("alice@{}.com", "a".repeat(63));
    // 63 + 1 + 63 + 1 + 63 + 1 + 61 = 253 bytes, the longest allowed domain.
    let longest_domain = format!("alice@{0}.{0}.{0}.{1}", "a".repeat(63), "a".repeat(61));
    assert_eq!(longest_domain.len() - "alice@".len(), 253);
    for address in [
        longest_domain.as_str(),
        "alice@blink.sv",
        "a.b-c_d+e@pay.example.co.uk",
        "0x7@xn--bcher-kva.example",
        "bob@sub-1.example.io",
        "x@a.b",
        long_label.as_str(),
    ] {
        assert_eq!(validate(&card_with_address(address)), Ok(()), "{address}");
    }
}

#[test]
fn non_lud16_lightning_addresses_are_rejected() {
    let long_label = format!("alice@{}.com", "a".repeat(64));
    // Every label is valid; only the 254-byte total is too long.
    let long_domain = format!("alice@{0}.{0}.{0}.{1}", "a".repeat(63), "a".repeat(62));
    assert_eq!(long_domain.len() - "alice@".len(), 254);
    for address in [
        // The case from the bug report: valid in old core, rejected at payout.
        "Alice!@Blink.sv",
        "Alice@blink.sv",
        "alice@Blink.sv",
        "alice!@blink.sv",
        "al ice@blink.sv",
        "alicé@blink.sv",
        "alice@blink.sv@x.com",
        "alice.blink.sv",
        "@blink.sv",
        "alice@",
        "alice@localhost",
        "alice@127.0.0.1",
        "alice@blink..sv",
        "alice@-blink.sv",
        "alice@blink-.sv",
        "alice@bl_ink.sv",
        "alice@blink.sv:8080",
        "alice@blink.sv.",
        long_label.as_str(),
        long_domain.as_str(),
    ] {
        assert_eq!(
            validate(&card_with_address(address)),
            Err(ValidationError::InvalidLightningAddress { index: 1 }),
            "{address}"
        );
    }
}

#[test]
fn weight_cap_is_inclusive() {
    let at_cap = card_with_weights(&[1, MAX_WEIGHT]);
    assert_eq!(validate(&at_cap), Ok(()));
    let over_cap = card_with_weights(&[1, MAX_WEIGHT + 1]);
    assert_eq!(
        validate(&over_cap),
        Err(ValidationError::WeightTooLarge {
            index: 1,
            weight: MAX_WEIGHT + 1,
        })
    );
}

#[test]
fn compute_payouts_rejects_weights_above_cap() {
    let card = card_with_weights(&[u64::MAX, u64::MAX - 1, 1]);
    assert_eq!(
        compute_payouts(&card, 2_100_000_000_000_000),
        Err(PayoutError::Validation(ValidationError::WeightTooLarge {
            index: 0,
            weight: u64::MAX,
        }))
    );
}

#[test]
fn compute_payouts_rejects_a_share_that_rounds_to_zero() {
    // The split alone gives [1.05e15, 1.05e15, 0]: weight 1 is too small a share.
    let card = card_with_weights(&[MAX_WEIGHT, MAX_WEIGHT - 1, 1]);
    assert_eq!(
        split(2_100_000_000_000_000, &[MAX_WEIGHT, MAX_WEIGHT - 1, 1]),
        Ok(vec![1_050_000_000_000_000, 1_050_000_000_000_000, 0])
    );
    assert_eq!(
        compute_payouts(&card, 2_100_000_000_000_000),
        Err(PayoutError::ShareRoundsToZero {
            index: 2,
            pubkey: pubkey(3),
        })
    );
}

/// Card weights mixing small and huge values, so some shares round to zero.
fn payout_input() -> impl Strategy<Value = (Vec<u64>, u64)> {
    let weight = prop_oneof![1_u64..=10, 1_u64..=MAX_WEIGHT];
    let prize = prop_oneof![0_u64..=1_000, any::<u64>()];
    (prop::collection::vec(weight, 2..=20), prize)
}

fn valid_split_input() -> impl Strategy<Value = (Vec<u64>, u64)> {
    prop::collection::vec(1_u64..=1_000_000, 1..=20).prop_flat_map(|weights| {
        let minimum_prize = weights.len() as u64;
        (Just(weights), minimum_prize..=2_100_000_000_000_000)
    })
}

const SIGNED_AT: u64 = 1_700_000_000;

fn nostr_keys(number: u8) -> Keys {
    let secret = format!("{number:064x}");
    Keys::parse(&secret).unwrap_or_else(|error| panic!("fixed secret key must parse: {error}"))
}

fn member_pubkey(number: u8) -> String {
    nostr_keys(number).public_key().to_hex()
}

fn signed_card(numbers: &[u8]) -> Card {
    Card::V1(CardV1 {
        version: 1,
        card_id: "phase-2/team-1".to_owned(),
        event: "Phase 2".to_owned(),
        team: "Team 1".to_owned(),
        members: numbers
            .iter()
            .enumerate()
            .map(|(index, number)| Member {
                pubkey: member_pubkey(*number),
                name: format!("Member {number}"),
                weight: (index as u64) + 1,
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
    Tag::parse(parts.iter().copied()).unwrap_or_else(|error| panic!("test tag must parse: {error}"))
}

fn tags(hash: &str, d_count: usize, splitcard_count: usize) -> Vec<Tag> {
    let mut result = Vec::new();
    for _ in 0..d_count {
        result.push(tag(&["d", hash]));
    }
    for _ in 0..splitcard_count {
        result.push(tag(&["splitcard", hash]));
    }
    result
}

/// Signs with all-zero BIP-340 auxiliary randomness so signatures are reproducible.
fn sign(keys: &Keys, unsigned: UnsignedEvent) -> Event {
    let secp = Secp256k1::signing_only();
    let id = unsigned.compute_id();
    let sig = keys.sign_schnorr_with_aux_rand(&secp, id.as_bytes(), &[0; 32]);
    unsigned
        .add_signature(sig)
        .unwrap_or_else(|error| panic!("fixed event must sign: {error}"))
}

fn sign_event(key_number: u8, event_kind: u16, event_tags: Vec<Tag>) -> Event {
    let keys = nostr_keys(key_number);
    let unsigned = UnsignedEvent::new(
        keys.public_key(),
        Timestamp::from_secs(SIGNED_AT),
        Kind::from_u16(event_kind),
        event_tags,
        "signed content",
    );
    sign(&keys, unsigned)
}

fn valid_signature(card: &Card, key_number: u8) -> Event {
    let hash = hash_hex(card);
    sign_event(key_number, SIGNATURE_KIND, tags(&hash, 1, 1))
}

/// A version 1 signer: no payout address in the signature.
fn v1_signer(number: u8) -> Signer {
    Signer {
        pubkey: member_pubkey(number),
        lightning_address: None,
    }
}

fn v1_signers(numbers: &[u8]) -> Vec<Signer> {
    numbers.iter().map(|number| v1_signer(*number)).collect()
}

fn pubkeys(numbers: &[u8]) -> Vec<String> {
    numbers
        .iter()
        .map(|number| member_pubkey(*number))
        .collect()
}

fn status_of(card: &Card, events: &[Event]) -> StatusReport {
    card_status(card, events).unwrap_or_else(|error| panic!("status must compute: {error}"))
}

fn rejected_ids(report: &StatusReport) -> Vec<EventId> {
    report.rejected.iter().map(|(id, _)| *id).collect()
}

// Signature template.

#[test]
fn signature_template_has_exact_fields() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let template = unsigned_signature_template(&hash, &member_pubkey(1), 123, None)
        .unwrap_or_else(|error| panic!("template must build: {error}"));
    assert_eq!(template.pubkey, nostr_keys(1).public_key());
    assert_eq!(template.created_at, Timestamp::from_secs(123));
    assert_eq!(template.kind, Kind::from_u16(SIGNATURE_KIND));
    assert_eq!(template.content, "");
    assert_eq!(
        template.tags.to_vec(),
        vec![tag(&["d", &hash]), tag(&["splitcard", &hash])]
    );
}

#[test]
fn signature_template_rejects_bad_card_hash() {
    let pubkey = member_pubkey(1);
    for bad_hash in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
    ] {
        assert_eq!(
            unsigned_signature_template(&bad_hash, &pubkey, 1, None),
            Err(SignatureTemplateError::InvalidCardHash)
        );
    }
}

#[test]
fn signature_template_rejects_bad_pubkey() {
    let hash = hash_hex(&signed_card(&[1, 2, 3]));
    // All-`f` is above the secp256k1 field prime, so it is never a valid x-only key.
    let uppercase = member_pubkey(1).to_uppercase();
    for bad_pubkey in ["", "not hex", &"f".repeat(64), &"a".repeat(63), &uppercase] {
        assert_eq!(
            unsigned_signature_template(&hash, bad_pubkey, 1, None),
            Err(SignatureTemplateError::InvalidPubkey)
        );
    }
}

// Signature verification.

#[test]
fn valid_signature_passes() {
    let card = signed_card(&[1, 2, 3]);
    let event = valid_signature(&card, 1);
    match verify_signature_event(&card, &event) {
        Ok(signer) => assert_eq!(signer, v1_signer(1)),
        Err(error) => panic!("valid signature must verify: {error}"),
    }
}

#[test]
fn signed_template_verifies_with_extra_tags_and_any_content() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let template = unsigned_signature_template(&hash, &member_pubkey(2), 5, None)
        .unwrap_or_else(|error| panic!("template must build: {error}"));
    let mut event_tags = template.tags.to_vec();
    event_tags.push(tag(&["t", "hackathon"]));
    event_tags.push(tag(&["p", &member_pubkey(1)]));
    let unsigned = UnsignedEvent::new(
        template.pubkey,
        template.created_at,
        template.kind,
        event_tags,
        "I agree to this split.",
    );
    let event = sign(&nostr_keys(2), unsigned);
    assert!(verify_signature_event(&card, &event).is_ok());
}

#[test]
fn wrong_event_id_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let mut event = valid_signature(&card, 1);
    event.content.push('x');
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::InvalidEventId)
    ));
}

#[test]
fn bad_signature_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let mut event = valid_signature(&card, 1);
    // A real signature, but by another key over another event.
    event.sig = valid_signature(&card, 2).sig;
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::InvalidSignature)
    ));
}

#[test]
fn wrong_kind_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(1, SIGNATURE_KIND + 1, tags(&hash, 1, 1));
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::WrongKind)
    ));
}

#[test]
fn missing_d_tag_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(1, SIGNATURE_KIND, tags(&hash, 0, 1));
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::MissingDTag)
    ));
}

#[test]
fn two_d_tags_are_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(1, SIGNATURE_KIND, tags(&hash, 2, 1));
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::MultipleDTags)
    ));
}

#[test]
fn d_tag_with_wrong_hash_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(
        1,
        SIGNATURE_KIND,
        vec![tag(&["d", &"b".repeat(64)]), tag(&["splitcard", &hash])],
    );
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::WrongDTagHash)
    ));
}

#[test]
fn missing_splitcard_tag_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(1, SIGNATURE_KIND, tags(&hash, 1, 0));
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::MissingSplitcardTag)
    ));
}

#[test]
fn two_splitcard_tags_are_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(1, SIGNATURE_KIND, tags(&hash, 1, 2));
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::MultipleSplitcardTags)
    ));
}

#[test]
fn splitcard_tag_with_wrong_hash_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let event = sign_event(
        1,
        SIGNATURE_KIND,
        vec![tag(&["d", &hash]), tag(&["splitcard", &"c".repeat(64)])],
    );
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::WrongSplitcardTagHash)
    ));
}

#[test]
fn signer_not_on_card_is_rejected() {
    let card = signed_card(&[1, 2, 3]);
    let event = valid_signature(&card, 4);
    assert!(matches!(
        verify_signature_event(&card, &event),
        Err(SignatureError::NonMember)
    ));
}

#[test]
fn signature_over_card_a_does_not_count_for_tampered_card_b() {
    let card_a = signed_card(&[1, 2, 3]);
    let event = valid_signature(&card_a, 1);
    let mut card_b = card_a.clone();
    v1(&mut card_b).members[0].weight += 1;
    assert!(verify_signature_event(&card_a, &event).is_ok());
    assert!(matches!(
        verify_signature_event(&card_b, &event),
        Err(SignatureError::WrongDTagHash)
    ));
    let report = status_of(&card_b, std::slice::from_ref(&event));
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: vec![],
            missing: pubkeys(&[1, 2, 3]),
        }
    );
    assert_eq!(rejected_ids(&report), vec![event.id]);
}

/// Pinned vector for checking with another Nostr implementation.
///
/// Secret key `00..01`, card hash `37a9ea…ca52` (the Phase 1 fixture card),
/// `created_at` 1700000000, signed via `unsigned_signature_template` with
/// all-zero BIP-340 aux randomness, so `sig` is reproducible too. Full event:
///
/// ```json
/// {"id":"efd8b98c288e4dc8f9e2efa9c7647a6bbdfb50208bdeab0be5321d85aa23f9e8","pubkey":"79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798","created_at":1700000000,"kind":30078,"tags":[["d","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"],["splitcard","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"]],"content":"","sig":"4aeaf8c05e75c8f2e1ea26b8057299b9d46966592a161c6e3756f7de2688599d4bd90d4c2c00492f1b125a94a0358c2620446b8acbb1b4808ff6f834e74b3ee0"}
/// ```
#[test]
fn signature_test_vector_is_pinned() {
    let card_hash = "37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52";
    assert_eq!(hash_hex(&fixture_card()), card_hash);
    let keys = nostr_keys(1);
    let template =
        unsigned_signature_template(card_hash, &keys.public_key().to_hex(), SIGNED_AT, None)
            .unwrap_or_else(|error| panic!("template must build: {error}"));
    let event = sign(&keys, template);
    assert!(event.verify_id());
    assert!(event.verify_signature());
    assert_eq!(
        event.id.to_hex(),
        "efd8b98c288e4dc8f9e2efa9c7647a6bbdfb50208bdeab0be5321d85aa23f9e8"
    );
    let json = serde_json::to_string(&event)
        .unwrap_or_else(|error| panic!("event must serialize: {error}"));
    assert_eq!(
        json,
        r#"{"id":"efd8b98c288e4dc8f9e2efa9c7647a6bbdfb50208bdeab0be5321d85aa23f9e8","pubkey":"79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798","created_at":1700000000,"kind":30078,"tags":[["d","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"],["splitcard","37a9ea893f875cc0d4ad0bc92fdbca5e8ad5cd048a09f3ca28e0057defdbca52"]],"content":"","sig":"4aeaf8c05e75c8f2e1ea26b8057299b9d46966592a161c6e3756f7de2688599d4bd90d4c2c00492f1b125a94a0358c2620446b8acbb1b4808ff6f834e74b3ee0"}"#
    );
}

// Card status.

#[test]
fn status_with_no_signatures_is_draft_with_everyone_missing() {
    let card = signed_card(&[1, 2, 3]);
    let report = status_of(&card, &[]);
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: vec![],
            missing: pubkeys(&[1, 2, 3]),
        }
    );
    assert!(report.rejected.is_empty());
}

#[test]
fn status_with_two_of_three_is_draft_with_the_right_member_missing() {
    let card = signed_card(&[1, 2, 3]);
    // Out of member order on purpose: output must follow card order.
    let events = [valid_signature(&card, 3), valid_signature(&card, 1)];
    let report = status_of(&card, &events);
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: v1_signers(&[1, 3]),
            missing: pubkeys(&[2]),
        }
    );
    assert!(report.rejected.is_empty());
}

#[test]
fn status_with_all_three_is_active() {
    let card = signed_card(&[1, 2, 3]);
    let events = [
        valid_signature(&card, 2),
        valid_signature(&card, 3),
        valid_signature(&card, 1),
    ];
    let report = status_of(&card, &events);
    assert_eq!(
        report.status,
        CardStatus::Active {
            signed: v1_signers(&[1, 2, 3]),
        }
    );
    assert!(report.rejected.is_empty());
}

#[test]
fn duplicate_signature_counts_once() {
    let card = signed_card(&[1, 2, 3]);
    let first = valid_signature(&card, 1);
    let later = UnsignedEvent::new(
        nostr_keys(1).public_key(),
        Timestamp::from_secs(SIGNED_AT + 60),
        Kind::from_u16(SIGNATURE_KIND),
        tags(&hash_hex(&card), 1, 1),
        "signing again",
    );
    let resigned = sign(&nostr_keys(1), later);
    let report = status_of(&card, &[first.clone(), first, resigned]);
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: v1_signers(&[1]),
            missing: pubkeys(&[2, 3]),
        }
    );
    assert!(report.rejected.is_empty());
}

#[test]
fn non_member_signature_is_rejected_on_draft_card() {
    let card = signed_card(&[1, 2, 3]);
    let outsider = valid_signature(&card, 4);
    let report = status_of(&card, &[valid_signature(&card, 1), outsider.clone()]);
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: v1_signers(&[1]),
            missing: pubkeys(&[2, 3]),
        }
    );
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].0, outsider.id);
    assert!(matches!(report.rejected[0].1, SignatureError::NonMember));
}

#[test]
fn non_member_signature_is_rejected_on_active_card() {
    let card = signed_card(&[1, 2, 3]);
    let outsider = valid_signature(&card, 4);
    let events = [
        valid_signature(&card, 1),
        outsider.clone(),
        valid_signature(&card, 2),
        valid_signature(&card, 3),
    ];
    let report = status_of(&card, &events);
    assert_eq!(
        report.status,
        CardStatus::Active {
            signed: v1_signers(&[1, 2, 3]),
        }
    );
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].0, outsider.id);
    assert!(matches!(report.rejected[0].1, SignatureError::NonMember));
}

#[test]
fn rejected_events_keep_input_order() {
    let card = signed_card(&[1, 2, 3]);
    let hash = hash_hex(&card);
    let wrong_kind = sign_event(2, 1, tags(&hash, 1, 1));
    let outsider = valid_signature(&card, 4);
    let missing_d = sign_event(3, SIGNATURE_KIND, tags(&hash, 0, 1));
    let report = status_of(
        &card,
        &[
            wrong_kind.clone(),
            valid_signature(&card, 1),
            outsider.clone(),
            missing_d.clone(),
        ],
    );
    assert_eq!(
        rejected_ids(&report),
        vec![wrong_kind.id, outsider.id, missing_d.id]
    );
    assert!(matches!(report.rejected[0].1, SignatureError::WrongKind));
    assert!(matches!(report.rejected[1].1, SignatureError::NonMember));
    assert!(matches!(report.rejected[2].1, SignatureError::MissingDTag));
}

#[test]
fn status_validates_card_first() {
    let mut card = signed_card(&[1, 2, 3]);
    v1(&mut card).members.truncate(1);
    assert!(matches!(
        card_status(&card, &[]),
        Err(StatusError::Validation(
            ValidationError::InvalidMemberCount { count: 1 }
        ))
    ));
}

// Superseding.

/// Old card has members 1, 2, 3; new card drops 3 and adds 4.
fn replacement() -> (Card, Card) {
    let old = signed_card(&[1, 2, 3]);
    let mut new = signed_card(&[1, 2, 4]);
    v1(&mut new).supersedes = Some(hash_hex(&old));
    (old, new)
}

#[test]
fn valid_replacement_passes() {
    let (old, new) = replacement();
    let events: Vec<Event> = [1, 2, 3, 4]
        .into_iter()
        .map(|number| valid_signature(&new, number))
        .collect();
    assert!(check_supersede(&old, &new, &events).is_ok());
}

#[test]
fn replacement_with_wrong_supersedes_hash_fails() {
    let (old, mut new) = replacement();
    v1(&mut new).supersedes = Some("b".repeat(64));
    let events: Vec<Event> = [1, 2, 3, 4]
        .into_iter()
        .map(|number| valid_signature(&new, number))
        .collect();
    assert!(matches!(
        check_supersede(&old, &new, &events),
        Err(SupersedeError::WrongSupersedesHash)
    ));
    v1(&mut new).supersedes = None;
    assert!(matches!(
        check_supersede(&old, &new, &events),
        Err(SupersedeError::WrongSupersedesHash)
    ));
}

#[test]
fn replacement_without_removed_members_signature_fails() {
    let (old, new) = replacement();
    let mut events: Vec<Event> = [1, 2, 4]
        .into_iter()
        .map(|number| valid_signature(&new, number))
        .collect();
    // Member 3's signature over the old card must not count for the new one.
    events.push(valid_signature(&old, 3));
    match check_supersede(&old, &new, &events) {
        Err(SupersedeError::MissingOldMemberSignatures { missing }) => {
            assert_eq!(missing, pubkeys(&[3]));
        }
        other => panic!("expected missing old member signatures, got {other:?}"),
    }
}

#[test]
fn replacement_without_new_members_signature_fails() {
    let (old, new) = replacement();
    let events: Vec<Event> = [1, 2, 3]
        .into_iter()
        .map(|number| valid_signature(&new, number))
        .collect();
    match check_supersede(&old, &new, &events) {
        Err(SupersedeError::MissingNewMemberSignatures { missing }) => {
            assert_eq!(missing, pubkeys(&[4]));
        }
        other => panic!("expected missing new member signatures, got {other:?}"),
    }
}

#[test]
fn supersede_signature_accepts_kept_added_and_removed_members() {
    let (old, new) = replacement();
    for number in [1, 2, 3, 4] {
        assert_eq!(
            verify_supersede_signature_event(&old, &new, &valid_signature(&new, number)).ok(),
            Some(v1_signer(number)),
        );
    }
}

#[test]
fn supersede_signature_rejects_outsiders_and_old_card_signatures() {
    let (old, new) = replacement();
    assert!(matches!(
        verify_supersede_signature_event(&old, &new, &valid_signature(&new, 5)),
        Err(SignatureError::NonMember)
    ));
    // A signature over the old card is not a signature over the new one.
    assert!(matches!(
        verify_supersede_signature_event(&old, &new, &valid_signature(&old, 3)),
        Err(SignatureError::WrongDTagHash)
    ));
}

proptest! {
    #[test]
    fn split_amounts_sum_to_prize((weights, prize) in valid_split_input()) {
        let amounts = split(prize, &weights).unwrap_or_else(|error| panic!("generated input must split: {error}"));
        prop_assert_eq!(amounts.iter().map(|amount| u128::from(*amount)).sum::<u128>(), u128::from(prize));
    }

    #[test]
    fn each_split_amount_is_floor_or_one_more((weights, prize) in valid_split_input()) {
        let amounts = split(prize, &weights).unwrap_or_else(|error| panic!("generated input must split: {error}"));
        let total_weight = weights.iter().map(|weight| u128::from(*weight)).sum::<u128>();
        for (weight, amount) in weights.iter().zip(amounts) {
            let base = u128::from(prize) * u128::from(*weight) / total_weight;
            prop_assert!(u128::from(amount) == base || u128::from(amount) == base + 1);
        }
    }

    #[test]
    fn scaling_weights_keeps_the_split(((weights, prize), factor) in (valid_split_input(), 2_u64..=1_000)) {
        let Some(scaled) = weights.iter().map(|weight| weight.checked_mul(factor)).collect::<Option<Vec<_>>>() else {
            prop_assume!(false);
            return Ok(());
        };
        prop_assert_eq!(split(prize, &weights), split(prize, &scaled));
    }

    #[test]
    fn repeated_splits_are_deterministic((weights, prize) in valid_split_input()) {
        prop_assert_eq!(split(prize, &weights), split(prize, &weights));
    }

    #[test]
    fn compute_payouts_errors_or_pays_every_member((weights, prize) in payout_input()) {
        if let Ok(payouts) = compute_payouts(&card_with_weights(&weights), prize) {
            prop_assert_eq!(payouts.len(), weights.len());
            prop_assert!(payouts.iter().all(|(_, amount)| *amount >= 1));
        }
    }
}
