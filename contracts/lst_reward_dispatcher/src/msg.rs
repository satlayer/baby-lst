use cosmwasm_schema::{QueryResponses, cw_serde};
use cosmwasm_std::Decimal;

use crate::state::Config;

#[cw_serde]
pub struct InstantiateMsg {
    pub hub_contract: String,
    pub reward_denom: String,
    pub satlayer_fee_addr: String,
    pub satlayer_fee_rate: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
        satlayer_fee_addr: Option<String>,
        satlayer_fee_rate: Option<Decimal>,
    },
    DispatchRewards {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
}
