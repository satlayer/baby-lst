use cosmwasm_std::{
    coins,
    testing::{message_info, mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage},
    to_json_binary, Addr, Coin, ContractResult, Decimal, Empty, OwnedDeps, SystemResult, Uint128,
    WasmQuery,
};

use cw20::TokenInfoResponse;

use lst_common::{
    errors::HubError,
    hub::{Config, CurrentBatch, InstantiateMsg, Parameters, State},
    validator::ValidatorResponse,
    ContractError,
};

use lst_staking_hub::{
    contract::instantiate,
    stake::execute_stake,
    state::{StakeType, CONFIG, CURRENT_BATCH, PARAMETERS, STATE},
};

fn mock_querier_with_balance(
    balances: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>> {
    let mut deps = mock_dependencies();
    deps.querier
        .bank
        .update_balance(Addr::unchecked("contract"), balances.to_vec());
    deps
}

// Helper function to setup the contract with default configuration
fn setup_contract(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>,
    staking_epoch_start_block_height: u64,
) {
    // Setup contract
    let info = message_info(&Addr::unchecked("creator"), &[]);
    let msg = InstantiateMsg {
        epoch_length: 100,
        unstaking_period: 200,
        staking_epoch_length_blocks: 4,
        staking_coin_denom: "uatom".to_string(),
        staking_epoch_start_block_height,
    };
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // Setup config
    let config = Config {
        owner: Addr::unchecked("creator"),
        lst_token: Some(Addr::unchecked("lst_token")),
        validators_registry_contract: Some(Addr::unchecked("validator_registry")),
        reward_dispatcher_contract: Some(Addr::unchecked("reward_dispatcher")),
    };
    CONFIG.save(deps.as_mut().storage, &config).unwrap();

    // Setup parameters
    let params = Parameters {
        paused: false,
        epoch_length: 100,
        unstaking_period: 200,
        staking_coin_denom: "uatom".to_string(),
    };
    PARAMETERS.save(deps.as_mut().storage, &params).unwrap();

    // Setup state
    let state = State {
        lst_exchange_rate: Decimal::one(),
        total_staked_amount: Uint128::zero(),
        last_index_modification: 0,
        unclaimed_unstaked_balance: Uint128::zero(),
        last_unbonded_time: 0,
        last_processed_batch: 0,
    };
    STATE.save(deps.as_mut().storage, &state).unwrap();

    // Setup current batch
    let current_batch = CurrentBatch {
        id: 1,
        requested_lst_token_amount: Uint128::zero(),
    };
    CURRENT_BATCH
        .save(deps.as_mut().storage, &current_batch)
        .unwrap();
}

// Helper function to mock validator and token responses
fn mock_validator_and_token_responses(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>,
    total_supply: Uint128,
) {
    let validators = vec![ValidatorResponse {
        address: "validator1".to_string(),
        total_delegated: Uint128::from(10000u128),
    }];
    
    let token_info = TokenInfoResponse {
        name: "LST Token".to_string(),
        symbol: "LST".to_string(),
        decimals: 6,
        total_supply,
    };
    
    deps.querier.update_wasm(move |query| match query {
        WasmQuery::Smart { contract_addr, msg } => {
            if contract_addr == "lst_token" && msg.as_slice().starts_with(b"{\"token_info\":") {
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&token_info).unwrap()))
            } else {
                SystemResult::Ok(ContractResult::Ok(to_json_binary(&validators).unwrap()))
            }
        }
        _ => SystemResult::Ok(ContractResult::Ok(to_json_binary(&validators).unwrap())),
    });
}

#[test]
fn test_execute_stake_lst_mint() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 2);
    mock_validator_and_token_responses(&mut deps, Uint128::zero());

    // Execute stake
    let info = message_info(&Addr::unchecked("user"), &coins(100, "uatom"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::LSTMint).unwrap();

    // Verify response
    assert_eq!(res.messages.len(), 2); // One for delegation, one for minting
    assert_eq!(
        res.attributes,
        vec![
            ("action", "mint"),
            ("from", "user"),
            ("staked", "100"),
            ("minted", "100")
        ]
    );

    // Verify state updates
    let state = STATE.load(deps.as_ref().storage).unwrap();
    assert_eq!(state.total_staked_amount, Uint128::new(100));
    assert_eq!(state.lst_exchange_rate, Decimal::one());
}

