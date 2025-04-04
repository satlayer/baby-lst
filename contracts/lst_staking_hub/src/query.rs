use cosmwasm_std::{Addr, Deps, Env, Storage, Uint128};
use cw_storage_plus::Bound;
use lst_common::{
    hub::{
        AllHistoryResponse, Config, ConfigResponse, CurrentBatch, Parameters, State,
        UnstakeHistory, UnstakeRequestsResponses, UserUnstakeRequestsResponse,
        WithdrawableUnstakedResponse,
    },
    to_checked_address,
    types::LstResult,
};

use crate::{
    contract::query_actual_state,
    math::decimal_multiplication,
    state::{
        read_unstake_history, CONFIG, CURRENT_BATCH, PARAMETERS, STATE, UNSTAKE_HISTORY,
        UNSTAKE_WAIT_LIST,
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
        .map(|item| {
            let (batch_id, lst_amount) = item?;
            let history = read_unstake_history(storage, batch_id)?;

            Ok(UserUnstakeRequestsResponse {
                batch_id,
                lst_amount,
                withdraw_exchange_rate: history.lst_withdraw_rate,
                applied_exchange_rate: history.lst_applied_exchange_rate,
                time: history.time,
                released: history.released,
            })
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
