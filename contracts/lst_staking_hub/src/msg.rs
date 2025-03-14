use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

use crate::Config;

#[cw_serde]
pub struct InstantiateMsg {
    pub staking_token: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    Stake { amount: Uint128 },
    Unstake { amount: Uint128 },
    WithdrawUnstaked {},
    UpdateConfig { lst_token: String, staking_denom: String },
    UpdateParams { pause: bool },
    ClaimRewardsAndRestake {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(Uint128)]
    TotalStaked {},
    #[returns(Uint128)]
    ExchangeRate {},
}