//! Exact weighted prize-split calculations.

use std::collections::BTreeMap;

use nostr::event::Event;

use crate::{
    card::{Card, Payout},
    error::{PayoutAddressError, PayoutError, SplitError},
    signature::verify_signature_event,
    validate::validate,
};

/// Splits a prize using weighted largest-remainder rounding.
///
/// ```
/// use splitcard_core::split::split;
///
/// let amounts = split(100_001, &[36, 35, 29])?;
/// assert_eq!(amounts, vec![36_001, 35_000, 29_000]);
/// # Ok::<(), splitcard_core::error::SplitError>(())
/// ```
///
/// # Errors
///
/// Returns an error when weights are absent or zero, or the prize is smaller
/// than the number of recipients.
pub fn split(prize_sats: u64, weights: &[u64]) -> Result<Vec<u64>, SplitError> {
    if weights.is_empty() {
        return Err(SplitError::NoWeights);
    }
    if prize_sats < weights.len() as u64 {
        return Err(SplitError::PrizeTooSmall {
            prize_sats,
            member_count: weights.len(),
        });
    }

    let mut total_weight = 0_u128;
    for (index, weight) in weights.iter().copied().enumerate() {
        if weight == 0 {
            return Err(SplitError::ZeroWeight { index });
        }
        total_weight += u128::from(weight);
    }

    let prize = u128::from(prize_sats);
    let mut amounts = Vec::with_capacity(weights.len());
    let mut remainders = Vec::with_capacity(weights.len());
    let mut base_total = 0_u128;

    for (index, weight) in weights.iter().copied().enumerate() {
        let numerator = prize * u128::from(weight);
        let base = numerator / total_weight;
        let remainder = numerator % total_weight;
        // `base` is at most `prize`, which is a `u64` by construction.
        #[allow(clippy::cast_possible_truncation)]
        let amount = base as u64;
        amounts.push(amount);
        remainders.push((remainder, index));
        base_total += base;
    }

    remainders.sort_unstable_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
    let left = prize - base_total;
    for (_, index) in remainders.into_iter().take(left as usize) {
        amounts[index] += 1;
    }

    Ok(amounts)
}

/// Validates a card of either version and calculates `(pubkey, sats)`
/// payouts in member order.
///
/// # Errors
///
/// Returns validation failures from the card, split failures from its weights,
/// or [`PayoutError::ShareRoundsToZero`] for the first member who would get 0 sats.
pub fn compute_payouts(card: &Card, prize_sats: u64) -> Result<Vec<(String, u64)>, PayoutError> {
    validate(card)?;
    let amounts = split(prize_sats, &card.weights())?;
    let payouts: Vec<(String, u64)> = card
        .pubkeys()
        .into_iter()
        .zip(amounts)
        .map(|(pubkey, amount)| (pubkey.to_owned(), amount))
        .collect();
    if let Some((index, (pubkey, _))) = payouts
        .iter()
        .enumerate()
        .find(|(_, (_, amount))| *amount == 0)
    {
        return Err(PayoutError::ShareRoundsToZero {
            index,
            pubkey: pubkey.clone(),
        });
    }
    Ok(payouts)
}

/// Pairs each member with their payout address, as `(pubkey, address)` in
/// member order.
///
/// A version 1 card names the addresses itself. A version 2 member's address
/// comes from their verified signature in `events`: the first valid event per
/// member counts, as in [`card_status`](crate::status::card_status). Invalid
/// events are ignored.
///
/// # Errors
///
/// Returns the card's validation error, or
/// [`PayoutAddressError::MissingSignature`] for the first version 2 member
/// without a valid signature.
pub fn payout_addresses(
    card: &Card,
    events: &[Event],
) -> Result<Vec<(String, String)>, PayoutAddressError> {
    validate(card)?;
    let card = match card {
        Card::V1(card) => {
            return Ok(card
                .members
                .iter()
                .map(|member| {
                    let Payout::Lightning { address } = &member.payout;
                    (member.pubkey.clone(), address.clone())
                })
                .collect());
        }
        Card::V2(_) => card,
    };
    let mut signed = BTreeMap::new();
    for event in events {
        if let Ok(signer) = verify_signature_event(card, event)
            && let Some(address) = signer.lightning_address
        {
            signed.entry(signer.pubkey).or_insert(address);
        }
    }
    card.pubkeys()
        .into_iter()
        .enumerate()
        .map(|(index, pubkey)| {
            signed
                .remove(pubkey)
                .map(|address| (pubkey.to_owned(), address))
                .ok_or_else(|| PayoutAddressError::MissingSignature {
                    index,
                    pubkey: pubkey.to_owned(),
                })
        })
        .collect()
}

/// Returns exact, display-ready basis points for positive weights.
///
/// # Errors
///
/// Returns the same errors as [`split`].
pub fn display_basis_points(weights: &[u64]) -> Result<Vec<u32>, SplitError> {
    split(10_000, weights).map(|amounts| {
        amounts
            .into_iter()
            .map(|amount| {
                // Each amount is at most the fixed 10,000-sat display prize.
                #[allow(clippy::cast_possible_truncation)]
                let basis_points = amount as u32;
                basis_points
            })
            .collect()
    })
}
