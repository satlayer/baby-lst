use std::collections::HashMap;

use cosmwasm_std::{
    to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, Uint128,
    WasmMsg,
};
use cw2::set_contract_version;

use crate::{
    helper::fetch_validator_info,
    state::{CONFIG, VALIDATOR_EXCLUDE_LIST, VALIDATOR_REGISTRY},
};
use lst_common::address::{convert_addr_by_prefix, VALIDATOR_ADDR_PREFIX};
use lst_common::{
    calculate_delegations,
    hub::ExecuteMsg::RedelegateProxy,
    to_checked_address,
    types::{LstResult, StdCoin},
    validator::{Config, ExecuteMsg, InstantiateMsg, QueryMsg, Validator, ValidatorResponse},
    ContractError, MigrateMsg,
};

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> LstResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .map_err(|_| ContractError::FailedToInitContract)?;

    let hub_contract = to_checked_address(deps.as_ref(), msg.hub_contract.as_ref())?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: info.sender,
            hub_contract,
        },
    )?;

    msg.validators
        .into_iter()
        .filter_map(|validator| {
            let validator_addr =
                convert_addr_by_prefix(validator.address.as_str(), VALIDATOR_ADDR_PREFIX);
            fetch_validator_info(&deps.querier, validator_addr)
                .ok()
                .flatten()
                .map(|info| {
                    VALIDATOR_REGISTRY
                        .save(
                            deps.storage,
                            info.address.as_bytes(),
                            &Validator {
                                address: info.address.clone(),
                            },
                        )
                        .ok()
                })
        })
        .count();

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> LstResult<Response> {
    match msg {
        ExecuteMsg::AddValidator { validator } => add_validator(deps, env, info, validator),
        ExecuteMsg::RemoveValidator { address } => remove_validator(deps, info, address),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
        } => update_config(deps, env, info, owner, hub_contract),
        ExecuteMsg::ProcessRedelegations {} => process_redelegations(deps),
    }
}

/// This can only be called by the contract ADMIN, enforced by `wasmd` separate from cosmwasm.
/// See https://github.com/CosmWasm/cosmwasm/issues/926#issuecomment-851259818
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    cw2::ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

fn add_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator: Validator,
) -> LstResult<Response> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;

    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    let validator_addr = convert_addr_by_prefix(validator.address.as_str(), VALIDATOR_ADDR_PREFIX);
    let validator_info = fetch_validator_info(&deps.querier, validator_addr)?;
    if let Some(info) = validator_info {
        VALIDATOR_REGISTRY.save(deps.storage, info.address.as_bytes(), &validator)?;
        VALIDATOR_EXCLUDE_LIST.remove(deps.storage, info.address);
    }

    Ok(Response::default()
        .add_attribute("action", "add_validator")
        .add_attribute("validator", validator.address.to_string()))
}

fn remove_validator(
    deps: DepsMut,
    info: MessageInfo,
    validator_addr: String,
) -> LstResult<Response> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;

    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    let validator_operator_addr =
        convert_addr_by_prefix(validator_addr.as_str(), VALIDATOR_ADDR_PREFIX);

    VALIDATOR_REGISTRY.remove(deps.storage, validator_operator_addr.as_bytes());
    VALIDATOR_EXCLUDE_LIST.save(deps.storage, validator_operator_addr, &true)?;

    Ok(Response::new()
        .add_attribute("action", "remove_validator")
        .add_attribute("validator", validator_addr))
}

