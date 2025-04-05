pub mod delegation;
pub mod errors;
pub mod hub;
pub mod msg;
pub mod rewards_msg;
mod signed_integer;
pub mod types;
pub mod validator;

pub use crate::signed_integer::SignedInt;
use cosmwasm_std::{Addr, CanonicalAddr, Deps};
use semver::Version;

use types::LstResult;
pub mod babylon_msg;

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

/// Validates if the migration is allowed based on the current and new versions.
pub fn validate_migration(
    deps: Deps,
    contract_name: &str,
    new_version: &str,
) -> Result<(), ContractError> {
    let stored_version = cw2::get_contract_version(deps.storage)?;

    if stored_version.contract != contract_name {
        return Err(ContractError::InvalidContractType);
    }

    let parsed_new_version = new_version.parse::<Version>().unwrap();
    let parsed_current_version = stored_version.version.parse::<Version>().unwrap();

    if parsed_current_version >= parsed_new_version {
        return Err(ContractError::MigrationNotAllowed(
            stored_version.version,
            new_version.to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_dependencies;
    use cw2::set_contract_version;

    const CONTRACT_NAME: &str = "contract";
    const CONTRACT_VERSION: &str = "0.1.0";

    #[test]
    fn test_valid_migration() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, CONTRACT_NAME, CONTRACT_VERSION).unwrap();

        let result = validate_migration(deps.as_ref(), CONTRACT_NAME, "0.2.0");
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_contract_name() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, CONTRACT_NAME, CONTRACT_VERSION).unwrap();

        let result = validate_migration(deps.as_ref(), "other_contract", "0.2.0");
        assert_eq!(result, Err(ContractError::InvalidContractType));
    }

    #[test]
    fn test_same_version_migration() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, CONTRACT_NAME, CONTRACT_VERSION).unwrap();

        let result = validate_migration(deps.as_ref(), CONTRACT_NAME, CONTRACT_VERSION);
        assert_eq!(
            result,
            Err(ContractError::MigrationNotAllowed(
                CONTRACT_VERSION.to_string(),
                CONTRACT_VERSION.to_string()
            ))
        );
    }

    #[test]
    fn test_downgrade_migration() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, CONTRACT_NAME, "0.2.0").unwrap();

        let result = validate_migration(deps.as_ref(), CONTRACT_NAME, CONTRACT_VERSION);
        assert_eq!(
            result,
            Err(ContractError::MigrationNotAllowed(
                "0.2.0".to_string(),
                CONTRACT_VERSION.to_string()
            ))
        );
    }

    #[test]
    fn test_multiple_version_jump() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, CONTRACT_NAME, CONTRACT_VERSION).unwrap();

        let result = validate_migration(deps.as_ref(), CONTRACT_NAME, "0.3.0");
        assert!(result.is_ok());
    }
}
