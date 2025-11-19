//! UpdatePoolAum instruction handler
//! 
//! This instruction allows anyone to update a pool's Assets Under Management (AUM) value.
//! The AUM is recalculated using current oracle prices and pool state. This is useful
//! for keeping pool statistics up-to-date and can be called permissionlessly.

use {
    crate::state::{
        perpetuals::Perpetuals,
        pool::{AumCalcMode, Pool},
    },
    anchor_lang::prelude::*,
};

/// Accounts required for updating pool AUM
#[derive(Accounts)]
pub struct UpdatePoolAum<'info> {
    /// Payer account (signer, pays for transaction fees)
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Main perpetuals program account
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account (mutable, AUM will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,
    // remaining accounts:
    //   pool.tokens.len() custody accounts (read-only, unsigned)
    //   pool.tokens.len() custody oracles (read-only, unsigned)
}

/// Update pool's Assets Under Management (AUM) value
/// 
/// This function recalculates the pool's AUM using current oracle prices and pool state.
/// The AUM is calculated using EMA (Exponential Moving Average) mode for price stability.
/// This can be called permissionlessly to keep pool statistics up-to-date.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// 
/// # Returns
/// `Result<u128>` - Updated AUM value in USD (scaled to USD_DECIMALS), or error
pub fn update_pool_aum<'info>(ctx: Context<'_, '_, 'info, 'info, UpdatePoolAum<'info>>) -> Result<u128> {
    let perpetuals: &Account<'_, Perpetuals> = ctx.accounts.perpetuals.as_ref();
    let pool = ctx.accounts.pool.as_mut();

    // Get current time for price calculations
    let curtime: i64 = perpetuals.get_time()?;

    // Update pool AUM
    msg!("Update pool asset under management");

    // Log previous AUM value for debugging
    msg!("Previous value: {}", pool.aum_usd);

    // Recalculate AUM using EMA mode
    // EMA mode uses exponential moving average prices for more stable calculations
    // ctx.remaining_accounts contains custody accounts and oracle accounts for all tokens
    pool.aum_usd =
        pool.get_assets_under_management_usd(AumCalcMode::EMA, ctx.remaining_accounts, curtime)?;

    // Log updated AUM value for debugging
    msg!("Updated value: {}", pool.aum_usd);

    Ok(pool.aum_usd)
}