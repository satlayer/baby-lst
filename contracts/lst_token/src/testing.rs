#![cfg(not(target_arch = "wasm32"))]

use crate::contract::{execute, instantiate, query};
use crate::msg::InstantiateMsg;
use cosmwasm_schema::serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, Env};
use cw20_base::msg::{ExecuteMsg, QueryMsg};
use cw_multi_test::{Contract, ContractWrapper};
use lst_common::babylon::{EpochingMsg, EpochingQuery};
use lst_common::testing::{BabylonApp, TestingContract};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TokenContract {
    pub addr: Addr,
    pub init: InstantiateMsg,
}

impl TestingContract<InstantiateMsg, ExecuteMsg, QueryMsg> for TokenContract {
    fn wrapper() -> Box<dyn Contract<EpochingMsg, EpochingQuery>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }

    fn default_init(app: &mut BabylonApp, _env: &Env) -> InstantiateMsg {
        InstantiateMsg {
            name: "BABY".to_string(),
            symbol: "BABY".to_string(),
            decimals: 6,
            hub_contract: Self::get_contract_addr(app, "StakingHubContract").to_string(),
            marketing: None,
        }
    }

    fn new(app: &mut BabylonApp, env: &Env, msg: Option<InstantiateMsg>) -> Self {
        let init = msg.unwrap_or(Self::default_init(app, env));
        let code_id = Self::store_code(app);
        let addr = Self::instantiate(app, code_id, "TokenContract", None, &init);
        Self { addr, init }
    }

    fn addr(&self) -> &Addr {
        &self.addr
    }
}
