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
    validator::{Config, ExecuteMsg, InstantiateMsg, QueryMsg, Validator, ValidatorResponse},
    ContractError, MigrateMsg,
};

use crate::state::{CONFIG, VALIDATOR_REGISTRY};

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
            hub_contract: hub_contract,
        },
    )?;

    msg.validators
        .into_iter()
        .try_for_each(|val| -> LstResult<()> {
            let checked_address = to_checked_address(deps.as_ref(), val.address.as_str())?;
            VALIDATOR_REGISTRY.save(deps.storage, checked_address.as_bytes(), &val)?;
            Ok(())
        })?;
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
    let val_checked_address = to_checked_address(deps.as_ref(), validator.address.as_str())?;
    VALIDATOR_REGISTRY.save(deps.storage, val_checked_address.as_bytes(), &validator)?;
    Ok(Response::default())
}

fn remove_validator(
    deps: DepsMut,
    _env: Env,
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

    VALIDATOR_REGISTRY.remove(deps.storage, validator_addr.as_bytes());

    let validators = query_validators(deps.as_ref())?;

    if validators.is_empty() {
        return Err(ValidatorError::LastValidatorRemovalNotAllowed.into());
    }

    let delegation = match deps
        .querier
        .query_delegation(hub_contract.clone(), validator_addr.clone())?
    {
        Some(delegation) if delegation.can_redelegate.amount >= delegation.amount.amount => {
            delegation
        }
        _ => return Ok(Response::new()),
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
            src_validator: validator_addr,
            redelegations,
        })?,
        funds: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract_string,
        msg: to_json_binary(&UpdateGlobalIndex {})?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages))
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

    if let Some(owner) = owner {
        let owner_raw = to_checked_address(deps.as_ref(), owner.as_str())?;
        CONFIG.update(deps.storage, |mut old_config| -> LstResult<Config> {
            old_config.owner = owner_raw;
            Ok(old_config)
        })?;
    }

    if let Some(hub_contract) = hub_contract {
        let hub_addr_raw = to_checked_address(deps.as_ref(), hub_contract.as_str())?;
        CONFIG.update(deps.storage, |mut old_config| -> LstResult<Config> {
            old_config.hub_contract = hub_addr_raw;
            Ok(old_config)
        })?;
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> LstResult<Binary> {
    match msg {
        QueryMsg::Config {} => query_config(deps),
        QueryMsg::ValidatorsDelegation {} => Ok(to_json_binary(&query_validators(deps)?)?),
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
