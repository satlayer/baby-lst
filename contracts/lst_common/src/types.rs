use cosmwasm_std::Empty;

use crate::ContractError;

pub type ResponseType = Empty;
pub type LstResult<T = ResponseType, E = ContractError> = Result<T, E>;
