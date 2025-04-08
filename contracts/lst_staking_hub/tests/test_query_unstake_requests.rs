use cosmwasm_std::testing::mock_dependencies;
use cosmwasm_std::{Addr, Decimal, Uint128};
use lst_common::hub::UnstakeHistory;
use lst_staking_hub::query::get_unstake_requests;
use lst_staking_hub::state::{UNSTAKE_HISTORY, UNSTAKE_WAIT_LIST};

#[test]
fn test_get_unstake_requests_success() {
    let mut deps = mock_dependencies();
    let address = Addr::unchecked("user1");
    let batch_id = 1u64;
    let lst_amount = Uint128::from(100u128);

    // Store wait list entry
    UNSTAKE_WAIT_LIST
        .save(
            deps.as_mut().storage,
            (address.clone(), batch_id),
            &lst_amount,
        )
        .unwrap();

    // Store unstake history
    let history = UnstakeHistory {
        batch_id,
        time: 1000,
        lst_token_amount: lst_amount,
        lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
        lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
        released: false,
    };
    UNSTAKE_HISTORY
        .save(deps.as_mut().storage, batch_id, &history)
        .unwrap();

    let result = get_unstake_requests(deps.as_ref().storage, address, None, None).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].batch_id, batch_id);
    assert_eq!(result[0].lst_amount, lst_amount);
    assert_eq!(result[0].withdraw_exchange_rate, history.lst_withdraw_rate);
    assert_eq!(
        result[0].applied_exchange_rate,
        history.lst_applied_exchange_rate
    );
    assert_eq!(result[0].time, history.time);
    assert_eq!(result[0].released, history.released);
}

#[test]
fn test_get_unstake_requests_skip_missing_history() {
    let mut deps = mock_dependencies();
    let address = Addr::unchecked("user1");

    // Store two wait list entries
    let batch_id1 = 1u64;
    let batch_id2 = 2u64;
    let lst_amount = Uint128::from(100u128);

    UNSTAKE_WAIT_LIST
        .save(
            deps.as_mut().storage,
            (address.clone(), batch_id1),
            &lst_amount,
        )
        .unwrap();
    UNSTAKE_WAIT_LIST
        .save(
            deps.as_mut().storage,
            (address.clone(), batch_id2),
            &lst_amount,
        )
        .unwrap();

    // Only store history for batch_id1
    let history = UnstakeHistory {
        batch_id: batch_id1,
        time: 1000,
        lst_token_amount: lst_amount,
        lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
        lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
        released: false,
    };
    UNSTAKE_HISTORY
        .save(deps.as_mut().storage, batch_id1, &history)
        .unwrap();

    let result = get_unstake_requests(deps.as_ref().storage, address, None, None).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].batch_id, batch_id1);
}

#[test]
fn test_get_unstake_requests_with_limit() {
    let mut deps = mock_dependencies();
    let address = Addr::unchecked("user1");

    // Store multiple wait list entries
    for i in 1..=5 {
        let batch_id = i as u64;
        let lst_amount = Uint128::from(100u128);

        UNSTAKE_WAIT_LIST
            .save(
                deps.as_mut().storage,
                (address.clone(), batch_id),
                &lst_amount,
            )
            .unwrap();

        let history = UnstakeHistory {
            batch_id,
            time: 1000,
            lst_token_amount: lst_amount,
            lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
            lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
            released: false,
        };
        UNSTAKE_HISTORY
            .save(deps.as_mut().storage, batch_id, &history)
            .unwrap();
    }

    // Test with limit of 3
    let result = get_unstake_requests(deps.as_ref().storage, address, None, Some(3)).unwrap();
    assert_eq!(result.len(), 3);
}

#[test]
fn test_get_unstake_requests_with_start_from() {
    let mut deps = mock_dependencies();
    let address = Addr::unchecked("user1");

    // Store multiple wait list entries
    for i in 1..=5 {
        let batch_id = i as u64;
        let lst_amount = Uint128::from(100u128);

        UNSTAKE_WAIT_LIST
            .save(
                deps.as_mut().storage,
                (address.clone(), batch_id),
                &lst_amount,
            )
            .unwrap();

        let history = UnstakeHistory {
            batch_id,
            time: 1000,
            lst_token_amount: lst_amount,
            lst_applied_exchange_rate: Decimal::from_ratio(2u128, 1u128),
            lst_withdraw_rate: Decimal::from_ratio(2u128, 1u128),
            released: false,
        };
        UNSTAKE_HISTORY
            .save(deps.as_mut().storage, batch_id, &history)
            .unwrap();
    }

    // Test starting from batch_id 3
    let result = get_unstake_requests(deps.as_ref().storage, address, Some(3), None).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].batch_id, 3);
}
