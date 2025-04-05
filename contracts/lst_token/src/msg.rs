use cosmwasm_schema::cw_serde;
use cw20_base::msg::InstantiateMarketingInfo;

#[cw_serde]
pub struct TokenInitMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<Cw20Coin>,
    pub hub_contract: String,
    pub marketing: Option<InstantiateMarketingInfo>,
}
