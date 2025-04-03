use crate::{constants::*, math::decimal_multiplication};
use cosmwasm_std::{Addr, Decimal, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use lst_common::{
    errors::HubError,
    hub::{Config, CurrentBatch, Parameters, State},
    types::LstResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);
pub const PARAMETERS: Item<Parameters> = Item::new(PARAMETERS_KEY);
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new(CURRENT_BATCH_KEY);
pub const STATE: Item<State> = Item::new(STATE_KEY);

pub const UNSTAKE_WAIT_LIST: Map<(Addr, u64), Uint128> = Map::new(UNSTAKE_WAIT_LIST_KEY);
pub const UNSTAKE_HISTORY: Map<u64, UnStakeHistory> = Map::new(UNSTAKE_HISTORY_KEY);

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnStakeHistory {
    pub batch_id: u64,
    pub time: u64,
    pub lst_token_amount: Uint128,
    pub lst_applied_exchange_rate: Decimal,
    pub lst_withdraw_rate: Decimal,
    pub released: bool,
}

pub fn read_unstake_history(storage: &dyn Storage, epoch_id: u64) -> LstResult<UnStakeHistory> {
    UNSTAKE_HISTORY
        .may_load(storage, epoch_id)?
        .ok_or(lst_common::ContractError::Hub(
            HubError::UnstakeHistoryNotFound {},
        ))
}

// Return all requested unstaked amount.
// This needs to be called after process withdraw rate function
// If the batch is released, this will return user's requested amount
// proportional to withdraw rate
pub fn get_finished_amount(
    storage: &dyn Storage,
    sender_addr: Addr,
) -> LstResult<(Uint128, Vec<u64>)> {
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let mut deprecated_batches: Vec<u64> = vec![];

    // Get all unstake wait list entries for this user
    let wait_list: Vec<(u64, Uint128)> = UNSTAKE_WAIT_LIST
        .prefix(sender_addr)
        .range(storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|item| {
            let (batch_id, lst_amount) = item?;
            Ok((batch_id, lst_amount))
        })
        .collect::<LstResult<Vec<_>>>()?;

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
