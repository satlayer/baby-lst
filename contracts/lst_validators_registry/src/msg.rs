use cosmwasm_schema::{QueryResponses, cw_serde};

use lst_common::msg::ValidatorResponse;

use crate::state::{Config, Validator};

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
