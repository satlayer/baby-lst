use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgBeginRedelegate;
use cosmwasm_std::{
    attr, entry_point, from_json, to_json_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    DistributionMsg, Env, Event, MessageInfo, QueryRequest, Response, Uint128, WasmMsg, WasmQuery,
};

use cw2::set_contract_version;

use cw20::Cw20ReceiveMsg;
use lst_common::hub::PendingDelegation;
use lst_common::types::{LstResult, ProtoCoin, ResponseType, StdCoin};
use lst_common::{
    babylon_msg::{CosmosAny, MsgWrappedBeginRedelegate},
    errors::HubError,
    hub::{
        Config, CurrentBatch, Cw20HookMsg, ExecuteMsg, InstantiateMsg, Parameters, QueryMsg, State,
    },
};
use lst_common::{ContractError, MigrateMsg};

use crate::config::{execute_update_config, execute_update_params};
use crate::constants::{
    AVERAGE_BLOCK_TIME, LST_EXCHANGE_RATE_UPDATED, MAX_EPOCH_LENGTH, MAX_UNSTAKING_PERIOD,
    NEW_AMOUNT, NEW_RATE, OLD_AMOUNT, OLD_RATE, TOTAL_STAKED_AMOUNT_UPDATED,
};
use crate::query::{
    query_config, query_current_batch, query_parameters, query_pending_delegation, query_state,
    query_unstake_requests, query_unstake_requests_limit, query_unstake_requests_limitation,
    query_withdrawable_unstaked,
};
use crate::stake::execute_stake;
use crate::state::{
    get_pending_delegation_amount, update_state, StakeType, UnstakeType, CONFIG, CURRENT_BATCH,
    PARAMETERS, PENDING_DELEGATION, STATE,
};
use crate::unstake::{
    execute_process_undelegations, execute_process_withdraw_requests, execute_unstake,
    execute_withdraw_unstaked, execute_withdraw_unstaked_for_batches,
};
use cw20_base::{msg::QueryMsg as Cw20QueryMsg, state::TokenInfo};
use lst_common::rewards_msg::ExecuteMsg::DispatchRewards;

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
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

    // epoch_length should be longer so that no two unstake requests can be processed in the same epoch
    if msg.epoch_length < (msg.staking_epoch_length_blocks * AVERAGE_BLOCK_TIME) {
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
        unclaimed_unstaked_balance: Uint128::zero(),
        last_unbonded_time: env.block.time.seconds(),
        last_processed_batch: 0u64,
    };
    STATE.save(deps.storage, &state)?;
    let events: Vec<Event> = vec![
        Event::new(LST_EXCHANGE_RATE_UPDATED)
            .add_attribute(OLD_RATE, Decimal::zero().to_string())
            .add_attribute(NEW_RATE, Decimal::one().to_string()),
        Event::new(TOTAL_STAKED_AMOUNT_UPDATED)
            .add_attribute(OLD_AMOUNT, Uint128::zero().to_string())
            .add_attribute(NEW_AMOUNT, Uint128::zero().to_string()),
    ];

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

    // Instantiate Pending Delegation
    let pending_delegation = PendingDelegation {
        staking_epoch_start_block_height: msg.staking_epoch_start_block_height,
        pending_staking_amount: Uint128::zero(),
        pending_unstaking_amount: Uint128::zero(),
        staking_epoch_length_blocks: msg.staking_epoch_length_blocks,
    };
    PENDING_DELEGATION.save(deps.storage, &pending_delegation)?;

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
        QueryMsg::PendingDelegation {} => {
            Ok(to_json_binary(&query_pending_delegation(deps, &env)?)?)
        }
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

    // get the pending delegation amount
    let (pending_staked_amount, pending_unstaked_amount) =
        get_pending_delegation_amount(deps, env)?;

    // Check the amount that contract thinks is staked, pending amount should not be included as we don't get that in the delegation query
    let state_total_staked =
        state.total_staked_amount - pending_staked_amount + pending_unstaked_amount;
    if state_total_staked.is_zero() {
        return Ok(state);
    }

    if state_total_staked.u128() > actual_total_staked.u128() {
        state.total_staked_amount =
            actual_total_staked + pending_staked_amount - pending_unstaked_amount;
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

    if sender != validator_registry_addr {
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

/// This can only be called by the contract ADMIN, enforced by `wasmd` separate from cosmwasm.
/// See https://github.com/CosmWasm/cosmwasm/issues/926#issuecomment-851259818
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    cw2::ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
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

#[cfg(test)]
mod tests {
    use crate::{
        config::execute_update_config,
        constants::{
            LST_EXCHANGE_RATE_UPDATED, NEW_AMOUNT, NEW_RATE, OLD_AMOUNT, OLD_RATE,
            TOTAL_STAKED_AMOUNT_UPDATED,
        },
        contract::{execute_redelegate_proxy, execute_update_global_index, instantiate},
    };
    use cosmos_sdk_proto::{cosmos::staking::v1beta1::MsgBeginRedelegate, traits::MessageExt};
    use cosmwasm_std::{
        attr, coin, coins, from_json,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, AnyMsg, Binary, Coin, ContractResult, CosmosMsg, Decimal, DistributionMsg,
        Event, FullDelegation, Response, SubMsg, SystemResult, Uint128, Validator, WasmQuery,
    };
    use cw20::Cw20QueryMsg;
    use cw20_base::state::TokenInfo;
    use lst_common::{
        babylon_msg::MsgWrappedBeginRedelegate,
        errors::{ContractError, HubError},
        hub::{Cw20HookMsg, InstantiateMsg},
        rewards_msg::ExecuteMsg::DispatchRewards,
        types::ProtoCoin,
    };

    use super::{execute_slashing, receive_cw20};

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            let response = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

            let event1 = Event::new(LST_EXCHANGE_RATE_UPDATED)
                .add_attribute(OLD_RATE, "0")
                .add_attribute(NEW_RATE, "1");
            let event2 = Event::new(TOTAL_STAKED_AMOUNT_UPDATED)
                .add_attribute(OLD_AMOUNT, "0")
                .add_attribute(NEW_AMOUNT, "0");

            assert_eq!(response, Response::new().add_events(vec![event1, event2]));
        }

        // InvalidEpochLength error, epoch_length is greater than MAX_EPOCH_LENGTH
        {
            let msg = InstantiateMsg {
                epoch_length: 604801,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            let err = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidEpochLength));
        }

        // InvalidEpochLength error
        {
            let msg = InstantiateMsg {
                epoch_length: 100,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            let err = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidEpochLength));
        }

        // InvalidUnstakingPeriod error
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 2419201,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            let err = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidUnstakingPeriod));
        }

        // InvalidPeriods error
        {
            let msg = InstantiateMsg {
                epoch_length: 72000,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            let err = instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InvalidPeriods));
        }
    }

    #[test]
    fn test_execute_slashing() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            let info = message_info(&owner, &[]);
            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // no delegations, successfully
        {
            let response = execute_slashing(deps.as_mut(), env.clone()).unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "check_slashing"),
                    attr("new_lst_exchange_rate", "1")
                ]
            );

            assert_eq!(
                response.events,
                vec![
                    Event::new("LstExchangeRateUpdated")
                        .add_attribute("old_rate", "1")
                        .add_attribute("new_rate", "1"),
                    Event::new("TotalStakedAmountUpdated")
                        .add_attribute("old_amount", "0")
                        .add_attribute("new_amount", "0")
                ]
            );
        }

        // with delegations, successfully
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: Cw20QueryMsg = from_json(msg).unwrap();
                    match msg {
                        Cw20QueryMsg::TokenInfo {} => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&TokenInfo {
                                name: "Test".to_string(),
                                symbol: "TT".to_string(),
                                decimals: 6,
                                total_supply: Uint128::new(1000),
                                mint: None,
                            })
                            .unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::Unknown {}),
            });

            let validator1 = deps.api.addr_make("validator1");
            let validator1_info = Validator::create(
                validator1.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );

            let validator1_full_delegation = FullDelegation::create(
                env.contract.address.clone(),
                validator1.to_string(),
                coin(100, denom),
                coin(120, denom),
                coins(1000, denom),
            );

            deps.querier
                .staking
                .update(denom, &[validator1_info], &[validator1_full_delegation]);

            let response = execute_slashing(deps.as_mut(), env.clone()).unwrap();

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "check_slashing"),
                    attr("new_lst_exchange_rate", "1")
                ]
            );

            assert_eq!(
                response.events,
                vec![
                    Event::new("LstExchangeRateUpdated")
                        .add_attribute("old_rate", "1")
                        .add_attribute("new_rate", "1"),
                    Event::new("TotalStakedAmountUpdated")
                        .add_attribute("old_amount", "0")
                        .add_attribute("new_amount", "0")
                ]
            );
        }
    }

    #[test]
    fn test_execute_redelegate_proxy() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let src_validator = deps.api.addr_make("src_validator");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // ValidatorRegistryNotSet error
        {
            let err = execute_redelegate_proxy(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                src_validator.to_string(),
                vec![],
            )
            .unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::ValidatorRegistryNotSet));
        }

        let validator_registry = deps.api.addr_make("validator_registry");
        execute_update_config(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            None,
            None,
            Some(validator_registry.to_string()),
            None,
        )
        .unwrap();

        // Unauthorized error
        {
            let err = execute_redelegate_proxy(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                src_validator.to_string(),
                vec![],
            )
            .unwrap_err();
            assert_eq!(err, ContractError::Unauthorized {});
        }

        // redelegate proxy successfully
        {
            let dst_validator = deps.api.addr_make("dst_validator");

            let info = message_info(&validator_registry, &[]);
            let response = execute_redelegate_proxy(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                src_validator.to_string(),
                vec![(
                    dst_validator.to_string(),
                    Coin::new(Uint128::new(100), denom),
                )],
            )
            .unwrap();

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Any(AnyMsg {
                    type_url: "/babylon.epoching.v1.MsgWrappedBeginRedelegate".to_string(),
                    value: Binary::from(
                        MsgWrappedBeginRedelegate {
                            msg: Some(MsgBeginRedelegate {
                                delegator_address: env.contract.address.to_string(),
                                validator_src_address: src_validator.to_string(),
                                validator_dst_address: dst_validator.to_string(),
                                amount: Some(ProtoCoin {
                                    denom: denom.to_string(),
                                    amount: "100".to_string()
                                }),
                            })
                        }
                        .to_bytes()
                        .unwrap()
                    ),
                }))]
            );
        }
    }

    #[test]
    fn test_receive_cw20() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // LstTokenNotSet error
        {
            let msg = cw20::Cw20ReceiveMsg {
                sender: owner.to_string(),
                amount: Uint128::from(100u128),
                msg: to_json_binary(&Cw20HookMsg::Unstake {}).unwrap(),
            };

            let err = receive_cw20(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::LstTokenNotSet));
        }

        {
            let lst_token = deps.api.addr_make("lst_token");

            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                Some(lst_token.to_string()),
                None,
                None,
            )
            .unwrap();

            let msg = cw20::Cw20ReceiveMsg {
                sender: owner.to_string(),
                amount: Uint128::from(100u128),
                msg: to_json_binary(&Cw20HookMsg::Unstake {}).unwrap(),
            };

            let lst_info = message_info(&lst_token, &[]);
            receive_cw20(deps.as_mut(), env.clone(), lst_info, msg.clone()).unwrap();

            let err = receive_cw20(deps.as_mut(), env.clone(), info.clone(), msg).unwrap_err();

            assert_eq!(err, ContractError::Unauthorized {});
        }
    }

    #[test]
    fn test_execute_update_global_index() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let info = message_info(&owner, &[]);

        // instantiate successfully
        {
            let msg = InstantiateMsg {
                epoch_length: 7200,
                staking_coin_denom: denom.to_string(),
                unstaking_period: 10000,
                staking_epoch_start_block_height: 100,
                staking_epoch_length_blocks: 360,
            };

            instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();
        }

        // RewardDispatcherNotSet error
        {
            let err = execute_update_global_index(deps.as_mut(), env.clone()).unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::RewardDispatcherNotSet));
        }

        let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
        execute_update_config(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            None,
            None,
            None,
            Some(reward_dispatcher.to_string()),
        )
        .unwrap();

        // update global index successfully without delegations
        {
            let response = execute_update_global_index(deps.as_mut(), env.clone()).unwrap();

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Wasm(
                    cosmwasm_std::WasmMsg::Execute {
                        contract_addr: reward_dispatcher.to_string(),
                        msg: to_json_binary(&DispatchRewards {}).unwrap(),
                        funds: vec![]
                    }
                ))]
            );
            assert_eq!(
                response.attributes,
                vec![attr("action", "update_global_index")]
            );
        }

        // update global index successfully with delegations
        {
            let validator1 = deps.api.addr_make("validator1");
            let validator1_info = Validator::create(
                validator1.to_string(),
                Decimal::percent(5),
                Decimal::percent(10),
                Decimal::percent(1),
            );

            let validator1_full_delegation = FullDelegation::create(
                env.contract.address.clone(),
                validator1.to_string(),
                coin(100, denom),
                coin(120, denom),
                coins(1000, denom),
            );

            deps.querier
                .staking
                .update(denom, &[validator1_info], &[validator1_full_delegation]);

            let response = execute_update_global_index(deps.as_mut(), env.clone()).unwrap();

            assert_eq!(
                response.messages,
                vec![
                    SubMsg::new(CosmosMsg::Distribution(
                        DistributionMsg::WithdrawDelegatorReward {
                            validator: validator1.to_string()
                        }
                    )),
                    SubMsg::new(CosmosMsg::Wasm(cosmwasm_std::WasmMsg::Execute {
                        contract_addr: reward_dispatcher.to_string(),
                        msg: to_json_binary(&DispatchRewards {}).unwrap(),
                        funds: vec![]
                    })),
                ]
            );
            assert_eq!(
                response.attributes,
                vec![attr("action", "update_global_index")]
            );
        }
    }
}
