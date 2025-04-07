use cw_storage_plus::{Item, Map};
use lst_common::validator::{Config, Validator};

pub const CONFIG: Item<Config> = Item::new("config");
pub const VALIDATOR_REGISTRY: Map<&[u8], Validator> = Map::new("validator_registry");

pub const VALIDATOR_EXCLUDE_LIST: Map<String, bool> = Map::new("validator_exclude_list");
