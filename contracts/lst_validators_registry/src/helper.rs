use cosmwasm_std::{QuerierWrapper, Validator};
use lst_common::types::LstResult;

pub(crate) fn fetch_validator_info(
    querier: &QuerierWrapper,
    val_address: String,
) -> LstResult<Option<Validator>> {
    Ok(querier.query_validator(val_address)?)
}
