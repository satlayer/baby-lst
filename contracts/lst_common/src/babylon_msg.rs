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

#[cfg(test)]
mod tests {
    use crate::babylon_msg::CosmosAny;
    use cosmwasm_std::testing::mock_dependencies;

    #[test]
    fn test_msg_wrapped_delegate() {
        let deps = mock_dependencies();
        let delegator = deps.api.addr_make("delegator");
        let validator = deps.api.addr_make("validator");

        let msg = super::MsgWrappedDelegate {
            msg: Some(cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate {
                delegator_address: delegator.to_string(),
                validator_address: validator.to_string(),
                amount: Some(cosmos_sdk_proto::cosmos::base::v1beta1::Coin {
                    denom: "ubbn".to_string(),
                    amount: "1000000".to_string(),
                }),
            }),
        }
        .to_any();
        assert_eq!(
            msg,
            super::CosmosMsg::Any(super::AnyMsg {
                type_url: "/babylon.epoching.v1.MsgWrappedDelegate".to_string(),
                value: super::Binary::from_base64("CpsBCkNjb3Ntd2FzbTE2bmYwbWh0Njg5MzdkMjdjd3JxcWR0d3Y5eTJhbG0wNmw1cm53Z2tlZnV3eXZ6NmRoMmxxdTJ3eDZtEkNjb3Ntd2FzbTFscTQweGd0cWgzZjN6dDlwcno0bTc0bDZkbGs1MDZ1czl5ZHA2OHZqN3N1MnV0a2hmbW1xZ3VzZXo1Gg8KBHViYm4SBzEwMDAwMDA").unwrap(),
            })
        );
    }
}
