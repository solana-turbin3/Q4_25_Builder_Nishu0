//! GetEntryPriceAndFee instruction handler
//! 
//! This is a view/query instruction that calculates entry price, liquidation price,
//! and fees for opening a new position. It allows users to preview the transaction
//! before executing it, helping them understand the costs and risks.

use {
    crate::state::{
        custody::Custody,
        oracle::OraclePrice,
        perpetuals::{NewPositionPricesAndFee, Perpetuals},
        pool::Pool,
        position::{Position, Side},
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying entry price and fee
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates prices and fees that would apply if a position were opened.
#[derive(Accounts)]
pub struct GetEntryPriceAndFee<'info> {
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

/// Parameters for querying entry price and fee
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetEntryPriceAndFeeParams {
    collateral: u64,
    size: u64,
    side: Side,
}

/// Calculate entry price, liquidation price, and fee for opening a position (view function)
/// 
/// This function simulates opening a position without actually executing the transaction.
/// It calculates:
/// 1. Entry price (with spread applied)
/// 2. Liquidation price (price at which position would be liquidated)
/// 3. Entry fee (with utilization-based adjustments)
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including collateral, size, and side
/// 
/// # Returns
/// `NewPositionPricesAndFee` struct containing:
/// - `entry_price`: Price at which position would be opened (scaled to PRICE_DECIMALS)
/// - `liquidation_price`: Price threshold for liquidation (scaled to PRICE_DECIMALS)
/// - `fee`: Fee amount that would be charged (in position token decimals, or collateral if short/virtual)
pub fn get_entry_price_and_fee(
    ctx: Context<GetEntryPriceAndFee>,
    params: &GetEntryPriceAndFeeParams,
) -> Result<NewPositionPricesAndFee> {
    // Validate inputs
    if params.collateral == 0 || params.size == 0 || params.side == Side::None {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    let pool = &ctx.accounts.pool;
    let custody = &ctx.accounts.custody;
    let collateral_custody = &ctx.accounts.collateral_custody;

    // Get current time for calculations
    let curtime = ctx.accounts.perpetuals.get_time()?;

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

    // Calculate entry price (applies spread based on position side)
    let entry_price = pool.get_entry_price(&token_price, &token_ema_price, params.side, custody)?;

    // Convert entry price to OraclePrice format for calculations
    let position_oracle_price = OraclePrice {
        price: entry_price,
        exponent: -(Perpetuals::PRICE_DECIMALS as i32),
    };
    
    // Calculate position size and collateral in USD
    let size_usd = position_oracle_price.get_asset_amount_usd(params.size, custody.decimals)?;
    let collateral_usd = min_collateral_price
        .get_asset_amount_usd(params.collateral, collateral_custody.decimals)?;

    // Calculate locked amount (tokens that would be locked for this position)
    // For shorts or virtual custodies, convert size_usd to collateral tokens first
    let locked_amount = if params.side == Side::Short || custody.is_virtual {
        custody.get_locked_amount(
            min_collateral_price.get_token_amount(size_usd, collateral_custody.decimals)?,
            params.side,
        )?
    } else {
        custody.get_locked_amount(params.size, params.side)?
    };

    // Create temporary position struct for liquidation price calculation
    let position = Position {
        side: params.side,
        price: entry_price,
        size_usd,
        collateral_usd,
        cumulative_interest_snapshot: collateral_custody.get_cumulative_interest(curtime)?,
        ..Position::default()
    };

    // Calculate liquidation price (price at which position would be liquidated)
    let liquidation_price = pool.get_liquidation_price(
        &position,
        &token_ema_price,
        custody,
        collateral_custody,
        curtime,
    )?;

    // Calculate entry fee (includes utilization-based adjustments)
    let mut fee = pool.get_entry_fee(
        custody.fees.open_position,
        params.size,
        locked_amount,
        collateral_custody,
    )?;

    // Convert fee to collateral token if needed
    // For shorts or virtual custodies, fee is calculated in position token, convert to collateral
    if params.side == Side::Short || custody.is_virtual {
        let fee_amount_usd = token_ema_price.get_asset_amount_usd(fee, custody.decimals)?;
        fee = collateral_token_ema_price
            .get_token_amount(fee_amount_usd, collateral_custody.decimals)?;
    }

    // Return calculated prices and fee
    Ok(NewPositionPricesAndFee {
        entry_price,
        liquidation_price,
        fee,
    })
}