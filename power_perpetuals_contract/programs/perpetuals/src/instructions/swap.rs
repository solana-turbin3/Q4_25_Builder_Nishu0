//! Swap instruction handler
//! 
//! This instruction allows users to swap tokens within a pool. Users deposit tokens
//! of one type (receiving custody) and receive tokens of another type (dispensing custody).
//! The swap amount is calculated based on oracle prices, fees are deducted, and token
//! ratios are validated to ensure pool stability.

use {
    crate::{
        error::PerpetualsError,
        math,
        state::{custody::Custody, oracle::OraclePrice, perpetuals::Perpetuals, pool::Pool},
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Token, TokenAccount},
};

/// Accounts required for swapping tokens within a pool
#[derive(Accounts)]
#[instruction(params: SwapParams)]
pub struct Swap<'info> {
    /// Owner of the swap transaction (signer)
    #[account()]
    pub owner: Signer<'info>,

    /// User's token account from which tokens will be deposited
    /// Must be owned by owner and have the same mint as receiving_custody
    #[account(
        mut,
        constraint = funding_account.mint == receiving_custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

    /// User's token account where tokens will be received
    /// Must be owned by owner and have the same mint as dispensing_custody
    #[account(
        mut,
        constraint = receiving_account.mint == dispensing_custody.mint,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

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

    /// Pool account (mutable, stats may be updated)
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
                 receiving_custody.mint.as_ref()],
        bump = receiving_custody.bump
    )]
    pub receiving_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the token being deposited
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = receiving_custody_oracle_account.key() == receiving_custody.oracle.oracle_account
    )]
    pub receiving_custody_oracle_account: AccountInfo<'info>,

    /// Pool's token account where deposited tokens are stored (mutable, tokens will be added)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 receiving_custody.mint.as_ref()],
        bump = receiving_custody.token_account_bump
    )]
    pub receiving_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// Custody account for the token being dispensed (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 dispensing_custody.mint.as_ref()],
        bump = dispensing_custody.bump
    )]
    pub dispensing_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the token being dispensed
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = dispensing_custody_oracle_account.key() == dispensing_custody.oracle.oracle_account
    )]
    pub dispensing_custody_oracle_account: AccountInfo<'info>,

    /// Pool's token account where dispensed tokens are stored (mutable, tokens will be transferred out)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 dispensing_custody.mint.as_ref()],
        bump = dispensing_custody.token_account_bump
    )]
    pub dispensing_custody_token_account: Box<Account<'info, TokenAccount>>,

    token_program: Program<'info, Token>,
}

/// Parameters for swapping tokens
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct SwapParams {
    /// Amount of tokens to deposit (in token decimals)
    pub amount_in: u64,
    /// Minimum tokens expected (slippage protection, in token decimals)
    pub min_amount_out: u64,
}

