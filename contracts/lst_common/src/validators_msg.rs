use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{CanonicalAddr, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub validators: Vec<Validator>,
    pub hub_contract: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    AddValidator {
        validator: Validator,
    },

    RemoveValidator {
        address: String,
    },

    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Vec<ValidatorResponse>)]
    ValidatorsDelegation {},
    #[returns(Config)]
    Config {},
}

#[cw_serde]
pub struct ValidatorResponse {
    #[serde(default)]
    pub total_delegated: Uint128,
    pub address: String,
}

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: CanonicalAddr,
}

#[cw_serde]
pub struct Validator {
    pub address: String,
}
