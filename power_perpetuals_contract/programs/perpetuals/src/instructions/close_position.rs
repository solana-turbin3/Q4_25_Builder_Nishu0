//! ClosePosition instruction handler
//! 
//! This instruction allows users to close an existing position.
//! It calculates profit/loss, collects fees, transfers remaining collateral back to the user,
//! and updates all relevant statistics. The position account is closed (deleted) after execution.

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

/// Accounts required for closing a position
/// 
/// The instruction calculates PnL, transfers collateral back to user,
/// updates custody statistics, and closes the position account.
#[derive(Accounts)]
pub struct ClosePosition<'info> {
    /// Position owner (must sign the transaction)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account to receive remaining collateral
    /// 
    /// Must match the collateral custody mint and be owned by the owner.
    #[account(
        mut,
        constraint = receiving_account.mint == collateral_custody.mint,
        has_one = owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

    /// Transfer authority PDA (authority for token accounts)
    /// 
    /// CHECK: This is a PDA, no data validation needed
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

    /// Pool account the position belongs to
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Position account to close
    /// 
    /// The `close = owner` constraint ensures the position account is closed
    /// and rent is returned to the owner after execution.
    #[account(
        mut,
        has_one = owner,
        seeds = [b"position",
                 owner.key().as_ref(),
                 pool.key().as_ref(),
                 custody.key().as_ref(),
                 &[position.side as u8]],
        bump = position.bump,
        close = owner
    )]
    pub position: Box<Account<'info, Position>>,

    /// Custody account for the position token (the asset being traded)
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

    /// Custody account for the collateral token (the asset used as margin)
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

    /// Pool's token account for collateral (source of collateral transfer)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 collateral_custody.mint.as_ref()],
        bump = collateral_custody.token_account_bump
    )]
    pub collateral_custody_token_account: Box<Account<'info, TokenAccount>>,

    /// Token program for token transfers
    token_program: Program<'info, Token>,
}

/// Parameters for closing a position
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ClosePositionParams {
    /// Minimum acceptable exit price (slippage protection, scaled to PRICE_DECIMALS)
    /// 
    /// For longs: must be <= actual exit price
    /// For shorts: must be >= actual exit price
    pub price: u64,
}

