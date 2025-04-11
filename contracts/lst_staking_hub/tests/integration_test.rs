#![cfg(any(test, feature = "testing"))]

use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{coin, coins, Decimal, Uint128, Validator};
use cw20::BalanceResponse;
use cw20::Cw20ExecuteMsg::IncreaseAllowance;
use cw_multi_test::{Executor, StakingInfo};
use lst_common::address::VALIDATOR_ADDR_PREFIX;
use lst_common::hub::ExecuteMsg::{ProcessWithdrawRequests, Stake, Unstake, UpdateConfig};
use lst_common::hub::QueryMsg::{ExchangeRate, PendingDelegation};
use lst_common::hub::PendingDelegation as PendingDelegationRes;
use lst_common::testing::{BabylonApp, TestingContract};
use lst_common::validator::ExecuteMsg::AddValidator;
use lst_common::validator::QueryMsg::ValidatorsDelegation;
use lst_common::validator::{Validator as LSTValidator, ValidatorResponse};
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

const DENOM: &str = "BABY"; // denominator of the staking token
const UNBONDING_TIME: u64 = 60; // time between unbonding and receiving tokens back (in seconds)

fn instantiate() -> (BabylonApp, TestContracts) {
    let block = mock_env().block;

    let mut app = BabylonApp::new(|router, api, storage| {
        let owner = api.addr_make("owner");
        let validator1_addr = api
            .with_prefix(VALIDATOR_ADDR_PREFIX)
            .addr_make("validator1");
        let validator1 = Validator::new(
            validator1_addr.to_string(),
            Decimal::percent(10), // 10% commission
            Decimal::percent(90), // 90% max comission
            Decimal::percent(1),  // 1% max change rate
        );
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
        // add a validator1
        router
            .staking
            .add_validator(api, storage, &block, validator1)
            .unwrap();
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

    // register validator
    validator_registry
        .execute(
            &mut app,
            &owner,
            &AddValidator {
                validator: LSTValidator {
                    address: validator1.to_string(),
                },
            },
        )
        .expect("Failed to add validator");

    (
        app,
        TestContracts {
            staking_hub,
            lst_token,
            validator_registry,
            reward_dispatcher,
        },
    )
}

#[test]
fn test_instantiate() {
    let (_app, tc) = instantiate();

    // Check that the contract was instantiated correctly
    assert_eq!(tc.staking_hub.init.epoch_length, 7200);
    assert_eq!(tc.staking_hub.init.unstaking_period, 64800);
    assert_eq!(tc.staking_hub.init.staking_coin_denom, "BABY");
    assert_eq!(tc.staking_hub.init.staking_epoch_length_blocks, 360);
    assert_eq!(tc.staking_hub.init.staking_epoch_start_block_height, 0);
}

#[test]
fn test_exchange_rate() {
    let (mut app, tc) = instantiate();

    let owner = app.api().addr_make("owner");
    let staker = app.api().addr_make("staker");
    let staker2 = app.api().addr_make("staker2");
    let validator1 = app.api().addr_make("validator1");

    {
        // query validator registry
        let res: Vec<ValidatorResponse> = tc
            .validator_registry
            .query(&app, &ValidatorsDelegation {})
            .unwrap();
        assert_eq!(
            res,
            vec![ValidatorResponse {
                total_delegated: Default::default(),
                address: validator1.to_string()
            },]
        );
    }

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

    // assert that the exchange rate is 1:1
    let exchange_rate: Uint128 = tc.staking_hub.query(&app, &ExchangeRate {}).unwrap();
    assert_eq!(exchange_rate, Uint128::new(1));

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

        // run process withdraw
        tc.staking_hub.execute(
            &mut app,
            &tc.staking_hub.addr(),
            &ProcessWithdrawRequests {},
        ).unwrap();

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

    let pending_delegation:PendingDelegationRes = tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(pending_delegation, PendingDelegationRes {
        staking_epoch_length_blocks: 360,
        staking_epoch_start_block_height: 12240,
        pending_staking_amount: Uint128::new(1_500_000),
        pending_unstaking_amount: Uint128::zero(),
    });

    let res = app.next_epoch().unwrap();

    let pending_delegation2:PendingDelegationRes = tc.staking_hub.query(&app, &PendingDelegation {}).unwrap();
    assert_eq!(pending_delegation2, PendingDelegationRes {
        staking_epoch_length_blocks: 360,
        staking_epoch_start_block_height: 12240 + 360, // next epoch
        pending_staking_amount: Uint128::zero(),
        pending_unstaking_amount: Uint128::zero(),
    });
}
