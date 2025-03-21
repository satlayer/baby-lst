use crate::constants::*;
use cosmwasm_std::{Addr, Decimal, Uint128};
use cw_storage_plus::{Item, Map};
use lst_common::hub::{Config, Parameters};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const CONFIG: Item<Config> = Item::new(CONFIG_KEY);
pub const PARAMETERS: Item<Parameters> = Item::new(PARAMETERS_KEY);
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new(CURRENT_BATCH_KEY);
pub const STATE: Item<State> = Item::new(STATE_KEY);

pub const UNSTAKE_WAIT_LIST: Map<Addr, UnstakeWaitEntity> = Map::new(UNSTAKE_WAIT_LIST_KEY);
pub const UNSTAKE_HISTORY: Map<u64, UnStakeHistory> = Map::new(UNSTAKE_HISTORY_KEY);
pub const TOTAL_STAKED: Item<Uint128> = Item::new("total_staked");

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

#[derive(JsonSchema, Serialize, Deserialize, Default)]
pub struct UnstakeWaitEntity {
    pub batch_id: u64,
    pub lst_token_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnStakeHistory {
    pub batch_id: u64,
    pub time: u64,
    pub lst_token_amount: Uint128,
    pub lst_applied_exchange_rate: Decimal,
    pub lst_withdraw_rate: Decimal,
    pub released: bool,
}
