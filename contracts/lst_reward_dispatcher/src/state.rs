use cw_storage_plus::Item;
use lst_common::rewards_msg::Config;

pub const CONFIG: Item<Config> = Item::new("config");
