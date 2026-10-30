//! Core errors get the same code and details over HTTP as from WASM.

use splitcard_core::{
    error::{PayoutError, SplitError, ValidationError},
    signature::SignatureError,
    supersede::SupersedeError,
};
use splitcard_server::error::ApiError;
use splitcard_wasm::error::JsError;

fn assert_same(api: &ApiError, js: &JsError) {
    assert_eq!(api.code, js.code);
    assert_eq!(api.message, js.message);
    assert_eq!(api.details, js.details, "{}", js.code);
}

fn validation_errors() -> Vec<ValidationError> {
    vec![
        ValidationError::UnsupportedVersion { version: 2 },
        ValidationError::EmptyField { field: "team" },
        ValidationError::InvalidMemberCount { count: 1 },
        ValidationError::InvalidPubkey { index: 1 },
        ValidationError::DuplicatePubkey { index: 1 },
        ValidationError::EmptyMemberName { index: 1 },
        ValidationError::ZeroWeight { index: 1 },
        ValidationError::WeightTooLarge {
            index: 1,
            weight: u64::MAX,
        },
        ValidationError::InvalidLightningAddress { index: 1 },
        ValidationError::InvalidMemberLabel { index: 1 },
        ValidationError::InvalidSupersedes,
        ValidationError::InvalidPeerReviewMaxShiftPoints,
        ValidationError::InvalidLeaveKeepPercent { keep_percent: 100 },
    ]
}

#[test]
fn validation_errors_match_wasm() {
    for error in validation_errors() {
        assert_same(&error.clone().into(), &error.into());
    }
}

#[test]
fn signature_errors_match_wasm() {
    let errors = || {
        vec![
            SignatureError::InvalidEventId,
            SignatureError::InvalidSignature,
            SignatureError::WrongKind,
            SignatureError::MissingDTag,
            SignatureError::MultipleDTags,
            SignatureError::WrongDTagHash,
            SignatureError::MissingSplitcardTag,
            SignatureError::MultipleSplitcardTags,
            SignatureError::WrongSplitcardTagHash,
            SignatureError::MissingLud16Tag,
            SignatureError::MultipleLud16Tags,
            SignatureError::InvalidLud16,
            SignatureError::UnexpectedLud16Tag,
            SignatureError::NonMember,
        ]
    };
    for (api, js) in errors().into_iter().zip(errors()) {
        assert_same(&api.into(), &js.into());
    }
}

#[test]
fn supersede_errors_match_wasm() {
    let errors = || {
        let mut errors = vec![
            SupersedeError::WrongSupersedesHash,
            SupersedeError::MissingNewMemberSignatures {
                missing: vec!["a".repeat(64)],
            },
            SupersedeError::MissingOldMemberSignatures {
                missing: vec!["b".repeat(64)],
            },
        ];
        for error in validation_errors() {
            errors.push(SupersedeError::InvalidOldCard(error.clone()));
            errors.push(SupersedeError::InvalidNewCard(error));
        }
        errors
    };
    for (api, js) in errors().into_iter().zip(errors()) {
        assert_same(&api.into(), &js.into());
    }
}

#[test]
fn split_and_payout_errors_match_wasm() {
    let splits = [
        SplitError::NoWeights,
        SplitError::ZeroWeight { index: 1 },
        SplitError::PrizeTooSmall {
            prize_sats: u64::MAX,
            member_count: 3,
        },
    ];
    for error in splits {
        assert_same(&error.clone().into(), &error.clone().into());
        let payout = PayoutError::Split(error);
        assert_same(&payout.clone().into(), &payout.into());
    }
    for payout in [
        PayoutError::Validation(ValidationError::ZeroWeight { index: 0 }),
        PayoutError::ShareRoundsToZero {
            index: 2,
            pubkey: "ab".repeat(32),
        },
    ] {
        assert_same(&payout.clone().into(), &payout.into());
    }
}
