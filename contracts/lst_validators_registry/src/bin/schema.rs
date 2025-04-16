use cosmwasm_schema::write_api;

use lst_common::validator::{ExecuteMsg, InstantiateMsg, QueryMsg};

#[cfg(not(tarpaulin_include))]
fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
