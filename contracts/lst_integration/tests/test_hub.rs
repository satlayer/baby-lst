mod setup;

use cosmwasm_std::{coin, Decimal, Uint128};
use cw20_base::msg::ExecuteMsg as Cw20ExecuteMsg;
use cw_multi_test::IntoBech32;
use lst_common::{
    hub::{AllHistoryResponse, QueryMsg as HubQueryMsg, State, WithdrawableUnstakedResponse},
    validator::{QueryMsg as ValidatorQueryMsg, Validator, ValidatorResponse},
    ContractError,
};
use setup::{ContractType, TestContext, FEE_ADDR, NATIVE_DENOM, UNBOUND_TIME, VALIDATOR_ADDR};

use lst_common::hub::ExecuteMsg as HubExecuteMsg;

pub const DELEGATOR: &str = "Delegator";

fn setup_contracts(mut ctx: TestContext) -> TestContext {
    ctx.init_hub_contract(100, UNBOUND_TIME);

    let hub_addr = ctx
        .get_contract_addr(ContractType::Hub)
        .unwrap()
        .to_string();

    ctx.init_reward_contract(
        TestContext::make_addr(FEE_ADDR).to_string(),
        Decimal::from_ratio(Uint128::new(5u128), Uint128::new(100u128)),
        hub_addr.clone(),
    );

    let validator_addr = VALIDATOR_ADDR.into_bech32_with_prefix("bbnvaloper");

    ctx.init_validator_registry(
        hub_addr.clone(),
        vec![Validator {
            address: validator_addr.to_string(),
        }],
    );

    ctx.init_token_contract(hub_addr);

    let _ = ctx
        .execute(
            ContractType::Hub,
            None,
            &HubExecuteMsg::UpdateConfig {
                owner: None,
                lst_token: ctx.get_contract_addr(ContractType::Token),
                validator_registry: ctx.get_contract_addr(ContractType::ValidatorRegistry),
                reward_dispatcher: ctx.get_contract_addr(ContractType::Reward),
            },
            &[],
        )
        .unwrap();

    ctx
}

fn setup_test() -> TestContext {
    let mut ctx = TestContext::new();
    ctx = setup_contracts(ctx);
    ctx
}

#[test]
fn test_staking_flow() {
    let delegated_amt = 10000000000u128;
    let initial_balance = 10000000000000000u128;
    let after_stake_balace = initial_balance - delegated_amt;

    // mint token to delegator
    let mut ctx = setup_test();
    ctx.mint_token(TestContext::make_addr(DELEGATOR), initial_balance);

    // send stake message
    ctx.execute(
        ContractType::Hub,
        Some(TestContext::make_addr(DELEGATOR)),
        &HubExecuteMsg::Stake {},
        &[coin(delegated_amt, NATIVE_DENOM)],
    )
    .unwrap();

    // hub contract state
    let state: State = ctx
        .query(ContractType::Hub, &HubQueryMsg::State {})
        .unwrap();

    println!("these are the state: {:?}", state);

    // query validator delegation
    let delegations = ctx.query::<Vec<ValidatorResponse>>(
        ContractType::ValidatorRegistry,
        &ValidatorQueryMsg::ValidatorsDelegation {},
    );

    println!("these are validator: {:?}", delegations);

    // send increase allowance msg to token contract
    ctx.execute(
        ContractType::Token,
        Some(TestContext::make_addr(DELEGATOR)),
        &Cw20ExecuteMsg::IncreaseAllowance {
            spender: ctx.get_contract_addr(ContractType::Hub).unwrap(),
            amount: Uint128::from(delegated_amt),
            expires: None,
        },
        &[],
    )
    .unwrap();

    // query delegator balance msg
    let delegator_balance = ctx
        .app
        .wrap()
        .query_balance(TestContext::make_addr(DELEGATOR), NATIVE_DENOM)
        .unwrap();

    assert_eq!(delegator_balance.amount.u128(), after_stake_balace);

    // increase the block time
    ctx.app.update_block(|block| {
        block.height += 10;
        block.time = block.time.plus_seconds(100);
    });

    // send unstake message
    ctx.execute(
        ContractType::Hub,
        Some(TestContext::make_addr(DELEGATOR)),
        &HubExecuteMsg::Unstake {
            amount: Uint128::from(delegated_amt),
        },
        &[],
    )
    .unwrap();

    // update the block time to more than unbound time
    ctx.app.update_block(|block| {
        block.height += 1;
        block.time = block.time.plus_seconds(UNBOUND_TIME + 10);
    });

    let all_histories: AllHistoryResponse = ctx
        .query(
            ContractType::Hub,
            &HubQueryMsg::AllHistory {
                start_from: Some(1),
                limit: Some(20),
            },
        )
        .unwrap();
    println!("thse are the all-histories {:?}", all_histories);

    ctx.execute(
        ContractType::Hub,
        Some(TestContext::make_addr(DELEGATOR)),
        &HubExecuteMsg::ProcessUndelegations {},
        &[],
    )
    .unwrap();

    ctx.app.update_block(|block| {
        block.height += 1;
        block.time = block.time.plus_seconds(UNBOUND_TIME);
    });

    let hub_balance = ctx
        .app
        .wrap()
        .query_balance(
            ctx.get_contract_addr(ContractType::Hub).unwrap(),
            NATIVE_DENOM,
        )
        .unwrap();

    println!("this is the hub balance:  {:?}", hub_balance);

    // ctx.execute(
    //     ContractType::Hub,
    //     Some(TestContext::make_addr(DELEGATOR)),
    //     &HubExecuteMsg::ProcessWithdrawRequests {},
    //     &[],
    // )
    // .unwrap();

    let withdrawable_requests: WithdrawableUnstakedResponse = ctx
        .query(
            ContractType::Hub,
            &HubQueryMsg::WithdrawableUnstaked {
                address: TestContext::make_addr(DELEGATOR).to_string(),
            },
        )
        .unwrap();

    println!(
        " these are WithdrawableUnstaked: {:?}",
        withdrawable_requests
    );

    ctx.execute(
        ContractType::Hub,
        Some(TestContext::make_addr(DELEGATOR)),
        &HubExecuteMsg::WithdrawUnstaked {},
        &[],
    )
    .unwrap();

    let delegator_balance = ctx
        .app
        .wrap()
        .query_balance(TestContext::make_addr(DELEGATOR), NATIVE_DENOM)
        .unwrap();

    assert_eq!(delegator_balance.amount.u128(), initial_balance);

    let delegations = ctx.query::<Vec<ValidatorResponse>>(
        ContractType::ValidatorRegistry,
        &ValidatorQueryMsg::ValidatorsDelegation {},
    );

    println!("these are validator after all: {:?}", delegations);
}

#[test]
fn update_config() {
    setup_test();
}
