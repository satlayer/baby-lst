#![cfg(any(test, feature = "testing"))]

use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{coin, coins, Addr, Coin, Decimal, Uint128, Validator};
use cw20::BalanceResponse;
use cw20::Cw20ExecuteMsg::IncreaseAllowance;
use cw_multi_test::{Executor, StakingInfo};
use lst_common::address::VALIDATOR_ADDR_PREFIX;
use lst_common::babylon::{
    DENOM, EPOCH_LENGTH, STAKING_EPOCH_LENGTH_BLOCKS, STAKING_EPOCH_START_BLOCK_HEIGHT,
    UNSTAKING_PERIOD,
};
use lst_common::hub::CurrentBatch as CurrentBatchRes;
use lst_common::hub::ExecuteMsg::{Stake, Unstake, UpdateConfig};
use lst_common::hub::PendingDelegation as PendingDelegationRes;
use lst_common::hub::QueryMsg::{CurrentBatch, ExchangeRate, PendingDelegation};
use lst_common::testing::{BabylonApp, TestingContract};
use lst_common::validator::ExecuteMsg::AddValidator;
use lst_common::validator::Validator as LSTValidator;
use lst_reward_dispatcher::testing::RewardDispatcherContract;
use lst_staking_hub::testing::StakingHubContract;
use lst_token::testing::TokenContract;
use lst_validators_registry::testing::ValidatorRegistryContract;

struct TestContracts {
    staking_hub: StakingHubContract,
    lst_token: TokenContract,
    validator_registry: ValidatorRegistryContract,
    reward_dispatcher: RewardDispatcherContract,
}

const UNBONDING_TIME: u64 = 180000; // time between unbonding and receiving tokens back (in seconds) - 50hours

fn instantiate() -> (BabylonApp, TestContracts, Vec<(Addr, Validator)>) {
    let block = mock_env().block;
    let mut validators: Vec<(Addr, Validator)> = vec![];

    let mut app = BabylonApp::new(|router, api, storage| {
        let owner = api.addr_make("owner");

        for i in 1..=10 {
            let validator_addr = api
                .with_prefix(VALIDATOR_ADDR_PREFIX)
                .addr_make(format!("validator{}", i).as_str());
            let validator = Validator::new(
                validator_addr.to_string(),
                Decimal::percent(10), // 10% commission
                Decimal::percent(90), // 90% max comission
                Decimal::percent(1),  // 1% max change rate
            );
            validators.push((validator_addr, validator));
        }

        router
            .bank
            .init_balance(storage, &owner, vec![coin(Uint128::MAX.u128(), DENOM)])
            .unwrap();
        // setup staking parameters
        router
            .staking
            .setup(
                storage,
                StakingInfo {
                    bonded_denom: DENOM.to_string(),
                    unbonding_time: UNBONDING_TIME,
                    apr: Decimal::percent(10),
                },
            )
            .unwrap();

        // custom starget simulate unbonding max delegator<->validator pair
        router.stargate.unbonding_time_secs = Some(UNBONDING_TIME);
        router.stargate.max_unbonding_entries = Some(100);

        for (_addr, validator) in validators.clone() {
            router
                .staking
                .add_validator(api, storage, &block, validator)
                .unwrap();
        }
    });

    let env = mock_env();

    let staking_hub = StakingHubContract::new(&mut app, &env, None);

    let owner = app.api().addr_make("owner");

    // create cw20 token
    let lst_token = TokenContract::new(&mut app, &env, None);

    // instantiate validator registry
    let validator_registry = ValidatorRegistryContract::new(&mut app, &env, None);

    // instantiate reward dispatcher
    let reward_dispatcher = RewardDispatcherContract::new(&mut app, &env, None);

    // update lst hub config
    staking_hub
        .execute(
            &mut app,
            &owner,
            &UpdateConfig {
                owner: Some(owner.to_string()),
                lst_token: Some(lst_token.addr().to_string()),
                validator_registry: Some(validator_registry.addr().to_string()),
                reward_dispatcher: Some(reward_dispatcher.addr().to_string()),
            },
        )
        .unwrap();

    // register validators
    for (_addr, validator) in validators.clone() {
        validator_registry
            .execute(
                &mut app,
                &owner,
                &AddValidator {
                    validator: LSTValidator {
                        address: validator.address,
                    },
                },
            )
            .expect("Failed to add validator");
    }

    (
        app,
        TestContracts {
            staking_hub,
            lst_token,
            validator_registry,
            reward_dispatcher,
        },
        validators,
    )
}

