use cosmwasm_schema::cw_serde;

use crate::state::Validator;

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
pub enum QueryMsg {
    ValidatorsDelegation {},
    Config {},
}
