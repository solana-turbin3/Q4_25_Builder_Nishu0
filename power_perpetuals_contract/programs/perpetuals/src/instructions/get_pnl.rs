//! GetPnl instruction handler
//! 
//! This is a view/query instruction that calculates the current profit and loss (PnL)
//! for an existing position. It computes unrealized PnL based on current market prices
//! compared to the position's entry price, without actually closing the position.

use {
    crate::state::{
        custody::Custody,
        oracle::OraclePrice,
        perpetuals::{Perpetuals, ProfitAndLoss},
        pool::Pool,
        position::Position,
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying profit and loss
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates unrealized PnL for an existing position.
#[derive(Accounts)]
pub struct GetPnl<'info> {
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

/// Parameters for querying profit and loss
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetPnlParams {}

/// Calculate profit and loss for a position (view function)
/// 
/// This function calculates the unrealized profit and loss for an existing position
/// based on current market prices compared to the position's entry price. It does
/// not close the position, only computes what the PnL would be if closed now.
/// 
/// PnL is calculated as:
/// - Profit: Positive difference between current exit price and entry price (for longs)
///            or between entry price and current exit price (for shorts)
/// - Loss: Negative difference, including fees and interest
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<ProfitAndLoss>` - Struct containing profit and loss amounts in USD, or error
pub fn get_pnl(ctx: Context<GetPnl>, _params: &GetPnlParams) -> Result<ProfitAndLoss> {
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

    // Compute profit and loss in USD
    // Returns (profit_usd, loss_usd, fee_amount)
    // We ignore fee_amount here as we only need profit/loss
    let (profit, loss, _) = pool.get_pnl_usd(
        position,
        &token_price,
        &token_ema_price,
        custody,
        &collateral_token_price,
        &collateral_token_ema_price,
        collateral_custody,
        curtime,
        false, // Not a liquidation
    )?;

    // Return profit and loss
    Ok(ProfitAndLoss { profit, loss })
}