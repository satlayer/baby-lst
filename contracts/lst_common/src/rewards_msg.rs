use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{CanonicalAddr, Decimal};

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

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: CanonicalAddr,
    pub reward_denom: String,
    pub satlayer_fee_addr: CanonicalAddr,
    pub satlayer_fee_rate: Decimal,
}
