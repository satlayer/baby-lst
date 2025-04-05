use std::collections::HashMap;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;

use lst_common::{
    calculate_delegations,
    errors::ValidatorError,
    hub::ExecuteMsg::{RedelegateProxy, UpdateGlobalIndex},
    to_checked_address,
    types::LstResult,
    validator::{
        Config, ExecuteMsg, InstantiateMsg, PendingRedelegation, QueryMsg, Validator,
        ValidatorResponse,
    },
    ContractError, MigrateMsg,
};

use crate::{
    helper::{convert_addr_by_prefix, fetch_validator_info, VALIDATOR_ADDR_PREFIX},
    state::{CONFIG, PENDING_REDELEGATIONS, REDELEGATION_COOLDOWN, VALIDATOR_REGISTRY},
};

const CONTRACT_NAME: &str = "crates.io:validator-registry";
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
        ExecuteMsg::RemoveValidator { address } => remove_validator(deps, env, info, address),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
        } => update_config(deps, env, info, owner, hub_contract),
        ExecuteMsg::RetryRedelegation { validator } => {
            retry_redelegation(deps, env, info, validator)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default().add_attribute("migrate", "successful"))
}

fn add_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator: Validator,
) -> LstResult<Response> {
    let Config {
        owner,
        hub_contract,
    } = CONFIG.load(deps.storage)?;

    if !(info.sender == owner || info.sender == hub_contract) {
        return Err(ContractError::Unauthorized {});
    }

    let validator_addr = convert_addr_by_prefix(validator.address.as_str(), VALIDATOR_ADDR_PREFIX);
    let validator_info = fetch_validator_info(&deps.querier, validator_addr)?;
    if let Some(info) = validator_info {
        VALIDATOR_REGISTRY.save(deps.storage, info.address.as_bytes(), &validator)?;
    }

    Ok(Response::default().add_attribute("validator", validator.address.to_string()))
}

fn remove_validator(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    validator_addr: String,
) -> LstResult<Response> {
    let Config {
        owner,
        hub_contract,
    } = CONFIG.load(deps.storage)?;

    if info.sender != owner {
        return Err(ContractError::Unauthorized {});
    }

    let validator_operator_addr =
        convert_addr_by_prefix(validator_addr.as_str(), VALIDATOR_ADDR_PREFIX);

    let validators = query_validators(deps.as_ref())?;

    if validators.is_empty() {
        return Err(ValidatorError::LastValidatorRemovalNotAllowed.into());
    }

    let delegation = match deps
        .querier
        .query_delegation(hub_contract.clone(), validator_addr.clone())?
    {
        Some(delegation) => {
            if delegation.can_redelegate.amount >= delegation.amount.amount {
                delegation
            } else {
                // Store pending redelegation if funds cannot be redelegated immediately
                let delegations =
                    calculate_delegations(delegation.amount.amount, validators.as_slice())?;
                let redelegations = validators
                    .iter()
                    .zip(delegations.iter())
                    .filter(|(_, amt)| !amt.is_zero())
                    .map(|(val, amt)| {
                        (
                            val.address.clone(),
                            Coin::new(amt.u128(), delegation.amount.denom.as_str()),
                        )
                    })
                    .collect::<Vec<_>>();

                PENDING_REDELEGATIONS.save(
                    deps.storage,
                    validator_operator_addr.as_bytes(),
                    &PendingRedelegation {
                        src_validator: validator_addr.clone(),
                        redelegations,
                        timestamp: env.block.time.seconds(),
                    },
                )?;

                // Only remove from registry after storing pending redelegation
                VALIDATOR_REGISTRY.remove(deps.storage, validator_operator_addr.as_bytes());
                return Ok(Response::new().add_attribute("status", "pending_redelegation"));
            }
        }
        None => {
            return Err(ValidatorError::ValidatorNotFound.into());
        }
    };

    let delegations = calculate_delegations(delegation.amount.amount, validators.as_slice())?;

    let redelegations = validators
        .iter()
        .zip(delegations.iter())
        .filter(|(_, amt)| !amt.is_zero())
        .map(|(val, amt)| {
            (
                val.address.clone(),
                Coin::new(amt.u128(), delegation.amount.denom.as_str()),
            )
        })
        .collect::<Vec<_>>();

    let hub_contract_string = hub_contract.to_string();

    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract_string.clone(),
        msg: to_json_binary(&RedelegateProxy {
            src_validator: validator_addr.clone(),
            redelegations,
        })?,
        funds: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract_string,
        msg: to_json_binary(&UpdateGlobalIndex {})?,
        funds: vec![],
    }));

    // Only remove from registry after successful redelegation
    VALIDATOR_REGISTRY.remove(deps.storage, validator_operator_addr.as_bytes());

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("validator", validator_addr)
        .add_attribute("undelegation", delegation.amount.amount))
}

fn retry_redelegation(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    validator_addr: String,
) -> LstResult<Response> {
    let Config { hub_contract, .. } = CONFIG.load(deps.storage)?;
    let validator_operator_addr =
        convert_addr_by_prefix(validator_addr.as_str(), VALIDATOR_ADDR_PREFIX);

    let pending = PENDING_REDELEGATIONS.load(deps.storage, validator_operator_addr.as_bytes())?;

    // Check if enough time has passed since the last attempt
    let time_since_last_attempt = env.block.time.seconds() - pending.timestamp;
    if time_since_last_attempt < REDELEGATION_COOLDOWN {
        // 24 hours
        return Err(ValidatorError::RedelegationCooldownNotMet.into());
    }

    let hub_contract_string = hub_contract.to_string();

    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract_string.clone(),
        msg: to_json_binary(&RedelegateProxy {
            src_validator: pending.src_validator,
            redelegations: pending.redelegations,
        })?,
        funds: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract_string,
        msg: to_json_binary(&UpdateGlobalIndex {})?,
        funds: vec![],
    }));

    // Remove pending redelegation after successful retry
    PENDING_REDELEGATIONS.remove(deps.storage, validator_operator_addr.as_bytes());

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("redelegation", validator_addr))
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
        QueryMsg::PendingRedelegations {} => {
            Ok(to_json_binary(&query_pending_redelegations(deps)?)?)
        }
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

fn query_pending_redelegations(deps: Deps) -> LstResult<Vec<(String, PendingRedelegation)>> {
    let pending_redelegations: Vec<(String, PendingRedelegation)> = PENDING_REDELEGATIONS
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (key, value) = item?;
            String::from_utf8(key)
                .map_err(|_| ValidatorError::InvalidKey.into())
                .map(|key_str| (key_str, value))
        })
        .collect::<LstResult<Vec<_>>>()?;
    Ok(pending_redelegations)
}
