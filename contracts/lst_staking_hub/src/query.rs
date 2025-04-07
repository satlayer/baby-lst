use cosmwasm_std::{Addr, Deps, Env, Storage, Uint128};
use cw_storage_plus::Bound;
use lst_common::{
    hub::{
        AllHistoryResponse, Config, ConfigResponse, CurrentBatch, Parameters, PendingDelegation,
        State, UnstakeHistory, UnstakeRequestsResponses, UserUnstakeRequestsResponse,
        WithdrawableUnstakedResponse,
    },
    to_checked_address,
    types::LstResult,
    ContractError,
};

use crate::{
    contract::query_actual_state,
    math::decimal_multiplication,
    state::{
        read_unstake_history, CONFIG, CURRENT_BATCH, PARAMETERS, PENDING_DELEGATION, STATE,
        UNSTAKE_HISTORY, UNSTAKE_WAIT_LIST,
    },
};

pub fn query_config(deps: Deps) -> LstResult<ConfigResponse> {
    let Config {
        owner,
        reward_dispatcher_contract,
        validators_registry_contract,
        lst_token,
    } = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        owner: owner.to_string(),
        reward_dispatcher_contract: reward_dispatcher_contract.map(|addr| addr.to_string()),
        validators_registry_contract: validators_registry_contract.map(|addr| addr.to_string()),
        lst_token: lst_token.map(|addr| addr.to_string()),
    })
}

pub fn query_pending_delegation(deps: Deps, env: &Env) -> LstResult<PendingDelegation> {
    let mut pending_delegation = PENDING_DELEGATION.load(deps.storage)?;

    let current_block_height = env.block.height;
    let passed_blocks = current_block_height - pending_delegation.staking_epoch_start_block_height;
    if passed_blocks >= pending_delegation.staking_epoch_length_blocks {
        pending_delegation.pending_staking_amount = Uint128::zero();
        pending_delegation.pending_unstaking_amount = Uint128::zero();
        let epochs_passed = passed_blocks / pending_delegation.staking_epoch_length_blocks;
        pending_delegation.staking_epoch_start_block_height +=
            epochs_passed * pending_delegation.staking_epoch_length_blocks;
    }
    Ok(pending_delegation)
}

pub fn query_state(deps: Deps, env: &Env) -> LstResult<State> {
    let mut state = STATE.load(deps.storage)?;
    query_actual_state(deps, env, &mut state)?;
    Ok(state)
}

pub fn query_current_batch(deps: Deps) -> LstResult<CurrentBatch> {
    Ok(CURRENT_BATCH.load(deps.storage)?)
}

pub fn query_parameters(deps: Deps) -> LstResult<Parameters> {
    Ok(PARAMETERS.load(deps.storage)?)
}

// This method gives an estimate of the amount that can be withdrawn.
// This is not accurate if there is delay in fast unbonding
// For accurate amount, query unstake requests and check if released is true
pub fn query_withdrawable_unstaked(
    deps: Deps,
    env: Env,
    address: String,
) -> LstResult<WithdrawableUnstakedResponse> {
    let params = PARAMETERS.load(deps.storage)?;
    let unstake_cutoff_time = env.block.time.seconds() - params.unstaking_period;
    let checked_addr = to_checked_address(deps, &address)?;

    Ok(WithdrawableUnstakedResponse {
        withdrawable: query_get_finished_amount(deps.storage, checked_addr, unstake_cutoff_time)?,
    })
}

pub fn query_unstake_requests(deps: Deps, address: String) -> LstResult<UnstakeRequestsResponses> {
    let checked_addr = to_checked_address(deps, &address)?;
    Ok(UnstakeRequestsResponses {
        address: address.clone(),
        requests: get_unstake_requests(deps.storage, checked_addr, None, None)?,
    })
}

pub fn query_unstake_requests_limit(
    deps: Deps,
    address: String,
    start_from: Option<u64>,
    limit: Option<u32>,
) -> LstResult<UnstakeRequestsResponses> {
    let checked_addr = to_checked_address(deps, &address)?;
    Ok(UnstakeRequestsResponses {
        address: address.clone(),
        requests: get_unstake_requests(deps.storage, checked_addr, start_from, limit)?,
    })
}

pub fn query_unstake_requests_limitation(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> LstResult<AllHistoryResponse> {
    let requests = all_unstake_history(deps.storage, start, limit)?;
    Ok(AllHistoryResponse {
        history: requests
            .iter()
            .map(|request| UnstakeHistory {
                batch_id: request.batch_id,
                time: request.time,
                lst_token_amount: request.lst_token_amount,
                lst_applied_exchange_rate: request.lst_applied_exchange_rate,
                lst_withdraw_rate: request.lst_withdraw_rate,
                released: request.released,
            })
            .collect(),
    })
}

fn query_get_finished_amount(
    storage: &dyn Storage,
    address: Addr,
    unstake_cutoff_time: u64,
) -> LstResult<Uint128> {
    let wait_list = UNSTAKE_WAIT_LIST
        .prefix(address)
        .range(storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (batch_id, lst_amount) = item?;
            Ok((batch_id, lst_amount))
        })
        .collect::<LstResult<Vec<_>>>()?;

    Ok(wait_list
        .into_iter()
        .fold(Uint128::zero(), |acc, (batch_id, lst_amount)| {
            if let Ok(history) = read_unstake_history(storage, batch_id) {
                if history.time < unstake_cutoff_time {
                    return acc
                        + decimal_multiplication(lst_amount, history.lst_applied_exchange_rate);
                }
            }
            acc
        }))
}

