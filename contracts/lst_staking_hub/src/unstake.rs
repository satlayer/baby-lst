use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgUndelegate;
use cosmwasm_std::{
    attr, coins, to_json_binary, BankMsg, CosmosMsg, Decimal, Decimal256, DecimalRangeExceeded,
    DepsMut, Env, MessageInfo, Response, Storage, Uint128, Uint256, WasmMsg,
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
    validator::ValidatorResponse,
    ContractError, SignedInt,
};

use crate::{
    contract::check_slashing,
    math::{decimal_multiplication, decimal_multiplication_256},
    state::{
        get_finished_amount, read_unstake_history, remove_unstake_wait_list, UnstakeType, CONFIG,
        CURRENT_BATCH, PARAMETERS, STATE, UNSTAKE_HISTORY, UNSTAKE_WAIT_LIST,
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
    let mut messages =
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
                return Err(ContractError::Hub(HubError::InvalidAmount));
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
                return Err(ContractError::Hub(HubError::InvalidAmount));
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

    let res = Response::new().add_messages(messages).add_attributes(vec![
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
) -> LstResult<Vec<CosmosMsg>> {
    // read parameters
    let params = PARAMETERS.load(deps.storage)?;
    let epoch_period = params.epoch_length;

    // check if slashing has occurred and update the exchange rate
    let mut state = check_slashing(deps, &env)?;

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

    // Store state's new exchange rate
    STATE.save(deps.storage, &state)?;

    Ok(messages)
}

// Provides a way to manually trigger the processing of unstaking requests
// Ensures that unstaking requests are processed even if not triggered by new requests
// Maintains proper state and generates necessary messages
pub fn execute_process_undelegations(mut deps: DepsMut, env: Env) -> LstResult<Response> {
    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    let messages = check_for_unstake_batch_epoch_completion(&mut deps, &env, &mut current_batch)?;

    let res = Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr(
            "process undelegations",
            current_batch.id.to_string(),
        )]);

    Ok(res)
}

/// Store undelegation wait list per each batch
/// HashMap<user's address, <batch_id, requested_amount>
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
    let staking_coin_denom = params.staking_coin_denom;

    let mut messages: Vec<CosmosMsg> = vec![];

    let delegator_address = env.contract.address;
    let all_delegations = deps.querier.query_all_delegations(&delegator_address)?;

    let mut validators = all_delegations
        .iter()
        .map(|d| ValidatorResponse {
            total_delegated: d.amount.amount,
            address: d.validator.to_string(),
        })
        .collect::<Vec<ValidatorResponse>>();

    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

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

    process_withdraw_rate(&mut deps, unstake_cutoff_time, hub_balance)?;

    Ok(Response::new())
}

// This is designed for an accurate unstaked amount calculation. Execute while processing withdraw_unstaked
// Handles the calculation and update of withdrawal rates after slashing events
fn process_withdraw_rate(
    deps: &mut DepsMut,
    unstake_cutoff_time: u64,
    hub_balance: Uint128,
) -> LstResult<()> {
    let mut state = STATE.load(deps.storage)?;

    // Get all unprocessed histories
    let histories = get_unprocessed_histories(
        deps.storage,
        state.last_processed_batch,
        unstake_cutoff_time,
    )?;

    if histories.is_empty() {
        return Ok(());
    }

    // Calculate total unstaked amount
    let total_unstaked_amount = calculate_newly_added_unstaked_amount(histories.clone());

    let balance_change = SignedInt::from_subtraction(hub_balance, state.prev_hub_balance);
    let actual_unstaked_amount = balance_change.0;

    let slashed_amount =
        SignedInt::from_subtraction(total_unstaked_amount, Uint256::from(actual_unstaked_amount));

    // Process each history record
    for (batch_id, history) in histories {
        let new_withdraw_rate = calculate_new_withdraw_rate(
            history.lst_token_amount,
            history.lst_withdraw_rate,
            total_unstaked_amount,
            slashed_amount,
        );

        let mut unstake_history_batch = history;
        unstake_history_batch.lst_applied_exchange_rate = new_withdraw_rate.unwrap();
        unstake_history_batch.released = true;
        UNSTAKE_HISTORY.save(deps.storage, batch_id, &unstake_history_batch)?;
        state.last_processed_batch = batch_id;
    }

    STATE.save(deps.storage, &state)?;
    Ok(())
}

