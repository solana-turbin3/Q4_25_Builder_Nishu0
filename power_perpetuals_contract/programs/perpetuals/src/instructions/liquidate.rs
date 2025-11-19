//! Liquidate instruction handler
//! 
//! This instruction allows anyone to liquidate a position that has exceeded maximum leverage.
//! When a position becomes undercollateralized (leverage exceeds limits), liquidators can
//! close the position and receive a reward. The position owner receives remaining collateral
//! after fees and rewards are deducted.

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

/// Accounts required for liquidating a position
#[derive(Accounts)]
pub struct Liquidate<'info> {
    /// Liquidator account (signer, receives liquidation reward)
    #[account(mut)]
    pub signer: Signer<'info>,

    /// Position owner's token account to receive remaining collateral after liquidation
    /// Must be owned by position owner and have the same mint as collateral custody
    #[account(
        mut,
        constraint = receiving_account.mint == collateral_custody.mint,
        constraint = receiving_account.owner == position.owner
    )]
    pub receiving_account: Box<Account<'info, TokenAccount>>,

    /// Liquidator's token account to receive liquidation reward
    /// Must be owned by liquidator and have the same mint as collateral custody
    #[account(
        mut,
        constraint = rewards_receiving_account.mint == collateral_custody.mint,
        constraint = rewards_receiving_account.owner == signer.key()
    )]
    pub rewards_receiving_account: Box<Account<'info, TokenAccount>>,

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

    /// Position account to liquidate (mutable, will be closed)
    /// Position is closed and rent is returned to liquidator
    #[account(
        mut,
        seeds = [b"position",
                 position.owner.as_ref(),
                 pool.key().as_ref(),
                 custody.key().as_ref(),
                 &[position.side as u8]],
        bump = position.bump,
        close = signer
    )]
    pub position: Box<Account<'info, Position>>,

    /// Custody account for the position token (mutable, stats will be updated)
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

    /// Custody account for the collateral token (mutable, stats will be updated)
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

/// Parameters for liquidating a position
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct LiquidateParams {}

