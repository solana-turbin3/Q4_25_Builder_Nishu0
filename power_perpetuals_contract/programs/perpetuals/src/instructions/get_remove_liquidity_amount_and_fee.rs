//! GetRemoveLiquidityAmountAndFee instruction handler
//! 
//! This is a view/query instruction that calculates the amount of tokens and fees
//! that would be returned when removing liquidity from a pool. It allows users to
//! preview the transaction before executing it, helping them understand the costs
//! and expected returns.

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
};

/// Accounts required for querying remove liquidity amount and fee
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates tokens and fees that would be returned if liquidity were removed.
#[derive(Accounts)]
pub struct GetRemoveLiquidityAmountAndFee<'info> {
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

    /// Custody account for the token being withdrawn (read-only)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the token being withdrawn
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    /// LP token mint for this pool (read-only, to get supply)
    #[account(
        seeds = [b"lp_token_mint",
                 pool.key().as_ref()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,
}

/// Parameters for querying remove liquidity amount and fee
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetRemoveLiquidityAmountAndFeeParams {
    lp_amount_in: u64,
}

/// Calculate remove liquidity amount and fee (view function)
/// 
/// This function simulates removing liquidity without actually executing the transaction.
/// It calculates:
/// 1. The USD value of LP tokens being redeemed (proportional to pool AUM)
/// 2. The amount of tokens that would be returned (using maximum price for conservative estimate)
/// 3. The fee that would be charged for removing liquidity
/// 
/// Formula: remove_amount_usd = (pool_aum_usd * lp_amount_in) / lp_supply
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including LP token amount to redeem
/// 
/// # Returns
/// `Result<AmountAndFee>` - Struct containing:
/// - `amount`: Tokens that would be returned after fees (in token decimals)
/// - `fee`: Fee amount that would be charged (in token decimals)
pub fn get_remove_liquidity_amount_and_fee<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetRemoveLiquidityAmountAndFee<'info>>,
    params: &GetRemoveLiquidityAmountAndFeeParams,
) -> Result<AmountAndFee> {
    // Validate inputs
    if params.lp_amount_in == 0 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    let pool = &ctx.accounts.pool;
    let custody = &ctx.accounts.custody;
    let token_id = pool.get_token_id(&custody.key())?;

    // Get current time for calculations
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

    // Calculate pool AUM using Min mode (conservative estimate)
    let pool_amount_usd =
        pool.get_assets_under_management_usd(AumCalcMode::Min, ctx.remaining_accounts, curtime)?;

    // Calculate USD value of LP tokens being redeemed
    // Formula: remove_amount_usd = (pool_aum_usd * lp_amount_in) / lp_supply
    let remove_amount_usd = math::checked_as_u64(math::checked_div(
        math::checked_mul(pool_amount_usd, params.lp_amount_in as u128)?,
        ctx.accounts.lp_token_mint.supply as u128,
    )?)?;

    // Use maximum price (spot or EMA) for conservative token amount calculation
    // This ensures users get a conservative estimate of tokens they'll receive
    let max_price = if token_price > token_ema_price {
        token_price
    } else {
        token_ema_price
    };
    // Convert USD amount to token amount using maximum price
    let remove_amount = max_price.get_token_amount(remove_amount_usd, custody.decimals)?;

    // Calculate remove liquidity fee
    let fee_amount =
        pool.get_remove_liquidity_fee(token_id, remove_amount, custody, &token_price)?;

    // Calculate amount to transfer after deducting fee
    let transfer_amount = math::checked_sub(remove_amount, fee_amount)?;

    // Return calculated amount and fee
    Ok(AmountAndFee {
        amount: transfer_amount,
        fee: fee_amount,
    })
}