use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");
pub const VALIDATOR_REGISTRY: Map<&[u8], Validator> = Map::new("validator_registry");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub hub_contract: Addr,
}

#[cw_serde]
pub struct Validator {
    pub address: String,
}
