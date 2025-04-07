use std::collections::HashMap;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;

use lst_common::{
    calculate_delegations,
    hub::ExecuteMsg::RedelegateProxy,
    to_checked_address,
    types::{LstResult, StdCoin},
    validate_migration,
    validator::{Config, ExecuteMsg, InstantiateMsg, QueryMsg, Validator, ValidatorResponse},
    ContractError, MigrateMsg,
};

use crate::{
    helper::{convert_addr_by_prefix, fetch_validator_info, VALIDATOR_ADDR_PREFIX},
    state::{CONFIG, VALIDATOR_EXCLUDE_LIST, VALIDATOR_REGISTRY},
};

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
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

#[cfg_attr(not(feature = "library"), entry_point)]
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    validate_migration(deps.as_ref(), CONTRACT_NAME, CONTRACT_VERSION)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default().add_attribute("migrate", "successful"))
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

#[cfg_attr(not(feature = "library"), entry_point)]
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
        .filter_map(|item| match item {
            Ok(i) => Some(i),
            Err(_) => None,
        })
        .collect::<Vec<String>>();

    Ok(excluded_lists)
}
