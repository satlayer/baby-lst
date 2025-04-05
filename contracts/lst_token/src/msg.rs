use cosmwasm_schema::cw_serde;
use cw20_base::msg::InstantiateMarketingInfo;

#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub hub_contract: String,
    pub marketing: Option<InstantiateMarketingInfo>,
}
