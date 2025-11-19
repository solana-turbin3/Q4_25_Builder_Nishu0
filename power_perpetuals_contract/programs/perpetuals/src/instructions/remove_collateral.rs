//! RemoveCollateral instruction handler
//! 
//! This instruction allows users to remove collateral from an existing position.
//! Removing collateral reduces the position's margin, which increases leverage.
//! The position's leverage must remain within acceptable limits after removal.

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
};

/// Accounts required for removing collateral from a position
#[derive(Accounts)]
#[instruction(params: RemoveCollateralParams)]
pub struct RemoveCollateral<'info> {
    /// Owner of the position (signer)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account where collateral will be returned
    /// Must be owned by owner and have the same mint as custody
    #[account(
        mut,
        constraint = receiving_account.mint == custody.mint,
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

    /// Pool account (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Position account to remove collateral from (mutable, owned by owner)
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

    /// Pool's token account where collateral is stored (mutable, tokens will be transferred out)
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

/// Parameters for removing collateral from a position
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemoveCollateralParams {
    collateral_usd: u64,
}

/// Remove collateral from an existing position
/// 
/// This function allows users to withdraw collateral from their position, reducing
/// the margin/collateral. Removing collateral increases leverage, so the function
/// validates that leverage remains within acceptable limits after removal.
/// 
/// The process:
/// 1. Validates permissions and inputs
/// 2. Gets current prices from oracles
/// 3. Calculates collateral amount to remove (using maximum price for conservative estimate)
/// 4. Updates position with reduced collateral
/// 5. Validates leverage remains within limits
/// 6. Transfers collateral from pool to user
/// 7. Updates custody statistics
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including collateral amount to remove in USD
/// 
/// # Returns
/// `Result<()>` - Success if collateral was removed successfully
pub fn remove_collateral(
    ctx: Context<RemoveCollateral>,
    params: &RemoveCollateralParams,
) -> Result<()> {
    // Check permissions
    // Both perpetuals and custody must allow collateral withdrawal
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let collateral_custody = ctx.accounts.collateral_custody.as_mut();
    require!(
        perpetuals.permissions.allow_collateral_withdrawal
            && custody.permissions.allow_collateral_withdrawal,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    // Collateral amount must be greater than 0 and less than position's current collateral
    msg!("Validate inputs");
    let position = ctx.accounts.position.as_mut();
    if params.collateral_usd == 0 || params.collateral_usd >= position.collateral_usd {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    let pool = ctx.accounts.pool.as_mut();

    // Get current time for calculations
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

    // Use maximum collateral price for conservative token amount calculation
    // This ensures users get a conservative estimate of tokens they'll receive
    let max_collateral_price = if collateral_token_price > collateral_token_ema_price {
        collateral_token_price
    } else {
        collateral_token_ema_price
    };

    // Calculate amount of collateral tokens to transfer
    // Convert USD amount to token amount using maximum price
    let collateral = max_collateral_price
        .get_token_amount(params.collateral_usd, collateral_custody.decimals)?;
    // Validate that calculated amount doesn't exceed available collateral
    if collateral > position.collateral_amount {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    msg!("Amount out: {}", collateral);

    // Update position with reduced collateral
    msg!("Update existing position");
    position.update_time = perpetuals.get_time()?;
    position.collateral_usd = math::checked_sub(position.collateral_usd, params.collateral_usd)?;
    position.collateral_amount = math::checked_sub(position.collateral_amount, collateral)?;

    // Validate position leverage after removing collateral
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

    // Transfer collateral tokens from pool's custody account to user's receiving account
    msg!("Transfer tokens");
    perpetuals.transfer_tokens(
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        collateral,
    )?;

    // Update custody statistics to reflect reduced collateral
    msg!("Update custody stats");
    collateral_custody.assets.collateral =
        math::checked_sub(collateral_custody.assets.collateral, collateral)?;

    // If custody and collateral_custody accounts are the same (e.g., for long positions),
    // ensure that data is synchronized between the two references
    if position.side == Side::Long && !custody.is_virtual {
        *custody = collateral_custody.clone();
    }

    Ok(())
}