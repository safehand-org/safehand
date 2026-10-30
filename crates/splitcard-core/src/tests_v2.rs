//! Version 2 cards and the `lud16` signature rules, plus the real version 1
//! card that must keep verifying exactly as before.

use nostr::{
    event::{Event, Kind, Tag, UnsignedEvent},
    key::Keys,
    types::Timestamp,
};
use secp256k1::Secp256k1;

use crate::{
    card::{Card, CardV2, LeaveRule, MemberV2, PeerReview, RulesV2},
    error::{PayoutAddressError, ValidationError},
    hash::{canonical_json, card_hash_hex},
    signature::{
        SIGNATURE_KIND, SignatureError, SignatureTemplateError, Signer,
        unsigned_signature_template, verify_signature_event,
    },
    split::{compute_payouts, payout_addresses},
    status::{CardStatus, card_status},
    supersede::{check_supersede, verify_supersede_signature_event},
    validate::{MAX_WEIGHT, validate},
};

fn keys(number: u8) -> Keys {
    Keys::parse(&format!("{number:064x}"))
        .unwrap_or_else(|error| panic!("test key must parse: {error}"))
}

fn pubkey(number: u8) -> String {
    keys(number).public_key().to_hex()
}

fn address(number: u8) -> String {
    format!("member{number}@example.com")
}

fn v2_card(numbers: &[u8]) -> CardV2 {
    CardV2 {
        version: 2,
        card_id: "lagos-2026/llamas".to_owned(),
        event: "Lagos 2026".to_owned(),
        team: "Llamas".to_owned(),
        members: numbers
            .iter()
            .zip(1_u64..)
            .map(|(number, weight)| MemberV2 {
                pubkey: pubkey(*number),
                weight,
                label: Some(format!("Member {number}")),
            })
            .collect(),
        rules: RulesV2 {
            peer_review: None,
            leave: LeaveRule::KeepPercent { keep_percent: 15 },
        },
        supersedes: None,
    }
}

fn hash(card: &Card) -> String {
    card_hash_hex(card).unwrap_or_else(|error| panic!("card must hash: {error}"))
}

fn tag(parts: &[&str]) -> Tag {
    Tag::parse(parts.iter().copied()).unwrap_or_else(|error| panic!("tag must parse: {error}"))
}

fn sign(number: u8, tags: Vec<Tag>) -> Event {
    let unsigned = UnsignedEvent::new(
        keys(number).public_key(),
        Timestamp::from_secs(1_700_000_000),
        Kind::from_u16(SIGNATURE_KIND),
        tags,
        "",
    );
    let id = unsigned.compute_id();
    let sig = keys(number).sign_schnorr_with_aux_rand(
        &Secp256k1::signing_only(),
        id.as_bytes(),
        &[0; 32],
    );
    unsigned
        .add_signature(sig)
        .unwrap_or_else(|error| panic!("event must sign: {error}"))
}

/// A signature over `card` with the given extra tags after `d` and `splitcard`.
fn signature_with(card: &Card, number: u8, extra: &[&[&str]]) -> Event {
    let hash = hash(card);
    let mut tags = vec![tag(&["d", &hash]), tag(&["splitcard", &hash])];
    tags.extend(extra.iter().map(|parts| tag(parts)));
    sign(number, tags)
}

/// A valid version 2 signature carrying the member's own address.
fn signature(card: &Card, number: u8) -> Event {
    signature_with(card, number, &[&["lud16", &address(number)]])
}

fn signer(number: u8) -> Signer {
    Signer {
        pubkey: pubkey(number),
        lightning_address: Some(address(number)),
    }
}

fn invalid(card: CardV2) -> ValidationError {
    match validate(&card.into()) {
        Ok(()) => panic!("card must be invalid"),
        Err(error) => error,
    }
}

// Schema and parsing.

