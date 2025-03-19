use cosmwasm_std::Uint128;
use cw_storage_plus::Item;

use lst_common::hub::Config;

pub const CONFIG: Item<Config> = Item::new("config");
pub const TOTAL_STAKED: Item<Uint128> = Item::new("total_staked");
