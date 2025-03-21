use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use lst_common::types::LstResult;

pub fn execute_withdraw_unstaked(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> LstResult<Response> {
    todo!()
}

pub fn execute_claim_rewards_and_restake(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> LstResult<Response> {
    todo!()
}