/// Close an existing position
/// 
/// This function:
/// 1. Validates permissions and inputs
/// 2. Calculates exit price and validates slippage protection
/// 3. Calculates profit/loss and fees
/// 4. Unlocks pool funds
/// 5. Transfers remaining collateral to user
/// 6. Updates custody statistics (volume, open interest, PnL)
/// 7. Removes position from custody tracking
/// 8. Closes the position account (returns rent to owner)
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including minimum acceptable exit price
/// 
/// # Returns
/// Error if validation fails, otherwise Ok(())
pub fn close_position(ctx: Context<ClosePosition>, params: &ClosePositionParams) -> Result<()> {
    // Check permissions
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let collateral_custody = ctx.accounts.collateral_custody.as_mut();
    require!(
        perpetuals.permissions.allow_close_position && custody.permissions.allow_close_position,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    msg!("Validate inputs");
    if params.price == 0 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    let position = ctx.accounts.position.as_mut();
    let pool = ctx.accounts.pool.as_mut();

    // Get current time for calculations
    let curtime = perpetuals.get_time()?;

    // Get position token prices (spot and EMA)
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

    // Get collateral token prices (spot and EMA)
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

    // Calculate exit price (applies spread based on position side)
    let exit_price = pool.get_exit_price(&token_price, &token_ema_price, position.side, custody)?;
    msg!("Exit price: {}", exit_price);

    // Validate slippage protection
    // For longs: exit_price must be >= params.price (user gets better or equal price)
    // For shorts: params.price must be >= exit_price (user gets better or equal price)
    if position.side == Side::Long {
        require_gte!(exit_price, params.price, PerpetualsError::MaxPriceSlippage);
    } else {
        require_gte!(params.price, exit_price, PerpetualsError::MaxPriceSlippage);
    }

    // Calculate final settlement amounts (collateral to return, fees, PnL)
    msg!("Settle position");
    let (transfer_amount, mut fee_amount, profit_usd, loss_usd) = pool.get_close_amount(
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

    // Convert fee to collateral token if needed
    // For shorts or virtual custodies, fee is in position token, convert to collateral
    let fee_amount_usd = token_ema_price.get_asset_amount_usd(fee_amount, custody.decimals)?;
    if position.side == Side::Short || custody.is_virtual {
        fee_amount = collateral_token_ema_price
            .get_token_amount(fee_amount_usd, collateral_custody.decimals)?;
    }

    msg!("Net profit: {}, loss: {}", profit_usd, loss_usd);
    msg!("Collected fee: {}", fee_amount);
    msg!("Amount out: {}", transfer_amount);

    // Unlock funds that were locked for this position
    collateral_custody.unlock_funds(position.locked_amount)?;

    // Check pool has sufficient funds available
    msg!("Check pool constraints");
    require!(
        pool.check_available_amount(transfer_amount, collateral_custody)?,
        PerpetualsError::CustodyAmountLimit
    );

    // Transfer remaining collateral to user
    msg!("Transfer tokens");
    perpetuals.transfer_tokens(
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        transfer_amount,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Track collected fees
    collateral_custody.collected_fees.close_position_usd = collateral_custody
        .collected_fees
        .close_position_usd
        .wrapping_add(fee_amount_usd);

    // Adjust owned assets based on PnL
    // If transfer_amount > collateral_amount: pool lost money (user profited)
    // If transfer_amount < collateral_amount: pool gained money (user lost)
    if transfer_amount > position.collateral_amount {
        let amount_lost = transfer_amount.saturating_sub(position.collateral_amount);
        collateral_custody.assets.owned =
            math::checked_sub(collateral_custody.assets.owned, amount_lost)?;
    } else {
        let amount_gained = position.collateral_amount.saturating_sub(transfer_amount);
        collateral_custody.assets.owned =
            math::checked_add(collateral_custody.assets.owned, amount_gained)?;
    }
    
    // Remove collateral from locked collateral tracking
    collateral_custody.assets.collateral = math::checked_sub(
        collateral_custody.assets.collateral,
        position.collateral_amount,
    )?;

    // Calculate and deduct protocol fee if pool has sufficient funds
    let protocol_fee = Pool::get_fee_amount(custody.fees.protocol_share, fee_amount)?;

    // Pay protocol_fee from custody if possible, otherwise no protocol_fee
    if pool.check_available_amount(protocol_fee, collateral_custody)? {
        collateral_custody.assets.protocol_fees =
            math::checked_add(collateral_custody.assets.protocol_fees, protocol_fee)?;

        collateral_custody.assets.owned =
            math::checked_sub(collateral_custody.assets.owned, protocol_fee)?;
    }

    // Update trade statistics and remove position from tracking
    // Handle differently if custody and collateral_custody are the same (long positions)
    if position.side == Side::Long && !custody.is_virtual {
        // For long positions where custody == collateral_custody, update collateral_custody stats
        collateral_custody.volume_stats.close_position_usd = collateral_custody
            .volume_stats
            .close_position_usd
            .wrapping_add(position.size_usd);

        // Update open interest (reduce by position size)
        if position.side == Side::Long {
            collateral_custody.trade_stats.oi_long_usd = collateral_custody
                .trade_stats
                .oi_long_usd
                .saturating_sub(position.size_usd);
        } else {
            collateral_custody.trade_stats.oi_short_usd = collateral_custody
                .trade_stats
                .oi_short_usd
                .saturating_sub(position.size_usd);
        }

        // Track aggregate profit/loss
        collateral_custody.trade_stats.profit_usd = collateral_custody
            .trade_stats
            .profit_usd
            .wrapping_add(profit_usd);
        collateral_custody.trade_stats.loss_usd = collateral_custody
            .trade_stats
            .loss_usd
            .wrapping_add(loss_usd);

        // Remove position from custody tracking (no separate collateral_custody to update)
        collateral_custody.remove_position(position, curtime, None)?;
        collateral_custody.update_borrow_rate(curtime)?;
        // Sync custody account data
        *custody = collateral_custody.clone();
    } else {
        // For positions where custody != collateral_custody, update custody stats
        custody.volume_stats.close_position_usd = custody
            .volume_stats
            .close_position_usd
            .wrapping_add(position.size_usd);

        // Update open interest
        if position.side == Side::Long {
            custody.trade_stats.oi_long_usd = custody
                .trade_stats
                .oi_long_usd
                .saturating_sub(position.size_usd);
        } else {
            custody.trade_stats.oi_short_usd = custody
                .trade_stats
                .oi_short_usd
                .saturating_sub(position.size_usd);
        }

        // Track aggregate profit/loss
        custody.trade_stats.profit_usd = custody.trade_stats.profit_usd.wrapping_add(profit_usd);
        custody.trade_stats.loss_usd = custody.trade_stats.loss_usd.wrapping_add(loss_usd);

        // Remove position from custody tracking (also update collateral_custody)
        custody.remove_position(position, curtime, Some(collateral_custody))?;
        // Update borrow rate for collateral custody
        collateral_custody.update_borrow_rate(curtime)?;
    }

    Ok(())
}