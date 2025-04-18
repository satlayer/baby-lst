use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Instantiate the validator registry contract
#[cw_serde]
pub struct InstantiateMsg {
    /// Address of the validators to delegate
    pub validators: Vec<Validator>,
    /// Address of the hub contract
    pub hub_contract: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Add new validator in the registry
    AddValidator {
        /// Address of the validator
        validator: Validator,
    },

    /// Remove validator from the registry
    RemoveValidator {
        /// Address of the valid
        address: String,
    },

    /// Admin can update the config using this method
    UpdateConfig {
        /// Owner of the contract
        owner: Option<String>,
        /// Address of the hub contract
        hub_contract: Option<String>,
    },
    /// Process redelegations if validator is removed
    ProcessRedelegations {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Return the delegation done by the hub contract in the network
    #[returns(Vec<ValidatorResponse>)]
    ValidatorsDelegation {},
    /// Return the configuration parameters of the contract
    #[returns(Config)]
    Config {},
    #[returns(Vec<String>)]
    ExcludeList,
}

#[cw_serde]
pub struct ValidatorResponse {
    /// Total delegated amount for the validator
    #[serde(default)]
    pub total_delegated: Uint128,
    /// Address of the validator
    pub address: String,
}

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub hub_contract: Addr,
}

#[cw_serde]
pub struct Validator {
    pub address: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PendingRedelegation {
    pub src_validator: String,
    pub redelegations: Vec<(String, Coin)>,
    pub timestamp: u64,
}
