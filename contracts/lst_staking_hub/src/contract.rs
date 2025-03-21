use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
};
use cw2::set_contract_version;

use lst_common::{
    hub::{ExecuteMsg, InstantiateMsg, QueryMsg},
    ContractError,
};

use crate::config::{execute_update_config, execute_update_params};
use crate::staking::{
    execute_claim_rewards_and_restake, execute_stake, execute_unstake, execute_withdraw_unstaked,
};
use crate::state::{CONFIG, TOTAL_STAKED};

const CONTRACT_NAME: &str = "crates.io:lst-staking-hub";
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
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            lst_token,
            staking_denom,
        } => execute_update_config(deps, env, info, lst_token, staking_denom),
        ExecuteMsg::Stake { amount } => execute_stake(deps, env, info, amount),
        ExecuteMsg::Unstake { amount } => execute_unstake(deps, env, info, amount),
        ExecuteMsg::WithdrawUnstaked {} => execute_withdraw_unstaked(deps, env, info),
        ExecuteMsg::ClaimRewardsAndRestake {} => execute_claim_rewards_and_restake(deps, env, info),
        ExecuteMsg::UpdateParams { pause } => execute_update_params(deps, env, info, pause),
        ExecuteMsg::CheckSlashing {} => todo!(),
        ExecuteMsg::RedelegateProxy {
            src_validator: _,
            redelegations: _,
        } => todo!(),
        ExecuteMsg::StakeRewards {} => todo!(),
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
        QueryMsg::Parameters {} => todo!(),
    }
}
