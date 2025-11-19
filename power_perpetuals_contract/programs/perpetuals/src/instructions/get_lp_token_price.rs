//! GetLpTokenPrice instruction handler
//! 
//! This is a view/query instruction that calculates the current price of LP tokens
//! in USD. The LP token price represents the value of each LP token based on the
//! pool's total Assets Under Management (AUM) divided by the LP token supply.

use {
    crate::{
        math,
        state::{
            perpetuals::Perpetuals,
            pool::{AumCalcMode, Pool},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::Mint,
    num_traits::Zero,
};

/// Accounts required for querying LP token price
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates the current USD value of LP tokens based on pool AUM.
#[derive(Accounts)]
pub struct GetLpTokenPrice<'info> {
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

    /// LP token mint for this pool (read-only, to get supply)
    #[account(
        seeds = [b"lp_token_mint",
                 pool.key().as_ref()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,
    // remaining accounts:
    //   pool.tokens.len() custody accounts (read-only, unsigned)
    //   pool.tokens.len() custody oracles (read-only, unsigned)
}

/// Parameters for querying LP token price
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetLpTokenPriceParams {}

/// Calculate the current price of LP tokens in USD (view function)
/// 
/// This function calculates the value of each LP token by dividing the pool's
/// total Assets Under Management (AUM) by the LP token supply.
/// 
/// Formula: lp_token_price = pool_aum_usd / lp_token_supply
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<u64>` - LP token price in USD (scaled to USD_DECIMALS), or 0 if supply is zero
pub fn get_lp_token_price(
    ctx: Context<GetLpTokenPrice>,
    _params: &GetLpTokenPriceParams,
) -> Result<u64> {
    // Calculate total Assets Under Management using EMA mode
    // This gives a smoothed value based on exponential moving average prices
    let aum_usd = math::checked_as_u64(ctx.accounts.pool.get_assets_under_management_usd(
        AumCalcMode::EMA,
        ctx.remaining_accounts,
        ctx.accounts.perpetuals.get_time()?,
    )?)?;

    msg!("aum_usd: {}", aum_usd);

    // Get current LP token supply
    let lp_supply = ctx.accounts.lp_token_mint.supply;

    msg!("lp_supply: {}", lp_supply);

    // If no LP tokens exist yet, return 0 (pool not initialized)
    if lp_supply.is_zero() {
        return Ok(0);
    }

    // Calculate LP token price: price = aum_usd / lp_supply
    // Handles decimal scaling between USD_DECIMALS and LP_DECIMALS
    let price_usd = math::checked_decimal_div(
        aum_usd,
        -(Perpetuals::USD_DECIMALS as i32),
        lp_supply,
        -(Perpetuals::LP_DECIMALS as i32),
        -(Perpetuals::USD_DECIMALS as i32),
    )?;

    msg!("price_usd: {}", price_usd);

    // Return LP token price in USD (scaled to USD_DECIMALS)
    Ok(price_usd)
}