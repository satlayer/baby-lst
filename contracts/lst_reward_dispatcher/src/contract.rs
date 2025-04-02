use cosmwasm_std::{
    attr, entry_point, to_json_binary, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use lst_common::{
    hub::{is_paused, ExecuteMsg::StakeRewards},
    to_checked_address,
    types::LstResult,
    ContractError, MigrateMsg,
};

use crate::state::CONFIG;
use lst_common::rewards_msg::{Config, ExecuteMsg, InstantiateMsg, QueryMsg};

const CONTRACT_NAME: &str = "crates.io:reward-dispatcher";
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
        satlayer_fee_addr,
        satlayer_fee_rate,
    } = msg;

    let config = Config {
        owner: info.sender,
        hub_contract: to_checked_address(deps.as_ref(), &hub_contract)?,
        reward_denom,
        satlayer_fee_addr: to_checked_address(deps.as_ref(), &satlayer_fee_addr)?,
        satlayer_fee_rate,
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
            satlayer_fee_addr,
            satlayer_fee_rate,
        } => execute_update_config(
            deps,
            env,
            info,
            owner,
            hub_contract,
            satlayer_fee_addr,
            satlayer_fee_rate,
        ),
    }
}

fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
    satlayer_fee_addr: Option<String>,
    satlayer_fee_rate: Option<Decimal>,
) -> LstResult<Response> {
    let config: Config = query_config(deps.as_ref())?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(o) = owner {
        let owner_addr = to_checked_address(deps.as_ref(), &o)?;

        CONFIG.update(deps.storage, |mut prev_config| -> LstResult<_> {
            prev_config.owner = owner_addr;
            Ok(prev_config)
        })?;
    }

    if let Some(h) = hub_contract {
        let hub_addr = to_checked_address(deps.as_ref(), &h)?;

        CONFIG.update(deps.storage, |mut prev_config| -> LstResult<_> {
            prev_config.hub_contract = hub_addr;
            Ok(prev_config)
        })?;
    }

    if let Some(s) = satlayer_fee_addr {
        let fee_addr = to_checked_address(deps.as_ref(), &s)?;

        CONFIG.update(deps.storage, |mut prev_config| -> LstResult<_> {
            prev_config.satlayer_fee_addr = fee_addr;
            Ok(prev_config)
        })?;
    }

    if let Some(rate) = satlayer_fee_rate {
        CONFIG.update(deps.storage, |mut prev_config| -> LstResult<_> {
            prev_config.satlayer_fee_rate = rate;
            Ok(prev_config)
        })?;
    }

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

    let reward_fee_amt = compute_fee(rewards.amount, config.satlayer_fee_rate);

    if !reward_fee_amt.is_zero() {
        let fee_coin = Coin {
            denom: config.reward_denom,
            amount: reward_fee_amt,
        };

        attrs.push(attr("fee", fee_coin.to_string()));

        messages.push(
            BankMsg::Send {
                to_address: config.satlayer_fee_addr.to_string(),
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
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default().add_attribute("migrate", "successful"))
}

fn compute_fee(amount: Uint128, fee_rate: Decimal) -> Uint128 {
    (Decimal::from_ratio(amount, 1u128) * fee_rate).to_uint_ceil()
}
