use std::{collections::HashMap, fmt};

use cosmwasm_std::{
    coins,
    testing::{mock_dependencies, mock_env, MockApi, MockQuerier, MockStorage},
    Addr, Coin, Decimal, Empty, Env, OwnedDeps, Validator as StdValidator,
};
use cw_multi_test::{
    error::AnyError, App, AppBuilder, AppResponse, BankSudo, Contract, ContractWrapper, Executor,
    IntoBech32, StakingInfo, SudoMsg,
};
use serde::{de::DeserializeOwned, Serialize};

use lst_common::{
    hub::InstantiateMsg as HubInstantiateMsg,
    rewards_msg::InstantiateMsg as RewardInstantiateMsg,
    types::LstResult,
    validator::{InstantiateMsg as ValidatorRegistryInitMsg, Validator},
};

use lst_reward_dispatcher::contract as reward;
use lst_staking_hub as hub;
use lst_token::{contract as token, msg::TokenInitMsg};
use lst_validators_registry::contract as validator_registry;

pub const OWNER: &str = "owner";
pub const NATIVE_DENOM: &str = "ubbn";
pub const FEE_ADDR: &str = "fee_receiver";
pub const UNBOUND_TIME: u64 = 300;
pub const VALIDATOR_ADDR: &str = "validator";

#[derive(Eq, PartialEq, Hash)]
pub enum ContractType {
    Hub,
    Reward,
    ValidatorRegistry,
    Token,
}

impl fmt::Display for ContractType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ContractType::Hub => "hub",
                ContractType::Reward => "reward",
                ContractType::ValidatorRegistry => "validator_registry",
                ContractType::Token => "token",
            }
        )
    }
}

pub struct TestContext {
    pub app: App,
    pub owner: Addr,
    pub contracts: HashMap<ContractType, String>,
    pub denom: String,
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TestContext {
    pub fn new() -> Self {
        let api = MockApi::default();
        let app = AppBuilder::default().build(|router, api, storage| {
            // setup staking params
            router
                .staking
                .setup(
                    storage,
                    StakingInfo {
                        bonded_denom: NATIVE_DENOM.to_string(),
                        unbonding_time: UNBOUND_TIME,
                        apr: Decimal::percent(10),
                    },
                )
                .unwrap();

            let validator_addr = VALIDATOR_ADDR.into_bech32_with_prefix("bbnvaloper");

            let valoper = StdValidator::new(
                validator_addr.to_string(),
                Decimal::percent(10),
                Decimal::percent(90),
                Decimal::percent(1),
            );

            // add validator
            router
                .staking
                .add_validator(api, storage, &mock_env().block, valoper)
                .unwrap();
        });
        Self {
            app,
            owner: api.addr_make(OWNER),
            contracts: HashMap::new(),
            denom: String::from(NATIVE_DENOM),
        }
    }

    pub fn mock_api() -> MockApi {
        MockApi::default()
    }

    pub fn env() -> Env {
        mock_env()
    }

    pub fn deps() -> OwnedDeps<MockStorage, MockApi, MockQuerier, Empty> {
        mock_dependencies()
    }

    pub fn make_addr(addr: &str) -> Addr {
        let api = TestContext::mock_api();
        api.addr_make(addr)
    }

    pub fn store_code(&mut self, contract: Box<dyn Contract<Empty>>) -> u64 {
        self.app.store_code(contract)
    }

    pub fn init_contract(
        &mut self,
        contract_type: ContractType,
        code_id: u64,
        msg: &impl Serialize,
        funds: &[Coin],
    ) {
        let addr = self
            .app
            .instantiate_contract(
                code_id,
                self.owner.clone(),
                msg,
                funds,
                contract_type.to_string(),
                None,
            )
            .unwrap();
        self.contracts.insert(contract_type, addr.to_string());
        // addr
    }

    pub fn get_contract_addr(&self, contract_type: ContractType) -> Option<String> {
        self.contracts.get(&contract_type).cloned()
    }

    pub fn mint_token(&mut self, to: Addr, amount: u128) {
        self.app
            .sudo(SudoMsg::Bank(BankSudo::Mint {
                to_address: to.to_string(),
                amount: coins(amount, self.denom.clone()),
            }))
            .unwrap();
    }

    pub fn execute<M>(
        &mut self,
        contract: ContractType,
        sender: impl Into<Option<Addr>>,
        msg: &M,
        funds: &[Coin],
    ) -> Result<AppResponse, AnyError>
    where
        M: Serialize + std::fmt::Debug,
    {
        let contract_addr = self.contracts.get(&contract).unwrap();
        self.app.execute_contract(
            sender.into().unwrap_or(self.owner.clone()),
            Addr::unchecked(contract_addr),
            msg,
            funds,
        )
    }

    pub fn query<T>(&mut self, contract: ContractType, msg: &impl serde::Serialize) -> LstResult<T>
    where
        T: DeserializeOwned,
    {
        let contract_addr = self
            .get_contract_addr(contract)
            .ok_or("Contract Not Found")
            .unwrap();
        Ok(self.app.wrap().query_wasm_smart::<T>(contract_addr, msg)?)
    }

    pub fn mock_hub_contract(&self) -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new(
            hub::execute,
            hub::instantiate,
            hub::query,
        ))
    }

    pub fn mock_token_contract(&self) -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new(
            token::execute,
            token::instantiate,
            token::query,
        ))
    }

    pub fn mock_reward_contract(&self) -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new(
            reward::execute,
            reward::instantiate,
            reward::query,
        ))
    }

    pub fn mock_validator_contract(&self) -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new(
            validator_registry::execute,
            validator_registry::instantiate,
            validator_registry::query,
        ))
    }

    pub fn init_hub_contract(&mut self, epoch_length: u64, unstaking_period: u64) -> &mut Self {
        let code_id = self.store_code(self.mock_hub_contract());
        self.init_contract(
            ContractType::Hub,
            code_id,
            &HubInstantiateMsg {
                epoch_length,
                staking_coin_denom: self.denom.clone(),
                unstaking_period,
            },
            &[],
        );
        self
    }

    pub fn init_reward_contract(
        &mut self,
        satlayer_fee_addr: String,
        satlayer_fee_rate: Decimal,
        hub_addr: String,
    ) -> &mut Self {
        let code_id = self.store_code(self.mock_reward_contract());
        self.init_contract(
            ContractType::Reward,
            code_id,
            &RewardInstantiateMsg {
                reward_denom: self.denom.clone(),
                satlayer_fee_addr,
                satlayer_fee_rate,
                hub_contract: hub_addr,
            },
            &[],
        );
        self
    }

    pub fn init_validator_registry(
        &mut self,
        hub_contract: String,
        validators: Vec<Validator>,
    ) -> &mut Self {
        let code_id = self.store_code(self.mock_validator_contract());
        self.init_contract(
            ContractType::ValidatorRegistry,
            code_id,
            &ValidatorRegistryInitMsg {
                validators,
                hub_contract,
            },
            &[],
        );
        self
    }

    pub fn init_token_contract(&mut self, hub_contract: String) -> &mut Self {
        let code_id = self.store_code(self.mock_token_contract());
        self.init_contract(
            ContractType::Token,
            code_id,
            &TokenInitMsg {
                hub_contract,
                name: "CW20TestToken".to_string(),
                symbol: "TST".to_string(),
                decimals: 6,
                initial_balances: vec![],
                marketing: None,
            },
            &[],
        );
        self
    }
}
