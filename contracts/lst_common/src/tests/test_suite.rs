use std::collections::HashMap;

use cosmwasm_std::{coins, testing::MockApi, Addr, Coin, Decimal, Empty};
use cw_multi_test::{
    error::AnyError, App, AppResponse, BankSudo, Contract, ContractWrapper, Executor, SudoMsg,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    hub::InstantiateMsg as HubInstantiateMsg, rewards_msg::InstantiateMsg as RewardInstantiateMsg,
    types::LstResult,
};

use lst_reward_dispatcher::contract as reward;
use lst_staking_hub as hub;

pub struct SatLayerTestSuite {
    pub app: App,
    pub owner: Addr,
    pub contracts: HashMap<String, Addr>,
    pub denom: String,
}

impl SatLayerTestSuite {
    pub fn new() -> Self {
        let api = MockApi::default();
        Self {
            app: App::default(),
            owner: api.addr_make("owner"),
            contracts: HashMap::new(),
            denom: String::from("ubbn"),
        }
    }

    pub fn store_code(&mut self, contract: Box<dyn Contract<Empty>>) -> u64 {
        self.app.store_code(contract)
    }

    pub fn deploy_contract(
        &mut self,
        label: &str,
        code_id: u64,
        msg: &impl Serialize,
        funds: &[Coin],
    ) -> Addr {
        let addr = self
            .app
            .instantiate_contract(code_id, self.owner.clone(), msg, funds, label, None)
            .unwrap();
        self.contracts.insert(label.to_string(), addr.clone());
        addr
    }

    pub fn get_contract_addr(&self, label: &str) -> Option<&Addr> {
        self.contracts.get(label)
    }

    pub fn mint_token(&mut self, to: String, amount: u128) {
        self.app
            .sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: to,
                amount: coins(amount, self.denom.clone()),
            }))
            .unwrap();
    }

    pub fn execute<M>(
        &mut self,
        contract: &str,
        sender: impl Into<Option<Addr>>,
        msg: &M,
        funds: &[Coin],
    ) -> Result<AppResponse, AnyError>
    where
        M: Serialize + std::fmt::Debug,
    {
        let contract_addr = self.contracts.get(contract.into()).unwrap();
        self.app.execute_contract(
            sender.into().unwrap_or(self.owner.clone()),
            contract_addr.clone(),
            msg,
            funds,
        )
    }

    pub fn query<T>(&mut self, contract: &str, msg: &impl serde::Serialize) -> LstResult<T>
    where
        T: DeserializeOwned,
    {
        let contract_addr = self
            .get_contract_addr(contract)
            .ok_or_else(|| "Contract Not Found")
            .unwrap();
        Ok(self.app.wrap().query_wasm_smart::<T>(contract_addr, msg)?)
    }

    pub fn setup_hub(&mut self, epoch_length: u64, unstaking_period: u64, funds: &[Coin]) -> Addr {
        let contract = Box::new(ContractWrapper::new(
            hub::execute,
            hub::instantiate,
            hub::query,
        ));

        let code_id = self.store_code(contract);
        self.deploy_contract(
            "hub",
            code_id,
            &HubInstantiateMsg {
                epoch_length,
                staking_coin_denom: self.denom.clone(),
                unstaking_period,
            },
            funds,
        )
    }

    pub fn setup_reward_dispatcher(
        &mut self,
        satlayer_fee_addr: Addr,
        satlayer_fee_rate: Decimal,
        hub_addr: Addr,
        funds: &[Coin],
    ) -> Addr {
        let contract = Box::new(ContractWrapper::new(
            reward::execute,
            reward::instantiate,
            reward::query,
        ));
        let code_id = self.store_code(contract);
        self.deploy_contract(
            "reward",
            code_id,
            &RewardInstantiateMsg {
                reward_denom: self.denom.clone(),
                satlayer_fee_addr: satlayer_fee_addr.to_string(),
                satlayer_fee_rate,
                hub_contract: hub_addr.to_string(),
            },
            funds,
        )
    }
}
