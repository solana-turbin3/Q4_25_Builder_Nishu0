//! AddLiquidity instruction handler
//! 
//! This instruction allows liquidity providers to deposit tokens into a pool
//! and receive LP (Liquidity Provider) tokens in return. LP tokens represent
//! a share of the pool's assets and can be redeemed later for a proportional
//! share of the pool. Fees are collected on deposits to incentivize the protocol.

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
    solana_program::program_error::ProgramError,
};

/// Accounts required for adding liquidity to a pool
#[derive(Accounts)]
#[instruction(params: AddLiquidityParams)]
pub struct AddLiquidity<'info> {
    /// Owner of the liquidity position (signer)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account from which tokens will be deposited
    /// Must be owned by owner and have the same mint as the custody
    #[account(
        mut,
        constraint = funding_account.mint == custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// User's LP token account where LP tokens will be minted
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

    /// Custody account for the token being deposited (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the token being deposited
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    /// Pool's token account where deposited tokens will be stored
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    /// LP token mint for this pool (mutable, will mint new LP tokens)
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

/// Parameters for adding liquidity to a pool
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddLiquidityParams {
    /// Amount of tokens to deposit (in token's native decimals)
    pub amount_in: u64,
    /// Minimum LP tokens expected (slippage protection, in LP token decimals)
    pub min_lp_amount_out: u64,
}

/// Add liquidity to a pool and receive LP tokens
/// 
/// This function allows users to deposit tokens into a pool and receive LP tokens
/// representing their share of the pool. The process:
/// 1. Validates permissions and inputs
/// 2. Calculates fees (protocol fee + liquidity fee)
/// 3. Validates token ratios remain within acceptable range
/// 4. Transfers tokens from user to pool
/// 5. Calculates LP tokens to mint based on current pool value
/// 6. Mints LP tokens to user
/// 7. Updates custody and pool statistics
/// 
/// LP tokens are calculated proportionally: lp_amount = (token_amount_usd * lp_supply) / pool_aum_usd
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including deposit amount and minimum LP tokens expected
/// 
/// # Returns
/// `Result<()>` - Success if liquidity was added successfully
pub fn add_liquidity(ctx: Context<AddLiquidity>, params: &AddLiquidityParams) -> Result<()> {
    // Check permissions
    // Both perpetuals and custody must allow adding liquidity, and custody must not be virtual
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    require!(
        perpetuals.permissions.allow_add_liquidity
            && custody.permissions.allow_add_liquidity
            && !custody.is_virtual,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    msg!("Validate inputs");
    if params.amount_in == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&custody.key())?;

    // Get current time for calculations
    let curtime = perpetuals.get_time()?;

    // Refresh pool AUM using EMA mode to adapt to token price changes
    // This ensures accurate fee calculations based on current pool value
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

    // Use minimum price (spot or EMA) for conservative LP token calculation
    let min_price = if token_price < token_ema_price {
        token_price
    } else {
        token_ema_price
    };

    // Calculate liquidity fee (fee charged for adding liquidity)
    let fee_amount =
        pool.get_add_liquidity_fee(token_id, params.amount_in, custody, &token_ema_price)?;
    msg!("Collected fee: {}", fee_amount);

    // Check pool constraints
    // Ensure token ratios remain within acceptable range after deposit
    msg!("Check pool constraints");
    let protocol_fee = Pool::get_fee_amount(custody.fees.protocol_share, fee_amount)?;
    let deposit_amount = math::checked_sub(params.amount_in, protocol_fee)?;
    require!(
        pool.check_token_ratio(token_id, deposit_amount, 0, custody, &token_ema_price)?,
        PerpetualsError::TokenRatioOutOfRange
    );

    // Transfer tokens from user's funding account to pool's custody account
    msg!("Transfer tokens");
    perpetuals.transfer_tokens_from_user(
        ctx.accounts.funding_account.to_account_info(),
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.amount_in,
    )?;

    // Compute total assets under management using Max mode
    // This gives the maximum pool value for LP token calculation
    msg!("Compute assets under management");
    let pool_amount_usd =
        pool.get_assets_under_management_usd(AumCalcMode::Max, ctx.remaining_accounts, curtime)?;

    // Calculate amount of LP tokens to mint
    // Formula: lp_amount = (token_amount_usd * lp_supply) / pool_aum_usd
    // If pool is empty (first deposit), LP amount equals token amount in USD
    let no_fee_amount = math::checked_sub(params.amount_in, fee_amount)?;
    require_gte!(
        no_fee_amount,
        1u64,
        PerpetualsError::InsufficientAmountReturned
    );

    // Convert token amount (after fees) to USD using minimum price
    let token_amount_usd = min_price.get_asset_amount_usd(no_fee_amount, custody.decimals)?;

    // Calculate LP tokens proportionally based on pool value
    let lp_amount = if pool_amount_usd == 0 {
        // First deposit: LP tokens equal token value in USD
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
    msg!("LP tokens to mint: {}", lp_amount);

    // Validate slippage protection
    // Ensure user receives at least the minimum expected LP tokens
    require!(
        lp_amount >= params.min_lp_amount_out,
        PerpetualsError::MaxPriceSlippage
    );

    // Mint LP tokens to user's LP token account
    perpetuals.mint_tokens(
        ctx.accounts.lp_token_mint.to_account_info(),
        ctx.accounts.lp_token_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lp_amount,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Track collected fees in USD
    custody.collected_fees.add_liquidity_usd = custody
        .collected_fees
        .add_liquidity_usd
        .wrapping_add(token_ema_price.get_asset_amount_usd(fee_amount, custody.decimals)?);

    // Track volume statistics in USD
    custody.volume_stats.add_liquidity_usd = custody
        .volume_stats
        .add_liquidity_usd
        .wrapping_add(token_ema_price.get_asset_amount_usd(params.amount_in, custody.decimals)?);

    // Update protocol fees (portion of liquidity fee that goes to protocol)
    custody.assets.protocol_fees = math::checked_add(custody.assets.protocol_fees, protocol_fee)?;

    // Update owned assets (tokens owned by the pool after deposit)
    custody.assets.owned = math::checked_add(custody.assets.owned, deposit_amount)?;

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