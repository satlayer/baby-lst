use cosmwasm_schema::write_api;

use cw20_base::msg::{ExecuteMsg, QueryMsg};
use lst_token::msg::TokenInitMsg;

fn main() {
    write_api! {
        instantiate: TokenInitMsg,
        execute: ExecuteMsg,
        query: QueryMsg,
    }
}
