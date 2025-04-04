use cosmos_sdk_proto::cosmos::staking::v1beta1::{MsgBeginRedelegate, MsgDelegate, MsgUndelegate};
use cosmwasm_std::{AnyMsg, Binary, CosmosMsg};

pub trait CosmosAny: Sized + prost::Message {
    const TYPE_URL: &'static str;

    fn to_any(&self) -> CosmosMsg {
        CosmosMsg::Any(AnyMsg {
            type_url: Self::TYPE_URL.to_string(),
            value: Binary::from(self.encode_to_vec()),
        })
    }
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgWrappedDelegate {
    #[prost(message, optional, tag = "1")]
    pub msg: Option<MsgDelegate>,
}

impl CosmosAny for MsgWrappedDelegate {
    const TYPE_URL: &'static str = "/babylon.epoching.v1.MsgWrappedDelegate";
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgWrappedBeginRedelegate {
    #[prost(message, optional, tag = "1")]
    pub msg: Option<MsgBeginRedelegate>,
}

impl CosmosAny for MsgWrappedBeginRedelegate {
    const TYPE_URL: &'static str = "/babylon.epoching.v1.MsgWrappedBeginRedelegate";
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgWrappedUndelegate {
    #[prost(message, optional, tag = "1")]
    pub msg: Option<MsgUndelegate>,
}

impl CosmosAny for MsgWrappedUndelegate {
    const TYPE_URL: &'static str = "/babylon.epoching.v1.MsgWrappedUndelegate";
}
