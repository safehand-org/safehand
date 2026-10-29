//! The `.ots` format against the official example proofs. Expected
//! commitments and heights were produced by `python-opentimestamps` 0.4.5.

use sha2::{Digest, Sha256};
use splitcard_server::ots::format::{
    Attestation, BITCOIN_TAG, FileHashOp, HEADER_MAGIC, Item, MAX_CALENDAR_RESPONSE_BYTES,
    MAX_DEPTH, MAX_FILE_BYTES, Op, OtsError, Timestamp, parse_file, parse_timestamp,
    serialize_file,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ots");

const EXAMPLES: [&str; 14] = [
    "bad-stamp.txt.ots",
    "bitcoin.pdf.ots",
    "different-blockchains.txt.ots",
    "empty.ots",
    "gdp2q25-2nd.pdf.ots",
    "hello-world.txt.ots",
    "incomplete.txt.ots",
    "known-and-unknown-notary.txt.ots",
    "merkle1.txt.ots",
    "merkle2.txt.ots",
    "merkle3.txt.ots",
    "two-calendars.txt.ots",
    "unknown-notary.txt.ots",
    "sha1-a-or-b.ots",
];

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture exists")
}

#[test]
fn every_official_example_parses_and_round_trips() {
    for name in EXAMPLES {
        let bytes = fixture(name);
        let file = parse_file(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(serialize_file(&file), bytes, "{name}");
    }
}

#[test]
fn file_digest_is_the_hash_of_the_stamped_file() {
    for name in ["incomplete.txt", "two-calendars.txt", "hello-world.txt"] {
        let file = parse_file(&fixture(&format!("{name}.ots"))).expect("parses");
        assert_eq!(file.file_hash_op, FileHashOp::Sha256);
        assert_eq!(
            file.digest,
            Sha256::digest(fixture(name)).to_vec(),
            "{name}"
        );
    }
}

#[test]
fn pending_commitments_match_the_reference_client() {
    let cases: [(&str, &[(&str, &str)]); 2] = [
        (
            "incomplete.txt.ots",
            &[(
                "https://alice.btc.calendar.opentimestamps.org",
                "57cfa5c46716df9bd9e83595bce439c58108d8fcc1678f30d4c6731c3f1fa6c79ed712c66fb1ac8d4e4eb0e7",
            )],
        ),
        (
            "two-calendars.txt.ots",
            &[
                (
                    "https://alice.btc.calendar.opentimestamps.org",
                    "57d46515fdd8c2334c77b1f204338bb2178d73e523988d7dcda13259d3a099f31623755deadd66b1688d5574",
                ),
                (
                    "https://bob.btc.calendar.opentimestamps.org",
                    "57d4651676e9ac3b3b57c1584828ea5162979c96483a03c87c69a1b8614d1cbaf387e0f5647b90ea1b270a97",
                ),
            ],
        ),
    ];
    for (name, expected) in cases {
        let file = parse_file(&fixture(name)).expect("parses");
        let found: Vec<(String, String)> = file
            .timestamp
            .pending_attestations(&file.digest)
            .expect("computable")
            .into_iter()
            .map(|pending| (pending.uri, hex::encode(pending.commitment)))
            .collect();
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(uri, commitment)| ((*uri).to_owned(), (*commitment).to_owned()))
            .collect();
        assert_eq!(found, expected, "{name}");
    }
}

#[test]
fn commitment_follows_a_known_path() {
    // incomplete.txt.ots: append, sha256, append, sha256, prepend, append.
    let file = parse_file(&fixture("incomplete.txt.ots")).expect("parses");
    let mut msg = file.digest.clone();
    let mut node = &file.timestamp;
    let mut ops = Vec::new();
    while let Some(Item::Op(op, child)) = node.items.first() {
        msg = op.apply(&msg).expect("within limits").expect("computable");
        ops.push(op.clone());
        node = child;
    }
    assert!(matches!(
        ops.as_slice(),
        [
            Op::Append(_),
            Op::Sha256,
            Op::Append(_),
            Op::Sha256,
            Op::Prepend(_),
            Op::Append(_)
        ]
    ));
    let pending = file
        .timestamp
        .pending_attestations(&file.digest)
        .expect("computable");
    assert_eq!(pending[0].commitment, msg);
    assert_eq!(pending[0].path, vec![0; 6]);
}

