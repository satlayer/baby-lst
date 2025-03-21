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
    #[error("Invalid amount")]
    InvalidAmount,

    #[error("Reward discpather contract not set")]
    RewardDispatcherNotSet,

    #[error("Validator registry contract not set")]
    ValidatorRegistryNotSet,

    #[error("LST Token contract not set")]
    LstTokenNotSet,

    #[error("Only one coin can be sent to the contract")]
    OnlyOneCoinAllowed,
}
