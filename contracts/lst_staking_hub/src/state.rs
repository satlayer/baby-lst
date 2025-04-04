use crate::{constants::*, math::decimal_multiplication};
use cosmwasm_std::{Addr, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use lst_common::{
    errors::HubError,
    hub::{Config, CurrentBatch, Parameters, State, UnstakeHistory},
    types::LstResult,
};

pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);
pub const PARAMETERS: Item<Parameters> = Item::new(PARAMETERS_KEY);
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new(CURRENT_BATCH_KEY);
pub const STATE: Item<State> = Item::new(STATE_KEY);

pub const UNSTAKE_WAIT_LIST: Map<(Addr, u64), Uint128> = Map::new(UNSTAKE_WAIT_LIST_KEY);
pub const UNSTAKE_HISTORY: Map<u64, UnstakeHistory> = Map::new(UNSTAKE_HISTORY_KEY);

#[derive(PartialEq)]
pub enum StakeType {
    LSTMint,
    StakeRewards,
}

#[derive(PartialEq)]
pub enum UnstakeType {
    BurnFlow,
    BurnFromFlow,
}

pub fn read_unstake_history(storage: &dyn Storage, epoch_id: u64) -> LstResult<UnstakeHistory> {
    UNSTAKE_HISTORY
        .may_load(storage, epoch_id)?
        .ok_or(lst_common::ContractError::Hub(
            HubError::UnstakeHistoryNotFound {},
        ))
}

// Return all requested unstaked amount.
// This needs to be called after process withdraw rate function
// If the batch is released, this will return user's requested amount
// proportional to new withdraw rate
pub fn get_finished_amount(
    storage: &dyn Storage,
    sender_addr: Addr,
) -> LstResult<(Uint128, Vec<u64>)> {
    // Get all unstake wait list entries for this user
    let wait_list: Vec<(u64, Uint128)> = UNSTAKE_WAIT_LIST
        .prefix(sender_addr)
        .range(storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (batch_id, lst_amount) = item?;
            Ok((batch_id, lst_amount))
        })
        .collect::<LstResult<Vec<_>>>()?;

    process_finished_amount(storage, wait_list)
}

// Return requested unstaked amount for specific batch IDs.
// This needs to be called after process withdraw rate function
// If the batch is released, this will return user's requested amount
// proportional to new withdraw rate
pub fn get_finished_amount_for_batches(
    storage: &dyn Storage,
    sender_addr: Addr,
    batch_ids: Vec<u64>,
) -> LstResult<(Uint128, Vec<u64>)> {
    // Get wait list entries for specified batch IDs
    let wait_list: Vec<(u64, Uint128)> = batch_ids
        .iter()
        .filter_map(|&batch_id| {
            UNSTAKE_WAIT_LIST
                .load(storage, (sender_addr.clone(), batch_id))
                .ok()
                .map(|lst_amount| (batch_id, lst_amount))
        })
        .collect();

    process_finished_amount(storage, wait_list)
}

// Common processing logic for calculating finished amounts
fn process_finished_amount(
    storage: &dyn Storage,
    wait_list: Vec<(u64, Uint128)>,
) -> LstResult<(Uint128, Vec<u64>)> {
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let mut deprecated_batches: Vec<u64> = vec![];

    // Process each wait list entry
    for (batch_id, lst_amount) in wait_list {
        // Get the unstake history for this batch
        if let Ok(history) = read_unstake_history(storage, batch_id) {
            if history.released {
                // Add batch id to deprecated list
                deprecated_batches.push(batch_id);

                // Calculate withdrawable amount using withdraw rate
                let amount = decimal_multiplication(lst_amount, history.lst_applied_exchange_rate);
                withdrawable_amount += amount;
            }
        }
    }

    Ok((withdrawable_amount, deprecated_batches))
}

// Remove unstaked batch id from user's unstake wait list
pub fn remove_unstake_wait_list(
    storage: &mut dyn Storage,
    batch_ids: Vec<u64>,
    sender_addr: Addr,
) -> LstResult<()> {
    // Remove entries with matching batch IDs
    for batch_id in batch_ids {
        UNSTAKE_WAIT_LIST.remove(storage, (sender_addr.clone(), batch_id));
    }
    Ok(())
}
