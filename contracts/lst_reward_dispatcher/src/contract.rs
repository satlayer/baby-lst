use cosmwasm_std::{
    attr, entry_point, to_json_binary, Addr, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal,
    Deps, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use lst_common::{
    hub::{is_paused, ExecuteMsg::StakeRewards},
    to_checked_address,
    types::LstResult,
    ContractError, MigrateMsg,
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

/// This can only be called by the contract ADMIN, enforced by `wasmd` separate from cosmwasm.
/// See https://github.com/CosmWasm/cosmwasm/issues/926#issuecomment-851259818
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> LstResult<Response> {
    cw2::ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
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

#[cfg(test)]
mod tests {
    use crate::contract::instantiate;
    use cosmwasm_std::{
        attr, coins, from_json,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, BankMsg, ContractResult, CosmosMsg, Decimal, SubMsg, SystemError,
        SystemResult, WasmMsg, WasmQuery,
    };
    use lst_common::{
        hub::{ExecuteMsg as HubExecuteMsg, Parameters, QueryMsg as HubQueryMsg},
        rewards_msg::InstantiateMsg,
        ContractError,
    };

    use super::{execute_dispatch_rewards, execute_update_config, query_config};

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let sender = deps.api.addr_make("sender");
        let hub_contract = deps.api.addr_make("hub_contract");
        let denom = "denom";
        let fee_addr = deps.api.addr_make("fee_addr");

        let info = message_info(&sender, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                hub_contract: hub_contract.to_string(),
                reward_denom: denom.to_string(),
                fee_addr: fee_addr.to_string(),
                fee_rate: "0.1".parse().unwrap(),
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        {
            let msg = InstantiateMsg {
                hub_contract: hub_contract.to_string(),
                reward_denom: denom.to_string(),
                fee_addr: fee_addr.to_string(),
                fee_rate: "0.4".parse().unwrap(),
            };

            let err = instantiate(deps.as_mut(), env, info, msg).unwrap_err();
            assert_eq!(err, ContractError::InvalidFeeRate {});
        }
    }

    #[test]
    fn test_execute_update_config() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let hub_contract = deps.api.addr_make("hub_contract");
        let denom = "denom";
        let fee_addr = deps.api.addr_make("fee_addr");
        let fee_rate = Decimal::percent(10);

        let info = message_info(&owner, &[]);

        // instantiate
        {
            let msg = InstantiateMsg {
                hub_contract: hub_contract.to_string(),
                reward_denom: denom.to_string(),
                fee_addr: fee_addr.to_string(),
                fee_rate,
            };
            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // update nothing
        {
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                None,
            )
            .unwrap();

            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.owner, owner);
            assert_eq!(config.hub_contract, hub_contract);
            assert_eq!(config.reward_denom, denom.to_string());
            assert_eq!(config.fee_addr, fee_addr);
            assert_eq!(config.fee_rate, fee_rate);
        }

        let new_owner = deps.api.addr_make("new_owner");

        // update config
        {
            let new_hub_contract = deps.api.addr_make("new_hub_contract");
            let new_fee_addr = deps.api.addr_make("new_fee_addr");
            let new_fee_rate = Decimal::percent(20);

            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                Some(new_owner.to_string()),
                Some(new_hub_contract.to_string()),
                Some(new_fee_addr.to_string()),
                Some(new_fee_rate),
            )
            .unwrap();

            let config = query_config(deps.as_ref()).unwrap();
            assert_eq!(config.owner, new_owner);
            assert_eq!(config.hub_contract, new_hub_contract);
            assert_eq!(config.reward_denom, denom.to_string());
            assert_eq!(config.fee_addr, new_fee_addr);
            assert_eq!(config.fee_rate, new_fee_rate);
        }

        // unauthorized error
        {
            let err = execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                None,
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }

        // update invalid fee rate
        {
            let info = message_info(&new_owner, &[]);
            let new_fee_rate = Decimal::percent(40);

            let err = execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                None,
                None,
                Some(new_fee_rate),
            )
            .unwrap_err();

            assert_eq!(err, ContractError::InvalidFeeRate {});
        }
    }

    #[test]
    fn test_execute_dispatch_rewards() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let hub_contract = deps.api.addr_make("hub_contract");
        let denom = "denom";
        let fee_addr = deps.api.addr_make("fee_addr");
        let fee_rate = Decimal::percent(10);

        let info = message_info(&owner, &[]);

        // instantiate
        {
            let msg = InstantiateMsg {
                hub_contract: hub_contract.to_string(),
                reward_denom: denom.to_string(),
                fee_addr: fee_addr.to_string(),
                fee_rate,
            };
            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // dispatch rewards successfully
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: HubQueryMsg = from_json(msg).unwrap();
                    match msg {
                        HubQueryMsg::Parameters {} => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&Parameters::default()).unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(SystemError::Unknown {}),
            });

            let balance = coins(1000, denom);
            deps.querier
                .bank
                .update_balance(env.clone().contract.address, balance);

            let info = message_info(&hub_contract, &[]);
            let response = execute_dispatch_rewards(deps.as_mut(), env.clone(), info).unwrap();
            assert_eq!(
                response.messages,
                vec![
                    SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                        to_address: fee_addr.to_string(),
                        amount: coins(100, denom)
                    })),
                    SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: hub_contract.to_string(),
                        msg: to_json_binary(&HubExecuteMsg::StakeRewards {}).unwrap(),
                        funds: coins(900, denom)
                    }))
                ]
            );

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "claim_rewards"),
                    attr("reward_amt", "900denom".to_string()),
                    attr("fee", "100denom".to_string()),
                ]
            );
        }

        // hub contract paused error
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: HubQueryMsg = from_json(msg).unwrap();
                    match msg {
                        HubQueryMsg::Parameters {} => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&&Parameters {
                                epoch_length: 100,
                                staking_coin_denom: "denom".to_string(),
                                unstaking_period: 100,
                                paused: true,
                            })
                            .unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(SystemError::Unknown {}),
            });

            let info = message_info(&hub_contract, &[]);
            let err = execute_dispatch_rewards(deps.as_mut(), env.clone(), info).unwrap_err();

            assert_eq!(err, ContractError::HubPaused {});
        }

        // unauthorized error
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: HubQueryMsg = from_json(msg).unwrap();
                    match msg {
                        HubQueryMsg::Parameters {} => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&Parameters::default()).unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(SystemError::Unknown {}),
            });

            let info = message_info(&owner, &[]);
            let err = execute_dispatch_rewards(deps.as_mut(), env.clone(), info).unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }
    }
}
