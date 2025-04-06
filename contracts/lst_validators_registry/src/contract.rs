use std::collections::HashMap;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    Response, StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;

use lst_common::{
    calculate_delegations,
    errors::ValidatorError,
    hub::ExecuteMsg::{RedelegateProxy, UpdateGlobalIndex},
    to_checked_address,
    types::LstResult,
    validate_migration,
    validator::{
        Config, ExecuteMsg, InstantiateMsg, PendingRedelegation, QueryMsg, ReDelegation, Validator,
        ValidatorResponse,
    },
    ContractError, MigrateMsg,
};

use crate::{
    helper::{convert_addr_by_prefix, fetch_validator_info, VALIDATOR_ADDR_PREFIX},
    state::{
        CONFIG, LAST_REDELEGATIONS, PENDING_REDELEGATIONS, REDELEGATION_COOLDOWN,
        VALIDATOR_EXCLUDE_LIST, VALIDATOR_REGISTRY,
    },
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
        ExecuteMsg::RemoveValidator { address } => remove_validator(deps, env, info, address),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
        } => update_config(deps, env, info, owner, hub_contract),
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
        VALIDATOR_EXCLUDE_LIST.remove(deps.storage, info.address.as_bytes());
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

    let validators = query_validators(deps.as_ref())?;

    if validators.is_empty() {
        return Err(ValidatorError::LastValidatorRemovalNotAllowed.into());
    }
    let validator_operator_addr =
        convert_addr_by_prefix(validator_addr.as_str(), VALIDATOR_ADDR_PREFIX);
    VALIDATOR_EXCLUDE_LIST.save(deps.storage, validator_operator_addr.as_bytes(), &true)?;

    let last_try = LAST_REDELEGATIONS
        .load(deps.storage, validator_addr.as_bytes())
        .unwrap_or(0);
    if env.block.time.seconds() - last_try < REDELEGATION_COOLDOWN {
        return Ok(Response::new());
    }

    let delegation = deps
        .querier
        .query_delegation(hub_contract.clone(), validator_addr.clone())?;
    let mut redelegations: Vec<(String, Coin)> = vec![];

    if let Some(delegation) = delegation {
        if delegation.can_redelegate.amount > Uint128::zero() {
            let delegations =
                calculate_delegations(delegation.can_redelegate.amount, validators.as_slice())?;
            redelegations = validators
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
        }
    }

    let hub_contract_string = hub_contract.to_string();

    let messages: Vec<CosmosMsg> = vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub_contract_string.clone(),
            msg: to_json_binary(&RedelegateProxy {
                src_validator: validator_addr.clone(),
                redelegations,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub_contract_string,
            msg: to_json_binary(&UpdateGlobalIndex {})?,
            funds: vec![],
        }),
    ];

    LAST_REDELEGATIONS.save(
        deps.storage,
        validator_addr.as_bytes(),
        &env.block.time.seconds(),
    )?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("remove validator", validator_addr))
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

        QueryMsg::GetRedelegations {
            pending_stake,
            pending_unstake,
        } => Ok(to_json_binary(&query_get_redelegations(
            deps,
            pending_stake,
            pending_unstake,
        )?)?),
        QueryMsg::GetActiveValidators {} => Ok(to_json_binary(&query_active_validators(deps))?),
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

fn query_get_redelegations(
    deps: Deps,
    pending_stake: u128,
    pending_unstake: u128,
) -> LstResult<Vec<ReDelegation>> {
    let mut delegations = HashMap::<String, u128>::new();

    let Config {
        owner: _,
        hub_contract,
    } = CONFIG.load(deps.storage)?;

    deps.querier
        .query_all_delegations(&hub_contract)?
        .into_iter()
        .for_each(|delegation| {
            delegations.insert(delegation.validator, delegation.amount.amount.into());
        });

    let mut continue_list: Vec<String> = Vec::new();
    let mut remove_list: Vec<String> = Vec::new();

    for del in delegations.iter() {
        if !VALIDATOR_EXCLUDE_LIST
            .load(deps.storage, del.0.clone().as_bytes())
            .unwrap_or(false)
        {
            if VALIDATOR_REGISTRY
                .load(deps.storage, del.0.as_bytes())
                .is_ok()
            {
                continue_list.push(del.0.clone());
            }
        } else {
            remove_list.push(del.0.clone());
        }
    }

    let stake_rebalance = pending_stake
        .checked_div(continue_list.len() as u128)
        .unwrap();
    let unstake_rebalnace = pending_unstake
        .checked_div((continue_list.len() as u128) + (remove_list.len() as u128))
        .unwrap();

    let mut redelegations = HashMap::<String, ReDelegation>::new();

    for add in continue_list {
        let delegate = *delegations.get(&add).unwrap_or(&0_u128);
        let total_stake = delegate.checked_add(stake_rebalance).unwrap();
        let (action, amount) = if unstake_rebalnace > total_stake {
            (1_u8, unstake_rebalnace)
        } else {
            (0, (total_stake - unstake_rebalnace))
        };
        let redelegation = ReDelegation {
            validator: add.clone(),
            amount,
            action,
        };
        redelegations.insert(add, redelegation);
    }

    for remove in remove_list {
        let delegate = *delegations.get(&remove).unwrap_or(&0_u128);
        let redelegation = ReDelegation {
            validator: remove.clone(),
            amount: delegate + unstake_rebalnace,
            action: 1,
        };
        redelegations.insert(remove, redelegation);
    }

    Ok(redelegations
        .values()
        .cloned()
        .collect::<Vec<ReDelegation>>())
}

fn query_active_validators(deps: Deps) -> Vec<String> {
    let keys = VALIDATOR_REGISTRY
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .collect::<StdResult<Vec<Vec<u8>>>>()
        .unwrap();
    keys.iter()
        .filter_map(|k| {
            if VALIDATOR_EXCLUDE_LIST.has(deps.storage, k) {
                return None;
            }
            String::from_utf8(k.clone()).ok()
        })
        .collect::<Vec<String>>()
}
