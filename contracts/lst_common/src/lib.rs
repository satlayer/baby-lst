pub mod errors;
pub mod hub;

use cosmwasm_std::{Addr, Deps};

pub use crate::errors::ContractError;

pub fn to_checked_address(deps: Deps, address: &str) -> Result<Addr, ContractError> {
    #[cfg(test)]
    return Ok(Addr::unchecked(address));
    #[cfg(not(test))]
    return deps
        .api
        .addr_validate(address)
        .map_err(|_| ContractError::InvalidAddress);
}