/// Swap tokens within a pool
/// 
/// This function allows users to swap tokens of one type for tokens of another type within
/// the same pool. The process:
/// 1. Validates permissions and inputs
/// 2. Fetches oracle prices for both tokens (spot and EMA)
/// 3. Calculates swap amount based on prices and pool state
/// 4. Calculates swap fees
/// 5. Validates slippage protection
/// 6. Validates token ratios remain within acceptable range
/// 7. Validates pool has sufficient available funds
/// 8. Transfers tokens (deposit from user, withdrawal to user)
/// 9. Updates custody statistics and borrow rates
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including input amount and minimum output amount
/// 
/// # Returns
/// `Result<()>` - Success if swap was executed successfully
pub fn swap(ctx: Context<Swap>, params: &SwapParams) -> Result<()> {
    // Check permissions
    // All three (perpetuals, receiving_custody, dispensing_custody) must allow swaps
    // Both custodies must not be virtual
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let receiving_custody = ctx.accounts.receiving_custody.as_mut();
    let dispensing_custody = ctx.accounts.dispensing_custody.as_mut();
    require!(
        perpetuals.permissions.allow_swap
            && receiving_custody.permissions.allow_swap
            && dispensing_custody.permissions.allow_swap
            && !receiving_custody.is_virtual
            && !dispensing_custody.is_virtual,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    msg!("Validate inputs");
    if params.amount_in == 0 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    // Ensure receiving and dispensing custodies are different
    require_keys_neq!(receiving_custody.key(), dispensing_custody.key());

    // Get current time and token IDs for calculations
    let pool = ctx.accounts.pool.as_mut();
    let curtime = perpetuals.get_time()?;
    let token_id_in = pool.get_token_id(&receiving_custody.key())?;
    let token_id_out = pool.get_token_id(&dispensing_custody.key())?;

    // Fetch oracle prices for the token being deposited (receiving custody)
    // Get both spot price and EMA price
    let received_token_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .receiving_custody_oracle_account
            .to_account_info(),
        &receiving_custody.oracle,
        curtime,
        false,
    )?;

    let received_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .receiving_custody_oracle_account
            .to_account_info(),
        &receiving_custody.oracle,
        curtime,
        receiving_custody.pricing.use_ema,
    )?;

    // Fetch oracle prices for the token being dispensed (dispensing custody)
    // Get both spot price and EMA price
    let dispensed_token_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .dispensing_custody_oracle_account
            .to_account_info(),
        &dispensing_custody.oracle,
        curtime,
        false,
    )?;

    let dispensed_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .dispensing_custody_oracle_account
            .to_account_info(),
        &dispensing_custody.oracle,
        curtime,
        dispensing_custody.pricing.use_ema,
    )?;

    // Calculate swap amount based on prices and pool state
    msg!("Compute swap amount");
    let amount_out = pool.get_swap_amount(
        &received_token_price,
        &received_token_ema_price,
        &dispensed_token_price,
        &dispensed_token_ema_price,
        receiving_custody,
        dispensing_custody,
        params.amount_in,
    )?;

    // Calculate swap fees
    // Fees are calculated for both input and output tokens
    let fees = pool.get_swap_fees(
        token_id_in,
        token_id_out,
        params.amount_in,
        amount_out,
        receiving_custody,
        &received_token_price,
        dispensing_custody,
        &dispensed_token_price,
    )?;
    msg!("Collected fees: {} {}", fees.0, fees.1);

    // Calculate amount user will receive after deducting output fee
    let no_fee_amount = math::checked_sub(amount_out, fees.1)?;
    msg!("Amount out: {}", no_fee_amount);
    
    // Validate slippage protection
    // Ensure user receives at least the minimum expected tokens
    require_gte!(
        no_fee_amount,
        params.min_amount_out,
        PerpetualsError::InsufficientAmountReturned
    );

    // Check pool constraints
    msg!("Check pool constraints");
    // Calculate protocol fees (portion of swap fees that go to protocol)
    let protocol_fee_in = Pool::get_fee_amount(receiving_custody.fees.protocol_share, fees.0)?;
    let protocol_fee_out = Pool::get_fee_amount(dispensing_custody.fees.protocol_share, fees.1)?;
    // Calculate net deposit and withdrawal amounts (after protocol fees)
    let deposit_amount = math::checked_sub(params.amount_in, protocol_fee_in)?;
    let withdrawal_amount = math::checked_add(no_fee_amount, protocol_fee_out)?;

    // Ensure token ratios remain within acceptable range after swap
    // Check both input token ratio (after deposit) and output token ratio (after withdrawal)
    require!(
        pool.check_token_ratio(
            token_id_in,
            deposit_amount,
            0,
            receiving_custody,
            &received_token_price
        )? && pool.check_token_ratio(
            token_id_out,
            0,
            withdrawal_amount,
            dispensing_custody,
            &dispensed_token_price
        )?,
        PerpetualsError::TokenRatioOutOfRange
    );
    
    // Ensure pool has sufficient available funds for withdrawal
    // (owned - locked >= withdrawal_amount)
    require!(
        math::checked_sub(
            dispensing_custody.assets.owned,
            dispensing_custody.assets.locked
        )? >= withdrawal_amount,
        PerpetualsError::CustodyAmountLimit
    );

    // Transfer tokens
    msg!("Transfer tokens");
    // Transfer tokens from user to pool (deposit)
    perpetuals.transfer_tokens_from_user(
        ctx.accounts.funding_account.to_account_info(),
        ctx.accounts
            .receiving_custody_token_account
            .to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.amount_in,
    )?;

    // Transfer tokens from pool to user (withdrawal, after fees)
    perpetuals.transfer_tokens(
        ctx.accounts
            .dispensing_custody_token_account
            .to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        no_fee_amount,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Update receiving custody stats (token being deposited)
    // Track volume in USD
    receiving_custody.volume_stats.swap_usd = receiving_custody.volume_stats.swap_usd.wrapping_add(
        received_token_price.get_asset_amount_usd(params.amount_in, receiving_custody.decimals)?,
    );

    // Track collected fees in USD
    receiving_custody.collected_fees.swap_usd =
        receiving_custody.collected_fees.swap_usd.wrapping_add(
            received_token_price.get_asset_amount_usd(fees.0, receiving_custody.decimals)?,
        );

    // Update owned assets (tokens owned by the pool after deposit)
    receiving_custody.assets.owned =
        math::checked_add(receiving_custody.assets.owned, deposit_amount)?;

    // Update protocol fees (portion of swap fee that goes to protocol)
    receiving_custody.assets.protocol_fees =
        math::checked_add(receiving_custody.assets.protocol_fees, protocol_fee_in)?;

    // Update dispensing custody stats (token being withdrawn)
    // Track collected fees in USD
    dispensing_custody.collected_fees.swap_usd =
        dispensing_custody.collected_fees.swap_usd.wrapping_add(
            dispensed_token_price.get_asset_amount_usd(fees.1, dispensing_custody.decimals)?,
        );

    // Track volume in USD
    dispensing_custody.volume_stats.swap_usd =
        dispensing_custody.volume_stats.swap_usd.wrapping_add(
            dispensed_token_price.get_asset_amount_usd(amount_out, dispensing_custody.decimals)?,
        );

    // Update protocol fees (portion of swap fee that goes to protocol)
    dispensing_custody.assets.protocol_fees =
        math::checked_add(dispensing_custody.assets.protocol_fees, protocol_fee_out)?;

    // Update owned assets (tokens owned by the pool after withdrawal)
    dispensing_custody.assets.owned =
        math::checked_sub(dispensing_custody.assets.owned, withdrawal_amount)?;

    // Update borrow rates for both custodies based on new utilization
    receiving_custody.update_borrow_rate(curtime)?;
    dispensing_custody.update_borrow_rate(curtime)?;

    Ok(())
}