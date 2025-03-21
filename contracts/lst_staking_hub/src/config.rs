use cosmwasm_std::{attr, DepsMut, Env, MessageInfo, Response};

use lst_common::{
    hub::{Config, Parameters},
    to_canoncial_addr,
    types::LstResult,
    ContractError,
};

use crate::state::{CONFIG, PARAMETERS};

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
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pause: Option<bool>,
    staking_coin_denom: Option<String>,
    epoch_length: Option<u64>,
    unstaking_period: Option<u64>,
) -> LstResult<Response> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;
    if to_canoncial_addr(deps.as_ref(), info.sender.as_str())? != owner {
        return Err(ContractError::Unauthorized {});
    }

    let params: Parameters = PARAMETERS.load(deps.storage)?;

    let new_params = Parameters {
        paused: pause.unwrap_or(params.paused),
        staking_coin_denom: staking_coin_denom.unwrap_or(params.staking_coin_denom),
        epoch_length: epoch_length.unwrap_or(params.epoch_length),
        unstaking_period: unstaking_period.unwrap_or(params.unstaking_period),
    };

    PARAMETERS.save(deps.storage, &new_params)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_params"),
        attr("sender", info.sender.to_string()),
        attr("paused", new_params.paused.to_string()),
        attr("staking_coin_denom", new_params.staking_coin_denom.clone()),
        attr("epoch_length", new_params.epoch_length.to_string()),
        attr("unstaking_period", new_params.unstaking_period.to_string()),
    ]))
}
