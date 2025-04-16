use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgUndelegate;
use cosmwasm_std::{
    attr, coins, to_json_binary, BankMsg, CosmosMsg, DepsMut, Env, Event, MessageInfo,
    QueryRequest, Response, Storage, Uint128, WasmMsg, WasmQuery,
};
use cw20::{AllowanceResponse, BalanceResponse, Cw20QueryMsg};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;

use lst_common::{
    babylon_msg::{CosmosAny, MsgWrappedUndelegate},
    delegation::calculate_undelegations,
    errors::HubError,
    hub::{CurrentBatch, State, UnstakeHistory},
    to_checked_address,
    types::{LstResult, ProtoCoin, ResponseType},
    validator::{QueryMsg::ValidatorsDelegation, ValidatorResponse},
    ContractError,
};

use crate::{
    contract::check_slashing,
    math::decimal_multiplication,
    state::{
        get_finished_amount, get_finished_amount_for_batches, get_pending_delegation_amount,
        read_unstake_history, remove_unstake_wait_list, update_pending_delegation_amount,
        update_state, UnstakeType, CONFIG, CURRENT_BATCH, PARAMETERS, STATE, UNSTAKE_HISTORY,
        UNSTAKE_WAIT_LIST,
    },
};

// This method is entry point for the unstaking and records the unstaking request, handle token burning, prepares validator undelegation,
// ensure proper authorization and sufficient funds, and maintains proper state and history
pub(crate) fn execute_unstake(
    mut deps: DepsMut,
    env: Env,
    amount: Uint128,
    sender: String,
    flow: UnstakeType,
) -> LstResult<Response<ResponseType>> {
    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    // Store the unstake request in the current batch
    update_unstake_batch_wait_list(&mut deps, &mut current_batch, sender.clone(), amount)?;

    // Check if unstake batch epoch has completed. If completed, returns undelegation messages
    let (mut messages, events) =
        check_for_unstake_batch_epoch_completion(&mut deps, &env, &mut current_batch)?;

    // send burn message to the token contract
    let config = CONFIG.load(deps.storage)?;
    let lst_token_addr = config.lst_token.ok_or(HubError::LstTokenNotSet)?;

    // Use burn or burn from depending upon the type of unstake
    let burn_msg = match flow {
        UnstakeType::BurnFlow => Cw20ExecuteMsg::Burn { amount },
        UnstakeType::BurnFromFlow => {
            // check if user has sufficient balance
            let balance_response: BalanceResponse = deps.querier.query_wasm_smart(
                &lst_token_addr,
                &Cw20QueryMsg::Balance {
                    address: sender.clone(),
                },
            )?;
            if balance_response.balance < amount {
                return Err(ContractError::Hub(HubError::InsufficientFunds));
            }
            // Query the allowance granted to the contract
            let allowance_response: AllowanceResponse = deps.querier.query_wasm_smart(
                &lst_token_addr,
                &Cw20QueryMsg::Allowance {
                    owner: sender.clone(),
                    spender: env.contract.address.to_string(),
                },
            )?;
            if allowance_response.allowance < amount {
                return Err(ContractError::Hub(HubError::InsufficientAllowance));
            }

            Cw20ExecuteMsg::BurnFrom {
                owner: sender.clone(),
                amount,
            }
        }
    };

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: lst_token_addr.to_string(),
        msg: to_json_binary(&burn_msg)?,
        funds: vec![],
    }));

    let res = Response::new()
        .add_messages(messages)
        .add_events(events)
        .add_attributes(vec![
            attr("action", "burn"),
            attr("from", sender),
            attr("burnt_amount", amount),
            attr("unstaked_amount", amount),
        ]);

    Ok(res)
}

