#![cfg(not(target_arch = "wasm32"))]

use crate::address::{convert_addr_by_prefix, VALIDATOR_ADDR_PREFIX};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Api, Binary, BlockInfo, Coin, CosmosMsg, CustomMsg, CustomQuery, Empty, Event, Querier,
    StakingMsg, Storage,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{AppResponse, CosmosRouter, Module};
use cw_storage_plus::Deque;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

// CONST
pub const EPOCH_LENGTH: u64 = 7200;
pub const DENOM: &str = "BABY";
pub const UNSTAKING_PERIOD: u64 = 183600;
pub const STAKING_EPOCH_START_BLOCK_HEIGHT: u64 = 0;
pub const STAKING_EPOCH_LENGTH_BLOCKS: u64 = 360;

// STATE
#[cw_serde]
pub struct EpochingMsgQueueItem {
    pub msg: CosmosMsg<EpochingMsg>,
    pub sender: Addr,
}

impl EpochingMsgQueueItem {
    pub fn new(msg: CosmosMsg<EpochingMsg>, sender: Addr) -> Self {
        Self { msg, sender }
    }
}

const EPOCHING_MSG_QUEUE: Deque<EpochingMsgQueueItem> = Deque::new("epoching_msg_queue");

// MODULE
pub struct BabylonModule {}

impl Default for BabylonModule {
    fn default() -> Self {
        Self::new()
    }
}

impl BabylonModule {
    pub fn new() -> Self {
        Self {}
    }
}

pub trait EpochingModule:
    Module<ExecT = EpochingMsg, QueryT = EpochingQuery, SudoT = Empty>
{
    fn push_msg(&self, storage: &mut dyn Storage, item: &EpochingMsgQueueItem) -> AnyResult<()> {
        EPOCHING_MSG_QUEUE.push_back(storage, item)?;
        Ok(())
    }

    fn on_epoch_end<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + CustomMsg,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        let mut events = vec![];

        // loop through epoching message queue and execute each one
        // note: due to cw-multi-test, this is done in a transaction, so every msg must succeed for the state to be committed.
        while let Some(item) = EPOCHING_MSG_QUEUE.pop_front(storage)? {
            let custom_msg = item.msg.change_custom().unwrap();

            // execute msg
            let res = router.execute(api, storage, block, item.sender, custom_msg)?;
            // collect events
            events.extend(res.events);
        }

        Ok(AppResponse {
            events,
            ..Default::default()
        })
    }
}

impl EpochingModule for BabylonModule {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EpochingMsg {
    /// `delegator_address` is automatically filled with the current contract's address.
    Delegate {
        validator: String,
        amount: Coin,
    },
    Undelegate {
        validator: String,
        amount: Coin,
    },

    NextEpoch {},
}

impl EpochingMsg {
    // Serialize to JSON binary format using serde
    pub fn to_binary(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Failed to serialize EpochingMsg")
    }

    // Deserialize from JSON binary format using serde
    pub fn from_binary(data: &[u8]) -> Self {
        serde_json::from_slice(data).expect("Failed to deserialize EpochingMsg")
    }
}

impl CustomMsg for EpochingMsg {}

impl From<EpochingMsg> for CosmosMsg<EpochingMsg> {
    fn from(original: EpochingMsg) -> Self {
        CosmosMsg::Custom(original)
    }
}

#[cw_serde]
pub enum EpochingQuery {}

impl CustomQuery for EpochingQuery {}

impl Module for BabylonModule {
    type ExecT = EpochingMsg;
    type QueryT = EpochingQuery;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        sender: Addr,
        msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + CustomMsg,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg {
            EpochingMsg::Delegate { validator, amount } => {
                let msg = StakingMsg::Delegate {
                    validator: convert_addr_by_prefix(&validator, VALIDATOR_ADDR_PREFIX),
                    amount: amount.clone(),
                };

                // TODO: validate message before adding to queue
                self.push_msg(storage, &EpochingMsgQueueItem::new(msg.into(), sender))?;

                let events = vec![Event::new("delegate")
                    .add_attribute("validator", &validator)
                    .add_attribute("amount", format!("{}{}", amount.amount, amount.denom))];

                Ok(AppResponse {
                    events,
                    ..Default::default()
                })
            }

            EpochingMsg::Undelegate { validator, amount } => {
                let msg = StakingMsg::Undelegate {
                    validator: convert_addr_by_prefix(&validator, VALIDATOR_ADDR_PREFIX),
                    amount: amount.clone(),
                };

                self.push_msg(storage, &EpochingMsgQueueItem::new(msg.into(), sender))?;
                let events = vec![Event::new("undelegate")
                    .add_attribute("validator", &validator)
                    .add_attribute("amount", format!("{}{}", amount.amount, amount.denom))];
                Ok(AppResponse {
                    events,
                    ..Default::default()
                })
            }

            EpochingMsg::NextEpoch {} => self.on_epoch_end(_api, storage, _router, _block),
        }
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        _request: Self::QueryT,
    ) -> AnyResult<Binary> {
        todo!()
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        todo!()
    }
}
