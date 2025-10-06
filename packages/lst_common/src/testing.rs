use crate::address::{convert_addr_by_prefix, VALIDATOR_ADDR_PREFIX};
use crate::babylon::{
    BabylonModule, EpochingMsg, EpochingQuery, EPOCH_LENGTH, STAKING_EPOCH_LENGTH_BLOCKS,
    STAKING_EPOCH_START_BLOCK_HEIGHT,
};
use crate::babylon_msg::MsgWrappedDelegate;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    to_json_binary, Addr, AnyMsg, Api, BlockInfo, Coin, CosmosMsg, CustomMsg, CustomQuery, Env,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw_multi_test::error::AnyResult;
use cw_multi_test::{
    App, AppResponse, BankKeeper, BasicAppBuilder, Contract, CosmosRouter, DistributionKeeper,
    Executor, GovFailingModule, IbcFailingModule, Router, StakeKeeper, Stargate, WasmKeeper,
};
use prost::Message;
use serde::de::DeserializeOwned;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

pub struct CustomStargate {}

impl Default for CustomStargate {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomStargate {
    pub fn new() -> Self {
        Self {}
    }
}

/// CustomStargate is to catch custom keeper messages sent to Babylon app through protobuf and do logic
///
/// TODO: separate into different keeper modules (etc. epoching, checkpointing, etc)
impl Stargate for CustomStargate {
    fn execute_any<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        sender: Addr,
        msg: AnyMsg,
    ) -> AnyResult<AppResponse>
    where
        ExecC: CustomMsg + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
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

                // TODO: fix this type conversion hack
                let custom_msg: ExecC = serde_json::from_slice(
                    EpochingMsg::Delegate {
                        validator: convert_addr_by_prefix(
                            &msg_delegate.validator_address,
                            VALIDATOR_ADDR_PREFIX,
                        ),
                        amount: Coin {
                            denom: amount.denom,
                            amount: Uint128::from_str(&amount.amount.to_string())?,
                        },
                    }
                    .to_binary()
                    .as_slice(),
                )?;

                // send the MsgDelegate to the staking module
                router.execute(api, storage, block, sender, CosmosMsg::Custom(custom_msg))
            }
            _ => {
                // Handle other messages
                Err(StdError::generic_err("Unknown message type").into())
            }
        }
    }
}

pub type BabylonAppWrapped = App<
    BankKeeper,
    MockApi,
    MockStorage,
    BabylonModule,
    WasmKeeper<EpochingMsg, EpochingQuery>,
    StakeKeeper,
    DistributionKeeper,
    IbcFailingModule,
    GovFailingModule,
    CustomStargate,
>;

pub struct BabylonApp(BabylonAppWrapped);

impl Deref for BabylonApp {
    type Target = BabylonAppWrapped;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BabylonApp {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl BabylonApp {
    pub fn new<F>(init_fn: F) -> Self
    where
        F: FnOnce(
            &mut Router<
                BankKeeper,
                BabylonModule,
                WasmKeeper<EpochingMsg, EpochingQuery>,
                StakeKeeper,
                DistributionKeeper,
                IbcFailingModule,
                GovFailingModule,
                CustomStargate,
            >,
            &MockApi,
            &mut dyn Storage,
        ),
    {
        Self(
            BasicAppBuilder::<EpochingMsg, EpochingQuery>::new_custom()
                .with_custom(BabylonModule::new())
                .with_stargate(CustomStargate::new())
                .with_block(BlockInfo {
                    height: STAKING_EPOCH_START_BLOCK_HEIGHT, // start the height from epoch 0
                    time: mock_env().block.time,
                    chain_id: mock_env().block.chain_id,
                })
                .build(init_fn),
        )
    }

    pub fn next_epoch(&mut self) -> AnyResult<AppResponse> {
        let sender = self.api().addr_make("epoching");
        let res = self.execute(sender, EpochingMsg::NextEpoch {}.into());

        // fast forward the block height to the next epoch
        self.update_block(|block_info: &mut BlockInfo| {
            let passed_epoch = block_info.height - STAKING_EPOCH_START_BLOCK_HEIGHT;
            let next_epoch = (passed_epoch / EPOCH_LENGTH) + 1;
            block_info.height =
                STAKING_EPOCH_START_BLOCK_HEIGHT + (next_epoch * STAKING_EPOCH_LENGTH_BLOCKS);
        });

        res
    }
}

/// TestingContract is a trait that provides a common interface for setting up testing contracts.
pub trait TestingContract<IM, EM, QM, ExecC = EpochingMsg, QueryC = EpochingQuery>
where
    IM: serde::Serialize,
    EM: serde::Serialize,
    QM: serde::Serialize,
    ExecC: CustomMsg + DeserializeOwned + 'static,
    QueryC: CustomQuery + DeserializeOwned + 'static,
{
    fn wrapper() -> Box<dyn Contract<EpochingMsg, EpochingQuery>>;

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
