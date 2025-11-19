//! GetLiquidationPrice instruction handler
//! 
//! This is a view/query instruction that calculates the liquidation price for a position,
//! optionally accounting for hypothetical collateral changes (add or remove).
//! It allows users to preview what the liquidation price would be after modifying
//! their position's collateral without actually executing the transaction.

use {
    crate::{
        math,
        state::{
            custody::Custody, oracle::OraclePrice, perpetuals::Perpetuals, pool::Pool,
            position::Position,
        },
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying liquidation price
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates the liquidation price that would apply after hypothetical collateral changes.
#[derive(Accounts)]
pub struct GetLiquidationPrice<'info> {
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
        constraint = position.collateral_custody == collateral_custody.key()
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

/// Parameters for querying liquidation price
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetLiquidationPriceParams {
    add_collateral: u64,
    remove_collateral: u64,
}

/// Calculate liquidation price for a position (view function)
/// 
/// This function simulates calculating the liquidation price after hypothetical
/// collateral changes without actually executing the transaction. It:
/// 1. Gets current prices from oracles
/// 2. Creates a temporary position copy
/// 3. Applies hypothetical collateral changes (add or remove)
/// 4. Calculates what the liquidation price would be after those changes
/// 
/// Liquidation price is the price at which the position would be liquidated
/// due to insufficient margin/collateral.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including hypothetical collateral changes
/// 
/// # Returns
/// `Result<u64>` - Liquidation price (scaled to PRICE_DECIMALS), or error
pub fn get_liquidation_price(
    ctx: Context<GetLiquidationPrice>,
    params: &GetLiquidationPriceParams,
) -> Result<u64> {
    // Get account references
    let custody = &ctx.accounts.custody;
    let collateral_custody = &ctx.accounts.collateral_custody;
    let curtime = ctx.accounts.perpetuals.get_time()?;

    // Get position token EMA price (used for liquidation calculations)
    let token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts.custody_oracle_account.to_account_info(),
        &custody.oracle,
        curtime,
        custody.pricing.use_ema,
    )?;

    // Get collateral token prices from oracle (spot and EMA)
    let collateral_token_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .collateral_custody_oracle_account
            .to_account_info(),
        &collateral_custody.oracle,
        curtime,
        false,
    )?;

    let collateral_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .collateral_custody_oracle_account
            .to_account_info(),
        &collateral_custody.oracle,
        curtime,
        collateral_custody.pricing.use_ema,
    )?;

    // Use minimum collateral price for conservative valuation
    // For stablecoins, caps price at 1 USD
    let min_collateral_price = collateral_token_price
        .get_min_price(&collateral_token_ema_price, collateral_custody.is_stable)?;

    // Create a temporary copy of the position to simulate changes
    let mut position = ctx.accounts.position.clone();
    position.update_time = ctx.accounts.perpetuals.get_time()?;

    // Apply hypothetical collateral addition if specified
    if params.add_collateral > 0 {
        let collateral_usd = min_collateral_price
            .get_asset_amount_usd(params.add_collateral, collateral_custody.decimals)?;
        position.collateral_usd = math::checked_add(position.collateral_usd, collateral_usd)?;
        position.collateral_amount =
            math::checked_add(position.collateral_amount, params.add_collateral)?;
    }
    
    // Apply hypothetical collateral removal if specified
    if params.remove_collateral > 0 {
        let collateral_usd = min_collateral_price
            .get_asset_amount_usd(params.remove_collateral, collateral_custody.decimals)?;
        // Validate that removal doesn't exceed available collateral
        if collateral_usd >= position.collateral_usd
            || params.remove_collateral >= position.collateral_amount
        {
            return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
        }
        position.collateral_usd = math::checked_sub(position.collateral_usd, collateral_usd)?;
        position.collateral_amount =
            math::checked_sub(position.collateral_amount, params.remove_collateral)?;
    }

    // Calculate and return liquidation price for the modified position
    ctx.accounts.pool.get_liquidation_price(
        &position,
        &token_ema_price,
        custody,
        collateral_custody,
        curtime,
    )
}