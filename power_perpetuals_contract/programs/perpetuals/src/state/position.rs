//! Position state for perpetuals trading
//! 
//! This module defines the Position account structure and related enums
//! for tracking user positions in power perpetuals.

use {
    crate::{math, state::perpetuals::Perpetuals},
    anchor_lang::prelude::*,
};

/// Position side (direction of the trade)
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Debug)]
pub enum Side {
    /// No position (closed or not opened)
    None,
    /// Long position (betting price will go up)
    Long,
    /// Short position (betting price will go down)
    Short,
}

impl Default for Side {
    fn default() -> Self {
        Self::None
    }
}

/// Collateral change operation type
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Debug)]
pub enum CollateralChange {
    /// No collateral change
    None,
    /// Adding collateral to position
    Add,
    /// Removing collateral from position
    Remove,
}

impl Default for CollateralChange {
    fn default() -> Self {
        Self::None
    }
}

/// Position account - tracks a user's perpetual position
/// 
/// Stores all information about an open position including:
/// - Position metadata (owner, pool, custodies)
/// - Position state (side, price, size, collateral)
/// - PnL tracking (unrealized profit/loss)
/// - Interest tracking (cumulative interest snapshot)
#[account]
#[derive(Default, Debug)]
pub struct Position {
    /// Owner of the position (user's wallet address)
    pub owner: Pubkey,
    /// Pool this position belongs to
    pub pool: Pubkey,
    /// Custody account for the position token (the asset being traded)
    pub custody: Pubkey,
    /// Custody account for the collateral token (the asset used as margin)

    pub collateral_custody: Pubkey,

    /// Timestamp when position was opened
    pub open_time: i64,
    /// Timestamp of last position update
    pub update_time: i64,
    /// Position side (Long, Short, or None)
    pub side: Side,
    /// Power multiplier for power perpetuals (1-5)
    /// power=1: linear perps, power=2: squared perps, etc.
    pub power: u8,
    /// Entry price scaled to PRICE_DECIMALS
    pub price: u64,
    /// Position size in USD (scaled to USD_DECIMALS)
    pub size_usd: u64,
    /// Borrowed size in USD (for leveraged positions, scaled to USD_DECIMALS)
    pub borrow_size_usd: u64,
    /// Collateral value in USD (scaled to USD_DECIMALS)
    pub collateral_usd: u64,
    /// Unrealized profit in USD (scaled to USD_DECIMALS)
    pub unrealized_profit_usd: u64,
    /// Unrealized loss in USD (scaled to USD_DECIMALS)
    pub unrealized_loss_usd: u64,
    /// Cumulative interest snapshot (for calculating interest owed)
    pub cumulative_interest_snapshot: u128,
    /// Amount of tokens locked for this position (in position token decimals)
    pub locked_amount: u64,
    /// Amount of collateral tokens (in collateral token decimals)
    pub collateral_amount: u64,

    /// Bump seed for the position PDA
    pub bump: u8,
}

impl Position {
    /// Account size in bytes (8 byte discriminator + data)
    pub const LEN: usize = 8 + std::mem::size_of::<Position>();

    /// Calculate initial leverage for the position
    /// 
    /// Leverage = size_usd / collateral_usd
    /// 
    /// # Returns
    /// Leverage in BPS (basis points), e.g., 40000 = 4x leverage
    /// 
    /// # Errors
    /// Returns error if collateral_usd is 0 (division by zero)
    pub fn get_initial_leverage(&self) -> Result<u64> {
        math::checked_as_u64(math::checked_div(
            math::checked_mul(self.size_usd as u128, Perpetuals::BPS_POWER)?,
            self.collateral_usd as u128,
        )?)
    }
}