use crate::constants::*;
use cosmwasm_std::{CanonicalAddr, Decimal, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);
pub const PARAMETERS: Item<Parameters> = Item::new(PARAMETERS_KEY);
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new(CURRENT_BATCH_KEY);
pub const STATE: Item<State> = Item::new(STATE_KEY);
pub const TOTAL_STAKED: Item<Uint128> = Item::new("total_staked");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Parameters {
    pub epoch_length: u64,
    pub staking_coin_denom: String,
    pub unstaking_period: u64,
    pub paused: Option<bool>,
}

#[derive(PartialEq)]
pub enum StakeType {
    LSTMint,
    StakeRewards,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatch {
    pub id: u64,
    pub requested_lst_token_amount: Uint128,
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