#[test]
fn test_execute_stake_rewards() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 10);
    mock_validator_and_token_responses(&mut deps, Uint128::zero());

    // First execute normal stake to create some LST tokens
    let info = message_info(&Addr::unchecked("user"), &coins(900, "uatom"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::LSTMint).unwrap();

    // Verify first stake response
    assert_eq!(res.messages.len(), 2); // One for delegation, one for minting
    assert_eq!(
        res.attributes,
        vec![
            ("action", "mint"),
            ("from", "user"),
            ("staked", "900"),
            ("minted", "900")
        ]
    );

    // Verify state after first stake
    let state = STATE.load(deps.as_ref().storage).unwrap();
    assert_eq!(state.total_staked_amount, Uint128::new(900));
    assert_eq!(state.lst_exchange_rate, Decimal::one());
    
    // Update mock for stake rewards
    mock_validator_and_token_responses(&mut deps, Uint128::new(900));
    
    // Now execute stake rewards
    let info = message_info(&Addr::unchecked("reward_dispatcher"), &coins(100, "uatom"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::StakeRewards).unwrap();

    // Verify stake rewards response
    assert_eq!(res.messages.len(), 1); // Only delegation message, no minting
    assert_eq!(
        res.attributes,
        vec![
            ("action", "stake_rewards"),
            ("from", "reward_dispatcher"),
            ("amount", "100")
        ]
    );

    // Verify state updates after stake rewards
    let state = STATE.load(deps.as_ref().storage).unwrap();
    assert_eq!(state.total_staked_amount, Uint128::new(1000)); // 900 + 100
    // Exchange rate should now be 1000/900 = 1.111...
    assert_eq!(state.lst_exchange_rate, Decimal::from_ratio(1000u128, 900u128));
}

#[test]
fn test_execute_stake_unauthorized_rewards() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 2);

    // Try to stake rewards with unauthorized address
    let info = message_info(&Addr::unchecked("unauthorized"), &coins(100, "uatom"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::StakeRewards);

    // Verify error
    assert_eq!(res, Err(ContractError::Unauthorized {}.into()));
}

#[test]
fn test_execute_stake_invalid_amount() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 2);

    // Try to stake with invalid amount
    let info = message_info(&Addr::unchecked("user"), &coins(0, "uatom"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::LSTMint);

    // Verify error
    assert_eq!(res, Err(ContractError::Hub(HubError::InvalidAmount).into()));
}

#[test]
fn test_execute_stake_invalid_denom() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 2);

    // Try to stake with invalid denom
    let info = message_info(&Addr::unchecked("user"), &coins(100, "invalid"));
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::LSTMint);

    // Verify error
    assert_eq!(res, Err(ContractError::Hub(HubError::InvalidAmount).into()));
}

#[test]
fn test_execute_stake_multiple_coins() {
    let mut deps = mock_querier_with_balance(&[Coin {
        denom: "uatom".to_string(),
        amount: Uint128::new(1000),
    }]);

    setup_contract(&mut deps, 2);
    mock_validator_and_token_responses(&mut deps, Uint128::zero());

    // Try to stake with multiple coins
    let info = message_info(
        &Addr::unchecked("user"),
        &[
            Coin {
                denom: "uatom".to_string(),
                amount: Uint128::new(100),
            },
            Coin {
                denom: "uatom".to_string(),
                amount: Uint128::new(50),
            },
        ],
    );
    let res = execute_stake(deps.as_mut(), mock_env(), info, StakeType::LSTMint);

    // Verify error
    assert_eq!(
        res,
        Err(ContractError::Hub(HubError::OnlyOneCoinAllowed).into())
    );
}
