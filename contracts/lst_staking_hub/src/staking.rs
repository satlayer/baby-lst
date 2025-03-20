use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult, Uint128};

pub fn execute_withdraw_unstaked(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> StdResult<Response> {
    todo!()
}

pub fn execute_claim_rewards_and_restake(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
) -> StdResult<Response> {
    todo!()
}