#[test]
fn test_instantiate() {
    let (_app, tc, _validators) = instantiate();

    // Check that the contract was instantiated correctly
    assert_eq!(tc.staking_hub.init.epoch_length, EPOCH_LENGTH);
    assert_eq!(tc.staking_hub.init.unstaking_period, UNSTAKING_PERIOD);
    assert_eq!(tc.staking_hub.init.staking_coin_denom, DENOM);
    assert_eq!(
        tc.staking_hub.init.staking_epoch_length_blocks,
        STAKING_EPOCH_LENGTH_BLOCKS
    );
    assert_eq!(
        tc.staking_hub.init.staking_epoch_start_block_height,
        STAKING_EPOCH_START_BLOCK_HEIGHT
    );
}

#[test]
fn test_unbonding() {
    let (mut app, tc, _validators) = instantiate();

    let owner = app.api().addr_make("owner");
    let staker = app.api().addr_make("staker");
    let staker2 = app.api().addr_make("staker2");

    {
        // get BABY token for staker
        app.send_tokens(owner.clone(), staker.clone(), &coins(1_000_000, DENOM))
            .unwrap();

        // staker stake 1_000_000 BABY
        tc.staking_hub
            .execute_with_funds(&mut app, &staker, &Stake {}, coins(1_000_000, DENOM))
            .unwrap();

        // assert that the staker has 1_000_000 LST token = 1:1 exchange rate
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000_000));
    }

    {
        // staker2 stake 500_000 BABY
        app.send_tokens(owner.clone(), staker2.clone(), &coins(500_000, DENOM))
            .unwrap();
        tc.staking_hub
            .execute_with_funds(&mut app, &staker2, &Stake {}, coins(500_000, DENOM))
            .unwrap();

        // assert that the staker2 has 500_000 LST token = 1:1 exchange rate
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker2.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(500_000));
    }

    // Next epoch
    app.next_epoch();

    // check if validator has delegated stake
    let res = app
        .wrap()
        .query_delegator_validators(
            tc.staking_hub.addr(), // cosmwasm1mzdhwvvh22wrt07w59wxyd58822qavwkx5lcej7aqfkpqqlhaqfsgn6fq2
        )
        .unwrap();
    assert_eq!(res.len(), 10);

    // check if contract balance is reduced
    let res = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(res, Coin::new(Uint128::zero(), DENOM));

    {
        // staker2 give allowance to staking hub
        tc.lst_token
            .execute(
                &mut app,
                &staker2,
                &IncreaseAllowance {
                    spender: tc.staking_hub.addr().to_string(),
                    amount: Uint128::new(200_000),
                    expires: None,
                },
            )
            .unwrap();
        // staker2 unstake 200_000 LST
        tc.staking_hub
            .execute(
                &mut app,
                &staker2,
                &Unstake {
                    amount: Uint128::new(200_000),
                },
            )
            .unwrap();

        // assert that the staker2 has 300_000 LST token left
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker2.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(500_000 - 200_000));
    }

    let pending_delegation2: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation2,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 360, // next epoch
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    // assert current_batch has unstaking amount
    let current_batch: CurrentBatchRes = tc.staking_hub.query(&app, &CurrentBatch {}).unwrap();
    assert_eq!(
        current_batch,
        CurrentBatchRes {
            id: 1,
            requested_lst_token_amount: Uint128::new(200_000),
        }
    );
}

