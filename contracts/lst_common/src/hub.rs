use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, Decimal, Deps, QueryRequest, StdResult, Uint128, WasmQuery,
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
    pub owner: Addr,
    // address of the reward dispatcher contract
    pub reward_dispatcher_contract: Option<Addr>,
    // optional address of the validators registry contract
    pub validators_registry_contract: Option<Addr>,
    // token address of the lst token
    pub lst_token: Option<Addr>,
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: String,
    pub reward_dispatcher_contract: Option<String>,
    pub validators_registry_contract: Option<String>,
    pub lst_token: Option<String>,
}

#[cw_serde]
pub struct CurrentBatch {
    pub id: u64,
    pub requested_lst_token_amount: Uint128,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(State)]
    State {},
    #[returns(CurrentBatch)]
    CurrentBatch {},
    #[returns(Parameters)]
    Parameters {},
    #[returns(Uint128)]
    ExchangeRate {},
    #[returns(WithdrawableUnstakedResponse)]
    WithdrawableUnstaked { address: String },
    #[returns(UnstakeRequestsResponse)]
    UnstakeRequests { address: String },
    #[returns(AllHistoryResponse)]
    AllHistory {
        start_from: Option<u64>,
        limit: Option<u32>,
    },
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

    ProcessUndelegations {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    pub lst_exchange_rate: Decimal,
    pub total_lst_token_amount: Uint128,
    pub last_index_modification: u64,
    pub prev_hub_balance: Uint128,
    pub last_unbonded_time: u64,
    pub last_processed_batch: u64,
}

impl State {
    pub fn update_lst_exchange_rate(
        &mut self,
        total_issued_lst_token: Uint128,
        requested_lst_token_amount: Uint128,
    ) {
        let actual_supply = total_issued_lst_token + requested_lst_token_amount;
        if self.total_lst_token_amount.is_zero() || actual_supply.is_zero() {
            self.lst_exchange_rate = Decimal::one();
        } else {
            self.lst_exchange_rate =
                Decimal::from_ratio(self.total_lst_token_amount, actual_supply);
        }
    }
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

#[cw_serde]
pub struct WithdrawableUnstakedResponse {
    pub withdrawable: Uint128,
}

pub type UnstakeRequest = Vec<(u64, Uint128)>;

#[cw_serde]
pub struct UnstakeRequestsResponse {
    pub address: String,
    pub requests: UnstakeRequest,
}

#[cw_serde]
pub struct AllHistoryResponse {
    pub history: Vec<UnstakeRequestsResponse>,
}
