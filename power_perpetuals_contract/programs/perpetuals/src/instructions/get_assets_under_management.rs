//! GetAssetsUnderManagement instruction handler
//! 
//! This is a view/query instruction that calculates and returns the total
//! Assets Under Management (AUM) for a pool. It's a read-only function that
//! doesn't modify any state, useful for displaying pool value to users.

use {
    crate::state::{
        perpetuals::Perpetuals,
        pool::{AumCalcMode, Pool},
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying assets under management
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates the total value of all assets in the pool.
#[derive(Accounts)]
pub struct GetAssetsUnderManagement<'info> {
    /// Main perpetuals program account (read-only)
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account to query (read-only)
    #[account(
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    
    // Remaining accounts (read-only, unsigned):
    //   - pool.custodies.len() custody accounts (for reading token balances)
    //   - pool.custodies.len() custody oracle accounts (for price feeds)
}

/// Parameters for querying assets under management
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetAssetsUnderManagementParams {}

/// Get total Assets Under Management (AUM) for a pool
/// 
/// This function calculates the total value of all assets in the pool in USD.
/// Uses EMA (Exponential Moving Average) price mode for calculation.
/// 
/// The AUM includes:
/// - Value of all tokens in the pool
/// - Optionally unrealized PnL from open positions (if configured)
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// Total AUM in USD (scaled to USD_DECIMALS)
pub fn get_assets_under_management(
    ctx: Context<GetAssetsUnderManagement>,
    _params: &GetAssetsUnderManagementParams,
) -> Result<u128> {
    ctx.accounts.pool.get_assets_under_management_usd(
        AumCalcMode::EMA,
        ctx.remaining_accounts,
        ctx.accounts.perpetuals.get_time()?,
    )
}