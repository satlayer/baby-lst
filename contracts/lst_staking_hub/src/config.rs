use cosmwasm_std::{
    attr, Addr, CosmosMsg, Deps, DepsMut, DistributionMsg, Env, MessageInfo, Response,
};

use lst_common::{
    errors::HubError,
    hub::{Config, Parameters},
    to_checked_address,
    types::{LstResult, ResponseType},
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
) -> LstResult<Response<ResponseType>> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    let mut config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg<ResponseType>> = vec![];

    if let Some(owner_addr) = owner {
        config.owner = to_checked_address(deps.as_ref(), &owner_addr)?;
    }

    if let Some(token) = lst_token {
        let new_token_addr = to_checked_address(deps.as_ref(), &token)?;
        if let Some(existing_token) = &config.lst_token {
            if existing_token != new_token_addr {
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
) -> LstResult<Response<ResponseType>> {
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

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr,
        testing::{message_info, mock_dependencies, mock_env},
        CosmosMsg, DistributionMsg, SubMsg,
    };
    use lst_common::{errors::HubError, hub::InstantiateMsg, ContractError};

    use crate::{config::execute_update_params, instantiate};

    use super::execute_update_config;

    #[test]
    fn test_execute_update_config() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };
            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // update None
        {
            let response = execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

            assert_eq!(response.messages, vec![]);

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "update_config"),
                    attr("owner", owner.to_string()),
                    attr("lst_token", "None"),
                    attr("reward_dispatcher", "None"),
                    attr("validator_registry", "None")
                ]
            );
        }

        // update all config successfully
        {
            let new_owner = deps.api.addr_make("new_owner");
            let lst_token = deps.api.addr_make("lst_token");
            let validator_registry = deps.api.addr_make("validator_registry");
            let reward_dispatcher = deps.api.addr_make("reward_dispatcher");

            let response = execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(new_owner.to_string()),
                Some(lst_token.to_string()),
                Some(validator_registry.to_string()),
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Distribution(
                    DistributionMsg::SetWithdrawAddress {
                        address: reward_dispatcher.to_string()
                    }
                ))]
            );

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "update_config"),
                    attr("owner", new_owner.to_string()),
                    attr("lst_token", lst_token.to_string()),
                    attr("reward_dispatcher", reward_dispatcher.to_string()),
                    attr("validator_registry", validator_registry.to_string())
                ]
            );
        }

        // unauthorized error
        {
            let wrong_owner = deps.api.addr_make("wrong_owner");
            let info = message_info(&wrong_owner, &[]);

            let err =
                execute_update_config(deps.as_mut(), env.clone(), info, None, None, None, None)
                    .unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }

        // LstTokenAlreadySet error
        {
            let new_owner = deps.api.addr_make("new_owner");
            let new_lst_token = deps.api.addr_make("new_lst_token");
            let info = message_info(&new_owner, &[]);

            let err = execute_update_config(
                deps.as_mut(),
                env.clone(),
                info,
                None,
                Some(new_lst_token.to_string()),
                None,
                None,
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::LstTokenAlreadySet));
        }
    }

    #[test]
    fn test_execute_update_params() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };
            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // update None
        {
            let response =
                execute_update_params(deps.as_mut(), env.clone(), info.clone(), None, None, None)
                    .unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "update_params"),
                    attr("paused", false.to_string()),
                    attr("staking_coin_denom", denom.to_string()),
                    attr("epoch_length", "7200"),
                    attr("unstaking_period", "10000")
                ]
            );
        }

        // update all config successfully
        {
            let response = execute_update_params(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(true),
                Some(1000),
                Some(8000),
            )
            .unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "update_params"),
                    attr("paused", true.to_string()),
                    attr("staking_coin_denom", denom.to_string()),
                    attr("epoch_length", "1000"),
                    attr("unstaking_period", "8000")
                ]
            );
        }

        // unauthorized error
        {
            let wrong_owner = deps.api.addr_make("wrong_owner");
            let info = message_info(&wrong_owner, &[]);

            let err = execute_update_params(deps.as_mut(), env.clone(), info, None, None, None)
                .unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }

        // InvalidEpochLength error
        {
            let err = execute_update_params(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(true),
                Some(604801),
                Some(8000),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidEpochLength));
        }

        // InvalidUnstakingPeriod error
        {
            let err = execute_update_params(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(true),
                Some(100),
                Some(2419201),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidUnstakingPeriod));
        }

        // InvalidUnstakingPeriod error
        {
            let err = execute_update_params(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(true),
                Some(1000),
                Some(100),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidPeriods));
        }
    }
}
