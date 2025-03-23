use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::ValidatorResponse;

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryValidators {
    #[returns(Vec<ValidatorResponse>)]
    GetValidatorsForDelegation {},
}
