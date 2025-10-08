#![cfg(any(test, feature = "testing"))]

use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{coin, coins, Addr, Decimal, Uint128, Validator};
use cw20::BalanceResponse;
use cw20::Cw20ExecuteMsg::IncreaseAllowance;
use cw_multi_test::{Executor, StakingInfo};
use lst_common::address::VALIDATOR_ADDR_PREFIX;
use lst_common::babylon::{
    DENOM, EPOCH_LENGTH, STAKING_EPOCH_LENGTH_BLOCKS, STAKING_EPOCH_START_BLOCK_HEIGHT,
    UNSTAKING_PERIOD,
};
use lst_common::hub::ExecuteMsg::{Stake, Unstake, UpdateConfig};
use lst_common::hub::PendingDelegation as PendingDelegationRes;
use lst_common::hub::QueryMsg::{ExchangeRate, PendingDelegation};
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
    let validator1 = app.api().addr_make("validator1");

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
        validators
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

    app.next_epoch()
        .expect("Failed to fast forward to next epoch");

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
fn test_smoke_unstake() {
    let (mut app, tc, _validators) = instantiate();
    let owner = app.api().addr_make("owner");

    let current_batch: lst_common::hub::CurrentBatch = tc
        .staking_hub
        .query(&app, &lst_common::hub::QueryMsg::CurrentBatch {  })
        .unwrap();

    println!("Current Batch: {:#?}", current_batch);

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


    let _res = app.next_epoch()
        .expect("Failed to fast forward to next epoch");

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

    let _res = app.next_epoch()
        .expect("Failed to fast forward to next epoch");

    let validators: Vec<lst_common::validator::ValidatorResponse> = tc
        .validator_registry
        .query(&app, &lst_common::validator::QueryMsg::ValidatorsDelegation {  })
        .unwrap();

    for validator in validators {
        // 200 stakers, each staking 1_000_000 BABY, total 200_000_000 BABY
        // delegated equally to 10 validators, each validator should have 2_000_000 BABY delegated
        let delegated_amnt = validator.total_delegated;
        println!("Validator: {}, Delegated: {}", validator.address, delegated_amnt);
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

    let current_batch: lst_common::hub::CurrentBatch = tc
        .staking_hub
        .query(&app, &lst_common::hub::QueryMsg::CurrentBatch {  })
        .unwrap();

    println!("Current Batch: {:#?}", current_batch);

    // Don't have any epoching msg for this one
    // Fast forwarding to next epoch just to simulate some time passing
    let _res = app.next_many_epochs(3).expect("Failed to fast forward to next epoch");


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

        let current_batch: lst_common::hub::CurrentBatch = tc
            .staking_hub
            .query(&app, &lst_common::hub::QueryMsg::CurrentBatch {  })
            .unwrap();

        println!("N-th Unstaker {:#?}, Current Batch: {:#?}", i, current_batch);
        let pending_delegation: PendingDelegationRes =
            tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
        println!("Pending Delegation: {:#?}", pending_delegation);
        let hub_balance = app
            .wrap()
            .query_balance(tc.staking_hub.addr(), DENOM)
            .unwrap();
        println!("Hub Balance: {:#?}", hub_balance);
        println!("-----------------------------------");

    }

    // Inducing epoch boundary
    // epoch length is 7200 sec, unbonding period is 180000 sec, so we need to fast forward at
    // least 25 epochs
    let _res = app.next_many_epochs(25).expect("Failed to fast forward to next epoch");

    // usually Undelegation is attempted at every unstake if it's past epoch boundary
    // But since the above loop is unstaking in single epoch, we need to manually call it here
    tc.staking_hub
        .execute(
            &mut app,
            &owner,
            &lst_common::hub::ExecuteMsg::ProcessUndelegations {  },
        )
        .unwrap();



    let pending_delegation: PendingDelegationRes =
        tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(
        pending_delegation,
        PendingDelegationRes {
            staking_epoch_length_blocks: 360,
            staking_epoch_start_block_height: 10800,
            pending_staking_amount: Uint128::zero(),
            pending_unstaking_amount: Uint128::new(199_000_000),
        }
    );


    app.next_many_epochs(25)
        .expect("Failed to fast forward to next epoch");

    // let pending_delegation: PendingDelegationRes =
    //     tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    // assert_eq!(
    //     pending_delegation,
    //     PendingDelegationRes {
    //         staking_epoch_length_blocks: 360,
    //         staking_epoch_start_block_height: 11520,
    //         pending_staking_amount: Uint128::zero(),
    //         pending_unstaking_amount: Uint128::zero(),
    //     }
    // );

    let hub_balance = app
        .wrap()
        .query_balance(tc.staking_hub.addr(), DENOM)
        .unwrap();
    assert_eq!(hub_balance.amount, Uint128::new(200_000_000));



}