fn process_redelegations(deps: DepsMut) -> LstResult<Response> {
    if VALIDATOR_EXCLUDE_LIST.is_empty(deps.storage) {
        return Ok(Response::default());
    }

    let Config { hub_contract, .. } = CONFIG.load(deps.storage)?;

    let mut delegations = HashMap::<String, StdCoin>::new();
    deps.querier
        .query_all_delegations(&hub_contract)?
        .into_iter()
        .for_each(|delegation| {
            delegations.insert(delegation.validator, delegation.amount);
        });

    let mut active_validator_delegations: Vec<ValidatorResponse> = VALIDATOR_REGISTRY
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|data| {
            let val_address = data?.1.address;
            Ok(ValidatorResponse {
                address: val_address.clone(),
                total_delegated: delegations
                    .get(&val_address)
                    .map(|coin| coin.amount)
                    .unwrap_or(Uint128::zero()),
            })
        })
        .collect::<LstResult<Vec<_>>>()?;
    active_validator_delegations.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

    let mut messages: Vec<CosmosMsg> = vec![];

    VALIDATOR_EXCLUDE_LIST
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .for_each(|item| {
            let (validator_addr, _) = item.ok().unwrap();

            let validator_delegation = delegations.get(&validator_addr);

            if let Some(delegation) = validator_delegation {
                if delegation.amount.is_zero() {
                    return;
                }

                let coin_distribution = calculate_delegations(
                    delegation.amount,
                    active_validator_delegations.as_slice(),
                )
                .unwrap();
                let redelegations = active_validator_delegations
                    .iter()
                    .zip(coin_distribution.iter())
                    .filter(|(_, amt)| !amt.is_zero())
                    .map(|(val, amt)| {
                        (
                            val.address.clone(),
                            Coin::new(amt.u128(), delegation.denom.as_str()),
                        )
                    })
                    .collect::<Vec<_>>();

                let msg = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: hub_contract.to_string(),
                    msg: to_json_binary(&RedelegateProxy {
                        src_validator: validator_addr.clone(),
                        redelegations,
                    })
                    .unwrap(),
                    funds: vec![],
                });
                messages.push(msg);
            }
        });

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "process_redelegation"))
}

// Update validator registry contract config. owner/hub_contract
// Only owner can execute the function
fn update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
) -> LstResult<Response> {
    let config = CONFIG.load(deps.storage)?;

    let owner_addr = config.owner;

    if info.sender != owner_addr {
        return Err(ContractError::Unauthorized {});
    }
    let mut res = Response::default();

    if let Some(owner) = owner {
        let owner_raw = to_checked_address(deps.as_ref(), owner.as_str())?;
        CONFIG.update(deps.storage, |mut old_config| -> LstResult<Config> {
            old_config.owner = owner_raw;
            Ok(old_config)
        })?;
        res = res.add_attribute("owner", owner);
    }

    if let Some(hub_contract) = hub_contract {
        let hub_addr_raw = to_checked_address(deps.as_ref(), hub_contract.as_str())?;
        CONFIG.update(deps.storage, |mut old_config| -> LstResult<Config> {
            old_config.hub_contract = hub_addr_raw;
            Ok(old_config)
        })?;
        res = res.add_attribute("hub", hub_contract);
    }

    Ok(res)
}

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> LstResult<Binary> {
    match msg {
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::ValidatorsDelegation {} => Ok(to_json_binary(&query_validators(deps)?)?),
        QueryMsg::ExcludeList => Ok(to_json_binary(&query_exclude_list(deps)?)?),
    }
}

fn query_config(deps: Deps) -> LstResult<Binary> {
    Ok(to_json_binary(&CONFIG.load(deps.storage)?)?)
}

fn query_validators(deps: Deps) -> LstResult<Vec<ValidatorResponse>> {
    let Config {
        owner: _,
        hub_contract,
    } = CONFIG.load(deps.storage)?;

    let mut delegations = HashMap::<String, Uint128>::new();

    deps.querier
        .query_all_delegations(&hub_contract)?
        .into_iter()
        .for_each(|delegation| {
            delegations.insert(delegation.validator, delegation.amount.amount);
        });

    let mut responses: Vec<ValidatorResponse> = VALIDATOR_REGISTRY
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|data| {
            let val_address = data?.1.address;
            Ok(ValidatorResponse {
                address: val_address.clone(),
                total_delegated: *delegations.get(&val_address).unwrap_or(&Uint128::zero()),
            })
        })
        .collect::<LstResult<Vec<_>>>()?;
    responses.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

    Ok(responses)
}

fn query_exclude_list(deps: Deps) -> LstResult<Vec<String>> {
    let excluded_lists = VALIDATOR_EXCLUDE_LIST
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .filter_map(|item| item.ok())
        .collect::<Vec<String>>();

    Ok(excluded_lists)
}

