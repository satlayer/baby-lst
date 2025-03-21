use cosmwasm_std::Addr;
use cw_storage_plus::Item;

pub const HUB_CONTRACT: Item<Addr> = Item::new("hub_contract");
