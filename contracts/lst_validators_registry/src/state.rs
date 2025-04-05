use cw_storage_plus::{Item, Map};
use lst_common::validator::{Config, PendingRedelegation, Validator};

pub const CONFIG: Item<Config> = Item::new("config");
pub const VALIDATOR_REGISTRY: Map<&[u8], Validator> = Map::new("validator_registry");
pub const PENDING_REDELEGATIONS: Map<&[u8], PendingRedelegation> =
    Map::new("pending_redelegations");

pub const REDELEGATION_COOLDOWN: u64 = 24 * 60 * 60;
