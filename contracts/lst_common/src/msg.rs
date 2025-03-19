use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct ValidatorResponse {
    #[serde(default)]
    pub total_delegated: Uint128,
    pub address: String,
}

#[cw_serde]
pub struct MigrateMsg {}
