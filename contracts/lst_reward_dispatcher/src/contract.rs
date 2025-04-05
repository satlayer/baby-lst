use cosmwasm_std::{
    attr, entry_point, to_json_binary, Addr, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal,
    Deps, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use lst_common::{
    hub::{is_paused, ExecuteMsg::StakeRewards},
    to_checked_address,
    types::LstResult,
    validate_migration, ContractError, MigrateMsg,
};

use crate::{state::CONFIG, MAX_FEE_RATE};
use lst_common::rewards_msg::{Config, ExecuteMsg, InstantiateMsg, QueryMsg};

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

    let InstantiateMsg {
        hub_contract,
        reward_denom,
        fee_addr,
        fee_rate,
    } = msg;

    // Validate fee rate if provided
    if fee_rate > MAX_FEE_RATE {
        return Err(ContractError::InvalidFeeRate {});
    }

    let config = Config {
        owner: info.sender,
        hub_contract: to_checked_address(deps.as_ref(), &hub_contract)?,
        reward_denom,
        fee_addr: to_checked_address(deps.as_ref(), &fee_addr)?,
        fee_rate,
    };

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> LstResult<Response> {
    match msg {
        ExecuteMsg::DispatchRewards {} => execute_dispatch_rewards(deps, env, info),

        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
            fee_addr,
            fee_rate,
        } => execute_update_config(deps, env, info, owner, hub_contract, fee_addr, fee_rate),
    }
}

fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
    fee_addr: Option<String>,
    fee_rate: Option<Decimal>,
) -> LstResult<Response> {
    is_authorized_sender(deps.as_ref(), info.sender)?;

    // Validate fee rate if provided
    if let Some(rate) = &fee_rate {
        if rate > &MAX_FEE_RATE {
            return Err(ContractError::InvalidFeeRate {});
        }
    }

    let mut config: Config = query_config(deps.as_ref())?;

    // Update config with all provided values in a single operation
    if let Some(o) = owner {
        config.owner = to_checked_address(deps.as_ref(), &o)?;
    }
    if let Some(h) = hub_contract {
        config.hub_contract = to_checked_address(deps.as_ref(), &h)?;
    }
    if let Some(s) = fee_addr {
        config.fee_addr = to_checked_address(deps.as_ref(), &s)?;
    }
    if let Some(rate) = fee_rate {
        config.fee_rate = rate;
    }

    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

/// Dispatches rewards to the hub contract.
///
/// This function checks if the hub contract is paused, verifies the sender's authorization,
/// calculates the fee, and sends the rewards and fee to the respective addresses.
fn execute_dispatch_rewards(deps: DepsMut, env: Env, info: MessageInfo) -> LstResult<Response> {
    let config = query_config(deps.as_ref())?;

    let hub_addr = config.hub_contract;
    if is_paused(deps.as_ref(), hub_addr.to_string())? {
        return Err(ContractError::HubPaused);
    }

    if hub_addr != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let mut rewards = deps
        .querier
        .query_balance(env.contract.address, config.reward_denom.clone())?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attrs: Vec<Attribute> = vec![];

    let reward_fee_amt = compute_fee(rewards.amount, config.fee_rate);

    if !reward_fee_amt.is_zero() {
        let fee_coin = Coin {
            denom: config.reward_denom,
            amount: reward_fee_amt,
        };

        attrs.push(attr("fee", fee_coin.to_string()));

        messages.push(
            BankMsg::Send {
                to_address: config.fee_addr.to_string(),
                amount: vec![fee_coin],
            }
            .into(),
        );
    }

    rewards.amount = rewards
        .amount
        .checked_sub(reward_fee_amt)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    if !rewards.amount.is_zero() {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub_addr.to_string(),
            msg: to_json_binary(&StakeRewards {})?,
            funds: vec![rewards.clone()],
        }));
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "claim_rewards"),
            attr("reward_amt", rewards.to_string()),
        ])
        .add_attributes(attrs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> LstResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
    }
}

fn query_config(deps: Deps) -> LstResult<Config> {
    Ok(CONFIG.load(deps.storage)?)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> LstResult<Response> {
    validate_migration(deps.as_ref(), CONTRACT_NAME, CONTRACT_VERSION)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default().add_attribute("migrate", "successful"))
}

fn compute_fee(amount: Uint128, fee_rate: Decimal) -> Uint128 {
    (Decimal::from_ratio(amount, 1u128) * fee_rate).to_uint_ceil()
}

fn is_authorized_sender(deps: Deps, sender: Addr) -> LstResult<()> {
    let Config { owner, .. } = CONFIG.load(deps.storage)?;
    if sender != owner {
        return Err(ContractError::Unauthorized {});
    }
    Ok(())
}