#[test]
fn test_stake_unstake_within_same_epoch() {
    let (mut app, tc, _validators) = instantiate();

    let owner = app.api().addr_make("owner");
    let staker = app.api().addr_make("staker");
    let staker2 = app.api().addr_make("staker2");
    let validator1 = app.api().addr_make("validator1");

    {
        // get BABY token for staker
        app.send_tokens(owner.clone(), staker.clone(), &coins(1_000_000, DENOM))
            .unwrap();

        // staker stake 1_000_000 BABY
        tc.staking_hub
            .execute_with_funds(&mut app, &staker, &Stake {}, coins(1_000_000, DENOM))
            .unwrap();

        // assert that the staker has 1_000_000 LST token = 1:1 exchange rate
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000_000));
    }

    {
        // staker2 stake 500_000 BABY
        app.send_tokens(owner.clone(), staker2.clone(), &coins(500_000, DENOM))
            .unwrap();
        tc.staking_hub
            .execute_with_funds(&mut app, &staker2, &Stake {}, coins(500_000, DENOM))
            .unwrap();

        // assert that the staker2 has 500_000 LST token = 1:1 exchange rate
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker2.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(500_000));
    }

    {
        // staker2 give allowance to staking hub
        tc.lst_token
            .execute(
                &mut app,
                &staker2,
                &IncreaseAllowance {
                    spender: tc.staking_hub.addr().to_string(),
                    amount: Uint128::new(200_000),
                    expires: None,
                },
            )
            .unwrap();
        // staker2 unstake 200_000 LST
        tc.staking_hub
            .execute(
                &mut app,
                &staker2,
                &Unstake {
                    amount: Uint128::new(200_000),
                },
            )
            .unwrap();

        // assert that the staker2 has 300_000 LST token left
        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker2.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(500_000 - 200_000));
    }

    // assert that the exchange rate is 1:1
    let exchange_rate: Uint128 = tc.staking_hub.query(&app, &ExchangeRate {}).unwrap();
    assert_eq!(exchange_rate, Uint128::new(1));

    // assert that pending staking amount is correct
    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 0,
            pending_staking_amount: Uint128::new(1_500_000), // should be 1_300_000 ??
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    app.next_epoch();

    let pending_delegation2: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation2,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 360, // next epoch
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    // assert that the exchange rate is 1:1 after end of epoch
    let exchange_rate: Uint128 = tc.staking_hub.query(&app, &ExchangeRate {}).unwrap();
    assert_eq!(exchange_rate, Uint128::new(1));
}

