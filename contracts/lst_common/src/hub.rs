use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, Decimal, Deps, QueryRequest, StdResult, Uint128, WasmQuery,
};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cw_serde]
pub struct InstantiateMsg {
    /// Time to batch the unstake request in the staking hub. Longer epoch length means user would have to wait longer to unstake.
    pub epoch_length: u64,
    /// Denom to use for staking
    pub staking_coin_denom: String,
    /// Unstaking period of the chain
    pub unstaking_period: u64,
    /// Staking epoch start block height, this is inclusive in the epoch. This height must match the starting height of the epoch of the chain.
    pub staking_epoch_start_block_height: u64,
    /// Staking epoch length in blocks e.g. 360 in testnet
    pub staking_epoch_length_blocks: u64,
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
    /// Returns the state variables in the contract. This method returns the actual exchange rate by dynamic caclulation rather than the stored one in the contract.
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
    #[returns(UnstakeRequestsResponses)]
    UnstakeRequests {
        /// Address of the user
        address: String,
    },
    /// Return the unstaking requests for a user in batches
    #[returns(UnstakeRequestsResponses)]
    UnstakeRequestsLimit {
        /// Address of the user
        address: String,
        /// Starting index for the history
        start_from: Option<u64>,
        /// No of data to return per request
        limit: Option<u32>,
    },
    /// Returns the unstaking requests history in batches
    #[returns(AllHistoryResponse)]
    AllHistory {
        /// Starting index for the history
        start_from: Option<u64>,
        /// No of data to return per request
        limit: Option<u32>,
    },
    /// Returns the pending delegation amount
    #[returns(PendingDelegation)]
    PendingDelegation {},
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
    /// User can withdraw the amount after the unstaking process has been completed for specific batch IDs.
    WithdrawUnstakedForBatches {
        batch_ids: Vec<u64>,
    },
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

    /// This method is used to process undelegations without calling the token contract. Batch is processed only if the epoch time has passed
    ProcessUndelegations {},

    /// This method is used to process the unstake requests that have already passed the unstaking period
    ProcessWithdrawRequests {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    pub lst_exchange_rate: Decimal,
    pub total_staked_amount: Uint128,
    pub last_index_modification: u64,
    pub unclaimed_unstaked_balance: Uint128,
    pub last_unbonded_time: u64,
    pub last_processed_batch: u64,
}

impl State {
    pub fn update_lst_exchange_rate(
        &mut self,
        total_issued_lst_token: Uint128,
        requested_lst_token_amount: Uint128,
    ) {
        let total_token_supply = total_issued_lst_token + requested_lst_token_amount;
        if self.total_staked_amount.is_zero() || total_token_supply.is_zero() {
            self.lst_exchange_rate = Decimal::one();
        } else {
            self.lst_exchange_rate =
                Decimal::from_ratio(self.total_staked_amount, total_token_supply);
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
    Unstake {},
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
pub struct UserUnstakeRequestsResponse {
    /// Batch id
    pub batch_id: u64,
    /// Amount of lst token unstaked by the user
    pub lst_amount: Uint128,
    /// Exchange rate of the lst token at the time of unstake
    pub withdraw_exchange_rate: Decimal,
    /// Exchange rate of the lst token at the time of withdrawal. If released is false, it would be same as withdraw_exchange_rate
    pub applied_exchange_rate: Decimal,
    /// Time at which the unstake request was made
    pub time: u64,
    /// Whether the unstake request is released to get updated withdraw rate
    pub released: bool,
}

#[cw_serde]
pub struct UnstakeRequestsResponses {
    /// Address of the user
    pub address: String,
    /// Unstake request details for the user
    pub requests: Vec<UserUnstakeRequestsResponse>,
}

#[cw_serde]
pub struct UnstakeHistory {
    /// Batch id of the unstake request
    pub batch_id: u64,
    /// Time at which the unstake request was made
    pub time: u64,
    /// Amount of lst token unstaked or burnt in the batch
    pub lst_token_amount: Uint128,
    /// Exchange rate of the lst token at the time of withdrawal/slashing is applied to this rate
    pub lst_applied_exchange_rate: Decimal,
    /// Exchange rate of the lst token at the time of unstake/burning of lst token
    pub lst_withdraw_rate: Decimal,
    /// Whether the batch is processsed/released to get updated withdraw rate
    pub released: bool,
}

#[cw_serde]
pub struct AllHistoryResponse {
    /// History of unstaking requests
    pub history: Vec<UnstakeHistory>,
}

#[cw_serde]
pub struct PendingDelegation {
    /// Staking epoch length in blocks e.g. 360 in testnet
    pub staking_epoch_length_blocks: u64,
    /// Staking epoch start block height, this is inclusive in the epoch
    pub staking_epoch_start_block_height: u64,
    /// Pending amount of staked tokens that are not yet delegated
    pub pending_staking_amount: Uint128,
    /// Pending amount of unstaked tokens that are not yet processed in the epoch
    pub pending_unstaking_amount: Uint128,
}
