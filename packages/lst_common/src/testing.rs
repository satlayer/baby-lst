use crate::address::{convert_addr_by_prefix, VALIDATOR_ADDR_PREFIX};
use crate::babylon_msg::MsgWrappedDelegate;
use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{
    to_json_binary, Addr, AnyMsg, Api, BlockInfo, Coin, CustomMsg, CustomQuery, Empty, Env,
    StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20_base;
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    App, AppResponse, BankKeeper, Contract, CosmosRouter, DistributionKeeper, Executor,
    FailingModule, GovFailingModule, IbcFailingModule, StakeKeeper, Stargate, WasmKeeper,
};
use prost::Message;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub struct CustomStargate {}

/// CustomStargate is to catch custom keeper messages sent to Babylon app through protobuf and do logic
/// 
/// TODO: separate into different keeper modules (etc. epoching, checkpointing, etc)
impl Stargate for CustomStargate {
    fn execute_any<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        sender: Addr,
        msg: AnyMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        println!("Custom Stargate execute_any called {:?}, {:?}", msg, sender);

        match msg.type_url.as_str() {
            "/babylon.epoching.v1.MsgWrappedDelegate" => {
                // Handle MsgDelegate to validator - reroute to default staking module
                let msg: MsgWrappedDelegate = MsgWrappedDelegate::decode(msg.value.as_slice())?;
                let msg_delegate = msg
                    .msg
                    .ok_or(StdError::generic_err("Missing MsgDelegate"))?;

                let amount = msg_delegate
                    .amount
                    .ok_or(StdError::generic_err("Missing amount"))?;

                // send the MsgDelegate to the staking module
                _router.execute(
                    _api,
                    _storage,
                    _block,
                    sender,
                    StakingMsg::Delegate {
                        validator: convert_addr_by_prefix(
                            &msg_delegate.validator_address,
                            VALIDATOR_ADDR_PREFIX,
                        ),
                        amount: Coin::new(
                            Uint128::from_str(amount.amount.as_str()).unwrap(),
                            amount.denom,
                        ),
                    }
                    .into(),
                )
            }
            _ => {
                // Handle other messages
                Err(StdError::generic_err("Unknown message type").into())
            }
        }
    }
}

pub type BabylonApp = App<
    BankKeeper,
    MockApi,
    MockStorage,
    FailingModule<Empty, Empty, Empty>,
    WasmKeeper<Empty, Empty>,
    StakeKeeper,
    DistributionKeeper,
    IbcFailingModule,
    GovFailingModule,
    CustomStargate,
>;

/// TestingContract is a trait that provides a common interface for setting up testing contracts.
pub trait TestingContract<IM, EM, QM>
where
    IM: serde::Serialize,
    EM: serde::Serialize,
    QM: serde::Serialize,
{
    fn wrapper() -> Box<dyn Contract<Empty>>;

    fn default_init(app: &mut BabylonApp, env: &Env) -> IM;

    fn new(app: &mut BabylonApp, env: &Env, msg: Option<IM>) -> Self;

    fn store_code(app: &mut BabylonApp) -> u64 {
        app.store_code(Self::wrapper())
    }

    fn instantiate(
        app: &mut BabylonApp,
        code_id: u64,
        label: &str,
        sender: Option<Addr>,
        msg: &IM,
    ) -> Addr {
        let admin = app.api().addr_make("admin");
        let sender = sender.unwrap_or_else(|| app.api().addr_make("owner"));
        let addr = app
            .instantiate_contract(code_id, sender, msg, &[], label, Some(admin.to_string()))
            .unwrap();
        Self::set_contract_addr(app, label, &addr);
        addr
    }

    /// Set the contract address in the storage for the given label.
    /// Using the storage system for easy orchestration of contract addresses for testing.
    fn set_contract_addr(app: &mut BabylonApp, label: &str, addr: &Addr) {
        let key = format!("CONTRACT:{}", label);
        let value = String::from_utf8(addr.as_bytes().to_vec()).unwrap();
        app.storage_mut().set(key.as_bytes(), value.as_bytes());
    }

    /// Get the contract address in the storage for the given label.
    fn get_contract_addr(app: &BabylonApp, label: &str) -> Addr {
        let key = format!("CONTRACT:{}", label);
        let value = app.storage().get(key.as_bytes()).unwrap();
        Addr::unchecked(String::from_utf8(value).unwrap())
    }

    fn addr(&self) -> &Addr;

    fn execute(&self, app: &mut BabylonApp, sender: &Addr, msg: &EM) -> AnyResult<AppResponse> {
        self.execute_with_funds(app, sender, msg, vec![])
    }

    fn execute_with_funds(
        &self,
        app: &mut BabylonApp,
        sender: &Addr,
        msg: &EM,
        funds: Vec<Coin>,
    ) -> AnyResult<AppResponse> {
        let msg_bin = to_json_binary(&msg).expect("cannot serialize ExecuteMsg");
        let execute_msg = WasmMsg::Execute {
            contract_addr: self.addr().to_string(),
            msg: msg_bin,
            funds,
        };

        app.execute(sender.clone(), execute_msg.into())
    }

    fn query<T: DeserializeOwned>(&self, app: &BabylonApp, msg: &QM) -> StdResult<T> {
        app.wrap().query_wasm_smart(self.addr(), &msg)
    }

    // TODO: fn migrate
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cw20TokenContract {
    pub addr: Addr,
    pub init: cw20_base::msg::InstantiateMsg,
}
