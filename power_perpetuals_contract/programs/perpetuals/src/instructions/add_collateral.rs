//! AddCollateral instruction handler
//! 
//! This instruction allows users to add additional collateral to an existing position.
//! Adding collateral increases the position's margin, which can help avoid liquidation
//! and allows for larger position sizes. The collateral is transferred from the user's
//! funding account to the pool's custody token account.

use {
    crate::{
        error::PerpetualsError,
        math,
        state::{
            custody::Custody,
            oracle::OraclePrice,
            perpetuals::Perpetuals,
            pool::Pool,
            position::{Position, Side},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Token, TokenAccount},
    solana_program::program_error::ProgramError,
};

/// Accounts required for adding collateral to a position
#[derive(Accounts)]
#[instruction(params: AddCollateralParams)]
pub struct AddCollateral<'info> {
    /// Owner of the position (signer)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account from which collateral will be transferred
    /// Must be owned by the owner and have the same mint as the position custody
    #[account(
        mut,
        constraint = funding_account.mint == custody.mint,
        has_one = owner
    )]
    pub funding_account: Box<Account<'info, TokenAccount>>,

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

    /// Pool account (mutable, as collateral stats will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Position account to add collateral to (mutable, owned by owner)
    #[account(
        mut,
        has_one = owner,
        seeds = [b"position",
                 owner.key().as_ref(),
                 pool.key().as_ref(),
                 custody.key().as_ref(),
                 &[position.side as u8]],
        bump = position.bump
    )]
    pub position: Box<Account<'info, Position>>,

    /// Custody account for the position token (mutable, for stats updates)
    #[account(
        mut,
        constraint = position.custody == custody.key()
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the position token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = custody_oracle_account.key() == custody.oracle.oracle_account
    )]
    pub custody_oracle_account: AccountInfo<'info>,

    /// Custody account for the collateral token (mutable, for stats updates)
    #[account(
        mut,
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

    /// Token account where collateral will be deposited (pool's custody token account)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 collateral_custody.mint.as_ref()],
        bump = collateral_custody.token_account_bump
    )]
    pub collateral_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// Token program for token transfers
    pub token_program: Program<'info, Token>,
}

/// Parameters for adding collateral to a position
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddCollateralParams {
    /// Amount of collateral tokens to add (in collateral token's native decimals)
    collateral: u64,
}

/// Add collateral to an existing position
/// 
/// This function allows users to increase the margin/collateral of their position.
/// Adding collateral:
/// - Increases position margin, reducing liquidation risk
/// - Allows for larger position sizes (if leverage allows)
/// - Transfers tokens from user's funding account to pool's custody account
/// 
/// The function validates:
/// - Collateral amount is greater than zero
/// - Position leverage remains within acceptable limits after adding collateral
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including the collateral amount to add
/// 
/// # Returns
/// `Result<()>` - Success if collateral was added successfully
pub fn add_collateral(ctx: Context<AddCollateral>, params: &AddCollateralParams) -> Result<()> {
    // Validate inputs
    msg!("Validate inputs");
    if params.collateral == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }
    
    // Get mutable references to accounts
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let collateral_custody = ctx.accounts.collateral_custody.as_mut();
    let position = ctx.accounts.position.as_mut();
    let pool = ctx.accounts.pool.as_mut();

    // Get current time for price calculations
    let curtime = perpetuals.get_time()?;

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

    // Calculate collateral amount in USD for position updates
    let collateral_usd = min_collateral_price
        .get_asset_amount_usd(params.collateral, collateral_custody.decimals)?;
    msg!("Amount in: {}", params.collateral);
    msg!("Collateral added in USD: {}", collateral_usd);

    // Update position with new collateral
    msg!("Update existing position");
    position.update_time = perpetuals.get_time()?;
    position.collateral_usd = math::checked_add(position.collateral_usd, collateral_usd)?;
    position.collateral_amount = math::checked_add(position.collateral_amount, params.collateral)?;

    // Validate position leverage after adding collateral
    // This ensures the position remains within acceptable risk limits
    msg!("Check position risks");
    require!(
        pool.check_leverage(
            position,
            &token_price,
            &token_ema_price,
            custody,
            &collateral_token_price,
            &collateral_token_ema_price,
            collateral_custody,
            curtime,
            true
        )?,
        PerpetualsError::MaxLeverage
    );

    // Transfer collateral tokens from user's funding account to pool's custody account
    msg!("Transfer tokens");
    perpetuals.transfer_tokens_from_user(
        ctx.accounts.funding_account.to_account_info(),
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.collateral,
    )?;

    // Update custody statistics to reflect new collateral
    msg!("Update custody stats");
    collateral_custody.assets.collateral =
        math::checked_add(collateral_custody.assets.collateral, params.collateral)?;

    // If custody and collateral_custody accounts are the same (e.g., for long positions),
    // ensure that data is synchronized between the two references
    if position.side == Side::Long && !custody.is_virtual {
        *custody = collateral_custody.clone();
    }

    Ok(())
}