#[test]
fn test_multi_unstaker_single_epoch() {
    let (mut app, tc, _validators) = instantiate();
    let owner = app.api().addr_make("owner");

    let mut stakers = vec![];

    for i in 1..=200 {
        let staker = app.api().addr_make(format!("staker{}", i).as_str());
        app.send_tokens(owner.clone(), staker.clone(), &coins(1_000_000, DENOM))
            .unwrap();
        stakers.push(staker);
    }

    // the first 100 stakers stake 1_000_000 BABY each
    for staker in stakers.iter().take(100) {
        tc.staking_hub
            .execute_with_funds(&mut app, staker, &Stake {}, coins(1_000_000, DENOM))
            .unwrap();

        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000_000));
    }

    // check hub balance
    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(100_000_000));

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 0,
            pending_staking_amount: Uint128::new(100_000_000),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    let _res = app.next_epoch();

    // the next 100 stakers stake 1_000_000 BABY each
    for staker in stakers.iter().skip(100) {
        tc.staking_hub
            .execute_with_funds(&mut app, staker, &Stake {}, coins(1_000_000, DENOM))
            .unwrap();

        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000_000));
    }
    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(100_000_000));

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 360,
            pending_staking_amount: Uint128::new(100_000_000),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    let _res = app.next_epoch();

    let validators: Vec<lst_common::validator::ValidatorResponse> = tc
        .validator_registry
        .query(
            &app,
            &lst_common::validator::QueryMsg::ValidatorsDelegation {},
        )
        .unwrap();

    for validator in validators {
        // 200 stakers, each staking 1_000_000 BABY, total 200_000_000 BABY
        // delegated equally to 10 validators, each validator should have 2_000_000 BABY delegated
        let delegated_amnt = validator.total_delegated;
        println!(
            "Validator: {}, Delegated: {}",
            validator.address, delegated_amnt
        );
        assert!(delegated_amnt.u128() == 20_000_000);
    }

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 720,
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    // Don't have any epoching msg for this one
    // Fast forwarding to next epoch just to simulate some time passing
    let _res = app.next_many_epochs(3);

    // Note - by this point,
    // the batch id 1 (genesis batch) is due and time for 2nd batch to be created
    // But due to the bug in the code the 0th unstaker is coupled to the 1st batch before creating
    // 2nd batch
    // Since 1st batch is alrady due, the 1st unstaker is able to undelegate faster.
    // This is gonna keep happening for the 1st staker at every batch epoch boundary
    // Observe the stdout in the following loop with `cargo test -- --nocapture`
    // to see that the 1st unstaker is coupled to the batch id 1.
    // Propose Fix: In the `execute_unstake()` function callstack of staking hub contract,
    // before coupling the unstaker to the current batch,
    // check if the current batch is due, if yes, process the batch and create a new batch
    // then couple the unstaker to the new batch.
    for i in 0..stakers.clone().len() {
        // give allowance to staking hub
        tc.lst_token
            .execute(
                &mut app,
                &stakers[i],
                &IncreaseAllowance {
                    spender: tc.staking_hub.addr().to_string(),
                    amount: Uint128::new(1_000_000),
                    expires: None,
                },
            )
            .unwrap();
        // unstake 1_000_000 LST
        tc.staking_hub
            .execute(
                &mut app,
                &stakers[i],
                &Unstake {
                    amount: Uint128::new(1_000_000),
                },
            )
            .unwrap();
    }

    // let both batches aged
    app.next_many_epochs(2);

    // Usually Undelegation is attempted implicitly at every unstake req if it's past epoch boundary
    // But since the above loop is unstaking in within the same epoch window, no undelegation is made
    // implicitly.
    // Keeper need to manually call it here
    tc.staking_hub
        .execute(
            &mut app,
            &owner,
            &lst_common::hub::ExecuteMsg::ProcessUndelegations {},
        )
        .unwrap();

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 2520,
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::new(199_000_000), // <- the 1st unstaker gets
                                                                 // batched with batch id 1,
        }
    );

    // babylon unbonding
    app.next_many_epochs(25);

    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(200_000_000));

    tc.staking_hub
        .execute(
            &mut app,
            &stakers[0],
            &lst_common::hub::ExecuteMsg::WithdrawUnstaked {}, //<- the 1st unstaker withdraws from
                                                               //batch id 1
        )
        .unwrap();

    let all_history: lst_common::hub::AllHistoryResponse = tc
        .staking_hub
        .query(
            &app,
            &lst_common::hub::QueryMsg::AllHistory {
                start_from: None,
                limit: None,
            },
        )
        .unwrap();

    assert_eq!(all_history.history.len(), 2);
    assert_eq!(all_history.history[0].released, true); // <- the 1st unstaker's made a claim and
                                                       // the batch id 1 is due to be released
    assert_eq!(all_history.history[1].released, false); // <- the 2nd batch is not yet released since
                                                        // nobody in the batch has claimed just yet.

    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(199_000_000));

    app.next_epoch();

    for i in 1..stakers.clone().len() {
        tc.staking_hub
            .execute(
                &mut app,
                &stakers[i],
                &lst_common::hub::ExecuteMsg::WithdrawUnstaked {},
            )
            .unwrap();

        let native_token_balance = app.wrap().query_balance(stakers[i].clone(), DENOM).unwrap();
        assert_eq!(native_token_balance.amount, Uint128::new(1_000_000));
    }

    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::zero());

    let all_history: lst_common::hub::AllHistoryResponse = tc
        .staking_hub
        .query(
            &app,
            &lst_common::hub::QueryMsg::AllHistory {
                start_from: None,
                limit: None,
            },
        )
        .unwrap();
    assert_eq!(all_history.history.len(), 2);
    assert_eq!(all_history.history[0].released, true);
    assert_eq!(all_history.history[1].released, true);
}

