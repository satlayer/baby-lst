use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, QueryRequest, Response,
    StdError, StdResult, Uint128, Validator, ValidatorResponse, WasmQuery,
};
use cw2::set_contract_version;

use crate::config::{execute_update_config, execute_update_params};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::stake::execute_stake;
use crate::staking::{
    execute_claim_rewards_and_restake, execute_unstake, execute_withdraw_unstaked,
};
use crate::state::{StakeType, State, CONFIG, TOTAL_STAKED};
use cw20::{Cw20QueryMsg, TokenInfoResponse};

const CONTRACT_NAME: &str = "lst-staking-hub";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    todo!()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::UpdateConfig {
            lst_token,
            staking_denom,
        } => execute_update_config(deps, env, info, lst_token, staking_denom),
        ExecuteMsg::Stake {} => execute_stake(deps, env, info, StakeType::LSTMint),
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::WithdrawUnstaked {} => execute_withdraw_unstaked(deps, env, info),
        ExecuteMsg::ClaimRewardsAndRestake {} => execute_claim_rewards_and_restake(deps, env, info),
        ExecuteMsg::UpdateParams { pause } => execute_update_params(deps, env, info, pause),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_json_binary(&config)
        }
        QueryMsg::TotalStaked {} => {
            let total = TOTAL_STAKED.load(deps.storage)?;
            to_json_binary(&total)
        }
        QueryMsg::ExchangeRate {} => {
            todo!()
        }
    }
}

// Check if slashing has happened and return the slashed amount
pub fn check_slashing(deps: &mut DepsMut, env: Env) -> StdResult<State> {
    todo!()
}

pub(crate) fn query_total_lst_token_issued(deps: Deps) -> StdResult<Uint128> {
    let token_address = deps.api.addr_humanize(
        &CONFIG
            .load(deps.storage)?
            .lst_token
            .ok_or_else(|| StdError::generic_err("LST token address is not set"))?,
    )?;
    let token_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token_address.to_string(),
            msg: to_json_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;

    Ok(token_info.total_supply)
}

pub(crate) fn calculate_delegations(
    amount: Uint128,
    validators: &[ValidatorResponse],
) -> (Uint128, Vec<Uint128>) {
    todo!()
}
