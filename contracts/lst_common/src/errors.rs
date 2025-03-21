use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
    #[error("Failed to init contact")]
    FailedToInitContract,

    #[error("Invalid Address")]
    InvalidAddress,

    #[error("Insufficient funds")]
    InsufficientFunds {},

    #[error("Invalid amount")]
    InvalidAmount {},

    #[error("Invalid reward rate")]
    InvalidRewardRate {},

    #[error("{0}")]
    Overflow(String),

    #[error("Hub contract paused")]
    HubPaused,

    #[error(transparent)]
    Validator(#[from] ValidatorError),
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
