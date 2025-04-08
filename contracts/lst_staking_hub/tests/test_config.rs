use cosmwasm_std::{
    testing::{message_info, mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage},
    DistributionMsg, Empty, OwnedDeps, Response,
};

use lst_common::{
    errors::HubError,
    hub::{InstantiateMsg, Parameters},
    ContractError,
};

use lst_staking_hub::{
    config::{execute_update_config, execute_update_params},
    contract::instantiate,
    state::{CONFIG, PARAMETERS},
};

const OWNER: &str = "owner";
const NEW_OWNER: &str = "new_owner";
const NOT_OWNER: &str = "not_owner";
const LST_TOKEN: &str = "lst_token";
const VALIDATOR_REGISTRY: &str = "validator_registry";
const REWARD_DISPATCHER: &str = "reward_dispatcher";

// Helper function to setup the contract with default configuration
fn setup_contract(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>,
    staking_epoch_start_block_height: u64,
) {
    // Setup contract
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let msg = InstantiateMsg {
        epoch_length: 100,
        unstaking_period: 200,
        staking_coin_denom: "uatom".to_string(),
        staking_epoch_length_blocks: 4,
        staking_epoch_start_block_height,
    };
    instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
}

// Helper function to setup test parameters
fn setup_test_params(deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>) -> Parameters {
    let params = Parameters {
        paused: false,
        epoch_length: 100,
        unstaking_period: 200,
        staking_coin_denom: "uatom".to_string(),
    };
    PARAMETERS.save(deps.as_mut().storage, &params).unwrap();
    params
}

// Helper function to check unauthorized access
fn test_unauthorized_access<F>(
    deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>,
    action: F,
) where
    F: FnOnce(
        &mut OwnedDeps<MockStorage, MockApi, MockQuerier<Empty>>,
        String,
    ) -> Result<Response, ContractError>,
{
    let info = message_info(&deps.api.addr_make(NOT_OWNER), &[]);
    let res = action(deps, info.sender.to_string());
    assert_eq!(res, Err(ContractError::Unauthorized {}));
}

#[test]
fn test_update_config_owner() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    // Check initial config
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    assert_eq!(config.owner, &deps.api.addr_make(OWNER));
    assert_eq!(config.lst_token, None);
    assert_eq!(config.validators_registry_contract, None);
    assert_eq!(config.reward_dispatcher_contract, None);

    // Update owner
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let new_owner = deps.api.addr_make(NEW_OWNER).to_string();
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        Some(new_owner),
        None,
        None,
        None,
    )
    .unwrap();

    // Check response attributes
    assert_eq!(
        res.attributes,
        vec![
            ("action", "update_config"),
            ("owner", deps.api.addr_make(NEW_OWNER).as_ref()),
            ("lst_token", "None"),
            ("reward_dispatcher", "None"),
            ("validator_registry", "None"),
        ]
    );

    // Check config was updated
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    assert_eq!(config.owner, deps.api.addr_make(NEW_OWNER));
}

#[test]
fn test_update_config_unauthorized() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    let info = message_info(&deps.api.addr_make(NOT_OWNER), &[]);
    test_unauthorized_access(&mut deps, |deps, _| {
        execute_update_config(
            deps.as_mut(),
            mock_env(),
            info,
            Some(NEW_OWNER.to_string()),
            None,
            None,
            None,
        )
    });
}

#[test]
fn test_update_config_lst_token() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    // Set LST token
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let lst_token = deps.api.addr_make(LST_TOKEN).to_string();
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        None,
        Some(lst_token),
        None,
        None,
    )
    .unwrap();

    // Check response attributes
    assert_eq!(
        res.attributes,
        vec![
            ("action", "update_config"),
            ("owner", deps.api.addr_make(OWNER).as_ref()),
            ("lst_token", deps.api.addr_make(LST_TOKEN).as_ref()),
            ("reward_dispatcher", "None"),
            ("validator_registry", "None"),
        ]
    );

    // Check config was updated
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    assert_eq!(config.lst_token, Some(deps.api.addr_make(LST_TOKEN)));

    // Try to set LST token again with different address
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let different_token = deps.api.addr_make("different_token").to_string();
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        None,
        Some(different_token),
        None,
        None,
    );

    // Check error
    assert_eq!(res, Err(ContractError::Hub(HubError::LstTokenAlreadySet)));

    // Try to set LST token again with same address (should succeed)
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let lst_token = deps.api.addr_make(LST_TOKEN).to_string();
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        None,
        Some(lst_token),
        None,
        None,
    );

    // Should succeed
    assert!(res.is_ok());
}

#[test]
fn test_update_config_validator_registry() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    // Set validator registry
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let validator_registry = deps.api.addr_make(VALIDATOR_REGISTRY).to_string();
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        None,
        None,
        Some(validator_registry.clone()),
        None,
    )
    .unwrap();

    // Check response attributes
    assert_eq!(
        res.attributes,
        vec![
            ("action", "update_config"),
            ("owner", deps.api.addr_make(OWNER).as_ref()),
            ("lst_token", "None"),
            ("reward_dispatcher", "None"),
            ("validator_registry", &validator_registry),
        ]
    );

    // Check config was updated
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    assert_eq!(
        config.validators_registry_contract,
        Some(deps.api.addr_make(VALIDATOR_REGISTRY))
    );
}

