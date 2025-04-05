use crate::{constants::*, math::decimal_multiplication};
use cosmwasm_std::{Addr, Deps, DepsMut, Env, Event, Storage, Uint128};
use cw_storage_plus::{Item, Map};
use lst_common::{
    errors::HubError,
    hub::{Config, CurrentBatch, Parameters, PendingDelegation, State, UnstakeHistory},
    types::LstResult,
};

pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);
pub const PARAMETERS: Item<Parameters> = Item::new(PARAMETERS_KEY);
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new(CURRENT_BATCH_KEY);
pub const STATE: Item<State> = Item::new(STATE_KEY);

/// HashMap<user's address, <batch_id, requested_amount>
pub const UNSTAKE_WAIT_LIST: Map<(Addr, u64), Uint128> = Map::new(UNSTAKE_WAIT_LIST_KEY);
pub const UNSTAKE_HISTORY: Map<u64, UnstakeHistory> = Map::new(UNSTAKE_HISTORY_KEY);

pub const PENDING_DELEGATION: Item<PendingDelegation> = Item::new(PENDING_DELEGATION_KEY);

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

// Update state
pub fn update_state(
    storage: &mut dyn Storage,
    old_state: State,
    new_state: State,
) -> LstResult<Vec<Event>> {
    let mut events: Vec<Event> = vec![];
    STATE.save(storage, &new_state)?;
    events.push(
        Event::new(LST_EXCHANGE_RATE_UPDATED)
            .add_attribute(OLD_RATE, old_state.lst_exchange_rate.to_string())
            .add_attribute(NEW_RATE, new_state.lst_exchange_rate.to_string()),
    );
    events.push(
        Event::new(TOTAL_STAKED_AMOUNT_UPDATED)
            .add_attribute(OLD_AMOUNT, old_state.total_staked_amount.to_string())
            .add_attribute(NEW_AMOUNT, new_state.total_staked_amount.to_string()),
    );
    Ok(events)
}

// get the pending staking and unstaking amount
pub fn get_pending_delegation_amount(deps: Deps, env: &Env) -> LstResult<(Uint128, Uint128)> {
    let pending_delegation = PENDING_DELEGATION.load(deps.storage)?;

    let current_block_height = env.block.height;
    let passed_blocks = current_block_height - pending_delegation.staking_epoch_start_block_height;
    // if epoch has not passed, return the pending amount
    if passed_blocks < pending_delegation.staking_epoch_length_blocks {
        return Ok((
            pending_delegation.pending_staking_amount,
            pending_delegation.pending_unstaking_amount,
        ));
    }
    // if epoch has passed, it's zero
    Ok((Uint128::zero(), Uint128::zero()))
}

