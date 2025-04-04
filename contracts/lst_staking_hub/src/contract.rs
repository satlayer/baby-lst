use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgBeginRedelegate;
use cosmwasm_std::{
    attr, entry_point, from_json, to_json_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Event, MessageInfo, QueryRequest, Response, Uint128, WasmMsg, WasmQuery,
};

use cw2::set_contract_version;

use cw20::Cw20ReceiveMsg;
use lst_common::types::{LstResult, ProtoCoin, ResponseType, StdCoin};
use lst_common::ContractError;
use lst_common::{
    babylon_msg::{CosmosAny, MsgWrappedBeginRedelegate},
    errors::HubError,
    hub::{
        Config, CurrentBatch, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Parameters, QueryMsg, State,
    },
};

use crate::config::{execute_update_config, execute_update_params};
use crate::constants::{
    LST_EXCHANGE_RATE_UPDATED, MAX_EPOCH_LENGTH, MAX_UNSTAKING_PERIOD, NEW_AMOUNT, NEW_RATE,
    OLD_AMOUNT, OLD_RATE, TOTAL_STAKED_AMOUNT_UPDATED,
};
use crate::query::{
    query_config, query_current_batch, query_parameters, query_state, query_unstake_requests,
    query_unstake_requests_limit, query_unstake_requests_limitation, query_withdrawable_unstaked,
};
use crate::stake::execute_stake;
use crate::state::{
    update_state, StakeType, UnstakeType, CONFIG, CURRENT_BATCH, PARAMETERS, STATE,
};
use crate::unstake::{
    execute_process_undelegations, execute_process_withdraw_requests, execute_unstake,
    execute_withdraw_unstaked, execute_withdraw_unstaked_for_batches,
};
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
) -> LstResult<Response<ResponseType>> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate epoch length
    if msg.epoch_length > MAX_EPOCH_LENGTH {
        return Err(ContractError::Hub(HubError::InvalidEpochLength));
    }

    // Validate unstaking period
    if msg.unstaking_period > MAX_UNSTAKING_PERIOD {
        return Err(ContractError::Hub(HubError::InvalidUnstakingPeriod));
    }

    // Validate epoch length is less than unstaking period
    if msg.epoch_length >= msg.unstaking_period {
        return Err(ContractError::Hub(HubError::InvalidPeriods));
    }

    let data = Config {
        owner: info.sender,
        lst_token: None,
        validators_registry_contract: None,
        reward_dispatcher_contract: None,
    };
    CONFIG.save(deps.storage, &data)?;

    // store state
    let state = State {
        lst_exchange_rate: Decimal::one(),
        total_staked_amount: Uint128::zero(),
        last_index_modification: env.block.time.seconds(),
        prev_hub_balance: Uint128::zero(),
        last_unbonded_time: env.block.time.seconds(),
        last_processed_batch: 0u64,
    };
    STATE.save(deps.storage, &state)?;
    let mut events: Vec<Event> = vec![];
    events.push(
        Event::new(LST_EXCHANGE_RATE_UPDATED)
            .add_attribute(OLD_RATE, Decimal::zero().to_string())
            .add_attribute(NEW_RATE, Decimal::one().to_string()),
    );
    events.push(
        Event::new(TOTAL_STAKED_AMOUNT_UPDATED)
            .add_attribute(OLD_AMOUNT, Uint128::zero().to_string())
            .add_attribute(NEW_AMOUNT, Uint128::zero().to_string()),
    );

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

    Ok(Response::new().add_events(events))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> LstResult<Response<ResponseType>> {
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
        ExecuteMsg::Unstake { amount } => execute_unstake(
            deps,
            env,
            amount,
            info.sender.to_string(),
            UnstakeType::BurnFromFlow,
        ),
        ExecuteMsg::WithdrawUnstaked {} => execute_withdraw_unstaked(deps, env, info),
        ExecuteMsg::WithdrawUnstakedForBatches { batch_ids } => {
            execute_withdraw_unstaked_for_batches(deps, env, info, batch_ids)
        }
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
        ExecuteMsg::ProcessUndelegations {} => execute_process_undelegations(deps, env),
        ExecuteMsg::ProcessWithdrawRequests {} => execute_process_withdraw_requests(deps, env),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> LstResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        QueryMsg::ExchangeRate {} => {
            let state = query_state(deps, &env)?;
            Ok(to_json_binary(&state.lst_exchange_rate)?)
        }
        QueryMsg::Parameters {} => Ok(to_json_binary(&query_parameters(deps)?)?),
        QueryMsg::State {} => Ok(to_json_binary(&query_state(deps, &env)?)?),
        QueryMsg::CurrentBatch {} => Ok(to_json_binary(&query_current_batch(deps)?)?),
        QueryMsg::WithdrawableUnstaked { address } => Ok(to_json_binary(
            &query_withdrawable_unstaked(deps, env, address)?,
        )?),
        QueryMsg::UnstakeRequests { address } => {
            Ok(to_json_binary(&query_unstake_requests(deps, address)?)?)
        }
        QueryMsg::UnstakeRequestsLimit {
            address,
            start_from,
            limit,
        } => Ok(to_json_binary(&query_unstake_requests_limit(
            deps, address, start_from, limit,
        )?)?),
        QueryMsg::AllHistory { start_from, limit } => Ok(to_json_binary(
            &query_unstake_requests_limitation(deps, start_from, limit)?,
        )?),
    }
}