/// Liquidate an undercollateralized position
/// 
/// This function allows liquidators to close positions that have exceeded maximum leverage.
/// The process:
/// 1. Validates permissions and position state (must exceed leverage limits)
/// 2. Calculates settlement amounts (collateral to return, fees, PnL)
/// 3. Calculates liquidation reward for liquidator
/// 4. Unlocks pool funds
/// 5. Transfers remaining collateral to position owner
/// 6. Transfers liquidation reward to liquidator
/// 7. Updates custody and pool statistics
/// 8. Removes position from custody tracking
/// 
/// Liquidation reward is calculated as a percentage of total amount out.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `_params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<()>` - Success if position was liquidated successfully
pub fn liquidate(ctx: Context<Liquidate>, _params: &LiquidateParams) -> Result<()> {
    // Check permissions
    // Both perpetuals and custody must allow closing positions
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let collateral_custody = ctx.accounts.collateral_custody.as_mut();
    require!(
        perpetuals.permissions.allow_close_position && custody.permissions.allow_close_position,
        PerpetualsError::InstructionNotAllowed
    );

    let position = ctx.accounts.position.as_mut();
    let pool = ctx.accounts.pool.as_mut();

    // Check if position can be liquidated
    // Position must exceed maximum leverage (check_leverage returns false)
    msg!("Check position state");
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

    // Validate that position exceeds maximum leverage (can be liquidated)
    // check_leverage returns true if position is safe, false if it exceeds limits
    // We require it to be false (unsafe) for liquidation
    require!(
        !pool.check_leverage(
            position,
            &token_price,
            &token_ema_price,
            custody,
            &collateral_token_price,
            &collateral_token_ema_price,
            collateral_custody,
            curtime,
            false
        )?,
        PerpetualsError::InvalidPositionState
    );

    // Calculate settlement amounts (collateral to return, fees, PnL)
    // Uses liquidation fee instead of regular exit fee
    msg!("Settle position");
    let (total_amount_out, mut fee_amount, profit_usd, loss_usd) = pool.get_close_amount(
        position,
        &token_price,
        &token_ema_price,
        custody,
        &collateral_token_price,
        &collateral_token_ema_price,
        collateral_custody,
        curtime,
        true, // liquidation = true
    )?;

    // Convert fee to collateral token if needed
    // For shorts or virtual custodies, fee is calculated in position token, convert to collateral
    let fee_amount_usd = token_ema_price.get_asset_amount_usd(fee_amount, custody.decimals)?;
    if position.side == Side::Short || custody.is_virtual {
        fee_amount = collateral_token_ema_price
            .get_token_amount(fee_amount_usd, collateral_custody.decimals)?;
    }

    msg!("Net profit: {}, loss: {}", profit_usd, loss_usd);
    msg!("Collected fee: {}", fee_amount);

    // Calculate liquidation reward (percentage of total amount out)
    let reward = Pool::get_fee_amount(custody.fees.liquidation, total_amount_out)?;
    // Calculate amount to return to position owner (after deducting reward)
    let user_amount = math::checked_sub(total_amount_out, reward)?;

    msg!("Amount out: {}", user_amount);
    msg!("Reward: {}", reward);

    // Unlock pool funds that were locked for this position
    collateral_custody.unlock_funds(position.locked_amount)?;

    // Check pool constraints
    // Ensure pool has enough funds to cover the liquidation
    msg!("Check pool constraints");
    require!(
        pool.check_available_amount(total_amount_out, collateral_custody)?,
        PerpetualsError::CustodyAmountLimit
    );

    // Transfer tokens
    // First transfer remaining collateral to position owner
    msg!("Transfer tokens");
    perpetuals.transfer_tokens(
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        user_amount,
    )?;

    // Then transfer liquidation reward to liquidator
    perpetuals.transfer_tokens(
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.rewards_receiving_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        reward,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Track collected liquidation fees
    collateral_custody.collected_fees.liquidation_usd = collateral_custody
        .collected_fees
        .liquidation_usd
        .wrapping_add(fee_amount_usd);

    // Update owned assets based on PnL
    // If total_amount_out > collateral_amount, pool lost funds (subtract difference)
    // If total_amount_out < collateral_amount, pool gained funds (add difference)
    if total_amount_out > position.collateral_amount {
        let amount_lost = total_amount_out.saturating_sub(position.collateral_amount);
        collateral_custody.assets.owned =
            math::checked_sub(collateral_custody.assets.owned, amount_lost)?;
    } else {
        let amount_gained = position.collateral_amount.saturating_sub(total_amount_out);
        collateral_custody.assets.owned =
            math::checked_add(collateral_custody.assets.owned, amount_gained)?;
    }
    // Remove collateral amount from custody tracking
    collateral_custody.assets.collateral = math::checked_sub(
        collateral_custody.assets.collateral,
        position.collateral_amount,
    )?;

    // Calculate and pay protocol fee if pool has sufficient funds
    let protocol_fee = Pool::get_fee_amount(custody.fees.protocol_share, fee_amount)?;

    // Pay protocol_fee from custody if possible, otherwise no protocol_fee
    if pool.check_available_amount(protocol_fee, collateral_custody)? {
        collateral_custody.assets.protocol_fees =
            math::checked_add(collateral_custody.assets.protocol_fees, protocol_fee)?;

        collateral_custody.assets.owned =
            math::checked_sub(collateral_custody.assets.owned, protocol_fee)?;
    }

    // Update trade statistics and remove position from tracking
    // If custody and collateral_custody accounts are the same (e.g., for long positions),
    // update collateral_custody stats and sync to custody
    if position.side == Side::Long && !custody.is_virtual {
        // Track liquidation volume
        collateral_custody.volume_stats.liquidation_usd = math::checked_add(
            collateral_custody.volume_stats.liquidation_usd,
            position.size_usd,
        )?;

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

        // Track profit and loss
        collateral_custody.trade_stats.profit_usd = collateral_custody
            .trade_stats
            .profit_usd
            .wrapping_add(profit_usd);
        collateral_custody.trade_stats.loss_usd = collateral_custody
            .trade_stats
            .loss_usd
            .wrapping_add(loss_usd);

        // Remove position from custody tracking and update borrow rate
        collateral_custody.remove_position(position, curtime, None)?;
        collateral_custody.update_borrow_rate(curtime)?;
        // Sync custody account with collateral_custody
        *custody = collateral_custody.clone();
    } else {
        // Update custody stats (position token custody)
        custody.volume_stats.liquidation_usd =
            math::checked_add(custody.volume_stats.liquidation_usd, position.size_usd)?;

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

        // Track profit and loss
        custody.trade_stats.profit_usd = custody.trade_stats.profit_usd.wrapping_add(profit_usd);
        custody.trade_stats.loss_usd = custody.trade_stats.loss_usd.wrapping_add(loss_usd);

        // Remove position from custody tracking (with collateral_custody reference)
        custody.remove_position(position, curtime, Some(collateral_custody))?;
        // Update borrow rate for collateral custody
        collateral_custody.update_borrow_rate(curtime)?;
    }

    Ok(())
}