#[test]
fn test_multi_unstaker_multi_epoch() {
    let (mut app, tc, _validators) = instantiate();
    let owner = app.api().addr_make("owner");

    let mut stakers = vec![];

    // Create 200 stakers, divided into 4 groups of 50 stakers each
    for i in 1..=200 {
        let staker = app.api().addr_make(format!("staker{}", i).as_str());
        app.send_tokens(owner.clone(), staker.clone(), &coins(1_000_000, DENOM))
            .unwrap();
        stakers.push(staker);
    }

    // all 200 stakers stake 1_000_000 BABY each
    for staker in stakers.clone().iter() {
        tc.staking_hub
            .execute_with_funds(&mut app, staker, &Stake {}, coins(1_000_000, DENOM))
            .unwrap();

        let BalanceResponse { balance } = tc
            .lst_token
            .query(
                &app,
                &cw20_base::msg::QueryMsg::Balance {
                    address: staker.to_string(),
                },
            )
            .unwrap();
        assert_eq!(balance, Uint128::new(1_000_000));
    }

    // check hub balance
    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(200_000_000));

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 0,
            pending_staking_amount: Uint128::new(200_000_000),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    let _res = app.next_epoch();

    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 360,
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::zero(),
        }
    );

    let validators: Vec<lst_common::validator::ValidatorResponse> = tc
        .validator_registry
        .query(
            &app,
            &lst_common::validator::QueryMsg::ValidatorsDelegation {},
        )
        .unwrap();

    for validator in validators {
        // 200 stakers, each staking 1_000_000 BABY, total 200_000_000 BABY
        // delegated equally to 10 validators, each validator should have 2_000_000 BABY delegated
        let delegated_amnt = validator.total_delegated;
        println!(
            "Validator: {}, Delegated: {}",
            validator.address, delegated_amnt
        );
        assert!(delegated_amnt.u128() == 20_000_000);
    }

    // simulate sometime passed
    app.next_many_epochs(100);

    // -------------------- Unstaking Phase --------------------
    {
        for i in 0..stakers.clone().len() {
            // give allowance to staking hub
            tc.lst_token
                .execute(
                    &mut app,
                    &stakers[i],
                    &IncreaseAllowance {
                        spender: tc.staking_hub.addr().to_string(),
                        amount: Uint128::new(1_000_000),
                        expires: None,
                    },
                )
                .unwrap();

            // unstake 1_000_000 LST
            tc.staking_hub
                .execute(
                    &mut app,
                    &stakers[i],
                    &Unstake {
                        amount: Uint128::new(1_000_000),
                    },
                )
                .unwrap();

            app.next_epoch();
        }

        // The undelegation for batch k is trigger by batch k+1 implicitly except for the last batch
        // So keeper has to trigger the undelegation manually
        tc.staking_hub
            .execute(
                &mut app,
                &owner,
                &lst_common::hub::ExecuteMsg::ProcessUndelegations {},
            )
            .unwrap();

        // by the end of the loop - (200 epochs passed)
        // about 88 batches are unbonded the hub has received token back
        // need to advance at least 25 epoch to make the last batch unbonded
        app.next_many_epochs(25);

        let hub_balance = app
            .wrap()
            .query_balance(tc.staking_hub.addr(), DENOM)
            .unwrap();

        assert_eq!(hub_balance.amount, Uint128::new(200_000_000));
    }

    // simulate some time passed
    app.next_epoch();

    // ------- Claim Phase --------

    //staker from the last batch claim first
    tc.staking_hub
        .execute(
            &mut app,
            &stakers[199],
            &lst_common::hub::ExecuteMsg::WithdrawUnstaked {},
        )
        .unwrap();
    let native_token_balance = app
        .wrap()
        .query_balance(stakers[199].clone(), DENOM)
        .unwrap();
    assert_eq!(native_token_balance.amount, Uint128::new(1_000_000));

    // Claim sequentially should be successful except the staker 199th above
    for i in 0..stakers.clone().len() - 1 {
        tc.staking_hub
            .execute(
                &mut app,
                &stakers[i],
                &lst_common::hub::ExecuteMsg::WithdrawUnstaked {},
            )
            .unwrap();

        let native_token_balance = app.wrap().query_balance(stakers[i].clone(), DENOM).unwrap();
        assert_eq!(native_token_balance.amount, Uint128::new(1_000_000));
    }

    let all_history: lst_common::hub::AllHistoryResponse = tc
        .staking_hub
        .query(
            &app,
            &lst_common::hub::QueryMsg::AllHistory {
                start_from: None,
                limit: None,
            },
        )
        .unwrap();

    for history in all_history.history {
        assert_eq!(history.released, true);
    }
}

#[test]
fn test_multi_unstaker_multi_epoch_undelegation_throttle() {}
