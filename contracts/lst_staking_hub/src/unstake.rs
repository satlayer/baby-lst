use cosmos_sdk_proto::{
    cosmos::base::v1beta1::Coin as ProtoCoin, cosmos::staking::v1beta1::MsgUndelegate,
    traits::MessageExt,
};
use cosmwasm_std::{
    attr, coins, to_json_binary, Addr, AnyMsg, BankMsg, Binary, CosmosMsg, Decimal, Decimal256,
    DepsMut, Env, MessageInfo, Response, StdError, Storage, Uint128, Uint256, WasmMsg,
};
use cw20::{AllowanceResponse, BalanceResponse, Cw20QueryMsg};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;

use lst_common::{
    babylon::epoching::v1::MsgWrappedUndelegate, delegation::calculate_undelegations,
    errors::HubError, hub::CurrentBatch, hub::State, to_checked_address, types::LstResult,
    validator::ValidatorResponse, ContractError, SignedInt,
};

use crate::{
    contract::check_slashing,
    math::{decimal_multiplication, decimal_multiplication_256},
    state::{
        get_finished_amount, read_unstake_history, remove_unstake_wait_list, UnStakeHistory,
        UnstakeType, CONFIG, CURRENT_BATCH, PARAMETERS, STATE, UNSTAKE_HISTORY, UNSTAKE_WAIT_LIST,
    },
};

fn check_for_undelegations(
    deps: &mut DepsMut,
    env: &Env,
    current_batch: &mut CurrentBatch,
) -> LstResult<Vec<CosmosMsg>> {
    // read parameters
    let params = PARAMETERS.load(deps.storage)?;
    let epoch_period = params.epoch_length;

    // check if slashing has occurred and update the exchange rate
    let mut state = check_slashing(deps, env.clone())?;

    let current_time = env.block.time.seconds();
    let passed_time = current_time - state.last_unbonded_time;

    let mut messages: Vec<CosmosMsg> = vec![];

    // if the epoch period is passed, the undelegate message would be sent
    if passed_time > epoch_period {
        let mut undelegate_msgs =
            process_undelegations(deps, env.clone(), current_batch, &mut state)?;
        messages.append(&mut undelegate_msgs);
    }

    // Store the new requested id in the batch
    CURRENT_BATCH.save(deps.storage, current_batch)?;

    // Store state's new exchange rate
    STATE.save(deps.storage, &state)?;

    Ok(messages)
}

