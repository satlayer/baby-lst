pub mod delegation;
pub mod errors;
pub mod hub;
pub mod msg;
pub mod rewards_msg;
mod signed_integer;
pub mod types;
pub mod validator;
pub mod binding;

pub mod babylon {
    pub mod epoching {
        // @@protoc_insertion_point(attribute:babylon.epoching.v1)
        pub mod v1 {
            include!("babylon.epoching.v1.rs");
            // @@protoc_insertion_point(babylon.epoching.v1)
        }
    }
}

pub use crate::signed_integer::SignedInt;
use cosmwasm_std::{Addr, CanonicalAddr, Deps};
use types::LstResult;

pub use crate::{
    delegation::calculate_delegations,
    errors::{ContractError, ValidatorError},
    msg::MigrateMsg,
};

pub fn to_checked_address(deps: Deps, address: &str) -> LstResult<Addr> {
    #[cfg(test)]
    return Ok(Addr::unchecked(address));
    #[cfg(not(test))]
    return deps
        .api
        .addr_validate(address)
        .map_err(|_| ContractError::InvalidAddress);
}

pub fn to_canoncial_addr(deps: Deps, addr: &str) -> LstResult<CanonicalAddr> {
    Ok(deps.api.addr_canonicalize(addr)?)
}
