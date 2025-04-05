use std::ops::Sub;

use cosmwasm_std::Uint128;

use crate::validator::ValidatorResponse;
use crate::{ContractError, ValidatorError, types::LstResult};

pub fn calculate_delegations(
    mut amt_to_delegate: Uint128,
    validators: &[ValidatorResponse],
) -> LstResult<Vec<Uint128>> {
    if validators.is_empty() {
        return Err(ValidatorError::EmptyValidatorSet.into());
    }

    let total_delegated: u128 = validators
        .iter()
        .map(|val| val.total_delegated.u128())
        .sum();

    let total_coins_to_distribute = Uint128::from(total_delegated) + amt_to_delegate;
    let (coins_per_val, remaining_coins) =
        distribute_coins(total_coins_to_distribute, validators.len());

    let target_delegations = target_coins_per_validator(coins_per_val, remaining_coins, validators);

    let mut delegations = vec![Uint128::zero(); validators.len()];

    for (index, (validator, target_delegation)) in
        validators.iter().zip(target_delegations).enumerate()
    {
        let val_current_delegation = validator.total_delegated;

        if target_delegation < val_current_delegation {
            continue;
        }

        let mut to_delegate = target_delegation.sub(val_current_delegation);

        if to_delegate > amt_to_delegate {
            to_delegate = amt_to_delegate;
        }

        delegations[index] = to_delegate;
        amt_to_delegate = amt_to_delegate
            .checked_sub(to_delegate)
            .map_err(|e| ContractError::Overflow(e.to_string()))?;

        if amt_to_delegate.is_zero() {
            break;
        }
    }

    // check if the amt to delegate is completly delegated
    // this is impossible unless the distribution algo is changed
    if amt_to_delegate.is_zero() {
        Ok(delegations)
    } else {
        Err(ValidatorError::DistributionFailed.into())
    }
}

pub fn calculate_undelegations(
    mut amt_to_undelegate: Uint128,
    mut validators: Vec<ValidatorResponse>,
) -> LstResult<Vec<Uint128>> {
    if validators.is_empty() {
        return Err(ValidatorError::EmptyValidatorSet.into());
    }

    let mut total_delegated = validators.iter().map(|val| val.total_delegated).sum();

    if amt_to_undelegate > total_delegated {
        return Err(ValidatorError::ExceedUndelegation.into());
    }

    let mut undelegations = vec![Uint128::zero(); validators.len()];

    while !amt_to_undelegate.is_zero() {
        let total_delegations_after_undelegation =
            total_delegated
                .checked_sub(amt_to_undelegate)
                .map_err(|e| ContractError::Overflow(e.to_string()))?;

        let (coins_per_val, remaining_coins) =
            distribute_coins(total_delegations_after_undelegation, validators.len());

        let target_undelegations =
            target_coins_per_validator(coins_per_val, remaining_coins, &validators);

        for (index, (validator, target_undelegation)) in
            validators.iter_mut().zip(target_undelegations).enumerate()
        {
            let mut to_undelegate = validator
                .total_delegated
                .checked_sub(target_undelegation.min(validator.total_delegated))
                .map_err(|e| ContractError::Overflow(e.to_string()))?;

            if to_undelegate > amt_to_undelegate {
                to_undelegate = amt_to_undelegate;
            }

            undelegations[index] = undelegations[index]
                .checked_add(to_undelegate)
                .map_err(|e| ContractError::Overflow(e.to_string()))?;
            amt_to_undelegate = amt_to_undelegate
                .checked_sub(to_undelegate)
                .map_err(|e| ContractError::Overflow(e.to_string()))?;
            total_delegated = total_delegated
                .checked_sub(to_undelegate)
                .map_err(|e| ContractError::Overflow(e.to_string()))?;
            validator.total_delegated = validator
                .total_delegated
                .checked_sub(to_undelegate)
                .map_err(|e| ContractError::Overflow(e.to_string()))?;

            if amt_to_undelegate.is_zero() {
                break;
            }
        }
    }

    Ok(undelegations)
}

// Splits coin evenly across validator after delegation/undelegation
// computes leftover coins after even spliting
fn distribute_coins(coins_to_distribute: Uint128, validators: usize) -> (u128, u128) {
    let val_count = validators as u128;
    (
        coins_to_distribute.u128() / val_count,
        coins_to_distribute.u128() % val_count,
    )
}

// computes actual delegation/redelation amt for each validator
fn target_coins_per_validator(
    coins_per_val: u128,
    remaining_coins: u128,
    validators: &[ValidatorResponse],
) -> Vec<Uint128> {
    validators
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let extra_coin = if (index + 1) as u128 <= remaining_coins {
                1u128
            } else {
                0u128
            };
            Uint128::from(coins_per_val + extra_coin)
        })
        .collect()
}
