use cosmos_sdk_proto::cosmos::base::v1beta1::Coin as Coin1;
use cosmos_sdk_proto::cosmos::staking::v1beta1::MsgDelegate as MsgDelegate1;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, CosmosMsg, Empty};

/// BabylonMsg is the message that the Babylon contract can send to the Cosmos zone.
/// The Cosmos zone has to integrate https://github.com/babylonlabs-io/wasmbinding for
/// handling these messages
#[cw_serde]
pub enum BabylonMsg {
    WrappedDelegate {
        delegator_address: String,
        validator_address: String,
        amount: Coin,
    },
}

// #[cw_serde]
// pub enum BabylonMsg {
//     WrappedDelegate { msg: Option<MsgDelegate> },
// }

// #[derive(
//     Default, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
// )]
// pub struct MsgDelegate(MsgDelegate1);

// #[derive(
//     Default, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
// )]
// pub struct Coin(Coin1);

// #[derive(
//     Default, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
// )]
// pub struct MsgDelegate {
//     pub delegator_address: String,
//     pub validator_address: String,
//     pub amount: Option<Coin>,
// }

// #[derive(
//     Default, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
// )]
// pub struct Coin {
//     pub denom: String,
//     pub amount: String,
// }

pub type BabylonSudoMsg = Empty;
pub type BabylonQuery = Empty;

// make BabylonMsg to implement CosmosMsg::CustomMsg
impl cosmwasm_std::CustomMsg for BabylonMsg {}

impl From<BabylonMsg> for CosmosMsg<BabylonMsg> {
    fn from(original: BabylonMsg) -> Self {
        CosmosMsg::Custom(original)
    }
}