#[test]
fn version_2_json_parses_as_version_2_and_hashes_canonically() {
    let json = format!(
        r#"{{"version":2,"card_id":"c","event":"e","team":"t","members":[{{"pubkey":"{}","weight":1,"label":"Alice"}},{{"pubkey":"{}","weight":1}}],"rules":{{"peer_review":null,"leave":{{"kind":"nothing"}}}},"supersedes":null}}"#,
        pubkey(1),
        pubkey(2),
    );
    let card = Card::from_json(&json).unwrap_or_else(|error| panic!("must parse: {error}"));
    let Card::V2(inner) = &card else {
        panic!("expected a version 2 card");
    };
    assert_eq!(inner.members[0].label.as_deref(), Some("Alice"));
    assert_eq!(inner.members[1].label, None);
    assert_eq!(inner.rules.leave, LeaveRule::Nothing {});
    assert_eq!(validate(&card), Ok(()));
    let canonical = canonical_json(&card).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        !canonical.contains(r#""label":null"#),
        "an absent label stays absent: {canonical}"
    );
    assert!(canonical.contains(r#""leave":{"kind":"nothing"}"#));
    let keep = json.replace(
        r#"{"kind":"nothing"}"#,
        r#"{"kind":"keep_percent","keep_percent":15}"#,
    );
    let Ok(Card::V2(keep)) = Card::from_json(&keep) else {
        panic!("keep_percent must parse");
    };
    assert_eq!(
        keep.rules.leave,
        LeaveRule::KeepPercent { keep_percent: 15 }
    );
}

#[test]
fn version_2_rejects_unknown_and_version_1_fields() {
    let base = v2_card(&[1, 2]);
    let json = serde_json::to_string(&Card::from(base)).unwrap_or_else(|error| panic!("{error}"));
    for (from, to) in [
        // A version 1 member field.
        (r#""weight":1"#, r#""weight":1,"name":"Ann""#),
        (
            r#""weight":1"#,
            r#""weight":1,"payout":{"lightning":{"address":"a@b.co"}}"#,
        ),
        // An unknown leave field, an unknown leave kind and a missing percent.
        (
            r#""leave":{"kind":"keep_percent","keep_percent":15}"#,
            r#""leave":{"kind":"nothing","extra":1}"#,
        ),
        (
            r#""leave":{"kind":"keep_percent","keep_percent":15}"#,
            r#""leave":{"kind":"everything"}"#,
        ),
        (
            r#""leave":{"kind":"keep_percent","keep_percent":15}"#,
            r#""leave":{"kind":"keep_percent"}"#,
        ),
        // The leave rule is required.
        (r#","leave":{"kind":"keep_percent","keep_percent":15}"#, ""),
    ] {
        assert!(json.contains(from), "{from} not in {json}");
        let changed = json.replacen(from, to, 1);
        assert!(
            Card::from_json(&changed).is_err(),
            "must be rejected: {changed}"
        );
    }
}

#[test]
fn other_versions_still_parse_as_version_1() {
    // Exactly as before version 2: a version 3 card parses, then fails validation.
    let v3 = REAL_V1_CARD.replacen(r#""version":1"#, r#""version":3"#, 1);
    let card = Card::from_json(&v3).unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(card, Card::V1(_)));
    assert_eq!(
        validate(&card),
        Err(ValidationError::UnsupportedVersion { version: 3 })
    );
    // A version 2 shape under another version is not a version 1 card.
    let json = serde_json::to_string(&Card::from(v2_card(&[1, 2])))
        .unwrap_or_else(|error| panic!("{error}"))
        .replacen(r#""version":2"#, r#""version":3"#, 1);
    assert!(Card::from_json(&json).is_err());
}

#[test]
fn every_version_2_validation_rule_is_enforced() {
    assert_eq!(validate(&v2_card(&[1, 2, 3]).into()), Ok(()));

    let mut card = v2_card(&[1, 2]);
    card.version = 3;
    assert_eq!(
        invalid(card),
        ValidationError::UnsupportedVersion { version: 3 }
    );

    for field in ["card_id", "event", "team"] {
        let mut card = v2_card(&[1, 2]);
        match field {
            "card_id" => card.card_id.clear(),
            "event" => card.event.clear(),
            _ => card.team.clear(),
        }
        assert_eq!(invalid(card), ValidationError::EmptyField { field });
    }

    assert_eq!(
        invalid(v2_card(&[1])),
        ValidationError::InvalidMemberCount { count: 1 }
    );
    let twenty: Vec<u8> = (1..=20).collect();
    assert_eq!(validate(&v2_card(&twenty).into()), Ok(()));
    let twenty_one: Vec<u8> = (1..=21).collect();
    assert_eq!(
        invalid(v2_card(&twenty_one)),
        ValidationError::InvalidMemberCount { count: 21 }
    );

    let mut card = v2_card(&[1, 2]);
    card.members[1].pubkey = card.members[1].pubkey.to_uppercase();
    assert_eq!(invalid(card), ValidationError::InvalidPubkey { index: 1 });

    let mut card = v2_card(&[1, 2]);
    card.members[1].pubkey = card.members[0].pubkey.clone();
    assert_eq!(invalid(card), ValidationError::DuplicatePubkey { index: 1 });

    let mut card = v2_card(&[1, 2]);
    card.members[0].weight = 0;
    assert_eq!(invalid(card), ValidationError::ZeroWeight { index: 0 });

    let mut card = v2_card(&[1, 2]);
    card.members[0].weight = MAX_WEIGHT;
    assert_eq!(validate(&card.clone().into()), Ok(()));
    card.members[0].weight = MAX_WEIGHT + 1;
    assert_eq!(
        invalid(card),
        ValidationError::WeightTooLarge {
            index: 0,
            weight: MAX_WEIGHT + 1
        }
    );

    let mut card = v2_card(&[1, 2]);
    card.supersedes = Some("A".repeat(64));
    assert_eq!(invalid(card), ValidationError::InvalidSupersedes);

    let mut card = v2_card(&[1, 2]);
    card.rules.peer_review = Some(PeerReview {
        max_shift_points: 51,
    });
    assert_eq!(
        invalid(card),
        ValidationError::InvalidPeerReviewMaxShiftPoints
    );
}

#[test]
fn labels_are_optional_short_and_free_of_control_characters() {
    let with_label = |label: Option<&str>| {
        let mut card = v2_card(&[1, 2]);
        card.members[1].label = label.map(str::to_owned);
        validate(&card.into())
    };
    let bad = Err(ValidationError::InvalidMemberLabel { index: 1 });
    assert_eq!(with_label(None), Ok(()));
    assert_eq!(with_label(Some("Ada Lovelace")), Ok(()));
    // Forty characters, counted as characters rather than bytes.
    assert_eq!(with_label(Some(&"é".repeat(40))), Ok(()));
    assert_eq!(with_label(Some(&"a".repeat(41))), bad);
    assert_eq!(with_label(Some("")), bad);
    assert_eq!(with_label(Some("Ada\nLovelace")), bad);
    assert_eq!(with_label(Some("Ada\u{7f}")), bad);
}

#[test]
fn leave_keep_percent_is_1_to_99() {
    let with_percent = |keep_percent: u32| {
        let mut card = v2_card(&[1, 2]);
        card.rules.leave = LeaveRule::KeepPercent { keep_percent };
        validate(&card.into())
    };
    assert_eq!(with_percent(1), Ok(()));
    assert_eq!(with_percent(99), Ok(()));
    for keep_percent in [0, 100, 1_000] {
        assert_eq!(
            with_percent(keep_percent),
            Err(ValidationError::InvalidLeaveKeepPercent { keep_percent })
        );
    }
    let mut card = v2_card(&[1, 2]);
    card.rules.leave = LeaveRule::Nothing {};
    assert_eq!(validate(&card.into()), Ok(()));
}

#[test]
fn changing_a_version_2_field_changes_the_hash() {
    let original = hash(&v2_card(&[1, 2]).into());
    let mut changed = v2_card(&[1, 2]);
    changed.members[0].label = None;
    assert_ne!(hash(&changed.into()), original);
    let mut changed = v2_card(&[1, 2]);
    changed.rules.leave = LeaveRule::Nothing {};
    assert_ne!(hash(&changed.into()), original);
    let mut changed = v2_card(&[1, 2]);
    changed.members[1].weight += 1;
    assert_ne!(hash(&changed.into()), original);
}

// Signature templates.

#[test]
fn template_adds_a_lud16_tag_only_when_given_an_address() {
    let card: Card = v2_card(&[1, 2]).into();
    let hash = hash(&card);
    let with = unsigned_signature_template(&hash, &pubkey(1), 5, Some("member1@example.com"))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        with.tags.to_vec(),
        vec![
            tag(&["d", &hash]),
            tag(&["splitcard", &hash]),
            tag(&["lud16", "member1@example.com"])
        ]
    );
    let without = unsigned_signature_template(&hash, &pubkey(1), 5, None)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(without.tags.len(), 2);
    for bad in ["Member1@example.com", "member1", "", "a@b@c.com"] {
        assert_eq!(
            unsigned_signature_template(&hash, &pubkey(1), 5, Some(bad)),
            Err(SignatureTemplateError::InvalidLightningAddress),
            "{bad:?}"
        );
    }
}

// Signature verification.

fn verify_error(card: &Card, event: &Event) -> SignatureError {
    match verify_signature_event(card, event) {
        Ok(signer) => panic!("expected an error, got {signer:?}"),
        Err(error) => error,
    }
}

#[test]
fn version_2_signature_returns_the_signed_address() {
    let card: Card = v2_card(&[1, 2]).into();
    let verified = verify_signature_event(&card, &signature(&card, 2))
        .unwrap_or_else(|error| panic!("must verify: {error}"));
    assert_eq!(verified, signer(2));
}

#[test]
fn version_2_signature_needs_exactly_one_valid_lud16_tag() {
    let card: Card = v2_card(&[1, 2]).into();
    assert!(matches!(
        verify_error(&card, &signature_with(&card, 1, &[])),
        SignatureError::MissingLud16Tag
    ));
    assert!(matches!(
        verify_error(
            &card,
            &signature_with(
                &card,
                1,
                &[&["lud16", "a@example.com"], &["lud16", "b@example.com"]]
            )
        ),
        SignatureError::MultipleLud16Tags
    ));
    for bad in [
        &["lud16", "Member1@Example.com"][..],
        &["lud16", "not-an-address"],
        &["lud16", ""],
        &["lud16"],
        &["lud16", "member1@example.com", "extra"],
    ] {
        assert!(
            matches!(
                verify_error(&card, &signature_with(&card, 1, &[bad])),
                SignatureError::InvalidLud16
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn version_1_signature_with_a_lud16_tag_is_rejected() {
    let card = real_v1_card();
    let event = signature_with(&card, 1, &[&["lud16", "member1@example.com"]]);
    assert!(matches!(
        verify_error(&card, &event),
        SignatureError::UnexpectedLud16Tag
    ));
}

#[test]
fn version_2_non_member_is_rejected_after_the_tag_checks() {
    let card: Card = v2_card(&[1, 2]).into();
    assert!(matches!(
        verify_error(&card, &signature(&card, 9)),
        SignatureError::NonMember
    ));
}

// Status and payouts.

#[test]
fn status_reports_each_signers_address_and_the_first_event_wins() {
    let card: Card = v2_card(&[1, 2, 3]).into();
    let second_address = signature_with(&card, 1, &[&["lud16", "other@example.com"]]);
    let unsigned = signature_with(&card, 3, &[]);
    let report = card_status(
        &card,
        &[
            signature(&card, 2),
            signature(&card, 1),
            second_address,
            unsigned,
        ],
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        report.status,
        CardStatus::Draft {
            signed: vec![signer(1), signer(2)],
            missing: vec![pubkey(3)],
        }
    );
    assert_eq!(report.rejected.len(), 1);
    assert!(matches!(
        report.rejected[0].1,
        SignatureError::MissingLud16Tag
    ));
}

#[test]
fn version_2_payouts_split_by_weight_and_pay_the_signed_addresses() {
    let card: Card = v2_card(&[1, 2, 3]).into();
    assert_eq!(
        compute_payouts(&card, 600),
        Ok(vec![(pubkey(1), 100), (pubkey(2), 200), (pubkey(3), 300)])
    );
    let events = [
        signature(&card, 3),
        signature(&card, 1),
        signature(&card, 2),
    ];
    let paired = payout_addresses(&card, &events).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        paired,
        vec![
            (pubkey(1), address(1)),
            (pubkey(2), address(2)),
            (pubkey(3), address(3)),
        ]
    );
    match payout_addresses(&card, &events[..2]) {
        Err(PayoutAddressError::MissingSignature { index, pubkey: key }) => {
            assert_eq!((index, key), (1, pubkey(2)));
        }
        other => panic!("expected MissingSignature, got {other:?}"),
    }
}

#[test]
fn version_1_payout_addresses_come_from_the_card() {
    let card = real_v1_card();
    let paired = payout_addresses(&card, &[]).unwrap_or_else(|error| panic!("{error}"));
    let addresses: Vec<&str> = paired.iter().map(|(_, address)| address.as_str()).collect();
    assert_eq!(
        addresses,
        [
            "alice@blink.sv",
            "bob@getalby.com",
            "chidi@walletofsatoshi.com"
        ]
    );
}

// Superseding.

#[test]
fn version_2_card_can_supersede_with_signed_addresses_from_everyone() {
    let old: Card = v2_card(&[1, 2, 3]).into();
    let mut new = v2_card(&[1, 2, 4]);
    new.supersedes = Some(hash(&old));
    let new: Card = new.into();
    let all: Vec<Event> = [1, 2, 3, 4]
        .into_iter()
        .map(|number| signature(&new, number))
        .collect();
    assert!(check_supersede(&old, &new, &all).is_ok());
    // The removed member 3 must sign the new card with a lud16 tag too.
    let removed_without_tag = signature_with(&new, 3, &[]);
    assert!(matches!(
        verify_supersede_signature_event(&old, &new, &removed_without_tag),
        Err(SignatureError::MissingLud16Tag)
    ));
    let mut without = all.clone();
    without[2] = removed_without_tag;
    assert!(check_supersede(&old, &new, &without).is_err());
    assert_eq!(
        verify_supersede_signature_event(&old, &new, &all[2]).ok(),
        Some(signer(3))
    );
}

// The real version 1 card, timestamped in Bitcoin block 973,120.

const REAL_V1_CARD: &str = r#"{"card_id":"ots-check-1792958355","event":"ots-check-1792958355","members":[{"name":"Alice","payout":{"lightning":{"address":"alice@blink.sv"}},"pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","weight":36},{"name":"Bob","payout":{"lightning":{"address":"bob@getalby.com"}},"pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","weight":35},{"name":"Chidi","payout":{"lightning":{"address":"chidi@walletofsatoshi.com"}},"pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","weight":29}],"rules":{"peer_review":null},"supersedes":null,"team":"Llamas","version":1}"#;
const REAL_V1_HASH: &str = "464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a";
const REAL_V1_SIGNATURES: [&str; 3] = [
    r#"{"id":"03dae6bf7e91891c473911a924a2fff31ccd6e212fed9ac3d49813a80ed9d705","pubkey":"1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"4116b055046d05ed594aceaa72f865ea3ab803218d2a0022c26dffab50eef747a36601f75155b63ab85ef6d01535ffb0d9bfa2bc932436cde8d0ffbac6109964"}"#,
    r#"{"id":"2f212fa7224252df5abaa03d2e1f39b216d4417297410804e5fd16a35ab16f2a","pubkey":"4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"0b65e7142f7299f522f95b52abf7391165d4ac59b4b0be4c8cc5e63559e43f79e66d7247dabbfc27bc1ad9ff6bf17acbc099f64cea5eec776f2905579a5027c7"}"#,
    r#"{"id":"064502d94fb1e6d13118aa2fdaa2e1ec5c7f1df17213883788416c41064160dc","pubkey":"531fe6068134503d2723133227c867ac8fa6c83c537e9a44c3c5bdbdcb1fe337","created_at":1792958355,"kind":30078,"tags":[["d","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"],["splitcard","464eb59d7ca6815ea63b793b252f9bc3690a3b110c45ea2d02af9d0a9a7a1d5a"]],"content":"","sig":"7cb44a25a234e15eaaca331d52d196aef9bd9fe817295da9491af72aae3a903b6a071789af5ed23aa5269fe1500262df86c8186ef06a478bde792d9e96c2e89a"}"#,
];

fn real_v1_card() -> Card {
    Card::from_json(REAL_V1_CARD).unwrap_or_else(|error| panic!("real card must parse: {error}"))
}

#[test]
fn real_v1_card_keeps_its_hash_canonical_json_and_status() {
    let card = real_v1_card();
    assert!(matches!(card, Card::V1(_)));
    assert_eq!(validate(&card), Ok(()));
    assert_eq!(
        canonical_json(&card).unwrap_or_else(|error| panic!("{error}")),
        REAL_V1_CARD
    );
    assert_eq!(hash(&card), REAL_V1_HASH);
    let events: Vec<Event> = REAL_V1_SIGNATURES
        .iter()
        .map(|json| serde_json::from_str(json).unwrap_or_else(|error| panic!("{error}")))
        .collect();
    let report = card_status(&card, &events).unwrap_or_else(|error| panic!("{error}"));
    assert!(report.rejected.is_empty());
    let signed: Vec<Signer> = card
        .pubkeys()
        .into_iter()
        .map(|pubkey| Signer {
            pubkey: pubkey.to_owned(),
            lightning_address: None,
        })
        .collect();
    assert_eq!(report.status, CardStatus::Active { signed });
    assert_eq!(
        compute_payouts(&card, 100_001).map(|payouts| payouts.len()),
        Ok(3)
    );
}
