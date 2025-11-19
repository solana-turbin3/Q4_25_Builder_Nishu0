//! GetLiquidationState instruction handler
//! 
//! This is a view/query instruction that checks whether a position is currently
//! at risk of liquidation. It validates the position's leverage against the pool's
//! maximum leverage requirements. Returns 0 if position is safe, 1 if at risk.

use {
    crate::state::{
        custody::Custody, oracle::OraclePrice, perpetuals::Perpetuals, pool::Pool,
        position::Position,
    },
    anchor_lang::prelude::*,
};

/// Accounts required for querying liquidation state
/// 
/// This instruction is read-only and doesn't modify any state.
/// It checks whether a position currently meets leverage requirements.
#[derive(Accounts)]
pub struct GetLiquidationState<'info> {
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

/// Parameters for querying liquidation state
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetLiquidationStateParams {}

/// Check liquidation state of a position (view function)
/// 
/// This function checks whether a position currently meets the pool's leverage
/// requirements. It validates the position's current leverage against maximum
/// allowed leverage based on current prices.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<u8>` - 0 if position is safe (leverage within limits), 1 if at risk (exceeds limits)
pub fn get_liquidation_state(
    ctx: Context<GetLiquidationState>,
    _params: &GetLiquidationStateParams,
) -> Result<u8> {
    // Get account references
    let custody = &ctx.accounts.custody;
    let collateral_custody = &ctx.accounts.collateral_custody;
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

    // Check if position leverage is within acceptable limits
    // Returns true if position is safe, false if it exceeds maximum leverage
    if ctx.accounts.pool.check_leverage(
        &ctx.accounts.position,
        &token_price,
        &token_ema_price,
        custody,
        &collateral_token_price,
        &collateral_token_ema_price,
        collateral_custody,
        curtime,
        false,
    )? {
        // Position is safe (leverage within limits)
        Ok(0)
    } else {
        // Position is at risk (leverage exceeds maximum allowed)
        Ok(1)
    }
}