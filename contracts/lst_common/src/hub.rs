use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, CanonicalAddr, Coin, Deps, QueryRequest, StdResult, Uint128, WasmQuery,
};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cw_serde]
pub struct InstantiateMsg {
    pub epoch_length: u64,
    pub staking_coin_denom: String,
    pub unstaking_period: u64,
}

#[cw_serde]
pub struct Config {
    // address of the owner of the contract
    pub owner: CanonicalAddr,
    // address of the reward dispatcher contract
    pub reward_dispatcher_contract: Option<CanonicalAddr>,
    // optional address of the validators registry contract
    pub validators_registry_contract: Option<CanonicalAddr>,
    // token address of the lst token
    pub lst_token: Option<CanonicalAddr>,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(Parameters)]
    Parameters {},
    #[returns(Uint128)]
    TotalStaked {},
    #[returns(Uint128)]
    ExchangeRate {},
}

#[cw_serde]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    Stake {},
    Unstake {
        amount: Uint128,
    },
    WithdrawUnstaked {},
    UpdateConfig {
        owner: Option<String>,
        lst_token: Option<String>,
        validator_registry: Option<String>,
        reward_dispatcher: Option<String>,
    },
    UpdateParams {
        pause: Option<bool>,
        epoch_length: Option<u64>,
        unstaking_period: Option<u64>,
    },
    CheckSlashing {},
    RedelegateProxy {
        src_validator: String,
        redelegations: Vec<(String, Coin)>,
    },

    StakeRewards {},

    UpdateGlobalIndex {},
}

#[cw_serde]
#[derive(Default)]
pub struct Parameters {
    pub epoch_length: u64,
    pub staking_coin_denom: String,
    pub unstaking_period: u64,
    #[serde(default = "default_hub_status")]
    pub paused: bool,
}

fn default_hub_status() -> bool {
    true
}

// check hub contract pause status
pub fn is_paused(deps: Deps, hub_addr: String) -> StdResult<bool> {
    let params: Parameters = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: hub_addr,
        msg: to_json_binary(&QueryMsg::Parameters {})?,
    }))?;

    Ok(params.paused)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    UnStake {},
}
