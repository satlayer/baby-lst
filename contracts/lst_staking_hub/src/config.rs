use cosmwasm_std::{DepsMut, Env, MessageInfo, Response};

use lst_common::types::LstResult;

pub fn execute_update_config(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _lst_token: String,
    _staking_denom: String,
) -> LstResult<Response> {
    todo!()
}

pub fn execute_update_params(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _pause: bool,
) -> LstResult<Response> {
    todo!()
}
