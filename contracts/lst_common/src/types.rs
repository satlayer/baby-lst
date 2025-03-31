use crate::{epoching::CosmosProtoMsg, ContractError};

pub type MessageType = CosmosProtoMsg;
pub type LstResult<T = MessageType, E = ContractError> = Result<T, E>;
