use cosmwasm_std::{
    coins,
    testing::{message_info, mock_env, MockApi},
    Addr, Empty, Env, MessageInfo,
};
use cw_multi_test::{App, BankSudo, Contract, ContractWrapper, Executor, SudoMsg};
use serde::de::DeserializeOwned;

use crate::contract::{execute, instantiate, query};
use lst_common::hub::{Config, ExecuteMsg, InstantiateMsg, Parameters, QueryMsg, State};

pub fn contract() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(execute, instantiate, query);
    Box::new(contract)
}

pub struct TestSetup {
    pub app: App,
    pub contract_addr: Addr,
    pub owner: Addr,
    pub user: Addr,
    pub lst_token: Addr,
    pub staking_coin_denom: String,
}

impl TestSetup {
    pub fn new() -> Self {
        let mut app = App::default();

        // Create test accounts
        let api = MockApi::default();
        let owner = api.addr_make("owner");
        let user = api.addr_make("user");
        let lst_token = api.addr_make("lst_token");

        // Set up initial balances
        app.sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: owner.to_string(),
            amount: coins(1000000, "stake"),
        }))
        .unwrap();
        app.sudo(SudoMsg::Bank(BankSudo::Mint {
            to_address: user.to_string(),
            amount: coins(1000000, "stake"),
        }))
        .unwrap();

        // Deploy the contract
        let contract_id = app.store_code(contract());
        let msg = InstantiateMsg {
            staking_coin_denom: "stake".to_string(),
            epoch_length: 100,
            unstaking_period: 1000,
        };

        let contract_addr = app
            .instantiate_contract(
                contract_id,
                owner.clone(),
                &msg,
                &[],
                "LST Staking Hub",
                None,
            )
            .unwrap();

        Self {
            app,
            contract_addr,
            owner,
            user,
            lst_token,
            staking_coin_denom: "stake".to_string(),
        }
    }

    pub fn mock_env(&self) -> Env {
        mock_env()
    }
    pub fn mock_info(&self, sender: &str, funds: &[cosmwasm_std::Coin]) -> MessageInfo {
        message_info(&Addr::unchecked(sender), funds)
    }

    pub fn execute(
        &mut self,
        sender: &Addr,
        msg: ExecuteMsg,
        funds: &[cosmwasm_std::Coin],
    ) -> cosmwasm_std::Response {
        let response = self
            .app
            .execute_contract(sender.clone(), self.contract_addr.clone(), &msg, funds)
            .unwrap();
        cosmwasm_std::Response::new()
            .add_attributes(response.events.into_iter().flat_map(|e| e.attributes))
    }

    pub fn query<T: DeserializeOwned>(&self, msg: QueryMsg) -> T {
        self.app
            .wrap()
            .query_wasm_smart(self.contract_addr.clone(), &msg)
            .unwrap()
    }

    pub fn get_config(&self) -> Config {
        self.query(QueryMsg::Config {})
    }

    pub fn get_state(&self) -> State {
        self.query(QueryMsg::State {})
    }

    pub fn get_parameters(&self) -> Parameters {
        self.query(QueryMsg::Parameters {})
    }
}
