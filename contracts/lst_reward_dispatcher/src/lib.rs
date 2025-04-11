use cosmwasm_std::Decimal;

pub mod contract;
mod state;
pub mod testing;

// we'll use a raw decimal value that represents 30%
pub const MAX_FEE_RATE: Decimal = Decimal::raw(300000000000000000);
