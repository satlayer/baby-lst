use cw_storage_plus::Item;
use lst_common::rewards_msg::Config;

pub const CONFIG: Item<Config> = Item::new("config");

#[cfg(test)]
mod tests {
    use cosmwasm_std::{CanonicalAddr, Decimal};
    use serde_json;

    use super::*;

    fn setup_config() -> Config {
        Config {
            owner: CanonicalAddr::from(vec![1, 2, 3]),
            hub_contract: CanonicalAddr::from(vec![4, 5, 6]),
            reward_denom: "whoami".to_string(),
            satlayer_fee_addr: CanonicalAddr::from(vec![7, 8, 9]),
            satlayer_fee_rate: Decimal::percent(5),
        }
    }

    #[test]
    fn test_config_serialize_deserialize() {
        let config = setup_config();

        let seralized = serde_json::to_string(&config).unwrap();

        let deseralized = serde_json::from_str::<Config>(&seralized).unwrap();

        assert_eq!(config.owner, deseralized.owner);
        assert_eq!(config.hub_contract, deseralized.hub_contract);
        assert_eq!(config.satlayer_fee_addr, deseralized.satlayer_fee_addr);
        assert_eq!(config.reward_denom, deseralized.reward_denom);
        assert_eq!(config.satlayer_fee_rate, deseralized.satlayer_fee_rate);
    }
}