// Checks if it's time to process unstaking requests based on epoch period, handles slashing events and updates exchange rate,
// triggers the undelegation process when needed, maintains proper state and batch information
fn check_for_unstake_batch_epoch_completion(
    deps: &mut DepsMut,
    env: &Env,
    current_batch: &mut CurrentBatch,
) -> LstResult<(Vec<CosmosMsg>, Vec<Event>)> {
    // read parameters
    let params = PARAMETERS.load(deps.storage)?;
    let epoch_period = params.epoch_length;

    let mut state = STATE.load(deps.storage)?;
    let mut events: Vec<Event> = vec![];
    let old_state = state.clone();

    // check if slashing has occurred and update the exchange rate
    let (slashing_events, _) = check_slashing(deps, env, &mut state)?;
    events.extend(slashing_events);

    let current_time = env.block.time.seconds();
    let passed_time = current_time - state.last_unbonded_time;

    let mut messages: Vec<CosmosMsg> = vec![];

    // if the epoch period is passed, the undelegate message would be sent
    if passed_time > epoch_period {
        let mut undelegate_msgs =
            process_undelegations_for_batch(deps, env.clone(), current_batch, &mut state)?;
        messages.append(&mut undelegate_msgs);
    }

    // Store the new requested id in the batch
    CURRENT_BATCH.save(deps.storage, current_batch)?;

    let undelegate_events = update_state(deps.storage, old_state, state)?;
    events.extend(undelegate_events);

    Ok((messages, events))
}

// Provides a way to manually trigger the processing of unstaking requests
// Ensures that unstaking requests are processed even if not triggered by new requests
// Maintains proper state and generates necessary messages
pub fn execute_process_undelegations(mut deps: DepsMut, env: Env) -> LstResult<Response> {
    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    if current_batch.requested_lst_token_amount == Uint128::zero() {
        return Ok(Response::new());
    }

    let (messages, events) =
        check_for_unstake_batch_epoch_completion(&mut deps, &env, &mut current_batch)?;

    let res = Response::new()
        .add_messages(messages)
        .add_events(events)
        .add_attributes(vec![attr(
            "process undelegations",
            current_batch.id.to_string(),
        )]);

    Ok(res)
}

/// Store undelegation wait list per each batch
/// Update the total lst token unstake request as well as for user in the batch
fn update_unstake_batch_wait_list(
    deps: &mut DepsMut,
    current_batch: &mut CurrentBatch,
    sender_address: String,
    amount: Uint128,
) -> LstResult<()> {
    // add the unstaking amount to the current batch
    current_batch.requested_lst_token_amount += amount;

    let checked_sender = to_checked_address(deps.as_ref(), &sender_address)?;

    // Check if there's an existing amount for this batch
    let existing_amount =
        UNSTAKE_WAIT_LIST.may_load(deps.storage, (checked_sender.clone(), current_batch.id))?;

    let new_amount = match existing_amount {
        Some(current_amount) => current_amount + amount,
        None => amount,
    };

    // Save the amount for this batch
    UNSTAKE_WAIT_LIST.save(
        deps.storage,
        (checked_sender, current_batch.id),
        &new_amount,
    )?;

    Ok(())
}

// Users request to unstake their tokens. The contract collects these requests into batches.
// When processing a batch: calculates actual amounts to undelegate, creates messages to undelegate from validators,
// records the history for future withdrawal
fn process_undelegations_for_batch(
    deps: &mut DepsMut,
    env: Env,
    current_batch: &mut CurrentBatch,
    state: &mut State,
) -> LstResult<Vec<CosmosMsg>> {
    // Apply the current exchange rate
    let unstaked_amount_in_batch = decimal_multiplication(
        current_batch.requested_lst_token_amount,
        state.lst_exchange_rate,
    );

    // send undelegate requests to possibly more than one validators
    let undelegate_msgs =
        pick_validator_for_undelegation(deps, env.clone(), unstaked_amount_in_batch)?;

    state.total_staked_amount = state
        .total_staked_amount
        .checked_sub(unstaked_amount_in_batch)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;
    update_pending_delegation_amount(deps, &env, None, Some(unstaked_amount_in_batch))?;

    // Store history for withdraw unstaked
    let history = UnstakeHistory {
        batch_id: current_batch.id,
        time: env.block.time.seconds(),
        lst_token_amount: current_batch.requested_lst_token_amount,
        lst_applied_exchange_rate: state.lst_exchange_rate,
        lst_withdraw_rate: state.lst_exchange_rate,
        released: false,
    };

    UNSTAKE_HISTORY.save(deps.storage, current_batch.id, &history)?;

    // batch info must be updated to new batch
    current_batch.id += 1;
    current_batch.requested_lst_token_amount = Uint128::zero();

    // last unstaked time must be updated to the current block time
    state.last_unbonded_time = env.block.time.seconds();

    Ok(undelegate_msgs)
}