#[test]
fn test_update_config_reward_dispatcher() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    // Set reward dispatcher
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let reward_dispatcher = deps.api.addr_make(REWARD_DISPATCHER);
    let res = execute_update_config(
        deps.as_mut(),
        mock_env(),
        info,
        None,
        None,
        None,
        Some(reward_dispatcher.to_string()),
    )
    .unwrap();

    // Check response attributes
    assert_eq!(
        res.attributes,
        vec![
            ("action", "update_config"),
            ("owner", deps.api.addr_make(OWNER).as_ref()),
            ("lst_token", "None"),
            (
                "reward_dispatcher",
                deps.api.addr_make(REWARD_DISPATCHER).as_ref()
            ),
            ("validator_registry", "None"),
        ]
    );

    // Check config was updated
    let config = CONFIG.load(deps.as_ref().storage).unwrap();
    assert_eq!(
        config.reward_dispatcher_contract,
        Some(deps.api.addr_make(REWARD_DISPATCHER))
    );

    // Check that the message to set withdrawal address was added
    assert_eq!(res.messages.len(), 1);
    match &res.messages[0].msg {
        cosmwasm_std::CosmosMsg::Distribution(DistributionMsg::SetWithdrawAddress { address }) => {
            assert_eq!(address, &deps.api.addr_make(REWARD_DISPATCHER).to_string());
        }
        _ => panic!("Expected SetWithdrawAddress message"),
    }
}

#[test]
fn test_update_params_pause() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    let params = setup_test_params(&mut deps);

    // Update parameters to pause the contract
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let res =
        execute_update_params(deps.as_mut(), mock_env(), info, Some(true), None, None).unwrap();

    // Check response attributes
    assert_eq!(
        res.attributes,
        vec![
            ("action", "update_params"),
            ("paused", "true"),
            ("staking_coin_denom", &params.staking_coin_denom),
            ("epoch_length", &params.epoch_length.to_string()),
            ("unstaking_period", &params.unstaking_period.to_string()),
        ]
    );

    // Check parameters were updated
    let updated_params = PARAMETERS.load(deps.as_ref().storage).unwrap();
    assert!(updated_params.paused);
    assert_eq!(updated_params.epoch_length, params.epoch_length);
    assert_eq!(updated_params.unstaking_period, params.unstaking_period);
    assert_eq!(updated_params.staking_coin_denom, params.staking_coin_denom);
}

#[test]
fn test_update_params_epoch_length() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    setup_test_params(&mut deps);

    // Test valid epoch length update
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let _res =
        execute_update_params(deps.as_mut(), mock_env(), info, None, Some(150), None).unwrap();

    let params = PARAMETERS.load(deps.as_ref().storage).unwrap();
    assert_eq!(params.epoch_length, 150);

    // Test invalid cases
    let test_cases = vec![
        (604801, HubError::InvalidEpochLength), // 7 days + 1 second
        (200, HubError::InvalidPeriods),        // Same as unstaking period
    ];

    for (epoch_length, expected_error) in test_cases {
        let info = message_info(&deps.api.addr_make(OWNER), &[]);
        let res = execute_update_params(
            deps.as_mut(),
            mock_env(),
            info,
            None,
            Some(epoch_length),
            None,
        );
        assert_eq!(res, Err(ContractError::Hub(expected_error)));
    }
}

#[test]
fn test_update_params_unstaking_period() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    setup_test_params(&mut deps);

    // Test valid unstaking period update
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let _res =
        execute_update_params(deps.as_mut(), mock_env(), info, None, None, Some(300)).unwrap();

    let params = PARAMETERS.load(deps.as_ref().storage).unwrap();
    assert_eq!(params.unstaking_period, 300);

    // Test invalid cases
    let test_cases = vec![
        (2419201, HubError::InvalidUnstakingPeriod), // 4 weeks + 1 second
        (100, HubError::InvalidPeriods),             // Same as epoch length
    ];

    for (unstaking_period, expected_error) in test_cases {
        let info = message_info(&deps.api.addr_make(OWNER), &[]);
        let res = execute_update_params(
            deps.as_mut(),
            mock_env(),
            info,
            None,
            None,
            Some(unstaking_period),
        );
        assert_eq!(res, Err(ContractError::Hub(expected_error)));
    }
}

#[test]
fn test_update_params_both_periods() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    setup_test_params(&mut deps);

    // Test valid update of both periods
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let _res =
        execute_update_params(deps.as_mut(), mock_env(), info, None, Some(150), Some(300)).unwrap();

    let params = PARAMETERS.load(deps.as_ref().storage).unwrap();
    assert_eq!(params.epoch_length, 150);
    assert_eq!(params.unstaking_period, 300);

    // Test invalid update
    let info = message_info(&deps.api.addr_make(OWNER), &[]);
    let res = execute_update_params(deps.as_mut(), mock_env(), info, None, Some(300), Some(300));
    assert_eq!(res, Err(ContractError::Hub(HubError::InvalidPeriods)));
}

#[test]
fn test_update_params_unauthorized() {
    let mut deps = mock_dependencies();
    setup_contract(&mut deps, 10);

    test_unauthorized_access(&mut deps, |deps, sender| {
        let info = message_info(&deps.api.addr_make(&sender), &[]);
        execute_update_params(deps.as_mut(), mock_env(), info, Some(true), None, None)
    });
}