#[cfg(test)]
mod tests {
    use crate::contract::{instantiate, query_config, query_exclude_list, remove_validator};
    use cosmwasm_std::{
        attr, coin, coins,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, CosmosMsg, Decimal, FullDelegation, SubMsg, Uint128,
        Validator as StdValidator, WasmMsg,
    };
    use lst_common::{
        address::VALIDATOR_ADDR_PREFIX,
        hub::ExecuteMsg as HubExecuteMsg,
        validator::{Config, InstantiateMsg, Validator, ValidatorResponse},
        ContractError,
    };

    use super::{add_validator, process_redelegations, query_validators, update_config};

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let hub_contract = deps.api.addr_make("hub_contract");
        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        let mock_api = deps.api.with_prefix(VALIDATOR_ADDR_PREFIX);
        let validator1 = mock_api.addr_make("validator1");
        let validator2 = mock_api.addr_make("validator2");

        let validator1_info = StdValidator::create(
            validator1.to_string(),
            Decimal::percent(5),
            Decimal::percent(10),
            Decimal::percent(1),
        );
        let validator2_info = StdValidator::create(
            validator2.to_string(),
            Decimal::percent(5),
            Decimal::percent(10),
            Decimal::percent(1),
        );

        let validator1_full_delegation = FullDelegation::create(
            hub_contract.clone(),
            validator1.to_string(),
            coin(100, denom),
            coin(120, denom),
            coins(1000, denom),
        );
        let validator2_full_delegation = FullDelegation::create(
            hub_contract.clone(),
            validator2.to_string(),
            coin(200, denom),
            coin(220, denom),
            coins(2000, denom),
        );

        let info = message_info(&owner, &[]);