#[test]
fn bitcoin_attestations_are_found() {
    for (name, height) in [
        ("hello-world.txt.ots", 358_391),
        ("bitcoin.pdf.ots", 465_751),
        ("gdp2q25-2nd.pdf.ots", 912_095),
        ("different-blockchains.txt.ots", 455_605),
    ] {
        let file = parse_file(&fixture(name)).expect("parses");
        assert_eq!(file.timestamp.bitcoin_heights(), vec![height], "{name}");
    }
    let incomplete = parse_file(&fixture("incomplete.txt.ots")).expect("parses");
    assert!(incomplete.timestamp.bitcoin_heights().is_empty());
}

fn attestations(timestamp: &Timestamp) -> Vec<Attestation> {
    let mut found = Vec::new();
    for item in &timestamp.items {
        match item {
            Item::Attestation(attestation) => found.push(attestation.clone()),
            Item::Op(_, child) => found.extend(attestations(child)),
        }
    }
    found
}

#[test]
fn unknown_attestations_are_preserved_not_rejected() {
    let bytes = fixture("unknown-notary.txt.ots");
    let file = parse_file(&bytes).expect("parses");
    assert_eq!(
        attestations(&file.timestamp),
        vec![Attestation::Unknown {
            tag: [1, 2, 3, 4, 5, 6, 7, 8],
            payload: vec![b'x'; 46],
        }]
    );
    assert_eq!(serialize_file(&file), bytes);
    // Ethereum and other attestations this crate does not know are kept too.
    let file = parse_file(&fixture("different-blockchains.txt.ots")).expect("parses");
    assert!(
        attestations(&file.timestamp)
            .iter()
            .any(|attestation| matches!(attestation, Attestation::Unknown { .. }))
    );
}

#[test]
fn official_invalid_examples_are_rejected() {
    assert!(matches!(
        parse_file(&fixture("invalid/bad-major-version.ots")),
        Err(OtsError::UnsupportedVersion { .. })
    ));
    assert_eq!(
        parse_file(&fixture("invalid/invalid-file-digest-type.ots")),
        Err(OtsError::UnknownFileHashOp { tag: 0xf3 })
    );
    assert_eq!(
        parse_file(&fixture("invalid/exceeds-max-msg-length.ots")),
        Err(OtsError::MessageLength)
    );
}

#[test]
fn every_truncation_is_rejected() {
    let bytes = fixture("hello-world.txt.ots");
    for len in 0..bytes.len() {
        assert!(parse_file(&bytes[..len]).is_err(), "prefix of {len} bytes");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(parse_file(&trailing), Err(OtsError::TrailingBytes));
}

#[test]
fn oversized_input_is_rejected() {
    let mut big = HEADER_MAGIC.to_vec();
    big.resize(MAX_FILE_BYTES + 1, 0);
    assert_eq!(
        parse_file(&big),
        Err(OtsError::TooLarge {
            limit: MAX_FILE_BYTES
        })
    );
    let response = vec![0_u8; MAX_CALENDAR_RESPONSE_BYTES + 1];
    assert_eq!(
        parse_timestamp(&response, 1),
        Err(OtsError::TooLarge {
            limit: MAX_CALENDAR_RESPONSE_BYTES
        })
    );
    // A length prefix larger than the payload limit is refused before reading.
    let mut huge_payload = vec![0x00];
    huge_payload.extend_from_slice(&BITCOIN_TAG);
    huge_payload.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0x0f]);
    assert!(matches!(
        parse_timestamp(&huge_payload, 1),
        Err(OtsError::BadLength { .. })
    ));
}

/// `depth` nested sha256 ops ending in a Bitcoin attestation, as bytes.
fn chain(depth: usize) -> Vec<u8> {
    let mut bytes = vec![0x08; depth];
    bytes.push(0x00);
    bytes.extend_from_slice(&BITCOIN_TAG);
    bytes.extend_from_slice(&[0x01, 0x01]);
    bytes
}

