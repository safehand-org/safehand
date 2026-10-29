//! Exact weighted prize-split calculations.

use crate::{
    card::Card,
    error::{PayoutError, SplitError},
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

/// Validates a card and calculates `(pubkey, sats)` payouts in member order.
///
/// # Errors
///
/// Returns validation failures from the card, split failures from its weights,
/// or [`PayoutError::ShareRoundsToZero`] for the first member who would get 0 sats.
pub fn compute_payouts(card: &Card, prize_sats: u64) -> Result<Vec<(String, u64)>, PayoutError> {
    validate(card)?;
    let weights: Vec<u64> = card.members.iter().map(|member| member.weight).collect();
    let amounts = split(prize_sats, &weights)?;
    let payouts: Vec<(String, u64)> = card
        .members
        .iter()
        .zip(amounts)
        .map(|(member, amount)| (member.pubkey.clone(), amount))
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
