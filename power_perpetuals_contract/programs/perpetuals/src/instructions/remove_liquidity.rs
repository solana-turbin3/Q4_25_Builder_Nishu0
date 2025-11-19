//! RemoveLiquidity instruction handler
//! 
//! This instruction allows liquidity providers to redeem LP tokens and withdraw
//! their share of the pool's assets. LP tokens are burned, and tokens are returned
//! to the user after deducting fees. The withdrawal must maintain acceptable token
//! ratios in the pool.

use {
    crate::{
        error::PerpetualsError,
        math,
        state::{
            custody::Custody,
            oracle::OraclePrice,
            perpetuals::Perpetuals,
            pool::{AumCalcMode, Pool},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Mint, Token, TokenAccount},
};

/// Accounts required for removing liquidity from a pool
#[derive(Accounts)]
#[instruction(params: RemoveLiquidityParams)]
pub struct RemoveLiquidity<'info> {
    /// Owner of the liquidity position (signer)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account where tokens will be returned
    /// Must be owned by owner and have the same mint as the custody
    #[account(
        mut,
        constraint = receiving_account.mint == custody.mint,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

    /// User's LP token account from which LP tokens will be burned
    /// Must be owned by owner and have the LP token mint
    #[account(
        mut,
        constraint = lp_token_account.mint == lp_token_mint.key(),
        has_one = owner
    )]
    pub lp_token_account: Box<Account<'info, TokenAccount>>,

    /// Transfer authority PDA for token transfers
    /// 
    /// CHECK: Empty PDA, authority for token accounts
    #[account(
        seeds = [b"transfer_authority"],
        bump = perpetuals.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    /// Main perpetuals program account
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account for the token being withdrawn (mutable, stats will be updated)
    #[account(
        mut,
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

    /// Pool's token account where tokens are stored (mutable, tokens will be transferred out)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    /// LP token mint for this pool (mutable, will burn LP tokens)
    #[account(
        mut,
        seeds = [b"lp_token_mint",
                 pool.key().as_ref()],
        bump = pool.lp_token_bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,

    token_program: Program<'info, Token>,
    // remaining accounts:
    //   pool.tokens.len() custody accounts (read-only, unsigned)
    //   pool.tokens.len() custody oracles (read-only, unsigned)
}

/// Parameters for removing liquidity from a pool
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemoveLiquidityParams {
    /// Amount of LP tokens to redeem (in LP token decimals)
    pub lp_amount_in: u64,
    /// Minimum tokens expected (slippage protection, in token decimals)
    pub min_amount_out: u64,
}

