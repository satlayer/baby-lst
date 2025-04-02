pub const CONFIG_KEY: &str = "config";
pub const PARAMETERS_KEY: &str = "parameters";
pub const CURRENT_BATCH_KEY: &str = "current_batch";
pub const STATE_KEY: &str = "state";

pub const UNSTAKE_WAIT_LIST_KEY: &str = "unstake_wait_list";
pub const UNSTAKE_HISTORY_KEY: &str = "unstake_history";

// Maximum allowed epoch length in seconds (1 week)
pub const MAX_EPOCH_LENGTH: u64 = 7 * 24 * 60 * 60;
// Maximum allowed unstaking period in seconds (4 weeks)
pub const MAX_UNSTAKING_PERIOD: u64 = 4 * 7 * 24 * 60 * 60;
