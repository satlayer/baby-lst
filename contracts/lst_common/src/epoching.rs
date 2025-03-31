// /// Params defines the parameters for the module.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct Params {
//     /// epoch_interval is the number of consecutive blocks to form an epoch
//     #[prost(uint64, tag = "1")]
//     pub epoch_interval: u64,
// }
/// MsgWrappedDelegate is the message for delegating stakes
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message, ::prost::J)]
pub struct MsgWrappedDelegate {
    #[prost(message, optional, tag = "1")]
    pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate>,
}
// /// MsgWrappedDelegate is the response to the MsgWrappedDelegate message
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedDelegateResponse {}
// /// MsgWrappedUndelegate is the message for undelegating stakes
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedUndelegate {
//     #[prost(message, optional, tag = "1")]
//     pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgUndelegate>,
// }
// /// MsgWrappedUndelegateResponse is the response to the MsgWrappedUndelegate
// /// message
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedUndelegateResponse {}
// /// MsgWrappedDelegate is the message for moving bonded stakes from a
// /// validator to another validator
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedBeginRedelegate {
//     #[prost(message, optional, tag = "1")]
//     pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgBeginRedelegate>,
// }
// /// MsgWrappedBeginRedelegateResponse is the response to the
// /// MsgWrappedBeginRedelegate message
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedBeginRedelegateResponse {}
// /// MsgWrappedCancelUnbondingDelegation is the message for cancelling
// /// an unbonding delegation
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedCancelUnbondingDelegation {
//     #[prost(message, optional, tag = "1")]
//     pub msg: ::core::option::Option<
//         cosmos_sdk_proto::cosmos::staking::v1beta1::MsgCancelUnbondingDelegation,
//     >,
// }
// /// MsgWrappedCancelUnbondingDelegationResponse is the response to the
// /// MsgWrappedCancelUnbondingDelegation message
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedCancelUnbondingDelegationResponse {}
// /// MsgWrappedEditValidator defines a message for updating validator description
// /// and commission rate.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedEditValidator {
//     #[prost(message, optional, tag = "1")]
//     pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgEditValidator>,
// }
// /// MsgWrappedEditValidatorResponse is the response to the MsgWrappedEditValidator message.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedEditValidatorResponse {}
// /// MsgWrappedStakingUpdateParams defines a message for updating x/staking module parameters.
// // #[allow(clippy::derive_partial_eq_without_eq)]
// // #[derive(Clone, PartialEq, ::prost::Message)]
// // pub struct MsgWrappedStakingUpdateParams {
// //     #[prost(message, optional, tag = "1")]
// //     pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgUpdateParams>,
// // }
// /// MsgWrappedStakingUpdateParamsResponse is the response to the MsgWrappedStakingUpdateParams message.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgWrappedStakingUpdateParamsResponse {}
// /// MsgUpdateParams defines a message for updating epoching module parameters.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgUpdateParams {
//     /// authority is the address of the governance account.
//     /// just FYI: cosmos.AddressString marks that this field should use type alias
//     /// for AddressString instead of string, but the functionality is not yet implemented
//     /// in cosmos-proto
//     #[prost(string, tag = "1")]
//     pub authority: ::prost::alloc::string::String,
//     /// params defines the epoching parameters to update.
//     ///
//     /// NOTE: All parameters must be supplied.
//     #[prost(message, optional, tag = "2")]
//     pub params: ::core::option::Option<Params>,
// }
// /// MsgUpdateParamsResponse is the response to the MsgUpdateParams message.
// #[allow(clippy::derive_partial_eq_without_eq)]
// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgUpdateParamsResponse {}
// // @@protoc_insertion_point(module)

// #[derive(Clone, PartialEq, ::prost::Message)]
// pub struct MsgEditValidatorResponse {}
// /// MsgDelegate defines a SDK message for performing a delegation of coins
// /// from a delegator to a validator.

use cosmwasm_std::CosmosMsg;

#[derive(Debug, Clone, PartialEq)]
pub enum CosmosProtoMsg {
    MsgWrappedDelegate {
        msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate>,
    },
}
impl cosmwasm_std::CustomMsg for CosmosProtoMsg {}
impl From<CosmosProtoMsg> for CosmosMsg<CosmosProtoMsg> {
    fn from(value: CosmosProtoMsg) -> Self {
        CosmosMsg::Custom(value)
    }
}

// impl TryFrom<&CosmosProtoMsg> for Any {
//     type Error = EncodeError;
//     fn try_from(proto: &CosmosProtoMsg) -> Result<Self, Self::Error> {
//         match proto {
//             CosmosProtoMsg::Delegate(msg) => msg.to_any(),
//         }
//     }
// }
