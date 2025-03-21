use cosmwasm_std::{
    attr, to_json_binary, Addr, CosmosMsg, DepsMut, Env, Response, StdResult, Storage, Uint128,
    WasmMsg,
};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;

use lst_common::{errors::HubError, types::LstResult, ContractError};

use crate::{
    contract::check_slashing,
    math::decimal_multiplication,
    state::{
        CurrentBatch, State, UnStakeHistory, UnstakeWaitEntity, CONFIG, CURRENT_BATCH, PARAMETERS,
        STATE, UNSTAKE_HISTORY, UNSTAKE_WAIT_LIST,
    },
};

pub(crate) fn execute_unstake(
    mut deps: DepsMut,
    env: Env,
    amount: Uint128,
    sender: String,
) -> LstResult<Response> {
    // read parameters
    let params = PARAMETERS.load(deps.storage)?;
    let epoch_period = params.epoch_length;

    // load current batch
    let mut current_batch = CURRENT_BATCH.load(deps.storage)?;

    // check if slashing has occurred and update the exchange rate
    let mut state = check_slashing(&mut deps, env.clone())?;

    // add the unstaking amount to the current batch
    current_batch.requested_lst_token_amount += amount;

    store_unstake_wait_list(deps.storage, current_batch.id, sender.clone(), amount)?;

    let current_time = env.block.time.seconds();
    let passed_time = current_time - state.last_unbonded_time;

    let mut messages: Vec<CosmosMsg> = vec![];

    // if the epoch period is passed, the undelegate message would be sent
    if passed_time >= epoch_period {
        let mut undelegate_msgs =
            process_undelegations(&mut deps, env, &mut current_batch, &mut state)?;
        messages.append(&mut undelegate_msgs);
    }

    // Store the new requested id in the batch
    CURRENT_BATCH.save(deps.storage, &current_batch)?;

    // Store state's new exchange rate
    STATE.save(deps.storage, &state)?;

    // send burn message to the token contract
    let config = CONFIG.load(deps.storage)?;
    let lst_token_addr = deps
        .api
        .addr_humanize(&config.lst_token.ok_or(HubError::LstTokenNotSet)?)?;

    let burn_msg = Cw20ExecuteMsg::Burn { amount };
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

/// Store undelegation wait list per each batch
/// HashMap<user's address, <batch_id, requested_amount>
fn store_unstake_wait_list(
    storage: &mut dyn Storage,
    batch_id: u64,
    sender_address: String,
    amount: Uint128,
) -> LstResult<()> {
    let sender_addr = Addr::unchecked(sender_address);

    // Try to load existing wait entity for this user
    let wait_entity = match UNSTAKE_WAIT_LIST.may_load(storage, sender_addr.clone())? {
        Some(mut entity) => {
            // Update existing entity
            entity.lst_token_amount += amount;
            entity
        }
        None => {
            // Create new entity
            UnstakeWaitEntity {
                batch_id,
                lst_token_amount: amount,
            }
        }
    };

    // Save updated/new entity
    UNSTAKE_WAIT_LIST.save(storage, sender_addr, &wait_entity)?;

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
    let delegator = env.contract.address;

    // send undelegate requests to possibly more than one validators
    let undelegate_msgs = pick_validator(deps, lst_undelegation_amount, delegator.to_string())?;

    state.total_lst_token_amount = state
        .total_lst_token_amount
        .checked_sub(lst_undelegation_amount)
        .map_err(|e| ContractError::Overflow(e.to_string()))?;

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

fn pick_validator(
    deps: &mut DepsMut,
    claim: Uint128,
    delegator: String,
) -> StdResult<Vec<CosmosMsg>> {
    todo!()
}
