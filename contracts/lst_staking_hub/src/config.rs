use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};

pub fn execute_update_config(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _lst_token: String,
    _staking_denom: String,
) -> StdResult<Response> {
    todo!()
}

pub fn execute_update_params(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _pause: bool,
) -> StdResult<Response> {
    todo!()
}
