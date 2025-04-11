#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query};
use cosmwasm_std::{Addr, Decimal, Env, Uint128};
use cw_multi_test::{Contract, ContractWrapper};
use lst_common::babylon::{EpochingMsg, EpochingQuery};
use lst_common::rewards_msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use lst_common::testing::{BabylonApp, TestingContract};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RewardDispatcherContract {
    pub addr: Addr,
    pub init: InstantiateMsg,
}

impl TestingContract<InstantiateMsg, ExecuteMsg, QueryMsg> for RewardDispatcherContract {
    fn wrapper() -> Box<dyn Contract<EpochingMsg, EpochingQuery>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }

    fn default_init(app: &mut BabylonApp, _env: &Env) -> InstantiateMsg {
        InstantiateMsg {
            hub_contract: Self::get_contract_addr(app, "StakingHubContract").to_string(),
            reward_denom: "BABY".to_string(),
            fee_addr: app.api().addr_make("fee_addr").to_string(),
            fee_rate: Decimal::new(Uint128::new(10)),
        }
    }

    fn new(app: &mut BabylonApp, env: &Env, msg: Option<InstantiateMsg>) -> Self {
        let init = msg.unwrap_or(Self::default_init(app, env));
        let code_id = Self::store_code(app);
        let addr = Self::instantiate(app, code_id, "RewardDispatcherContract", None, &init);
        Self { addr, init }
    }

    fn addr(&self) -> &Addr {
        &self.addr
    }
}
