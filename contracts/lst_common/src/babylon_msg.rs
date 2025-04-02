use cosmwasm_std::{AnyMsg, Binary, CosmosMsg};

use crate::types::LstResult;

pub trait CosmosAny: Sized + prost::Message {
    const TYPE_URL: &'static str;

    fn to_any(&self) -> LstResult<CosmosMsg> {
        Ok(CosmosMsg::Any(AnyMsg {
            type_url: Self::TYPE_URL.to_string(),
            value: Binary::from(self.encode_to_vec()),
        }))
    }
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgWrappedDelegate {
    #[prost(message, optional, tag = "1")]
    pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate>,
}

impl CosmosAny for MsgWrappedDelegate {
    const TYPE_URL: &'static str = "/babylon.epoching.v1.MsgWrappedDelegate";
}
