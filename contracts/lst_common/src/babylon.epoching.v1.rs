/// MsgWrappedDelegate is the message for delegating stakes
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgWrappedDelegate {
    #[prost(message, optional, tag="1")]
    pub msg: ::core::option::Option<cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate>,
}
// @@protoc_insertion_point(module)
