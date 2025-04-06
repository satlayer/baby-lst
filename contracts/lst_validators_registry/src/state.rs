use cw_storage_plus::{Item, Map};
use lst_common::validator::{Config, PendingRedelegation, Validator};

pub const CONFIG: Item<Config> = Item::new("config");
pub const VALIDATOR_REGISTRY: Map<&[u8], Validator> = Map::new("validator_registry");
pub const VALIDATOR_EXCLUDELIST: Map<&[u8], bool> = Map::new("validator_excludelist");
pub const PENDING_REDELEGATIONS: Map<&[u8], PendingRedelegation> =
    Map::new("pending_redelegations");
pub const LAST_REDELEGATIONS: Map<&[u8], u64> = Map::new("last_redelegations");
pub const REDELEGATION_COOLDOWN: u64 = 24 * 60 * 60;