// Helper function to get unprocessed histories
// Only return the histories for which the unstaking cutoff time has passed, haven't been released yet and exists in storage
fn get_unprocessed_histories(
    storage: &dyn Storage,
    start_batch: u64,
    unstake_cutoff_time: u64,
) -> LstResult<Vec<(u64, UnstakeHistory)>> {
    let mut histories = Vec::new();
    let mut batch_id = start_batch + 1;

    loop {
        match read_unstake_history(storage, batch_id) {
            Ok(h) => {
                if h.time > unstake_cutoff_time {
                    break;
                }
                if !h.released {
                    histories.push((batch_id, h));
                } else {
                    break;
                }
            }
            Err(_) => break,
        }
        batch_id += 1;
    }

    Ok(histories)
}

// Sums up actual unstaked amount using the burnt lst amount and withdraw rate at time of unstaking
// After slashing, the amount and rate is updated
fn calculate_newly_added_unstaked_amount(histories: Vec<(u64, UnstakeHistory)>) -> Uint256 {
    let total_unstaked_amount = histories.iter().fold(Uint256::zero(), |acc, (_, history)| {
        let lst_burnt_amount = Uint256::from(history.lst_token_amount);
        let lst_historical_rate = Decimal256::from(history.lst_withdraw_rate);
        let lst_unstaked_amount = decimal_multiplication_256(lst_burnt_amount, lst_historical_rate);
        acc + lst_unstaked_amount
    });

    total_unstaked_amount
}

// Handle slashing events and adjusting withdrawal rates
// This method calculates a new withdrawal rate for a batch of unstaked tokens after a slashing event has occured.
// It ensures that the impact of slashing is properly distributed across different batches of unstaked tokens.
fn calculate_new_withdraw_rate(
    total_lst_burnt: Uint128,
    withdraw_rate: Decimal,
    total_unstaked_amount: Uint256,
    slashed_amount: SignedInt,
) -> LstResult<Decimal> {
    if total_lst_burnt.is_zero() || total_unstaked_amount.is_zero() || slashed_amount.0.is_zero() {
        return Ok(withdraw_rate);
    }

    // calculate the unstaked amount of the batch using the withdraw rate
    let burnt_amount_of_batch = Uint256::from(total_lst_burnt);
    let historical_rate_of_batch = Decimal256::from(withdraw_rate);
    let unstaked_amount_of_batch =
        decimal_multiplication_256(burnt_amount_of_batch, historical_rate_of_batch);

    // Calculate batch weight and slashed amount in one step
    let batch_slashing_weight =
        Decimal256::from_ratio(unstaked_amount_of_batch, total_unstaked_amount);
    let slashed_amount_of_batch =
        decimal_multiplication_256(Uint256::from(slashed_amount.0), batch_slashing_weight);

    // Handle slashing adjustment based on direction
    let actual_unstaked_amount_of_batch = if slashed_amount.1 {
        // Negative slashing: add to unstaked amount
        unstaked_amount_of_batch + slashed_amount_of_batch
    } else {
        // Positive slashing: subtract from unstaked amount
        unstaked_amount_of_batch
            .checked_sub(slashed_amount_of_batch)
            .unwrap_or(Uint256::zero())
    };

    // Calculate and return new rate
    Ok(
        Decimal256::from_ratio(actual_unstaked_amount_of_batch, burnt_amount_of_batch)
            .try_into()
            .map_err(|e: DecimalRangeExceeded| ContractError::Overflow(e.to_string()))?,
    )
}

// Process the withdrawal of unstaked tokens by users
// This method allows users to withdraw their unstaked tokens after the unstaking period has elapsed. It handles the calculation of
// withdrawable amounts, updates the state, and send the tokens to the user.
pub fn execute_withdraw_unstaked(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> LstResult<Response<ResponseType>> {
    // Early parameter loading
    let params = PARAMETERS.load(deps.storage)?;
    let unstake_cutoff_time = env.block.time.seconds() - params.unstaking_period;

    // Get hub balance
    let hub_balance = deps
        .querier
        .query_balance(&env.contract.address, &*params.staking_coin_denom)?
        .amount;

    // Process withdrawal rate first (MUST be before get_finished_amount)
    process_withdraw_rate(&mut deps, unstake_cutoff_time, hub_balance)?;

    // Get withdrawable amount after rates are updated
    let (withdraw_amount, deprecated_batches) =
        get_finished_amount(deps.storage, info.sender.clone())?;

    // Early validation
    if withdraw_amount.is_zero() {
        return Err(lst_common::ContractError::Hub(
            HubError::NoWithdrawableAssets,
        ));
    }

    // Clean up and state update
    remove_unstake_wait_list(deps.storage, deprecated_batches, info.sender.clone())?;

    let prev_balance = hub_balance
        .checked_sub(withdraw_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    STATE.update(deps.storage, |mut state| -> LstResult<_> {
        state.prev_hub_balance = prev_balance;
        Ok(state)
    })?;

    // Create response
    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: coins(withdraw_amount.u128(), &*params.staking_coin_denom),
        })
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
