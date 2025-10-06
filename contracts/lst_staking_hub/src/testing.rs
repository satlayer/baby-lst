#![cfg(not(target_arch = "wasm32"))]

use crate::{execute, instantiate, query};
use cosmwasm_std::{Addr, Env};
use cw_multi_test::{Contract, ContractWrapper};
use lst_common::babylon::{
    EpochingMsg, EpochingQuery, DENOM, EPOCH_LENGTH, STAKING_EPOCH_LENGTH_BLOCKS,
    STAKING_EPOCH_START_BLOCK_HEIGHT, UNSTAKING_PERIOD,
};
use lst_common::hub::{ExecuteMsg, InstantiateMsg, QueryMsg};
use lst_common::testing::{BabylonApp, TestingContract};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StakingHubContract {
    pub addr: Addr,
    pub init: InstantiateMsg,
}

impl TestingContract<InstantiateMsg, ExecuteMsg, QueryMsg> for StakingHubContract {
    fn wrapper() -> Box<dyn Contract<EpochingMsg, EpochingQuery>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }

    fn default_init(_app: &mut BabylonApp, _env: &Env) -> InstantiateMsg {
        InstantiateMsg {
            epoch_length: EPOCH_LENGTH,
            staking_coin_denom: DENOM.to_string(),
            unstaking_period: UNSTAKING_PERIOD,
            staking_epoch_start_block_height: STAKING_EPOCH_START_BLOCK_HEIGHT,
            staking_epoch_length_blocks: STAKING_EPOCH_LENGTH_BLOCKS,
        }
    }

    fn new(app: &mut BabylonApp, env: &Env, msg: Option<InstantiateMsg>) -> Self {
        let init = msg.unwrap_or(Self::default_init(app, env));
        let code_id = Self::store_code(app);
        let addr = Self::instantiate(app, code_id, "StakingHubContract", None, &init);
        Self { addr, init }
    }

    fn addr(&self) -> &Addr {
        &self.addr
    }
}
