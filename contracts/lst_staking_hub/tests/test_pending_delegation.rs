use cosmwasm_std::testing::{mock_dependencies, mock_env, MockApi, MockQuerier};
use cosmwasm_std::{Env, MemoryStorage, OwnedDeps, Storage, Uint128};
use lst_common::hub::PendingDelegation;
use lst_staking_hub::state::{
    get_pending_delegation_amount, update_pending_delegation_amount, PENDING_DELEGATION,
};

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