// Selects and distributes the undelegation amount across validators
// Determines which validators to undelegate from
// Creates the necessary messages for the actual undelegation
fn pick_validator_for_undelegation(
    deps: &mut DepsMut,
    env: Env,
    claim: Uint128,
) -> LstResult<Vec<CosmosMsg>> {
    let params = PARAMETERS.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let staking_coin_denom = params.staking_coin_denom;

    let mut messages: Vec<CosmosMsg> = vec![];

    let delegator_address = env.contract.address;

    let validators_registry_contract = config
        .validators_registry_contract
        .ok_or(HubError::ValidatorRegistryNotSet)?;

    let validators: Vec<ValidatorResponse> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: validators_registry_contract.to_string(),
            msg: to_json_binary(&ValidatorsDelegation {})?,
        }))?;

    let undelegations = calculate_undelegations(claim, validators.clone())?;

    for (index, undelegated_amount) in undelegations.iter().enumerate() {
        if undelegated_amount.is_zero() {
            continue;
        }

        let msg = prepare_wrapped_undelegate_msg(
            staking_coin_denom.clone(),
            undelegated_amount.to_string(),
            delegator_address.to_string(),
            validators[index].address.to_string(),
        );

        messages.push(msg);
    }

    Ok(messages)
}

// This method is used to process the unstake requests that have already passed the unstaking period
// Anyone can call this method to process the unstake requests
pub fn execute_process_withdraw_requests(mut deps: DepsMut, env: Env) -> LstResult<Response> {
    let params = PARAMETERS.load(deps.storage)?;
    let unstake_cutoff_time = env.block.time.seconds() - params.unstaking_period;

    // Get hub balance
    let hub_balance = deps
        .querier
        .query_balance(&env.contract.address, &*params.staking_coin_denom)?
        .amount;
    let (pending_staking_amount, _) = get_pending_delegation_amount(deps.as_ref(), &env)?;
    let actual_free_balance = hub_balance
        .checked_sub(pending_staking_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    let (events, left_over_unstaked_amount) =
        process_withdraw_rate(&mut deps, unstake_cutoff_time, actual_free_balance)?;

    // This should be done whenever we release the funds for unstake withdraw claims
    let unclaimed_unstaked_balance = actual_free_balance
        .checked_sub(left_over_unstaked_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    STATE.update(deps.storage, |mut state| -> LstResult<_> {
        state.unclaimed_unstaked_balance = unclaimed_unstaked_balance;
        Ok(state)
    })?;

    Ok(Response::new()
        .add_events(events)
        .add_attributes(vec![attr("action", "process_withdraw_requests")]))
}

// This is designed for an accurate unstaked amount calculation. Execute while processing withdraw_unstaked
// Handles the calculation and update of withdrawal rates after slashing events
// Entire free hub balance is considered as unstaked amount. This makes an assumption that the hub balance is not slashed and
// and the hub balanced is only used to fulfill the unstake requests. Any amount sent to the hub is also used to fulfill the unstake requests.
fn process_withdraw_rate(
    deps: &mut DepsMut,
    unstake_cutoff_time: u64,
    hub_balance: Uint128,
) -> LstResult<(Vec<Event>, Uint128)> {
    let mut state = STATE.load(deps.storage)?;
    let old_state = state.clone();

    let total_unstaked_amount = hub_balance
        .checked_sub(state.unclaimed_unstaked_balance)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    // Get all unprocessed histories
    let (histories, left_over_unstaked_amount) = get_unprocessed_histories(
        deps.storage,
        state.last_processed_batch,
        unstake_cutoff_time,
        total_unstaked_amount,
    )?;

    if histories.is_empty() {
        return Ok((vec![], left_over_unstaked_amount));
    }

    // Process each history record
    for (batch_id, history) in histories {
        let mut unstake_history_batch = history;
        unstake_history_batch.released = true;
        UNSTAKE_HISTORY.save(deps.storage, batch_id, &unstake_history_batch)?;
        state.last_processed_batch = batch_id;
    }

    let withdraw_rate_events = update_state(deps.storage, old_state, state)?;
    Ok((withdraw_rate_events, left_over_unstaked_amount))
}

// Helper function to get unprocessed histories
// Only return the histories for which the unstaking cutoff time has passed, haven't been released yet and exists in storage
// Also checks if contract has sufficient balance to release the funds
fn get_unprocessed_histories(
    storage: &dyn Storage,
    start_batch: u64,
    unstake_cutoff_time: u64,
    contract_balance: Uint128,
) -> LstResult<(Vec<(u64, UnstakeHistory)>, Uint128)> {
    let mut histories = Vec::new();
    let mut batch_id = start_batch + 1;
    let mut remaining_balance = contract_balance;

    loop {
        match read_unstake_history(storage, batch_id) {
            Ok(h) => {
                if h.time > unstake_cutoff_time {
                    break;
                }
                if !h.released {
                    // Check if we have enough balance to release this batch
                    let required_amount =
                        decimal_multiplication(h.lst_token_amount, h.lst_withdraw_rate);
                    if remaining_balance >= required_amount {
                        histories.push((batch_id, h));
                        remaining_balance = remaining_balance
                            .checked_sub(required_amount)
                            .map_err(|e| ContractError::Overflow(e.to_string()))?;
                    } else {
                        // If we don't have enough balance, stop processing
                        break;
                    }
                } else {
                    break;
                }
            }
            Err(_) => break,
        }
        batch_id += 1;
    }

    Ok((histories, remaining_balance))
}

// Process the withdrawal of unstaked tokens by users
// This method allows users to withdraw their unstaked tokens after the unstaking period has elapsed. It handles the calculation of
// withdrawable amounts, updates the state, and send the tokens to the user.
pub fn execute_withdraw_unstaked(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> LstResult<Response<ResponseType>> {
    execute_withdraw_unstaked_impl(deps, env, info, None)
}

// Process the withdrawal of unstaked tokens by users for specific batch IDs
// This method allows users to withdraw their unstaked tokens after the unstaking period has elapsed for specific batches.
// It handles the calculation of withdrawable amounts, updates the state, and send the tokens to the user.
pub fn execute_withdraw_unstaked_for_batches(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    batch_ids: Vec<u64>,
) -> LstResult<Response<ResponseType>> {
    execute_withdraw_unstaked_impl(deps, env, info, Some(batch_ids))
}

// Internal implementation of withdraw unstaked functionality
fn execute_withdraw_unstaked_impl(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    batch_ids: Option<Vec<u64>>,
) -> LstResult<Response<ResponseType>> {
    // Early parameter loading
    let params = PARAMETERS.load(deps.storage)?;
    let unstake_cutoff_time = env.block.time.seconds() - params.unstaking_period;

    // Get hub balance
    let hub_balance = deps
        .querier
        .query_balance(&env.contract.address, &*params.staking_coin_denom)?
        .amount;

    let (pending_staking_amount, _) = get_pending_delegation_amount(deps.as_ref(), &env)?;
    let actual_free_balance = hub_balance
        .checked_sub(pending_staking_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    // Process withdrawal rate first (MUST be before get_finished_amount)
    let (rate_update_events, left_over_unstaked_amount) =
        process_withdraw_rate(&mut deps, unstake_cutoff_time, actual_free_balance)?;

    // Get withdrawable amount after rates are updated
    let (withdraw_amount, deprecated_batches) = match batch_ids {
        Some(ids) => get_finished_amount_for_batches(deps.storage, info.sender.clone(), ids)?,
        None => get_finished_amount(deps.storage, info.sender.clone())?,
    };

    // Early validation
    if withdraw_amount.is_zero() {
        return Err(lst_common::ContractError::Hub(
            HubError::NoWithdrawableAssets,
        ));
    }

    // Clean up and state update
    remove_unstake_wait_list(deps.storage, deprecated_batches, info.sender.clone())?;

    // This should be done whenever we release the funds for unstake withdraw claims
    let unclaimed_unstaked_balance = actual_free_balance
        .checked_sub(withdraw_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?
        .checked_sub(left_over_unstaked_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    STATE.update(deps.storage, |mut state| -> LstResult<_> {
        state.unclaimed_unstaked_balance = unclaimed_unstaked_balance;
        Ok(state)
    })?;

    // Create response
    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: coins(withdraw_amount.u128(), &*params.staking_coin_denom),
        })
        .add_events(rate_update_events)
        .add_attributes(vec![
            attr("action", "finish_burn"),
            attr("from", env.contract.address),
            attr("amount", withdraw_amount),
        ]))
}

// Prepare the custom wrapped undelegate message
fn prepare_wrapped_undelegate_msg(
    denom: String,
    amount: String,
    delegator_address: String,
    validator_address: String,
) -> CosmosMsg {
    let coin = ProtoCoin { denom, amount };

    let undelegate_msg = MsgUndelegate {
        delegator_address,
        validator_address,
        amount: Some(coin),
    };

    MsgWrappedUndelegate {
        msg: Some(undelegate_msg),
    }
    .to_any()
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        attr, from_json,
        testing::{message_info, mock_dependencies, mock_env},
        to_json_binary, ContractResult, CosmosMsg, Response, SubMsg, SystemResult, Uint128,
        WasmMsg, WasmQuery,
    };
    use cw20::{AllowanceResponse, BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
    use lst_common::{
        errors::HubError,
        hub::{CurrentBatch, InstantiateMsg},
        ContractError,
    };

    use crate::{
        config::execute_update_config,
        instantiate,
        state::{UnstakeType, CURRENT_BATCH},
        unstake::{execute_process_withdraw_requests, execute_withdraw_unstaked_for_batches},
    };

    use super::{execute_process_undelegations, execute_unstake, execute_withdraw_unstaked};

    #[test]
    fn test_execute_unstake() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let owner = deps.api.addr_make("owner");
        let denom = "denom";
        let amount = Uint128::new(100);
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
            let err = execute_unstake(
                deps.as_mut(),
                env.clone(),
                amount,
                owner.to_string(),
                UnstakeType::BurnFlow,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::Hub(HubError::LstTokenNotSet));
        }

        let reward_dispatcher = deps.api.addr_make("reward_dispatcher");
        let validator_registry = deps.api.addr_make("validator_registry");
        let lst_token = deps.api.addr_make("lst_token");

        // update config successfully
        {
            execute_update_config(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                None,
                Some(lst_token.to_string()),
                Some(validator_registry.to_string()),
                Some(reward_dispatcher.to_string()),
            )
            .unwrap();
        }

        // unstake successfully: UnstakeType::BurnFlow
        {
            let response = execute_unstake(
                deps.as_mut(),
                env.clone(),
                amount,
                owner.to_string(),
                UnstakeType::BurnFlow,
            )
            .unwrap();

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: lst_token.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::Burn { amount }).unwrap(),
                    funds: vec![]
                }))]
            );

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "burn"),
                    attr("from", owner.to_string()),
                    attr("burnt_amount", "100"),
                    attr("unstaked_amount", "100"),
                ]
            );
        }

        // unstake error: InsufficientFunds
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: Cw20QueryMsg = from_json(msg).unwrap();
                    match msg {
                        Cw20QueryMsg::Balance { address: _ } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_json_binary(&BalanceResponse {
                                    balance: Uint128::new(10),
                                })
                                .unwrap(),
                            ))
                        }
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::Unknown {}),
            });

            let err = execute_unstake(
                deps.as_mut(),
                env.clone(),
                amount,
                owner.to_string(),
                UnstakeType::BurnFromFlow,
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InsufficientFunds));
        }

        // unstake error: InsufficientAllowance
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: Cw20QueryMsg = from_json(msg).unwrap();
                    match msg {
                        Cw20QueryMsg::Balance { address: _ } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_json_binary(&BalanceResponse {
                                    balance: Uint128::new(1000),
                                })
                                .unwrap(),
                            ))
                        }
                        Cw20QueryMsg::Allowance {
                            owner: _,
                            spender: _,
                        } => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&AllowanceResponse {
                                allowance: Uint128::new(10),
                                expires: cw20::Expiration::Never {},
                            })
                            .unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::Unknown {}),
            });

            let err = execute_unstake(
                deps.as_mut(),
                env.clone(),
                amount,
                owner.to_string(),
                UnstakeType::BurnFromFlow,
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::InsufficientAllowance));
        }

        // unstake successfully: UnstakeType::BurnFlow
        {
            deps.querier.update_wasm(move |query| match query {
                WasmQuery::Smart {
                    contract_addr: _,
                    msg,
                } => {
                    let msg: Cw20QueryMsg = from_json(msg).unwrap();
                    match msg {
                        Cw20QueryMsg::Balance { address: _ } => {
                            SystemResult::Ok(ContractResult::Ok(
                                to_json_binary(&BalanceResponse {
                                    balance: Uint128::new(1000),
                                })
                                .unwrap(),
                            ))
                        }
                        Cw20QueryMsg::Allowance {
                            owner: _,
                            spender: _,
                        } => SystemResult::Ok(ContractResult::Ok(
                            to_json_binary(&AllowanceResponse {
                                allowance: Uint128::new(1000),
                                expires: cw20::Expiration::Never {},
                            })
                            .unwrap(),
                        )),
                        _ => panic!("unexpected query"),
                    }
                }
                _ => SystemResult::Err(cosmwasm_std::SystemError::Unknown {}),
            });

            let response = execute_unstake(
                deps.as_mut(),
                env.clone(),
                amount,
                owner.to_string(),
                UnstakeType::BurnFromFlow,
            )
            .unwrap();

            assert_eq!(
                response.messages,
                vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: lst_token.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::BurnFrom {
                        owner: owner.to_string(),
                        amount,
                    })
                    .unwrap(),
                    funds: vec![]
                }))]
            );

            assert_eq!(
                response.attributes,
                vec![
                    attr("action", "burn"),
                    attr("from", owner.to_string()),
                    attr("burnt_amount", "100"),
                    attr("unstaked_amount", "100"),
                ]
            );
        }
    }

    #[test]
    fn test_execute_withdraw_unstaked() {
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

        // NoWithdrawableAssets error
        {
            let err =
                execute_withdraw_unstaked(deps.as_mut(), env.clone(), info.clone()).unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::NoWithdrawableAssets));
        }
    }

    #[test]
    fn test_execute_withdraw_unstaked_for_batches() {
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

        // NoWithdrawableAssets error
        {
            let err = execute_withdraw_unstaked_for_batches(
                deps.as_mut(),
                env.clone(),
                info.clone(),
                vec![1],
            )
            .unwrap_err();

            assert_eq!(err, ContractError::Hub(HubError::NoWithdrawableAssets));
        }
    }

    #[test]
    fn test_execute_process_undelegations() {
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

        // execute process undelegations successfully: requested_lst_token_amount is 0
        {
            let response = execute_process_undelegations(deps.as_mut(), env.clone()).unwrap();
            assert_eq!(response, Response::new());
        }

        // execute process undelegations successfully
        {
            let current_batch = CurrentBatch {
                id: 1,
                requested_lst_token_amount: Uint128::new(100),
            };
            CURRENT_BATCH
                .save(deps.as_mut().storage, &current_batch)
                .unwrap();

            let response = execute_process_undelegations(deps.as_mut(), env.clone()).unwrap();
            assert_eq!(
                response.attributes,
                vec![attr("process undelegations", "1")]
            );
        }
    }

    #[test]
    fn test_execute_process_withdraw_requests() {
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

        // execute process undelegations successfully: requested_lst_token_amount is 0
        {
            let response = execute_process_withdraw_requests(deps.as_mut(), env.clone()).unwrap();
            assert_eq!(
                response.attributes,
                vec![attr("action", "process_withdraw_requests")]
            );
        }
    }
}