// Handler for tracking slashing
pub fn execute_slashing(mut deps: DepsMut, env: Env) -> LstResult<Response<ResponseType>> {
    let mut state = STATE.load(deps.storage)?;
    // call slashing
    let (events, state) = check_slashing(&mut deps, &env, &mut state)?;
    Ok(Response::new().add_events(events).add_attributes(vec![
        attr("action", "check_slashing"),
        attr("new_lst_exchange_rate", state.lst_exchange_rate.to_string()),
    ]))
}

// Check if slashing has happened and return the slashed amount
pub fn check_slashing<'a>(
    deps: &mut DepsMut,
    env: &Env,
    state: &'a mut State,
) -> LstResult<(Vec<Event>, &'a State)> {
    let old_state = state.clone();

    query_actual_state(deps.as_ref(), env, state)?;

    let events = update_state(deps.storage, old_state, state.clone())?;
    Ok((events, state))
}

pub(crate) fn query_total_lst_token_issued(deps: Deps) -> LstResult<Uint128> {
    let token_address = &CONFIG
        .load(deps.storage)?
        .lst_token
        .ok_or(HubError::LstTokenNotSet)?;

    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: token_address.to_string(),
        msg: to_json_binary(&Cw20QueryMsg::TokenInfo {})?,
    }))?;

    Ok(token_info.total_supply)
}

pub fn query_actual_state<'a>(deps: Deps, env: &Env, state: &'a mut State) -> LstResult<&'a State> {
    let delegations = deps
        .querier
        .query_all_delegations(env.contract.address.clone())?;
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
    let state_total_staked = state.total_staked_amount;
    if state_total_staked.is_zero() {
        return Ok(state);
    }

    if state_total_staked.u128() > actual_total_staked.u128() {
        state.total_staked_amount = actual_total_staked;
    }

    // Need total issued for updating the exchange rate
    let lst_total_issued = query_total_lst_token_issued(deps)?;
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let current_requested_lst_token_amount = current_batch.requested_lst_token_amount;

    state.update_lst_exchange_rate(lst_total_issued, current_requested_lst_token_amount);

    Ok(state)
}

pub fn execute_redelegate_proxy(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    src_validator: String,
    redelegations: Vec<(String, StdCoin)>,
) -> LstResult<Response<ResponseType>> {
    let sender = info.sender;
    let config = CONFIG.load(deps.storage)?;
    let validator_registry_addr = config
        .validators_registry_contract
        .ok_or_else(|| ContractError::Hub(HubError::ValidatorRegistryNotSet))?;

    if sender != validator_registry_addr && sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let messages: Vec<CosmosMsg<ResponseType>> = redelegations
        .into_iter()
        .map(|(dst_validator, amount)| {
            prepare_wrapped_begin_redelegate_msg(
                amount.denom,
                amount.amount.to_string(),
                env.contract.address.to_string(),
                src_validator.clone(),
                dst_validator,
            )
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
    // only lst token contract can execute this message
    let config = CONFIG.load(deps.storage)?;
    let lst_token_addr = config
        .lst_token
        .ok_or_else(|| ContractError::Hub(HubError::LstTokenNotSet))?;

    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Unstake {} => {
            if info.sender == lst_token_addr {
                execute_unstake(
                    deps,
                    env,
                    cw20_msg.amount,
                    cw20_msg.sender,
                    UnstakeType::BurnFlow,
                )
            } else {
                Err(ContractError::Unauthorized {})
            }
        }
    }
}

pub fn execute_update_global_index(deps: DepsMut, env: Env) -> LstResult<Response<ResponseType>> {
    let mut messages: Vec<CosmosMsg<ResponseType>> = vec![];

    let config = CONFIG.load(deps.storage)?;
    let reward_address = &config
        .reward_dispatcher_contract
        .ok_or(HubError::RewardDispatcherNotSet)?;

    // Send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(&deps, env.contract.address.to_string())?;
    messages.append(&mut withdraw_msgs);

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_address.to_string(),
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

fn withdraw_all_rewards(
    deps: &DepsMut,
    delegator: String,
) -> LstResult<Vec<CosmosMsg<ResponseType>>> {
    let mut messages: Vec<CosmosMsg<ResponseType>> = vec![];

    let delegations = deps.querier.query_all_delegations(delegator)?;

    if !delegations.is_empty() {
        for delegation in delegations {
            let msg: CosmosMsg<ResponseType> =
                CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward {
                    validator: delegation.validator,
                });
            messages.push(msg);
        }
    }

    Ok(messages)
}

fn prepare_wrapped_begin_redelegate_msg(
    denom: String,
    amount: String,
    delegator_address: String,
    validator_src_address: String,
    validator_dst_address: String,
) -> CosmosMsg {
    let redelegate_msg = MsgBeginRedelegate {
        delegator_address,
        validator_src_address,
        validator_dst_address,
        amount: Some(ProtoCoin { denom, amount }),
    };

    MsgWrappedBeginRedelegate {
        msg: Some(redelegate_msg),
    }
    .to_any()
}
