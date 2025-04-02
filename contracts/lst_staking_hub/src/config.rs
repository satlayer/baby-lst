use cosmwasm_std::{
    attr, Addr, CosmosMsg, Deps, DepsMut, DistributionMsg, Env, MessageInfo, Response,
};

use lst_common::{
    hub::{Config, Parameters},
    to_checked_address,
    types::{LstResult, ResponseType},
    ContractError,
};

use crate::state::{CONFIG, PARAMETERS};

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    lst_token: Option<String>,
    validator_registry: Option<String>,
    reward_dispatcher: Option<String>,
) -> LstResult<Response<ResponseType>> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    let mut config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg<ResponseType>> = vec![];

    if let Some(owner_addr) = owner {
        config.owner = to_checked_address(deps.as_ref(), &owner_addr)?;
    }

    if let Some(token) = lst_token {
        config.lst_token = Some(to_checked_address(deps.as_ref(), &token)?);
    }

    if let Some(registry) = validator_registry {
        config.validators_registry_contract = Some(to_checked_address(deps.as_ref(), &registry)?);
    }

    if let Some(dispatcher) = reward_dispatcher {
        let checked_dispatcher = to_checked_address(deps.as_ref(), &dispatcher)?;
        config.reward_dispatcher_contract = Some(checked_dispatcher.clone());

        messages.push(CosmosMsg::Distribution(
            DistributionMsg::SetWithdrawAddress {
                address: checked_dispatcher.to_string(),
            },
        ));
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "update_config")
        .add_attributes(vec![
            attr("owner", config.owner.to_string()),
            attr(
                "lst_token",
                config
                    .lst_token
                    .map_or(String::from("None"), |a| a.to_string()),
            ),
            attr(
                "reward_dispatcher",
                config
                    .reward_dispatcher_contract
                    .map_or(String::from("None"), |a| a.to_string()),
            ),
            attr(
                "validator_registry",
                config
                    .validators_registry_contract
                    .map_or(String::from("None"), |a| a.to_string()),
            ),
        ]))
}

pub fn execute_update_params(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pause: Option<bool>,
    epoch_length: Option<u64>,
    unstaking_period: Option<u64>,
) -> LstResult<Response<ResponseType>> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    let params: Parameters = PARAMETERS.load(deps.storage)?;

    let new_params = Parameters {
        paused: pause.unwrap_or(params.paused),
        staking_coin_denom: params.staking_coin_denom,
        epoch_length: epoch_length.unwrap_or(params.epoch_length),
        unstaking_period: unstaking_period.unwrap_or(params.unstaking_period),
    };

    PARAMETERS.save(deps.storage, &new_params)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_params"),
        attr("paused", new_params.paused.to_string()),
        attr("staking_coin_denom", new_params.staking_coin_denom.clone()),
        attr("epoch_length", new_params.epoch_length.to_string()),
        attr("unstaking_period", new_params.unstaking_period.to_string()),
    ]))
}

fn is_authorized_sender(deps: Deps, sender: Addr) -> LstResult<()> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;
    if sender != owner {
        return Err(ContractError::Unauthorized {});
    }
    Ok(())
}
