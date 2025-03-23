use cosmwasm_std::{Deps, Env};
use lst_common::{
    hub::{
        AllHistoryResponse, ConfigResponse, CurrentBatch, Parameters, State,
        UnstakeRequestsResponse, WithdrawableUnstakedResponse,
    },
    types::LstResult,
};

pub fn query_config(deps: Deps) -> LstResult<ConfigResponse> {
    todo!()
}

pub fn query_state(deps: Deps, env: Env) -> LstResult<State> {
    todo!()
}

pub fn query_current_batch(deps: Deps) -> LstResult<CurrentBatch> {
    todo!()
}

pub fn query_parameters(deps: Deps) -> LstResult<Parameters> {
    todo!()
}

pub fn query_withdrawable_unstaked(
    deps: Deps,
    env: Env,
    address: String,
) -> LstResult<WithdrawableUnstakedResponse> {
    todo!()
}

pub fn query_unstake_requests(
    deps: Deps,
    env: Env,
    address: String,
) -> LstResult<UnstakeRequestsResponse> {
    todo!()
}

pub fn query_unstake_requests_limitation(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> LstResult<AllHistoryResponse> {
    todo!()
}
