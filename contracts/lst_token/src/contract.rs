use std::env;

use cosmwasm_std::{
    Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, entry_point,
};
use cw2::set_contract_version;
use cw20::MinterResponse;
use cw20_base::{
    ContractError,
    allowances::{
        execute_burn_from as cw20_burn_from, execute_decrease_allowance,
        execute_increase_allowance, execute_send_from, execute_transfer_from,
    },
    contract::{
        execute_burn as cw20_burn, execute_mint, execute_send, execute_transfer,
        execute_update_marketing, execute_update_minter, execute_upload_logo,
        instantiate as cw20_init, query as cw20_query,
    },
    msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg},
};

use crate::{msg::TokenInitMsg, state::HUB_CONTRACT};

const CONTRACT_NAME: &str = "crates.io:satBaby-lst";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: TokenInitMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION).unwrap();

    HUB_CONTRACT.save(deps.storage, &deps.api.addr_validate(&msg.hub_contract)?)?;

    let TokenInitMsg {
        name,
        symbol,
        decimals,
        initial_balances,
        hub_contract,
        marketing,
    } = msg;

    cw20_init(
        deps,
        env,
        info,
        InstantiateMsg {
            name,
            symbol,
            decimals,
            initial_balances,
            mint: Some(MinterResponse {
                minter: hub_contract,
                cap: None,
            }),
            marketing,
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let _hub_addr = HUB_CONTRACT.load(deps.storage)?;
    // TODO: check if the contract is paused

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default().add_attribute("migrate", "successful"))
}

fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    //TODO: add submessage to checking slashing and update exchange rate

    let res = cw20_burn(deps, env, info, amount)?;
    Ok(Response::new().add_attributes(res.attributes))
}

fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    //TODO: add submessage to check slashing and update exchange rate
    let res = cw20_burn_from(deps, env, info, owner, amount)?;
    Ok(Response::default().add_attributes(res.attributes))
}
