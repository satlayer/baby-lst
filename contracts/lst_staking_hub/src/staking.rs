use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, Uint128};

use lst_common::ContractError;

pub fn execute_stake(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _amount: Uint128,
) -> Result<Response, ContractError> {
    todo!()
}

pub fn execute_unstake(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _amount: Uint128,
) -> Result<Response, ContractError> {
    todo!()
}

pub fn execute_withdraw_unstaked(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> Result<Response, ContractError> {
    todo!()
}

pub fn execute_claim_rewards_and_restake(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> Result<Response, ContractError> {
    todo!()
}
