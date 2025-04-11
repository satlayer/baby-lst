pub mod delegation;
pub mod errors;
pub mod hub;
pub mod msg;
pub mod rewards_msg;
mod signed_integer;
pub mod types;
pub mod validator;

pub use crate::signed_integer::SignedInt;
use cosmwasm_std::{Addr, Deps};

use types::LstResult;
pub mod address;
pub mod babylon_msg;
pub mod babylon;
pub mod testing;

pub use crate::{
    delegation::calculate_delegations,
    errors::{ContractError, ValidatorError},
    msg::MigrateMsg,
};

#[allow(unused_variables)]
pub fn to_checked_address(deps: Deps, address: &str) -> LstResult<Addr> {
    #[cfg(test)]
    return Ok(Addr::unchecked(address));
    #[cfg(not(test))]
    return deps
        .api
        .addr_validate(address)
        .map_err(|_| ContractError::InvalidAddress);
}
