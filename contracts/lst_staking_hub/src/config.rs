use cosmwasm_std::{
    attr, Addr, CosmosMsg, Deps, DepsMut, DistributionMsg, Env, MessageInfo, Response,
};

use lst_common::{
    errors::HubError,
    hub::{Config, Parameters},
    to_checked_address,
    types::LstResult,
    ContractError,
};

use crate::constants::{MAX_EPOCH_LENGTH, MAX_UNSTAKING_PERIOD};
use crate::state::{CONFIG, PARAMETERS};

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    lst_token: Option<String>,
    validator_registry: Option<String>,
    reward_dispatcher: Option<String>,
) -> LstResult<Response> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    let mut config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(owner_addr) = owner {
        config.owner = to_checked_address(deps.as_ref(), &owner_addr)?;
    }

    if let Some(token) = lst_token {
        let new_token_addr = to_checked_address(deps.as_ref(), &token)?;
        if let Some(existing_token) = &config.lst_token {
            if existing_token != &new_token_addr {
                return Err(ContractError::Hub(HubError::LstTokenAlreadySet));
            }
        } else {
            config.lst_token = Some(new_token_addr);
        }
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
) -> LstResult<Response> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    let mut params: Parameters = PARAMETERS.load(deps.storage)?;

    // Validate periods if either is provided
    if let (Some(epoch_len), Some(unstake_period)) = (epoch_length, unstaking_period) {
        if epoch_len > MAX_EPOCH_LENGTH {
            return Err(ContractError::Hub(HubError::InvalidEpochLength));
        }
        if unstake_period > MAX_UNSTAKING_PERIOD {
            return Err(ContractError::Hub(HubError::InvalidUnstakingPeriod));
        }
        if epoch_len >= unstake_period {
            return Err(ContractError::Hub(HubError::InvalidPeriods));
        }
    } else {
        // Validate individual parameters
        if let Some(epoch_len) = epoch_length {
            if epoch_len > MAX_EPOCH_LENGTH {
                return Err(ContractError::Hub(HubError::InvalidEpochLength));
            }
            if epoch_len >= params.unstaking_period {
                return Err(ContractError::Hub(HubError::InvalidPeriods));
            }
        }
        if let Some(unstake_period) = unstaking_period {
            if unstake_period > MAX_UNSTAKING_PERIOD {
                return Err(ContractError::Hub(HubError::InvalidUnstakingPeriod));
            }
            if params.epoch_length >= unstake_period {
                return Err(ContractError::Hub(HubError::InvalidPeriods));
            }
        }
    }

    // Update parameters
    params.paused = pause.unwrap_or(params.paused);
    params.epoch_length = epoch_length.unwrap_or(params.epoch_length);
    params.unstaking_period = unstaking_period.unwrap_or(params.unstaking_period);

    PARAMETERS.save(deps.storage, &params)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_params"),
        attr("paused", params.paused.to_string()),
        attr("staking_coin_denom", params.staking_coin_denom.clone()),
        attr("epoch_length", params.epoch_length.to_string()),
        attr("unstaking_period", params.unstaking_period.to_string()),
    ]))
}

fn is_authorized_sender(deps: Deps, sender: Addr) -> LstResult<()> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;
    if sender != owner {
        return Err(ContractError::Unauthorized {});
    }
    Ok(())
}
