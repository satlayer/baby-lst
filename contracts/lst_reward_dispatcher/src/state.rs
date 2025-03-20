use cosmwasm_schema::cw_serde;
use cosmwasm_std::{CanonicalAddr, Decimal};
use cw_storage_plus::Item;

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: CanonicalAddr,
    pub reward_denom: String,
    pub satlayer_fee_addr: CanonicalAddr,
    pub satlayer_fee_rate: Decimal,
}