// Update the pending delegation amount
// if epoch has not passed, add the amount to the pending amount
// if epoch has passed, set the amount to the amount passed in param and update the epoch start block height
pub fn update_pending_delegation_amount(
    deps: &mut DepsMut,
    env: &Env,
    staking_amount: Option<Uint128>,
    unstaking_amount: Option<Uint128>,
) -> LstResult<()> {
    let mut pending_delegation = PENDING_DELEGATION.load(deps.storage)?;
    let current_block_height = env.block.height;
    let passed_blocks = current_block_height - pending_delegation.staking_epoch_start_block_height;

    if passed_blocks < pending_delegation.staking_epoch_length_blocks {
        pending_delegation.pending_staking_amount += staking_amount.unwrap_or(Uint128::zero());
        pending_delegation.pending_unstaking_amount += unstaking_amount.unwrap_or(Uint128::zero());
    } else {
        pending_delegation.pending_staking_amount = staking_amount.unwrap_or(Uint128::zero());
        pending_delegation.pending_unstaking_amount = unstaking_amount.unwrap_or(Uint128::zero());
        let epochs_passed = (current_block_height
            - pending_delegation.staking_epoch_start_block_height)
            / pending_delegation.staking_epoch_length_blocks;
        pending_delegation.staking_epoch_start_block_height +=
            epochs_passed * pending_delegation.staking_epoch_length_blocks;
    }

    PENDING_DELEGATION.save(deps.storage, &pending_delegation)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier};
    use cosmwasm_std::{MemoryStorage, OwnedDeps, Uint128};

    fn setup_test_env() -> (OwnedDeps<MemoryStorage, MockApi, MockQuerier>, Env) {
        let deps = mock_dependencies();
        let env = mock_env();
        (deps, env)
    }

    fn setup_pending_delegation(
        storage: &mut dyn Storage,
        start_height: u64,
        staking_amount: Option<Uint128>,
        unstaking_amount: Option<Uint128>,
        epoch_length: u64,
    ) {
        let pending_delegation = PendingDelegation {
            staking_epoch_start_block_height: start_height,
            pending_staking_amount: staking_amount.unwrap_or(Uint128::zero()),
            pending_unstaking_amount: unstaking_amount.unwrap_or(Uint128::zero()),
            staking_epoch_length_blocks: epoch_length,
        };
        PENDING_DELEGATION
            .save(storage, &pending_delegation)
            .unwrap();
    }

    #[test]
    fn test_get_pending_delegation_amount() {
        let (mut deps, mut env) = setup_test_env();

        // Test case 1: Within same epoch
        setup_pending_delegation(
            deps.as_mut().storage,
            664561,
            Some(Uint128::from(1000u128)),
            Some(Uint128::from(500u128)),
            360,
        );
        env.block.height = 664600; // Within first epoch
        let (staking, unstaking) = get_pending_delegation_amount(deps.as_ref(), &env).unwrap();
        assert_eq!(staking, Uint128::from(1000u128));
        assert_eq!(unstaking, Uint128::from(500u128));

        // Test case 2: After epoch has passed
        env.block.height = 670000; // After first epoch
        let (staking, unstaking) = get_pending_delegation_amount(deps.as_ref(), &env).unwrap();
        assert_eq!(staking, Uint128::zero());
        assert_eq!(unstaking, Uint128::zero());

        // Test case 3: Exactly at epoch boundary
        env.block.height = 664921; // End of first epoch
        let (staking, unstaking) = get_pending_delegation_amount(deps.as_ref(), &env).unwrap();
        assert_eq!(staking, Uint128::zero());
        assert_eq!(unstaking, Uint128::zero());
    }

    #[test]
    fn test_update_pending_delegation_amount() {
        let (mut deps, mut env) = setup_test_env();

        // Test case 1: Within same epoch - should add to existing amounts
        setup_pending_delegation(
            deps.as_mut().storage,
            664561,
            Some(Uint128::from(1000u128)),
            Some(Uint128::from(500u128)),
            360,
        );
        env.block.height = 664600;
        update_pending_delegation_amount(
            &mut deps.as_mut(),
            &env,
            Some(Uint128::from(500u128)),
            Some(Uint128::from(200u128)),
        )
        .unwrap();

        let pending_delegation = PENDING_DELEGATION.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            pending_delegation.pending_staking_amount,
            Uint128::from(1500u128)
        );
        assert_eq!(
            pending_delegation.pending_unstaking_amount,
            Uint128::from(700u128)
        );
        assert_eq!(pending_delegation.staking_epoch_start_block_height, 664561);

        // Test case 2: After epoch has passed - should reset amounts and update start height
        env.block.height = 664930;
        update_pending_delegation_amount(
            &mut deps.as_mut(),
            &env,
            Some(Uint128::from(2000u128)),
            Some(Uint128::from(1000u128)),
        )
        .unwrap();

        let pending_delegation = PENDING_DELEGATION.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            pending_delegation.pending_staking_amount,
            Uint128::from(2000u128)
        );
        assert_eq!(
            pending_delegation.pending_unstaking_amount,
            Uint128::from(1000u128)
        );
        assert_eq!(pending_delegation.staking_epoch_start_block_height, 664921);

        // Test case 3: Multiple epochs passed - should update to correct start height
        setup_pending_delegation(
            deps.as_mut().storage,
            664561,
            Some(Uint128::from(1000u128)),
            Some(Uint128::from(500u128)),
            360,
        );
        env.block.height = 666100; // Multiple epochs passed
        update_pending_delegation_amount(
            &mut deps.as_mut(),
            &env,
            Some(Uint128::from(3000u128)),
            Some(Uint128::from(1500u128)),
        )
        .unwrap();

        let pending_delegation = PENDING_DELEGATION.load(deps.as_ref().storage).unwrap();
        assert_eq!(
            pending_delegation.pending_staking_amount,
            Uint128::from(3000u128)
        );
        assert_eq!(
            pending_delegation.pending_unstaking_amount,
            Uint128::from(1500u128)
        );
        assert_eq!(pending_delegation.staking_epoch_start_block_height, 666001);
    }
}
