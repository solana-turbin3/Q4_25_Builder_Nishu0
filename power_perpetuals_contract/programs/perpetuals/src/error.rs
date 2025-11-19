use anchor_lang::prelude::*;

#[error_code]
pub enum PerpetualsError {
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Unsupported oracle type")]
    UnsupportedOracle,
    #[msg("Invalid oracle account")]
    InvalidOracleAccount,
    #[msg("Stale oracle price")]
    StaleOraclePrice,
    #[msg("Invalid oracle price")]
    InvalidOraclePrice,
}

