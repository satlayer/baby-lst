use cosmwasm_schema::{cw_serde, QueryResponses};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    Delegate {},
}

// Ok(cosmwasm_std::WasmMsg::Execute {
//     contract_addr: token_addr.to_string(),
//     msg: to_json_binary(&cw20::Cw20ExecuteMsg::Transfer {
//         recipient: recipient.to_string(),
//         amount,
//     })?,
//     funds: vec![],
// }
// .into())