use crate::testing::TestSetup;
use cosmwasm_std::{Decimal, Uint128};

#[test]
fn test_instantiation() {
    let setup = TestSetup::new();

    let config = setup.get_config();
    assert_eq!(config.owner, setup.owner);
    assert_eq!(config.lst_token, None);
    assert_eq!(config.validators_registry_contract, None);
    assert_eq!(config.reward_dispatcher_contract, None);

    let state = setup.get_state();
    assert_eq!(state.lst_exchange_rate, Decimal::one());
    assert_eq!(state.total_lst_token_amount, Uint128::zero());

    let params = setup.get_parameters();
    assert_eq!(params.epoch_length, 100);
    assert_eq!(params.unstaking_period, 1000);
    assert_eq!(params.staking_coin_denom, "stake");
    assert_eq!(params.paused, false);
}
