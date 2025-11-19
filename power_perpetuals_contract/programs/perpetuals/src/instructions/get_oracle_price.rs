//! GetOraclePrice instruction handler
//! 
//! This is a view/query instruction that retrieves the current price from an oracle
//! for a specific custody token. It can return either the spot price or the EMA
//! (Exponential Moving Average) price based on the parameters.

use {
    crate::state::{custody::Custody, oracle::OraclePrice, perpetuals::Perpetuals, pool::Pool},
    anchor_lang::prelude::*,
};

/// Accounts required for querying oracle price
/// 
/// This instruction is read-only and doesn't modify any state.
/// It retrieves the current price from the oracle for a specific custody token.
#[derive(Accounts)]
pub struct GetOraclePrice<'info> {
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

    /// Custody account for the token (read-only)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,
}

/// Parameters for querying oracle price
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetOraclePriceParams {
    ema: bool,
}

/// Get oracle price for a custody token (view function)
/// 
/// This function retrieves the current price from the oracle for a specific token.
/// It can return either the spot price or the EMA price based on the parameters.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including whether to use EMA price
/// 
/// # Returns
/// `Result<u64>` - Price scaled to PRICE_DECIMALS, or error
pub fn get_oracle_price(
    ctx: Context<GetOraclePrice>,
    params: &GetOraclePriceParams,
) -> Result<u64> {
    // Get account references
    let custody = &ctx.accounts.custody;
    let curtime = ctx.accounts.perpetuals.get_time()?;

    // Get price from oracle (spot or EMA based on params.ema)
    let price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        &custody.oracle,
        curtime,
        params.ema,
    )?;

    // Scale price to PRICE_DECIMALS and return
    Ok(price
        .scale_to_exponent(-(Perpetuals::PRICE_DECIMALS as i32))?
        .price)
}