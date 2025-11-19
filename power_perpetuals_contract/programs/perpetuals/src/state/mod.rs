pub mod oracle;
pub mod perpetuals;

use anchor_lang::prelude::*;

/// Pool account - tracks the overall state of the power perpetuals pool
#[account]
pub struct Pool {
    /// Bump seed for the pool PDA
    pub bump: u8,
    /// Authority that can update pool parameters
    pub authority: Pubkey,
    /// Total collateral deposited in the pool
    pub total_collateral: u64,
    /// Total notional value of all positions (squared exposure)
    pub total_notional: u64,
    /// Oracle price feed for the underlying asset
    pub oracle: Pubkey,
    /// Funding rate accumulator (for tracking funding payments)
    pub funding_rate: i64,
    /// Last update timestamp
    pub last_update: i64,
}

impl Pool {
    pub const LEN: usize = 8 + // discriminator
        1 + // bump
        32 + // authority
        8 + // total_collateral
        8 + // total_notional
        32 + // oracle
        8 + // funding_rate
        8; // last_update
}

/// Position account - tracks a user's power perpetual position
#[account]
pub struct Position {
    /// Owner of the position
    pub owner: Pubkey,
    /// Pool this position belongs to
    pub pool: Pubkey,
    /// Notional size of the position (squared exposure)
    pub notional: u64,
    /// Collateral backing this position
    pub collateral: u64,
    /// Entry price (for PnL calculation)
    pub entry_price: u64,
    /// Entry timestamp
    pub entry_time: i64,
    /// Funding rate at entry (for tracking funding payments)
    pub entry_funding_rate: i64,
}

impl Position {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // pool
        8 + // notional
        8 + // collateral
        8 + // entry_price
        8 + // entry_time
        8; // entry_funding_rate
}

