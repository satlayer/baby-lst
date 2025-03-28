use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal};

/// Instantiate the reward contract
#[cw_serde]
pub struct InstantiateMsg {
    /// Address of the staking hub contract
    pub hub_contract: String,
    /// Denom of the staking reward token
    pub reward_denom: String,
    /// Address to receive the fee from the rewards
    pub satlayer_fee_addr: String,
    /// Rate at which fee is taken from rewards
    pub satlayer_fee_rate: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Update the config by admin
    UpdateConfig {
        /// Owner of the contract
        owner: Option<String>,
        /// Address of the hub contract
        hub_contract: Option<String>,
        /// Address to receive the fee from the rewards
        satlayer_fee_addr: Option<String>,
        /// Rate at which fee is taken from rewards
        satlayer_fee_rate: Option<Decimal>,
    },
    /// Dispatch the rewards to the staking hub contract and stake those rewards
    DispatchRewards {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the config values of the contract
    #[returns(Config)]
    Config {},
}

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub hub_contract: Addr,
    pub reward_denom: String,
    pub satlayer_fee_addr: Addr,
    pub satlayer_fee_rate: Decimal,
}
