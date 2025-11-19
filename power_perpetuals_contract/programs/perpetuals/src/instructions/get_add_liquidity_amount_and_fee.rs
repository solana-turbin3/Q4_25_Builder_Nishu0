//! GetAddLiquidityAmountAndFee instruction handler
//! 
//! This is a view/query instruction that calculates the amount of LP tokens
//! and fees that would be received for adding a given amount of liquidity.
//! This allows users to preview the transaction before executing it.

use {
    crate::{
        math,
        state::{
            custody::Custody,
            oracle::OraclePrice,
            perpetuals::{AmountAndFee, Perpetuals},
            pool::{AumCalcMode, Pool},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::Mint,
    solana_program::program_error::ProgramError,
};

/// Accounts required for querying add liquidity amount and fee
/// 
/// This instruction is read-only and doesn't modify any state.
/// It only calculates and returns the expected LP tokens and fees.
#[derive(Accounts)]
pub struct GetAddLiquidityAmountAndFee<'info> {
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

    /// Custody account for the token being deposited (read-only)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the custody token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    /// LP token mint for the pool (read-only, to get current supply)
    #[account(
        seeds = [b"lp_token_mint",
                 pool.key().as_ref()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,
    
    // Remaining accounts (read-only, unsigned):
    //   - pool.custodies.len() custody accounts (for AUM calculation)
    //   - pool.custodies.len() custody oracle accounts (for price feeds)
}

/// Parameters for querying add liquidity amount and fee
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetAddLiquidityAmountAndFeeParams {
    amount_in: u64,
}

/// Calculate LP tokens and fees for adding liquidity (view function)
/// 
/// This function simulates adding liquidity without actually executing the transaction.
/// It calculates:
/// 1. The fee that would be charged
/// 2. The amount of LP tokens that would be minted
/// 
/// LP token calculation:
/// - If pool is empty: LP tokens = token_amount_usd
/// - Otherwise: LP tokens = (token_amount_usd * lp_supply) / pool_aum_usd
/// 
/// Uses Max AUM mode to ensure user gets fair share based on maximum pool value.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including deposit amount
/// 
/// # Returns
/// `AmountAndFee` struct containing:
/// - `amount`: LP tokens that would be minted (in LP token decimals)
/// - `fee`: Fee amount that would be charged (in token's native decimals)
pub fn get_add_liquidity_amount_and_fee(
    ctx: Context<GetAddLiquidityAmountAndFee>,
    params: &GetAddLiquidityAmountAndFeeParams,
) -> Result<AmountAndFee> {
    // Validate inputs
    if params.amount_in == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }
    let pool = &ctx.accounts.pool;
    let custody = &ctx.accounts.custody;
    let token_id = pool.get_token_id(&custody.key())?;

    // Get current time for price calculations
    let curtime = ctx.accounts.perpetuals.get_time()?;

    // Get token prices from oracle (spot and EMA)
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

    // Calculate fee that would be charged
    let fee_amount =
        pool.get_add_liquidity_fee(token_id, params.amount_in, custody, &token_price)?;
    
    // Calculate amount after fee deduction
    let no_fee_amount = math::checked_sub(params.amount_in, fee_amount)?;

    // Calculate pool AUM using Max mode (ensures fair LP token calculation)
    let pool_amount_usd =
        pool.get_assets_under_management_usd(AumCalcMode::Max, ctx.remaining_accounts, curtime)?;

    // Use minimum price for conservative LP token calculation
    let min_price = if token_price < token_ema_price {
        token_price
    } else {
        token_ema_price
    };
    
    // Convert token amount (after fee) to USD value
    let token_amount_usd = min_price.get_asset_amount_usd(no_fee_amount, custody.decimals)?;

    // Calculate LP tokens to mint
    // Formula: LP_tokens = (token_amount_usd * lp_supply) / pool_aum_usd
    let lp_amount = if pool_amount_usd == 0 {
        // First liquidity provider: LP tokens = token value in USD
        token_amount_usd
    } else {
        // Subsequent deposits: proportional to existing LP supply
        math::checked_as_u64(math::checked_div(
            math::checked_mul(
                token_amount_usd as u128,
                ctx.accounts.lp_token_mint.supply as u128,
            )?,
            pool_amount_usd,
        )?)?
    };

    // Return calculated amounts
    Ok(AmountAndFee {
        amount: lp_amount,
        fee: fee_amount,
    })
}