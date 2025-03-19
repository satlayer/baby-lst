use std::ops::Sub;

use cosmwasm_std::{StdError, Uint128};

use crate::{msg::ValidatorResponse, types::LstResult, ContractError, ValidatorError};

pub fn calculate_delegations(
    mut amt_to_delegate: Uint128,
    validators: &[ValidatorResponse],
) -> LstResult<Vec<Uint128>> {
    if validators.is_empty() {
        return Err(ValidatorError::EmptyValidatorSet.into());
    }

    let val_count = validators.len() as u128;

    let total_delegated: u128 = validators
        .iter()
        .map(|val| val.total_delegated.u128())
        .sum();

    let total_coins_to_distribute = Uint128::from(total_delegated) + amt_to_delegate;
    let coins_per_val = total_coins_to_distribute.u128() / val_count;
    let remaining_coins = total_coins_to_distribute.u128() % val_count;

    let mut delegations = vec![Uint128::zero(); validators.len()];

    for (index, validator) in validators.iter().enumerate() {
        let extra_coin = if (index + 1) as u128 <= remaining_coins {
            1u128
        } else {
            0u128
        };

        let val_current_delegation = validator.total_delegated;

        if coins_per_val + extra_coin < val_current_delegation.u128() {
            continue;
        }

        let mut to_delegate = Uint128::from(coins_per_val + extra_coin).sub(val_current_delegation);

        if to_delegate > amt_to_delegate {
            to_delegate = amt_to_delegate;
        }

        delegations[index] = to_delegate;
        amt_to_delegate = amt_to_delegate
            .checked_sub(to_delegate)
            .map_err(|e| ContractError::Std(StdError::overflow(e)))?;

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
