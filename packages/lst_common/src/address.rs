use bech32::Bech32;

pub const VALIDATOR_ADDR_PREFIX: &str = "bbnvaloper";

pub fn convert_addr_by_prefix(address: &str, prefix: &str) -> String {
    let (_hrp, data) = bech32::decode(address).expect("Invalid Bech32 account address");
    let hrp = bech32::Hrp::parse(prefix).expect("Invalid prefix");
    bech32::encode::<Bech32>(hrp, &data).expect("Failed to encode address")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_to_validator_addr() {
        let val_account_addr = "bbn109x4ruspxarwt62puwcenhclw36l9v7jcgrj48";
        let converted_val_addr = convert_addr_by_prefix(val_account_addr, VALIDATOR_ADDR_PREFIX);
        assert_eq!(
            converted_val_addr,
            "bbnvaloper109x4ruspxarwt62puwcenhclw36l9v7j92f0ex"
        );
    }

    #[test]
    fn test_same_prefix() {
        let bbn_addr = "bbnvaloper109x4ruspxarwt62puwcenhclw36l9v7j92f0ex";

        let result = convert_addr_by_prefix(bbn_addr, VALIDATOR_ADDR_PREFIX);
        assert_eq!(bbn_addr, result);
    }

    #[test]
    #[should_panic]
    fn test_invalid_address() {
        convert_addr_by_prefix("invalid_addr", VALIDATOR_ADDR_PREFIX);
    }

    #[test]
    #[should_panic]
    fn test_invalid_prefix() {
        let address = "bbnvaloper109x4ruspxarwt62puwcenhclw36l9v7j92f0ex";
        convert_addr_by_prefix(address, "");
    }

    #[test]
    fn test_multiple_conversion() {
        let original = "bbn109x4ruspxarwt62puwcenhclw36l9v7jcgrj48";
        let to_bbn_valoper = convert_addr_by_prefix(original, VALIDATOR_ADDR_PREFIX);
        let back_bbn = convert_addr_by_prefix(&to_bbn_valoper, "bbn");
        assert_eq!(original, back_bbn);
    }
}
