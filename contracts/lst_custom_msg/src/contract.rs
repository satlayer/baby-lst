#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError};
use cw2::set_contract_version;

use crate::{
    error::ContractError,
    msg::{ExecuteMsg, InstantiateMsg},
};

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", msg.owner))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Delegate {} => execute::delegate(deps, info, env),
    }
}

pub mod execute {
    use cosmos_sdk_proto::traits::MessageExt;
    use cosmwasm_std::AnyMsg;
    use lst_common::babylon::epoching::v1::MsgWrappedDelegate;

    use super::*;

    pub fn delegate(
        _deps: DepsMut,
        info: MessageInfo,
        env: Env,
    ) -> Result<Response, ContractError> {
        let amount = cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
            denom: info.funds[0].denom.clone(),
            amount: info.funds[0].amount.to_string(),
        };
        let delegate_msg = cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate {
            delegator_address: env.contract.address.to_string(),
            validator_address: "bbnvaloper109x4ruspxarwt62puwcenhclw36l9v7j92f0ex".to_string(),
            amount: Some(amount),
        };
        let bytes = MsgWrappedDelegate {
            msg: Some(delegate_msg),
        }
        .to_bytes()
        .map_err(|_| StdError::generic_err("Failed to serialize MsgWrappedDelegate"))?;

        let msg: CosmosMsg = CosmosMsg::Any(AnyMsg {
            type_url: "/babylon.epoching.v1.MsgWrappedDelegate".to_string(),
            value: Binary::from(bytes),
        });
        Ok(Response::new()
            .add_attribute("action", "delegate")
            .add_attribute("amount", info.funds[0].amount.to_string())
            .add_attribute("delegator_address", env.contract.address.to_string())
            .add_message(msg))
    }
}
