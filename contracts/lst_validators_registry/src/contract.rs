use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult, entry_point};
use cw2::set_contract_version;

use lst_common::{ContractError, to_checked_address};

use crate::{
    msg::InstantiateMsg,
    state::{CONFIG, Config, VALIDATOR_REGISTRY},
};

const CONTRACT_NAME: &str = "crates.io:validator-registry";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)
        .map_err(|_| ContractError::FailedToInitContract)?;

    let hub_contract = to_checked_address(deps.as_ref(), msg.hub_contract.as_ref())?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(info.sender.as_str())?,
            hub_contract: deps.api.addr_canonicalize(hub_contract.as_str())?,
        },
    )?;

    msg.validators
        .into_iter()
        .try_for_each(|val| -> Result<(), ContractError> {
            let checked_address = to_checked_address(deps.as_ref(), val.address.as_str())?;
            VALIDATOR_REGISTRY.save(deps.storage, checked_address.as_bytes(), &val)?;
            Ok(())
        })?;
    Ok(Response::default())
}
