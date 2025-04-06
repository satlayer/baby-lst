use cosmwasm_schema::{
    schemars::JsonSchema,
    serde::{Deserialize, Serialize},
};
use cosmwasm_std::{Uint128, Uint256};

#[derive(
    Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, JsonSchema,
)]
pub struct SignedInt(#[schemars(with = "String")] pub Uint128, pub bool);

impl SignedInt {
    pub fn from_subtraction<A, B>(minuend: A, subtrahend: B) -> SignedInt
    where
        A: Into<Uint256>,
        B: Into<Uint256>,
    {
        let minuend_256: Uint256 = minuend.into();
        let subtrahend_256: Uint256 = subtrahend.into();
        let subtraction_256 = minuend_256.checked_sub(subtrahend_256);

        if let Ok(result) = subtraction_256 {
            return SignedInt(Uint128::try_from(result).unwrap(), false);
        }

        // If subtraction fails, calculate the negative value
        SignedInt(
            Uint128::try_from(subtrahend_256.checked_sub(minuend_256).unwrap()).unwrap(),
            true,
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::Uint128;

    #[test]
    fn from_subtraction() {
        let min = Uint128::new(1000010);
        let sub = Uint128::new(1000000);
        let signed_integer = SignedInt::from_subtraction(min, sub);
        assert_eq!(signed_integer.0, Uint128::new(10));
        assert!(!signed_integer.1);

        //check negative values
        let min = Uint128::new(1000000);
        let sub = Uint128::new(1100000);
        let signed_integer = SignedInt::from_subtraction(min, sub);
        assert_eq!(signed_integer.0, Uint128::new(100000));
        assert!(signed_integer.1);
    }
}
