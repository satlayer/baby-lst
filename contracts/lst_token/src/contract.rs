use std::env;

use cosmwasm_std::{
    entry_point, to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::MinterResponse;
use cw20_base::{
    allowances::{
        execute_burn_from as cw20_burn_from, execute_decrease_allowance,
        execute_increase_allowance, execute_send_from, execute_transfer_from,
    },
    contract::{
        execute_burn as cw20_burn, execute_mint, execute_send, execute_transfer,
        execute_update_marketing, execute_update_minter, execute_upload_logo,
        instantiate as cw20_init, query as cw20_query,
    },
    msg::{ExecuteMsg, InstantiateMsg as Cw20InstantiateMsg, MigrateMsg, QueryMsg},
    ContractError,
};

use lst_common::hub::ExecuteMsg::CheckSlashing;
use lst_common::types::LstResult;
use crate::{msg::InstantiateMsg, state::HUB_CONTRACT};

const CONTRACT_NAME: &str = concat!("crates.io:", env!("CARGO_PKG_NAME"));
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    HUB_CONTRACT.save(deps.storage, &deps.api.addr_validate(&msg.hub_contract)?)?;

    let InstantiateMsg {
        name,
        symbol,
        decimals,
        hub_contract,
        marketing,
    } = msg;

    cw20_init(
        deps,
        env,
        info,
        Cw20InstantiateMsg {
            name,
            symbol,
            decimals,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: hub_contract,
                cap: None,
            }),
            marketing,
        },
    )
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }

        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),

        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),

        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),

        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),

        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),

        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),

        ExecuteMsg::UpdateMinter { new_minter } => {
            execute_update_minter(deps, env, info, new_minter)
        }

        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),

        ExecuteMsg::UploadLogo(logo) => execute_upload_logo(deps, env, info, logo),

        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),

        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    cw20_query(deps, env, msg)
}

/// This can only be called by the contract ADMIN, enforced by `wasmd` separate from cosmwasm.
/// See https://github.com/CosmWasm/cosmwasm/issues/926#issuecomment-851259818
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: lst_common::MigrateMsg) -> LstResult<Response> {
    cw2::ensure_from_older_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let message = build_check_slashing_sub_msg(deps.as_ref())?;

    let res = cw20_burn(deps, env, info, amount)?;
    Ok(Response::new()
        .add_submessage(message)
        .add_attributes(res.attributes))
}

fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let message = build_check_slashing_sub_msg(deps.as_ref())?;

    let res = cw20_burn_from(deps, env, info, owner, amount)?;
    Ok(Response::default()
        .add_submessage(message)
        .add_attributes(res.attributes))
}

fn build_check_slashing_sub_msg(deps: Deps) -> Result<SubMsg, ContractError> {
    let hub_contract = HUB_CONTRACT.load(deps.storage)?;

    Ok(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: hub_contract.to_string(),
        msg: to_json_binary(&CheckSlashing {})?,
        funds: vec![],
    })))
}
