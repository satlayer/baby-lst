use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Failed to init contract")]
    FailedToInitContract,

    #[error("Invalid Address")]
    InvalidAddress,

    #[error("Insufficient funds")]
    InsufficientFunds {},

    #[error("Invalid reward rate")]
    InvalidRewardRate {},

    #[error("{0}")]
    Overflow(String),

    #[error("Hub contract paused")]
    HubPaused,

    #[error(transparent)]
    Validator(#[from] ValidatorError),

    #[error(transparent)]
    Hub(#[from] HubError),

    #[error("Fee rate must be less than 30%")]
    InvalidFeeRate {},
}

#[derive(Error, Debug, PartialEq)]
#[error("Validator error: {0}")]
pub enum ValidatorError {
    #[error("Cannot remove the last validator in the registry")]
    LastValidatorRemovalNotAllowed,

    #[error("Empty validator set")]
    EmptyValidatorSet,

    #[error("Complete redelegation failed")]
    DistributionFailed,

    #[error("Undelegation amount exceeds total delegations")]
    ExceedUndelegation,
}

#[derive(Error, Debug, PartialEq)]
#[error("Hub error: {0}")]
pub enum HubError {
    #[error("Invalid hook message")]
    InvalidHookMsg,

    #[error("Hub is paused")]
    Paused,

    #[error("Invalid amount")]
    InvalidAmount,

    #[error("Reward dispatcher contract not set")]
    RewardDispatcherNotSet,

    #[error("Validator registry contract not set")]
    ValidatorRegistryNotSet,

    #[error("LST Token contract not set")]
    LstTokenNotSet,

    #[error("Only one coin can be sent to the contract")]
    OnlyOneCoinAllowed,

    #[error("Burn requests not found for the specified time period")]
    UnstakeHistoryNotFound,

    #[error("No withdrawable assets are available yet")]
    NoWithdrawableAssets,

    #[error("Epoch length exceeds maximum allowed value")]
    InvalidEpochLength,

    #[error("Unstaking period exceeds maximum allowed value")]
    InvalidUnstakingPeriod,

    #[error("Epoch length must be less than unstaking period")]
    InvalidPeriods,

    #[error("User balance less than the amount to unstake")]
    InsufficientFunds,

    #[error("User allowance to hub contractless than the amount to unstake")]
    InsufficientAllowance,
}