pub(crate) fn execute_unstake(
    mut deps: DepsMut,
    env: Env,
    amount: Uint128,
    sender: String,
    flow: UnstakeType,
) -> LstResult<Response> {
    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    // add the unstaking amount to the current batch
    current_batch.requested_lst_token_amount += amount;

    let checked_sender = to_checked_address(deps.as_ref(), &sender)?;
    store_unstake_wait_list(deps.storage, current_batch.id, checked_sender, amount)?;

    let mut messages = check_for_undelegations(&mut deps, &env, &mut current_batch)?;

    // send burn message to the token contract
    let config = CONFIG.load(deps.storage)?;
    let lst_token_addr = config.lst_token.ok_or(HubError::LstTokenNotSet)?;

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

            //Query the allowance granted to the contract
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

pub fn execute_process_undelegations(mut deps: DepsMut, env: Env) -> LstResult<Response> {
    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    let messages = check_for_undelegations(&mut deps, &env, &mut current_batch)?;

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
fn store_unstake_wait_list(
    storage: &mut dyn Storage,
    batch_id: u64,
    sender_address: Addr,
    amount: Uint128,
) -> LstResult<()> {
    // Check if there's an existing amount for this batch
    let existing_amount =
        UNSTAKE_WAIT_LIST.may_load(storage, (sender_address.clone(), batch_id))?;

    let new_amount = match existing_amount {
        Some(current_amount) => current_amount + amount,
        None => amount,
    };

    // Save the amount for this batch
    UNSTAKE_WAIT_LIST.save(storage, (sender_address, batch_id), &new_amount)?;

    Ok(())
}

fn process_undelegations(
    deps: &mut DepsMut,
    env: Env,
    current_batch: &mut CurrentBatch,
    state: &mut State,
) -> LstResult<Vec<CosmosMsg>> {
    // Apply the current exchange rate
    let lst_undelegation_amount = decimal_multiplication(
        current_batch.requested_lst_token_amount,
        state.lst_exchange_rate,
    );

    // send undelegate requests to possibly more than one validators
    let undelegate_msgs = pick_validator(deps, env.clone(), lst_undelegation_amount)?;

    state.total_lst_token_amount = state
        .total_lst_token_amount
        .checked_sub(lst_undelegation_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

    // Store history for withdraw unstaked
    let history = UnStakeHistory {
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

fn pick_validator(deps: &mut DepsMut, env: Env, claim: Uint128) -> LstResult<Vec<CosmosMsg>> {
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
        )?;

        messages.push(msg);
    }

    Ok(messages)
}

// This is designed for an accurate unstaked amount calculation
// Execute while processing withdraw_unstaked
fn process_withdraw_rate(
    deps: &mut DepsMut,
    historical_time: u64,
    hub_balance: Uint128,
) -> LstResult<()> {
    let mut state = STATE.load(deps.storage)?;

    let last_processed_batch = state.last_processed_batch;

    let (lst_total_unstaked_amount, batch_count) =
        calculate_newly_added_unstaked_amount(deps.storage, last_processed_batch, historical_time);

    if batch_count < 1 {
        return Ok(());
    }

    let balance_change = SignedInt::from_subtraction(hub_balance, state.prev_hub_balance);
    let actual_unstaked_amount = balance_change.0;

    let lst_slashed_amount = SignedInt::from_subtraction(
        lst_total_unstaked_amount,
        Uint256::from(actual_unstaked_amount),
    );

    let mut iterator = last_processed_batch + 1;
    loop {
        let history: UnStakeHistory;
        match read_unstake_history(deps.storage, iterator) {
            Ok(h) => {
                if h.time > historical_time {
                    break;
                }
                if !h.released {
                    history = h
                } else {
                    break;
                }
            }
            Err(_) => break,
        }

        let lst_new_withdraw_rate = calculate_new_withdraw_rate(
            history.lst_token_amount,
            history.lst_withdraw_rate,
            lst_total_unstaked_amount,
            lst_slashed_amount,
        );

        let mut history_for_i = history;
        // store the history and mark it as released
        history_for_i.lst_withdraw_rate = lst_new_withdraw_rate;
        history_for_i.released = true;
        UNSTAKE_HISTORY.save(deps.storage, iterator, &history_for_i)?;
        state.last_processed_batch = iterator;
        iterator += 1;
    }

    STATE.save(deps.storage, &state)?;

    Ok(())
}

fn calculate_newly_added_unstaked_amount(
    storage: &mut dyn Storage,
    last_processed_batch: u64,
    historical_time: u64,
) -> (Uint256, u64) {
    let mut lst_total_unstaked_amount = Uint256::zero();
    let mut batch_count: u64 = 0;

    // Iterate over unstaked histories that have been processed
    // to calculate the newly added unstaked amount
    let mut i = last_processed_batch + 1;
    loop {
        let history: UnStakeHistory;
        match read_unstake_history(storage, i) {
            Ok(h) => {
                if h.time > historical_time {
                    break;
                }
                if !h.released {
                    history = h.clone();
                } else {
                    break;
                }
            }
            Err(_) => break,
        }

        let lst_burnt_amount = Uint256::from(history.lst_token_amount);
        let lst_historical_rate = Decimal256::from(history.lst_withdraw_rate);
        let lst_unstaked_amount = decimal_multiplication_256(lst_burnt_amount, lst_historical_rate);

        lst_total_unstaked_amount += lst_unstaked_amount;
        batch_count += 1;
        i += 1;
    }

    (lst_total_unstaked_amount, batch_count)
}

fn calculate_new_withdraw_rate(
    amount: Uint128,
    withdraw_rate: Decimal,
    total_unstaked_amount: Uint256,
    slashed_amount: SignedInt,
) -> Decimal {
    let burnt_amount_of_batch = Uint256::from(amount);
    let historical_rate_of_batch = Decimal256::from(withdraw_rate);
    let unstaked_amount_of_batch =
        decimal_multiplication_256(burnt_amount_of_batch, historical_rate_of_batch);

    let batch_slashing_weight = if total_unstaked_amount != Uint256::zero() {
        Decimal256::from_ratio(unstaked_amount_of_batch, total_unstaked_amount)
    } else {
        Decimal256::zero()
    };

    let mut slashed_amount_of_batch =
        decimal_multiplication_256(Uint256::from(slashed_amount.0), batch_slashing_weight);

    let actual_unstaked_amount_of_batch: Uint256;

    // If slashed amount is negative, there should be summation instead of subtraction
    if slashed_amount.1 {
        slashed_amount_of_batch = if slashed_amount_of_batch > Uint256::one() {
            slashed_amount_of_batch - Uint256::one()
        } else {
            Uint256::zero()
        };
        actual_unstaked_amount_of_batch = unstaked_amount_of_batch + slashed_amount_of_batch;
    } else {
        if slashed_amount.0.u128() != 0u128 {
            slashed_amount_of_batch += Uint256::one();
        }
        actual_unstaked_amount_of_batch = Uint256::from(
            SignedInt::from_subtraction(unstaked_amount_of_batch, slashed_amount_of_batch).0,
        );
    }

    if burnt_amount_of_batch != Uint256::zero() {
        Decimal256::from_ratio(actual_unstaked_amount_of_batch, burnt_amount_of_batch)
            .try_into()
            .unwrap()
    } else {
        withdraw_rate
    }
}

pub fn execute_withdraw_unstaked(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> LstResult<Response> {
    let sender_human = info.sender;
    let contract_address = env.contract.address.clone();

    // read params
    let params = PARAMETERS.load(deps.storage)?;
    let unstaking_period = params.unstaking_period;
    let staking_coin_denom = params.staking_coin_denom;

    let historical_time = env.block.time.seconds() - unstaking_period;

    // query hub balance for process withdraw rate
    let hub_balance = deps
        .querier
        .query_balance(&contract_address, &*staking_coin_denom)?
        .amount;

    process_withdraw_rate(&mut deps, historical_time, hub_balance)?;

    let (withdraw_amount, deprecated_batches) =
        get_finished_amount(deps.storage, sender_human.clone())?;

    if withdraw_amount.is_zero() {
        return Err(lst_common::ContractError::Hub(
            HubError::NoWithdrawableAssets,
        ));
    }

    // remove the previous batches for the user
    remove_unstake_wait_list(deps.storage, deprecated_batches, sender_human.clone())?;
    // Update previous balance used for calculation in next staking token batch release
    let prev_balance = hub_balance
        .checked_sub(withdraw_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;
    STATE.update(deps.storage, |mut last_state| -> LstResult<_> {
        last_state.prev_hub_balance = prev_balance;
        Ok(last_state)
    })?;

    // Send the money to the user
    let msgs: Vec<CosmosMsg> = vec![BankMsg::Send {
        to_address: sender_human.to_string(),
        amount: coins(withdraw_amount.u128(), &*staking_coin_denom),
    }
    .into()];

    let res = Response::new().add_messages(msgs).add_attributes(vec![
        attr("action", "finish_burn"),
        attr("from", contract_address),
        attr("amount", withdraw_amount),
    ]);
    Ok(res)
}

fn prepare_wrapped_undelegate_msg(
    denom: String,
    amount: String,
    delegator_address: String,
    validator_address: String,
) -> LstResult<CosmosMsg> {
    let coin = ProtoCoin { denom, amount };

    let undelegate_msg = MsgUndelegate {
        delegator_address,
        validator_address,
        amount: Some(coin),
    };

    let bytes = MsgWrappedUndelegate {
        msg: Some(undelegate_msg),
    }
    .to_bytes()
    .map_err(|_| StdError::generic_err("Failed to serialize MsgWrappedUndelegate"))?;

    let msg: CosmosMsg = CosmosMsg::Any(AnyMsg {
        type_url: "/babylon.epoching.v1.MsgWrappedUndelegate".to_string(),
        value: Binary::from(bytes),
    });

    return Ok(msg);
}
