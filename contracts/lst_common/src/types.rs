use crate::ContractError;

pub type LstResult<T, E = ContractError> = Result<T, E>;
