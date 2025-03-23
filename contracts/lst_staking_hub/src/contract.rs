use cosmwasm_std::{
    attr, entry_point, from_json, to_json_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut,
    DistributionMsg, Env, MessageInfo, QueryRequest, Response, StakingMsg, StdError, StdResult,
    Uint128, WasmMsg, WasmQuery,
};

use cw2::set_contract_version;

use cw20::Cw20ReceiveMsg;
use lst_common::errors::HubError;
use lst_common::hub::{Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Parameters, QueryMsg};
use lst_common::types::LstResult;
use lst_common::ContractError;

use crate::config::{execute_update_config, execute_update_params};
use crate::stake::execute_stake;
use crate::state::{
    CurrentBatch, StakeType, State, CONFIG, CURRENT_BATCH, PARAMETERS, STATE, TOTAL_STAKED,
};
use crate::unstake::{execute_unstake, execute_withdraw_unstaked};
use cw20_base::{msg::QueryMsg as Cw20QueryMsg, state::TokenInfo};
use lst_common::rewards_msg::ExecuteMsg::DispatchRewards;

const CONTRACT_NAME: &str = "crates.io:lst-staking-hub";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> LstResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let sender = info.sender;
    let sender_raw = deps.api.addr_canonicalize(sender.as_str())?;

    let data = Config {
        owner: sender_raw,
        lst_token: None,
        validators_registry_contract: None,
        reward_dispatcher_contract: None,
    };
    CONFIG.save(deps.storage, &data)?;

    // store state
    let state = State {
        lst_exchange_rate: Decimal::one(),
        total_lst_token_amount: Uint128::zero(),
        last_index_modification: env.block.time.seconds(),
        prev_hub_balance: Uint128::zero(),
        last_unbonded_time: env.block.time.seconds(),
        last_processed_batch: 0u64,
    };
    STATE.save(deps.storage, &state)?;

    // Instantiate parameters
    let params = Parameters {
        epoch_length: msg.epoch_length,
        staking_coin_denom: msg.staking_coin_denom,
        paused: false,
        unstaking_period: msg.unstaking_period,
    };
    PARAMETERS.save(deps.storage, &params)?;

    // Instantiate current batch
    let batch = CurrentBatch {
        id: 1,
        requested_lst_token_amount: Default::default(),
    };
    CURRENT_BATCH.save(deps.storage, &batch)?;

    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> LstResult<Response> {
    if let ExecuteMsg::UpdateParams {
        pause,
        epoch_length,
        unstaking_period,
    } = msg
    {
        return execute_update_params(deps, env, info, pause, epoch_length, unstaking_period);
    }

    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused {
        return Err(ContractError::Hub(HubError::Paused));
    }

    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Stake {} => execute_stake(deps, env, info, StakeType::LSTMint),
        ExecuteMsg::StakeRewards {} => execute_stake(deps, env, info, StakeType::StakeRewards),
        ExecuteMsg::Unstake { amount } => {
            execute_unstake(deps, env, amount, info.sender.to_string())
        }
        ExecuteMsg::WithdrawUnstaked {} => execute_withdraw_unstaked(deps, env, info),
        ExecuteMsg::CheckSlashing {} => execute_slashing(deps, env),
        ExecuteMsg::UpdateParams {
            pause,
            epoch_length,
            unstaking_period,
        } => execute_update_params(deps, env, info, pause, epoch_length, unstaking_period),
        ExecuteMsg::UpdateConfig {
            owner,
            lst_token,
            validator_registry,
            reward_dispatcher,
        } => execute_update_config(
            deps,
            env,
            info,
            owner,
            lst_token,
            validator_registry,
            reward_dispatcher,
        ),
        ExecuteMsg::RedelegateProxy {
            src_validator,
            redelegations,
        } => execute_redelegate_proxy(deps, env, info, src_validator, redelegations),
        ExecuteMsg::UpdateGlobalIndex {} => execute_update_global_index(deps, env),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => {
            let config = CONFIG.load(deps.storage)?;
            to_json_binary(&config)
        }
        QueryMsg::TotalStaked {} => {
            let total = TOTAL_STAKED.load(deps.storage)?;
            to_json_binary(&total)
        }
        QueryMsg::ExchangeRate {} => {
            todo!()
        }
        QueryMsg::Parameters {} => todo!(),
    }
}

// Handler for tracking slashing
pub fn execute_slashing(mut deps: DepsMut, env: Env) -> LstResult<Response> {
    // call slashing
    let state = check_slashing(&mut deps, env)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "check_slashing"),
        attr("new_lst_exchange_rate", state.lst_exchange_rate.to_string()),
    ]))
}