pub fn get_unstake_requests(
    storage: &dyn Storage,
    address: Addr,
    start_from: Option<u64>,
    limit: Option<u32>,
) -> LstResult<Vec<UserUnstakeRequestsResponse>> {
    UNSTAKE_WAIT_LIST
        .prefix(address)
        .range(
            storage,
            start_from.map(Bound::inclusive),
            None,
            cosmwasm_std::Order::Ascending,
        )
        .take(limit.unwrap_or(u32::MAX) as usize)
        .filter_map(|item| {
            let (batch_id, lst_amount) = match item {
                Ok(item) => item,
                Err(e) => return Some(Err(ContractError::Std(e))),
            };

            if let Ok(history) = read_unstake_history(storage, batch_id) {
                return Some(Ok(UserUnstakeRequestsResponse {
                    batch_id,
                    lst_amount,
                    withdraw_exchange_rate: history.lst_withdraw_rate,
                    applied_exchange_rate: history.lst_applied_exchange_rate,
                    time: history.time,
                    released: history.released,
                }));
            }
            None
        })
        .collect()
}

fn all_unstake_history(
    storage: &dyn Storage,
    start: Option<u64>,
    limit: Option<u32>,
) -> LstResult<Vec<UnstakeHistory>> {
    UNSTAKE_HISTORY
        .range(
            storage,
            start.map(Bound::inclusive),
            None,
            cosmwasm_std::Order::Ascending,
        )
        .take(limit.unwrap_or(u32::MAX) as usize)
        .map(|item| Ok(item?.1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Addr, Decimal, Uint128};
    use lst_common::hub::UnstakeHistory;

    #[test]
    fn test_get_unstake_requests_success() {
        let mut deps = mock_dependencies();
        let address = Addr::unchecked("user1");
        let batch_id = 1u64;
        let lst_amount = Uint128::from(100u128);

        // Store wait list entry
        UNSTAKE_WAIT_LIST
            .save(
                deps.as_mut().storage,
                (address.clone(), batch_id),
                &lst_amount,
            )
            .unwrap();

        // Store unstake history
        let history = UnstakeHistory {
            batch_id,
            time: 1000,
            lst_token_amount: lst_amount,
            lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
            lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
            released: false,
        };
        UNSTAKE_HISTORY
            .save(deps.as_mut().storage, batch_id, &history)
            .unwrap();

        let result = get_unstake_requests(deps.as_ref().storage, address, None, None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].batch_id, batch_id);
        assert_eq!(result[0].lst_amount, lst_amount);
        assert_eq!(result[0].withdraw_exchange_rate, history.lst_withdraw_rate);
        assert_eq!(
            result[0].applied_exchange_rate,
            history.lst_applied_exchange_rate
        );
        assert_eq!(result[0].time, history.time);
        assert_eq!(result[0].released, history.released);
    }

    #[test]
    fn test_get_unstake_requests_skip_missing_history() {
        let mut deps = mock_dependencies();
        let address = Addr::unchecked("user1");

        // Store two wait list entries
        let batch_id1 = 1u64;
        let batch_id2 = 2u64;
        let lst_amount = Uint128::from(100u128);

        UNSTAKE_WAIT_LIST
            .save(
                deps.as_mut().storage,
                (address.clone(), batch_id1),
                &lst_amount,
            )
            .unwrap();
        UNSTAKE_WAIT_LIST
            .save(
                deps.as_mut().storage,
                (address.clone(), batch_id2),
                &lst_amount,
            )
            .unwrap();

        // Only store history for batch_id1
        let history = UnstakeHistory {
            batch_id: batch_id1,
            time: 1000,
            lst_token_amount: lst_amount,
            lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
            lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
            released: false,
        };
        UNSTAKE_HISTORY
            .save(deps.as_mut().storage, batch_id1, &history)
            .unwrap();

        let result = get_unstake_requests(deps.as_ref().storage, address, None, None).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].batch_id, batch_id1);
    }

    #[test]
    fn test_get_unstake_requests_with_limit() {
        let mut deps = mock_dependencies();
        let address = Addr::unchecked("user1");

        // Store multiple wait list entries
        for i in 1..=5 {
            let batch_id = i as u64;
            let lst_amount = Uint128::from(100u128);

            UNSTAKE_WAIT_LIST
                .save(
                    deps.as_mut().storage,
                    (address.clone(), batch_id),
                    &lst_amount,
                )
                .unwrap();

            let history = UnstakeHistory {
                batch_id,
                time: 1000,
                lst_token_amount: lst_amount,
                lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
                lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
                released: false,
            };
            UNSTAKE_HISTORY
                .save(deps.as_mut().storage, batch_id, &history)
                .unwrap();
        }

        // Test with limit of 3
        let result = get_unstake_requests(deps.as_ref().storage, address, None, Some(3)).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_get_unstake_requests_with_start_from() {
        let mut deps = mock_dependencies();
        let address = Addr::unchecked("user1");

        // Store multiple wait list entries
        for i in 1..=5 {
            let batch_id = i as u64;
            let lst_amount = Uint128::from(100u128);

            UNSTAKE_WAIT_LIST
                .save(
                    deps.as_mut().storage,
                    (address.clone(), batch_id),
                    &lst_amount,
                )
                .unwrap();

            let history = UnstakeHistory {
                batch_id,
                time: 1000,
                lst_token_amount: lst_amount,
                lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
                lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
                released: false,
            };
            UNSTAKE_HISTORY
                .save(deps.as_mut().storage, batch_id, &history)
                .unwrap();
        }

        // Test starting from batch_id 3
        let result = get_unstake_requests(deps.as_ref().storage, address, Some(3), None).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].batch_id, 3);
    }
}
