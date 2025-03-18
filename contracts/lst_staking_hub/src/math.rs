use cosmwasm_std::{Decimal, Uint128, Uint256};

// since decimal is 18 decimals in cosmwasm, we multiply the numerator by 10^18 to balance out
// the conversion of the denominator to atomics
const DECIMAL_FRACTIONAL: Uint256 = Uint256::from_u128(1_000_000_000_000_000_000u128); // 1*10**18

pub fn decimal_division(numerator: Uint128, denominator: Decimal) -> Uint128 {
    let scaled_numerator = Uint256::from(numerator) * DECIMAL_FRACTIONAL;
    (scaled_numerator / Uint256::from(denominator.atomics()))
        .try_into()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_division() {
        // Test case 1: Very small denominator (close to zero)
        let result = decimal_division(
            Uint128::from(1000u128),
            Decimal::from_ratio(1u128, 1000000u128),
        );
        assert_eq!(result, Uint128::from(1000000000u128));

        // Test case 2: Very large numbers with decimal result
        let result = decimal_division(
            Uint128::from(1000000000000000000000u128),
            Decimal::from_ratio(3u128, 2u128),
        );
        assert_eq!(result, Uint128::from(666666666666666666666u128));

        // Test case 3: Division with large denominator
        let result = decimal_division(
            Uint128::from(1000000000000000000u128),
            Decimal::from_ratio(1000000u128, 1u128),
        );
        assert_eq!(result, Uint128::from(1000000000000u128));

        // Test case 4: Complex ratio with large numbers
        let result = decimal_division(
            Uint128::from(1000000000000000000u128),
            Decimal::from_ratio(100u128, 3u128),
        );
        assert_eq!(result, Uint128::from(30000000000000000u128));
    }
}
