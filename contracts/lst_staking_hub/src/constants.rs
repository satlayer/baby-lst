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

//Event names
pub const LST_EXCHANGE_RATE_UPDATED: &str = "LstExchangeRateUpdated";
pub const TOTAL_STAKED_AMOUNT_UPDATED: &str = "TotalStakedAmountUpdated";
pub const OLD_RATE: &str = "old_rate";
pub const NEW_RATE: &str = "new_rate";
pub const OLD_AMOUNT: &str = "old_amount";
pub const NEW_AMOUNT: &str = "new_amount";

pub const PENDING_DELEGATION_KEY: &str = "pending_delegation";

// being generous on block time, to avoid staking epoch length being too short
pub const AVERAGE_BLOCK_TIME: u64 = 20; // seconds