// Check if slashing has happened and return the slashed amount
pub fn check_slashing(deps: &mut DepsMut, env: Env) -> LstResult<State> {
    let state = query_actual_state(deps.as_ref(), env)?;

    STATE.save(deps.storage, &state)?;
    Ok(state)
}

pub(crate) fn query_total_lst_token_issued(deps: Deps) -> StdResult<Uint128> {
    let token_address = deps.api.addr_humanize(
        &CONFIG
            .load(deps.storage)?
            .lst_token
            .ok_or_else(|| StdError::generic_err("LST token address is not set"))?,
    )?;
    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: token_address.to_string(),
        msg: to_json_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(token_info.total_supply)
}

fn query_actual_state(deps: Deps, env: Env) -> LstResult<State> {
    let mut state = STATE.load(deps.storage)?;
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
    if delegations.is_empty() {
        return Ok(state);
    }

    // read params
    let params = PARAMETERS.load(deps.storage)?;
    let staking_coin_denom = params.staking_coin_denom;

    // check the actual bonded amount
    let mut actual_total_staked = Uint128::zero();
    for delegation in &delegations {
        if delegation.amount.denom == staking_coin_denom {
            actual_total_staked += delegation.amount.amount;
        }
    }

    // Check the amount that contract thinks is staked
    let state_total_staked = state.total_lst_token_amount;
    if state_total_staked.is_zero() {
        return Ok(state);
    }

    // Need total issued for updating the exchange rate
    let lst_total_issued = query_total_lst_token_issued(deps)?;
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let current_requested_lst_token_amount = current_batch.requested_lst_token_amount;

    if state_total_staked.u128() > actual_total_staked.u128() {
        state.total_lst_token_amount = actual_total_staked;
    }

    state.update_lst_exchange_rate(lst_total_issued, current_requested_lst_token_amount);

    Ok(state)
}

pub fn execute_redelegate_proxy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    src_validator: String,
    redelegations: Vec<(String, Coin)>,
) -> LstResult<Response> {
    let sender_contract_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let config = CONFIG.load(deps.storage)?;
    let validator_registry_addr = config
        .validators_registry_contract
        .ok_or_else(|| ContractError::Hub(HubError::ValidatorRegistryNotSet))?;

    if sender_contract_addr != validator_registry_addr && sender_contract_addr != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let messages: Vec<CosmosMsg> = redelegations
        .into_iter()
        .map(|(dst_validator, amount)| {
            cosmwasm_std::CosmosMsg::Staking(StakingMsg::Redelegate {
                src_validator: src_validator.clone(),
                dst_validator,
                amount,
            })
        })
        .collect();

    let res = Response::new().add_messages(messages);
    Ok(res)
}

// cw20 token receive handler
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> LstResult<Response> {
    let contract_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    // only lst token contract can execute this message
    let config = CONFIG.load(deps.storage)?;
    let lst_token_addr = config
        .lst_token
        .ok_or_else(|| ContractError::Hub(HubError::LstTokenNotSet))?;

    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::UnStake {} => {
            if contract_addr == lst_token_addr {
                execute_unstake(deps, env, cw20_msg.amount, info.sender.to_string())
            } else {
                Err(ContractError::Unauthorized {})
            }
        }
    }
}

pub fn execute_update_global_index(deps: DepsMut, env: Env) -> LstResult<Response> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let config = CONFIG.load(deps.storage)?;
    let reward_address = deps.api.addr_humanize(
        &config
            .reward_dispatcher_contract
            .ok_or_else(|| ContractError::Hub(HubError::RewardDispatcherNotSet))?,
    );

    // Send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(&deps, env.contract.address.to_string())?;
    messages.append(&mut withdraw_msgs);

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_address?.to_string(),
        msg: to_json_binary(&DispatchRewards {})?,
        funds: vec![],
    }));

    //update state last modified time
    STATE.update(deps.storage, |mut last_state| -> LstResult<_> {
        last_state.last_index_modification = env.block.time.seconds();
        Ok(last_state)
    })?;

    let res = Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "update_global_index")]);
    Ok(res)
}

fn withdraw_all_rewards(deps: &DepsMut, delegator: String) -> LstResult<Vec<CosmosMsg>> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let delegations = deps.querier.query_all_delegations(delegator)?;

    if !delegations.is_empty() {
        for delegation in delegations {
            let msg: CosmosMsg =
                CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward {
                    validator: delegation.validator,
                });
            messages.push(msg);
        }
    }

    Ok(messages)
}