/// Remove liquidity from a pool and burn LP tokens
/// 
/// This function allows users to redeem LP tokens and withdraw their proportional
/// share of the pool's assets. The process:
/// 1. Validates permissions and inputs
/// 2. Calculates AUM and token amount to return (proportional to LP tokens)
/// 3. Calculates remove liquidity fee
/// 4. Validates slippage protection
/// 5. Validates token ratios remain within acceptable range
/// 6. Validates pool has sufficient available funds
/// 7. Transfers tokens from pool to user
/// 8. Burns LP tokens
/// 9. Updates custody and pool statistics
/// 
/// Formula: remove_amount_usd = (pool_aum_usd * lp_amount_in) / lp_supply
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including LP token amount and minimum tokens expected
/// 
/// # Returns
/// `Result<()>` - Success if liquidity was removed successfully
pub fn remove_liquidity(
    ctx: Context<RemoveLiquidity>,
    params: &RemoveLiquidityParams,
) -> Result<()> {
    // Check permissions
    // Both perpetuals and custody must allow removing liquidity, and custody must not be virtual
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    require!(
        perpetuals.permissions.allow_remove_liquidity
            && custody.permissions.allow_remove_liquidity
            && !custody.is_virtual,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    msg!("Validate inputs");
    if params.lp_amount_in == 0 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&custody.key())?;

    // compute assets under management
    msg!("Compute assets under management");
    let curtime = perpetuals.get_time()?;

    // Refresh pool AUM using EMA mode to adapt to token price changes
    // This ensures accurate fee calculations based on current pool value
    msg!("Compute assets under management");
    pool.aum_usd =
        pool.get_assets_under_management_usd(AumCalcMode::EMA, ctx.remaining_accounts, curtime)?;

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

    // Use maximum price (spot or EMA) for conservative token amount calculation
    // This ensures users get a conservative estimate of tokens they'll receive
    let max_price = if token_price > token_ema_price {
        token_price
    } else {
        token_ema_price
    };

    // Calculate pool AUM using Min mode (conservative estimate)
    let pool_amount_usd =
        pool.get_assets_under_management_usd(AumCalcMode::Min, ctx.remaining_accounts, curtime)?;

    // Calculate USD value of LP tokens being redeemed
    // Formula: remove_amount_usd = (pool_aum_usd * lp_amount_in) / lp_supply
    let remove_amount_usd = math::checked_as_u64(math::checked_div(
        math::checked_mul(pool_amount_usd, params.lp_amount_in as u128)?,
        ctx.accounts.lp_token_mint.supply as u128,
    )?)?;

    // Convert USD amount to token amount using maximum price
    let remove_amount = max_price.get_token_amount(remove_amount_usd, custody.decimals)?;

    // Calculate remove liquidity fee
    let fee_amount =
        pool.get_remove_liquidity_fee(token_id, remove_amount, custody, &token_ema_price)?;
    msg!("Collected fee: {}", fee_amount);

    // Calculate amount to transfer after deducting fee
    let transfer_amount = math::checked_sub(remove_amount, fee_amount)?;
    msg!("Amount out: {}", transfer_amount);

    // Validate slippage protection
    // Ensure user receives at least the minimum expected tokens
    require!(
        transfer_amount >= params.min_amount_out,
        PerpetualsError::MaxPriceSlippage
    );

    // Check pool constraints
    msg!("Check pool constraints");
    // Calculate protocol fee (portion of liquidity fee that goes to protocol)
    let protocol_fee = Pool::get_fee_amount(custody.fees.protocol_share, fee_amount)?;
    // Total withdrawal amount includes both user amount and protocol fee
    let withdrawal_amount = math::checked_add(transfer_amount, protocol_fee)?;
    // Ensure token ratios remain within acceptable range after withdrawal
    require!(
        pool.check_token_ratio(token_id, 0, withdrawal_amount, custody, &token_ema_price)?,
        PerpetualsError::TokenRatioOutOfRange
    );

    // Ensure pool has sufficient available funds (owned - locked >= withdrawal_amount)
    require!(
        math::checked_sub(custody.assets.owned, custody.assets.locked)? >= withdrawal_amount,
        PerpetualsError::CustodyAmountLimit
    );

    // Transfer tokens from pool's custody account to user's receiving account
    msg!("Transfer tokens");
    perpetuals.transfer_tokens(
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        transfer_amount,
    )?;

    // Burn LP tokens from user's LP token account
    msg!("Burn LP tokens");
    perpetuals.burn_tokens(
        ctx.accounts.lp_token_mint.to_account_info(),
        ctx.accounts.lp_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.lp_amount_in,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Track collected fees in USD
    custody.collected_fees.remove_liquidity_usd = custody
        .collected_fees
        .remove_liquidity_usd
        .wrapping_add(token_ema_price.get_asset_amount_usd(fee_amount, custody.decimals)?);

    // Track volume statistics in USD
    custody.volume_stats.remove_liquidity_usd = custody
        .volume_stats
        .remove_liquidity_usd
        .wrapping_add(remove_amount_usd);

    // Update protocol fees (portion of liquidity fee that goes to protocol)
    custody.assets.protocol_fees = math::checked_add(custody.assets.protocol_fees, protocol_fee)?;

    // Update owned assets (tokens owned by the pool after withdrawal)
    custody.assets.owned = math::checked_sub(custody.assets.owned, withdrawal_amount)?;

    // Update borrow rate based on new utilization
    custody.update_borrow_rate(curtime)?;

    // Update pool statistics
    msg!("Update pool stats");
    // Exit custody account (release borrow from Anchor's account context)
    custody.exit(&crate::ID)?;
    // Refresh pool AUM using EMA mode for accurate tracking
    pool.aum_usd =
        pool.get_assets_under_management_usd(AumCalcMode::EMA, ctx.remaining_accounts, curtime)?;

    Ok(())
}