        // instantiate successfully
        deps.querier.staking.update(
            denom,
            &[validator1_info, validator2_info],
            &[validator1_full_delegation, validator2_full_delegation],
        );
        let msg = InstantiateMsg {
            validators: vec![
                Validator {
                    address: validator1.to_string(),
                },
                Validator {
                    address: validator2.to_string(),
                },
            ],
            hub_contract: hub_contract.to_string(),
        };

        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
    }

    #[test]
    fn test_add_validator() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let hub_contract = deps.api.addr_make("hub_contract");
        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                validators: vec![],
                hub_contract: hub_contract.to_string(),
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        let mock_api = deps.api.with_prefix(VALIDATOR_ADDR_PREFIX);
        let validator1 = mock_api.addr_make("validator1");

        // add validator successfully
        {
            let validator1_info = StdValidator::create(
                validator1.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );

            let validator1_full_delegation = FullDelegation::create(
                hub_contract,
                validator1.to_string(),
                coin(100, denom),
                coin(120, denom),
                coins(1000, denom),
            );

            deps.querier
                .staking
                .update(denom, &[validator1_info], &[validator1_full_delegation]);

            let validator = Validator {
                address: validator1.to_string(),
            };
            let response = add_validator(deps.as_mut(), env.clone(), info, validator).unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "add_validator"),
                    attr("validator", validator1.to_string())
                ]
            );
        }

        // unauthorized error
        {
            let new_owner = deps.api.addr_make("new_owner");
            let info = message_info(&new_owner, &[]);

            let validator = Validator {
                address: validator1.to_string(),
            };
            let err = add_validator(deps.as_mut(), env.clone(), info, validator).unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }

        // query validators
        {
            let result = query_validators(deps.as_ref()).unwrap();
            assert_eq!(
                result,
                vec![ValidatorResponse {
                    total_delegated: Uint128::new(100),
                    address: validator1.to_string(),
                }]
            )
        }
    }

    #[test]
    fn test_remove_validator() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let hub_contract = deps.api.addr_make("hub_contract");
        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                validators: vec![],
                hub_contract: hub_contract.to_string(),
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        let mock_api = deps.api.with_prefix(VALIDATOR_ADDR_PREFIX);
        let validator1 = mock_api.addr_make("validator1");

        // add validator successfully
        {
            let validator1_info = StdValidator::create(
                validator1.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );

            let validator1_full_delegation = FullDelegation::create(
                env.clone().contract.address,
                validator1.to_string(),
                coin(100, denom),
                coin(120, denom),
                coins(1000, denom),
            );

            deps.querier
                .staking
                .update(denom, &[validator1_info], &[validator1_full_delegation]);

            let validator = Validator {
                address: validator1.to_string(),
            };
            add_validator(deps.as_mut(), env.clone(), info.clone(), validator).unwrap();
        }

        // remove validator successfully
        {
            let response = remove_validator(deps.as_mut(), info, validator1.to_string()).unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "remove_validator"),
                    attr("validator", validator1.to_string())
                ]
            );
        }

        // query validators
        {
            let result = query_validators(deps.as_ref()).unwrap();
            assert_eq!(result, vec![])
        }

        // query exclude list
        {
            let result = query_exclude_list(deps.as_ref()).unwrap();
            assert_eq!(result, vec![validator1.to_string()])
        }

        // unauthorized error
        {
            let new_owner = deps.api.addr_make("new_owner");
            let info = message_info(&new_owner, &[]);

            let err = remove_validator(deps.as_mut(), info, validator1.to_string()).unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }
    }

    #[test]
    fn test_process_redelegations() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let hub_contract = deps.api.addr_make("hub_contract");
        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        let mock_api = deps.api.with_prefix(VALIDATOR_ADDR_PREFIX);
        let validator1 = mock_api.addr_make("validator1");
        let validator2 = mock_api.addr_make("validator2");

        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let validator1_info = StdValidator::create(
                validator1.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );
            let validator2_info = StdValidator::create(
                validator2.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );

            let validator1_full_delegation = FullDelegation::create(
                hub_contract.clone(),
                validator1.to_string(),
                coin(100, denom),
                coin(120, denom),
                coins(1000, denom),
            );
            let validator2_full_delegation = FullDelegation::create(
                hub_contract.clone(),
                validator2.to_string(),
                coin(200, denom),
                coin(220, denom),
                coins(2000, denom),
            );

            // instantiate successfully
            deps.querier.staking.update(
                denom,
                &[validator1_info, validator2_info],
                &[validator1_full_delegation, validator2_full_delegation],
            );
            let msg = InstantiateMsg {
                validators: vec![
                    Validator {
                        address: validator1.to_string(),
                    },
                    Validator {
                        address: validator2.to_string(),
                    },
                ],
                hub_contract: hub_contract.to_string(),
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // remove validator successfully
        {
            let response = remove_validator(deps.as_mut(), info, validator1.to_string()).unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "remove_validator"),
                    attr("validator", validator1.to_string())
                ]
            );
        }

        // process redelegations successfully
        {
            let response = process_redelegations(deps.as_mut()).unwrap();

            assert_eq!(
                response.attributes,
                vec![attr("action", "process_redelegation")]
            );

            let redelegate_proxy_msg = HubExecuteMsg::RedelegateProxy {
                src_validator: validator1.to_string(),
                redelegations: vec![(validator2.to_string(), coin(100, denom))],
            };

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: hub_contract.to_string(),
                    msg: to_json_binary(&redelegate_proxy_msg).unwrap(),
                    funds: vec![],
                })),]
            )
        }
    }

    #[test]
    fn test_update_config() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let hub_contract = deps.api.addr_make("hub_contract");
        let owner = deps.api.addr_make("owner");

        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                validators: vec![],
                hub_contract: hub_contract.to_string(),
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        let new_owner = deps.api.addr_make("new_owner");
        let new_hub_contract = deps.api.addr_make("new_hub_contract");

        // update config successfully
        {
            let response = update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(new_owner.to_string()),
                Some(new_hub_contract.to_string()),
            )
            .unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("owner", new_owner.to_string()),
                    attr("hub", new_hub_contract.to_string())
                ]
            )
        }

        // query validators
        {
            let result = query_config(deps.as_ref()).unwrap();
            assert_eq!(
                result,
                to_json_binary(&Config {
                    owner: new_owner,
                    hub_contract: new_hub_contract,
                })
                .unwrap()
            )
        }

        // unauthorized error
        {
            let wrong_owner = deps.api.addr_make("wrong_owner");
            let info = message_info(&wrong_owner, &[]);

            let err =
                update_config(deps.as_mut(), env.clone(), info.clone(), None, None).unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }
    }
}