#[test]
fn excessive_depth_is_rejected_without_overflowing_the_stack() {
    let deepest = parse_timestamp(&chain(MAX_DEPTH - 1), 1).expect("at the limit");
    assert_eq!(deepest.depth(), MAX_DEPTH);
    assert_eq!(
        parse_timestamp(&chain(MAX_DEPTH), 1),
        Err(OtsError::TooDeep)
    );
    assert_eq!(
        parse_timestamp(&chain(1_000_000), 1).err(),
        Some(OtsError::TooLarge {
            limit: MAX_CALENDAR_RESPONSE_BYTES
        })
    );
    assert_eq!(parse_timestamp(&chain(5_000), 1), Err(OtsError::TooDeep));
    // A response merged below a node gets that node's depth counted.
    assert_eq!(
        parse_timestamp(&chain(10), MAX_DEPTH - 5),
        Err(OtsError::TooDeep)
    );
}

#[test]
fn non_minimal_varints_are_rejected() {
    // Bitcoin height 1 encoded as 0x81 0x00 instead of 0x01.
    let mut bytes = vec![0x00];
    bytes.extend_from_slice(&BITCOIN_TAG);
    bytes.extend_from_slice(&[0x02, 0x81, 0x00]);
    assert_eq!(parse_timestamp(&bytes, 1), Err(OtsError::InvalidVarUint));
}

#[test]
fn merging_an_upgrade_keeps_every_other_branch() {
    let bytes = fixture("two-calendars.txt.ots");
    let mut file = parse_file(&bytes).expect("parses");
    let pending = file
        .timestamp
        .pending_attestations(&file.digest)
        .expect("computable");
    let alice = &pending[0];

    // What a calendar returns once the commitment is in a block.
    let upgrade = Timestamp::new(vec![Item::Op(
        Op::Sha256,
        Timestamp::new(vec![Item::Op(
            Op::Append(vec![0xaa; 32]),
            Timestamp::new(vec![Item::Op(
                Op::Sha256,
                Timestamp::new(vec![Item::Attestation(Attestation::Bitcoin {
                    height: 912_345,
                })]),
            )]),
        )]),
    )]);
    file.timestamp
        .merge_at(&alice.path, upgrade.clone())
        .expect("merges");

    let merged = serialize_file(&file);
    let reparsed = parse_file(&merged).expect("merged proof parses");
    assert_eq!(reparsed.timestamp.bitcoin_heights(), vec![912_345]);
    // Both pending attestations survive, with the same commitments.
    let after = reparsed
        .timestamp
        .pending_attestations(&reparsed.digest)
        .expect("computable");
    assert_eq!(after, pending);
    // The original bytes are a prefix-compatible subset: merging again is a no-op.
    let mut again = reparsed.clone();
    again
        .timestamp
        .merge_at(&alice.path, upgrade)
        .expect("merges");
    assert_eq!(serialize_file(&again), merged);
}

#[test]
fn merge_at_rejects_bad_paths_and_depth() {
    let mut file = parse_file(&fixture("incomplete.txt.ots")).expect("parses");
    let leaf = Timestamp::new(vec![Item::Attestation(Attestation::Bitcoin { height: 1 })]);
    assert_eq!(
        file.timestamp.merge_at(&[5], leaf.clone()),
        Err(OtsError::PathNotFound)
    );
    assert_eq!(
        file.timestamp.merge_at(&[0], Timestamp::default()),
        Err(OtsError::Empty)
    );
    let deep = parse_timestamp(&chain(MAX_DEPTH - 1), 1).expect("parses");
    assert_eq!(file.timestamp.merge_at(&[0], deep), Err(OtsError::TooDeep));
}

#[test]
fn ops_compute_like_the_reference() {
    assert_eq!(
        Op::Sha256
            .apply(b"")
            .expect("ok")
            .map(hex::encode)
            .as_deref(),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(Op::Append(vec![2]).apply(&[1]), Ok(Some(vec![1, 2])));
    assert_eq!(Op::Prepend(vec![2]).apply(&[1]), Ok(Some(vec![2, 1])));
    assert_eq!(Op::Reverse.apply(&[1, 2]), Ok(Some(vec![2, 1])));
    assert_eq!(Op::Hexlify.apply(&[0xab]), Ok(Some(b"ab".to_vec())));
    assert_eq!(Op::Ripemd160.apply(b"x"), Ok(None));
    assert_eq!(
        Op::Append(vec![0; 4096]).apply(&[1]),
        Err(OtsError::MessageLength)
    );
    assert_eq!(Op::Reverse.apply(&[]), Err(OtsError::MessageLength));
}
