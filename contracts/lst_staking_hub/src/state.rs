use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    // token address of the lst token 
    pub lst_token: Addr,
    // denom of the staking token
    pub staking_denom: String,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const TOTAL_STAKED: Item<Uint128> = Item::new("total_staked");
