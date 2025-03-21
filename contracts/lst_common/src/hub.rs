use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, CanonicalAddr, Coin, Deps, QueryRequest, StdResult, Uint128, WasmQuery,
};

#[cw_serde]
pub struct InstantiateMsg {
    pub staking_token: String,
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
    Stake {},
    Unstake {
        amount: Uint128,
    },
    WithdrawUnstaked {},
    UpdateConfig {
        lst_token: String,
        staking_denom: String,
    },
    UpdateParams {
        pause: Option<bool>,
        staking_coin_denom: Option<String>,
        epoch_length: Option<u64>,
        unstaking_period: Option<u64>,
    },
    ClaimRewardsAndRestake {},
    CheckSlashing {},

    RedelegateProxy {
        src_validator: String,
        redelegations: Vec<(String, Coin)>,
    },

    StakeRewards {},
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
