use cosmwasm_std::Empty;

use crate::ContractError;

pub type ResponseType = Empty;
pub type LstResult<T = ResponseType, E = ContractError> = Result<T, E>;

pub type ProtoCoin = cosmos_sdk_proto::cosmos::base::v1beta1::Coin;
pub type StdCoin = cosmwasm_std::Coin;
