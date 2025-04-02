use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, Decimal, Deps, QueryRequest, StdResult, Uint128, WasmQuery,
};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cw_serde]
pub struct InstantiateMsg {
    /// Time to batch the unstake request in the staking hub. Should match the epoch length of chain
    pub epoch_length: u64,
    /// Denom to use for staking
    pub staking_coin_denom: String,
    /// Unstaking period of the chain
    pub unstaking_period: u64,
}

#[cw_serde]
pub struct Config {
    /// address of the owner of the contract
    pub owner: Addr,
    /// address of the reward dispatcher contract
    pub reward_dispatcher_contract: Option<Addr>,
    /// optional address of the validators registry contract
    pub validators_registry_contract: Option<Addr>,
    /// token address of the lst token
    pub lst_token: Option<Addr>,
}

#[cw_serde]
pub struct ConfigResponse {
    /// Owner of the contract
    pub owner: String,
    /// Reward dispatcher contract address
    pub reward_dispatcher_contract: Option<String>,
    /// Validator registry contract address
    pub validators_registry_contract: Option<String>,
    /// LST token address
    pub lst_token: Option<String>,
}

#[cw_serde]
pub struct CurrentBatch {
    /// Batch id of the current unstaking batch
    pub id: u64,
    /// Total lst token amount requested in the batch
    pub requested_lst_token_amount: Uint128,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Returns the config values of the contract
    #[returns(ConfigResponse)]
    Config {},
    /// Retursn the state variables in the contract
    #[returns(State)]
    State {},
    /// Returns the details of current unstaking batch
    #[returns(CurrentBatch)]
    CurrentBatch {},
    /// Returns the parameters values
    #[returns(Parameters)]
    Parameters {},
    /// Returns the current exchange rate
    #[returns(Uint128)]
    ExchangeRate {},
    /// Returns the total amount a user can withdraw from his pending unstaked requests
    #[returns(WithdrawableUnstakedResponse)]
    WithdrawableUnstaked {
        /// Address of the user
        address: String,
    },
    /// Return all the unstaking requests for a user
    #[returns(UnstakeRequestsResponse)]
    UnstakeRequests {
        /// Address of the user
        address: String,
    },
    /// Returns the unstaking requests history in batches
    #[returns(AllHistoryResponse)]
    AllHistory {
        /// Starting index for the history
        start_from: Option<u64>,
        /// No of data to return per request
        limit: Option<u32>,
    },
}

#[cw_serde]
pub enum ExecuteMsg {
    /// This hook is called to unstake from the token contract. To unstake tokens, user can simply transfer the tokens to staking hub contract.
    Receive(Cw20ReceiveMsg),
    /// Stake the amount sent in the funds. Only staking denom fund is accepted.
    Stake {},
    Unstake {
        amount: Uint128,
    },
    /// User can withdraw the amount after the unstaking process has been completed.
    WithdrawUnstaked {},
    /// Admin can update these parameters for configuration of the contract.
    UpdateConfig {
        /// Owner of the contract
        owner: Option<String>,
        /// lst token address
        lst_token: Option<String>,
        /// validator registry address
        validator_registry: Option<String>,
        /// reward dispatcher address
        reward_dispatcher: Option<String>,
    },
    /// Admin can update these parameters from this method
    UpdateParams {
        /// Pause/Unpause the status of contract
        pause: Option<bool>,
        /// Epoch length of the unstaking batch
        epoch_length: Option<u64>,
        /// Amount of time the chain takes for unstaking
        unstaking_period: Option<u64>,
    },
    /// Check if slashing has happened. If slashing has happened, the exchange rate is updated accordingly.
    CheckSlashing {},
    /// This method is used to update the validators delegation from the validators registry contract. The change in validators set in registry contract is update by using this method.
    RedelegateProxy {
        /// Validator address from which delegation has to be removed
        src_validator: String,
        /// new delegation list
        redelegations: Vec<(String, Coin)>,
    },

    /// This method is used by rewards dispatcher contract to stake the rewards accrued from staking
    StakeRewards {},

    /// This method is open to call to update the state of the contract like exchange rate, rewards.
    UpdateGlobalIndex {},

    /// This method is used to process undelegations without calling the token contract. Batch is processed only if the epoch time has
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

/// check hub contract pause status
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

/// Amount of unstaked tokens that can be withdrawn by user
#[cw_serde]
pub struct WithdrawableUnstakedResponse {
    /// total amount
    pub withdrawable: Uint128,
}

/// Batch id and lst token amount unstaked by user
pub type UnstakeRequest = Vec<(u64, Uint128)>;

#[cw_serde]
pub struct UnstakeRequestsResponse {
    /// Address of the user
    pub address: String,
    /// Batch Id and Amount of token unstaked by the user
    pub requests: UnstakeRequest,
}

#[cw_serde]
pub struct AllHistoryResponse {
    /// History of unstaking requests
    pub history: Vec<UnstakeRequestsResponse>,
}
