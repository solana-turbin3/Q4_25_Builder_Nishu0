//! GetExitPriceAndFee instruction handler
//! 
//! This is a view/query instruction that calculates exit price and fees
//! for closing an existing position. It allows users to preview the transaction
//! before executing it, helping them understand the costs and expected returns.

use {
    crate::state::{
        custody::Custody,
        oracle::OraclePrice,
        perpetuals::{Perpetuals, PriceAndFee},
        pool::Pool,
        position::{Position, Side},
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying exit price and fee
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates prices and fees that would apply if a position were closed.
#[derive(Accounts)]
pub struct GetExitPriceAndFee<'info> {
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

    /// Position account to query (read-only)
    #[account(
        seeds = [b"position",
                 position.owner.as_ref(),
                 pool.key().as_ref(),
                 custody.key().as_ref(),
                 &[position.side as u8]],
        bump = position.bump
    )]
    pub position: Box<Account<'info, Position>>,

    /// Custody account for the position token (read-only)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the position token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    /// Custody account for the collateral token (read-only)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 collateral_custody.mint.as_ref()],
        bump = collateral_custody.bump
    )]
    pub collateral_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the collateral token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = collateral_custody_oracle_account.key() == collateral_custody.oracle.oracle_account
    )]
    pub collateral_custody_oracle_account: AccountInfo<'info>,
}

/// Parameters for querying exit price and fee
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetExitPriceAndFeeParams {}

/// Calculate exit price and fee for closing a position (view function)
/// 
/// This function simulates closing a position without actually executing the transaction.
/// It calculates:
/// 1. Exit price (with spread applied based on position side)
/// 2. Exit fee (fee charged for closing the position)
/// 
/// For shorts or virtual custodies, the fee is converted from position token to collateral token.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// `PriceAndFee` struct containing:
/// - `price`: Exit price at which position would be closed (scaled to PRICE_DECIMALS)
/// - `fee`: Exit fee amount that would be charged (in position token decimals, or collateral if short/virtual)
pub fn get_exit_price_and_fee(
    ctx: Context<GetExitPriceAndFee>,
    _params: &GetExitPriceAndFeeParams,
) -> Result<PriceAndFee> {
    // Get account references
    let position = &ctx.accounts.position;
    let pool = &ctx.accounts.pool;
    let curtime = ctx.accounts.perpetuals.get_time()?;
    let custody = &ctx.accounts.custody;
    let collateral_custody = &ctx.accounts.collateral_custody;

    // Get position token prices from oracle (spot and EMA)
    let token_price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        &custody.oracle,
        curtime,
        false,
    )?;

    let token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        &custody.oracle,
        curtime,
        custody.pricing.use_ema,
    )?;

    // Get collateral token EMA price (needed for fee conversion)
    let collateral_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .collateral_custody_oracle_account
            .to_account_info(),
        &collateral_custody.oracle,
        curtime,
        collateral_custody.pricing.use_ema,
    )?;

    // Calculate exit price (applies spread based on position side)
    // For longs: uses short spread (minimum price)
    // For shorts: uses long spread (maximum price)
    let price = pool.get_exit_price(&token_price, &token_ema_price, position.side, custody)?;

    // Calculate position size in tokens for fee calculation
    let size = token_ema_price.get_token_amount(position.size_usd, custody.decimals)?;

    // Calculate exit fee (initially in position token decimals)
    let mut fee = pool.get_exit_fee(size, custody)?;

    // Convert fee to collateral token if needed
    // For shorts or virtual custodies, fee is calculated in position token, convert to collateral
    if position.side == Side::Short || custody.is_virtual {
        let fee_amount_usd = token_ema_price.get_asset_amount_usd(fee, custody.decimals)?;
        fee = collateral_token_ema_price
            .get_token_amount(fee_amount_usd, collateral_custody.decimals)?;
    }
    
    // Return calculated exit price and fee
    Ok(PriceAndFee { price, fee })
}
