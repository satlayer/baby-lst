use cosmwasm_schema::write_api;

use lst_common::rewards_msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

#[cfg(not(tarpaulin_include))]
fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
