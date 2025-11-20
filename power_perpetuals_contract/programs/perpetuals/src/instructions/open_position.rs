//! OpenPosition instruction handler
//! 
//! This instruction allows users to open a new trading position (long or short).
//! Users deposit collateral and specify the position size and side. The position
//! is initialized with entry price, leverage is validated, and funds are locked
//! to cover potential profit payouts.

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
    anchor_lang::error::ErrorCode::ConstraintRaw,
};

/// Accounts required for opening a new position
#[derive(Accounts)]
#[instruction(params: OpenPositionParams)]
pub struct OpenPosition<'info> {
    /// Owner of the position (signer)
    #[account(mut)]
    pub owner: Signer<'info>,

    /// User's token account from which collateral will be transferred
    /// Must be owned by owner and have the same mint as collateral custody
    #[account(
        mut,
        constraint = funding_account.mint == collateral_custody.mint,
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

    /// Pool account (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// New position account to be initialized (PDA derived from owner, pool, custody, side)
    #[account(
        init,
        payer = owner,
        space = Position::LEN,
        seeds = [b"position",
                 owner.key().as_ref(),
                 pool.key().as_ref(),
                 custody.key().as_ref(),
                 &[params.side as u8]],
        bump
    )]
    pub position: Box<Account<'info, Position>>,

    /// Custody account for the position token (mutable, stats will be updated)
    #[account(
        mut,
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

    /// Custody account for the collateral token (mutable, stats will be updated)
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 collateral_custody.mint.as_ref()],
        bump = collateral_custody.bump
    )]
    pub collateral_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the collateral token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = collateral_custody_oracle_account.key() == collateral_custody.oracle.oracle_account
    )]
    pub collateral_custody_oracle_account: AccountInfo<'info>,

    /// Pool's token account where collateral will be deposited
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 collateral_custody.mint.as_ref()],
        bump = collateral_custody.token_account_bump
    )]
    pub collateral_custody_token_account: Box<Account<'info, TokenAccount>>,

    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
}

/// Parameters for opening a new position
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct OpenPositionParams {
    /// Maximum acceptable entry price (slippage protection, scaled to PRICE_DECIMALS)
    /// For longs: must be >= actual entry price
    /// For shorts: must be <= actual entry price
    pub price: u64,
    /// Amount of collateral tokens to deposit (in collateral token's native decimals)
    pub collateral: u64,
    /// Position size in tokens (in position token's native decimals)
    pub size: u64,
    /// Position side (Long or Short)
    pub side: Side,
    /// Power multiplier for power perpetuals (1-5)
    /// 1 = linear perps, 2 = squared perps, 3 = cubed, etc.
    pub power: u8,
}

/// Open a new trading position
/// 
/// This function allows users to open a new position (long or short) by depositing collateral.
/// The process:
/// 1. Validates permissions and inputs
/// 2. Calculates entry price (with spread applied)
/// 3. Validates slippage protection
/// 4. Calculates position parameters (size USD, collateral USD, locked amount)
/// 5. Calculates entry fee
/// 6. Initializes position account
/// 7. Validates leverage is within acceptable limits
/// 8. Locks funds for potential profit payouts
/// 9. Transfers collateral and fees from user to pool
/// 10. Updates custody and pool statistics
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including price, collateral, size, and side
/// 
/// # Returns
/// `Result<()>` - Success if position was opened successfully
pub fn open_position(ctx: Context<OpenPosition>, params: &OpenPositionParams) -> Result<()> {
    // Check permissions
    // Both perpetuals and custody must allow opening positions
    // Position token cannot be a stablecoin
    msg!("Check permissions");
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let custody = ctx.accounts.custody.as_mut();
    let collateral_custody = ctx.accounts.collateral_custody.as_mut();
    require!(
        perpetuals.permissions.allow_open_position
            && custody.permissions.allow_open_position
            && !custody.is_stable,
        PerpetualsError::InstructionNotAllowed
    );

    // Validate inputs
    msg!("Validate inputs");
    if params.price == 0 || params.collateral == 0 || params.size == 0 || params.side == Side::None
    {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }

    // Validate power parameter (must be 1-5)
    // power=1: linear perps, power=2: squared, ..., power=5: max power
    require!(
        params.power >= 1 && params.power <= 5,
        PerpetualsError::InvalidPositionState
    );

    // Determine if collateral custody is different from position custody
    // For shorts or virtual custodies, must use a different stablecoin as collateral
    let use_collateral_custody = params.side == Side::Short || custody.is_virtual;
    if use_collateral_custody {
        // For shorts/virtual: collateral custody must be different and must be a stablecoin
        require_keys_neq!(custody.key(), collateral_custody.key());
        require!(
            collateral_custody.is_stable && !collateral_custody.is_virtual,
            PerpetualsError::InvalidCollateralCustody
        );
    } else {
        // For longs: collateral custody must be the same as position custody
        require_keys_eq!(custody.key(), collateral_custody.key());
    };
    let position = ctx.accounts.position.as_mut();
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

    // Use minimum collateral price for conservative valuation
    // For stablecoins, caps price at 1 USD
    let min_collateral_price = collateral_token_price
        .get_min_price(&collateral_token_ema_price, collateral_custody.is_stable)?;

    // Calculate entry price (applies spread based on position side)
    let position_price =
        pool.get_entry_price(&token_price, &token_ema_price, params.side, custody)?;
    msg!("Entry price: {}", position_price);

    // Validate slippage protection
    // For longs: user's max price must be >= actual entry price (user gets better or equal price)
    // For shorts: actual entry price must be >= user's max price (user gets better or equal price)
    if params.side == Side::Long {
        require_gte!(
            params.price,
            position_price,
            PerpetualsError::MaxPriceSlippage
        );
    } else {
        require_gte!(
            position_price,
            params.price,
            PerpetualsError::MaxPriceSlippage
        );
    }

    // Calculate position parameters
    // Convert entry price to OraclePrice format for calculations
    let position_oracle_price = OraclePrice {
        price: position_price,
        exponent: -(Perpetuals::PRICE_DECIMALS as i32),
    };
    // Calculate position size and collateral in USD
    let size_usd = position_oracle_price.get_asset_amount_usd(params.size, custody.decimals)?;
    let collateral_usd = min_collateral_price
        .get_asset_amount_usd(params.collateral, collateral_custody.decimals)?;

    // Calculate locked amount (tokens that will be locked for potential profit payouts)
    // For shorts or virtual custodies, convert size_usd to collateral tokens first
    let locked_amount = if use_collateral_custody {
        custody.get_locked_amount(
            min_collateral_price.get_token_amount(size_usd, collateral_custody.decimals)?,
            params.side,
        )?
    } else {
        custody.get_locked_amount(params.size, params.side)?
    };

    // Calculate borrow size USD (used for leverage calculations)
    // If max_payoff_mult is set, use locked amount; otherwise use position size
    let borrow_size_usd = if custody.pricing.max_payoff_mult as u128 != Perpetuals::BPS_POWER {
        if use_collateral_custody {
            // Use maximum collateral price for conservative calculation
            let max_collateral_price = if collateral_token_price < collateral_token_ema_price {
                collateral_token_ema_price
            } else {
                collateral_token_price
            };
            max_collateral_price.get_asset_amount_usd(locked_amount, collateral_custody.decimals)?
        } else {
            position_oracle_price.get_asset_amount_usd(locked_amount, custody.decimals)?
        }
    } else {
        size_usd
    };

    // Calculate entry fee (includes utilization-based adjustments)
    let mut fee_amount = pool.get_entry_fee(
        custody.fees.open_position,
        params.size,
        locked_amount,
        collateral_custody,
    )?;
    let fee_amount_usd = token_ema_price.get_asset_amount_usd(fee_amount, custody.decimals)?;
    // Convert fee to collateral token if needed
    if use_collateral_custody {
        fee_amount = collateral_token_ema_price
            .get_token_amount(fee_amount_usd, collateral_custody.decimals)?;
    }
    msg!("Collected fee: {}", fee_amount);

    // Calculate total amount to transfer (collateral + fee)
    let transfer_amount = math::checked_add(params.collateral, fee_amount)?;
    msg!("Amount in: {}", transfer_amount);

    // Initialize new position account with all parameters
    msg!("Initialize new position");
    position.owner = ctx.accounts.owner.key();
    position.pool = pool.key();
    position.custody = custody.key();
    position.collateral_custody = collateral_custody.key();
    position.open_time = perpetuals.get_time()?;
    position.update_time = 0;
    position.side = params.side;
    position.power = params.power;
    position.price = position_price;
    position.size_usd = size_usd;
    position.borrow_size_usd = borrow_size_usd;
    position.collateral_usd = collateral_usd;
    position.unrealized_profit_usd = 0;
    position.unrealized_loss_usd = 0;
    position.cumulative_interest_snapshot = collateral_custody.get_cumulative_interest(curtime)?;
    position.locked_amount = locked_amount;
    position.collateral_amount = params.collateral;
    position.bump = ctx.bumps.position;

    // Validate position leverage and locked amount
    msg!("Check position risks");
    require!(
        position.locked_amount > 0,
        PerpetualsError::InsufficientAmountReturned
    );
    // Ensure position leverage is within acceptable limits
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
            true // new_position = true
        )?,
        PerpetualsError::MaxLeverage
    );

    // Lock funds for potential profit payouts
    // This ensures the pool has enough liquidity to pay profits if position becomes profitable
    collateral_custody.lock_funds(position.locked_amount)?;

    // Transfer collateral and fee from user's funding account to pool's custody account
    msg!("Transfer tokens");
    perpetuals.transfer_tokens_from_user(
        ctx.accounts.funding_account.to_account_info(),
        ctx.accounts
            .collateral_custody_token_account
            .to_account_info(),
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        transfer_amount,
    )?;

    // Update custody statistics
    msg!("Update custody stats");
    // Track collected fees
    collateral_custody.collected_fees.open_position_usd = collateral_custody
        .collected_fees
        .open_position_usd
        .wrapping_add(fee_amount_usd);

    // Update collateral tracking
    collateral_custody.assets.collateral =
        math::checked_add(collateral_custody.assets.collateral, params.collateral)?;

    // Calculate and track protocol fee (portion of entry fee that goes to protocol)
    let protocol_fee = Pool::get_fee_amount(custody.fees.protocol_share, fee_amount)?;
    collateral_custody.assets.protocol_fees =
        math::checked_add(collateral_custody.assets.protocol_fees, protocol_fee)?;

    // Update trade statistics and add position to tracking
    // If custody and collateral_custody accounts are the same (e.g., for long positions),
    // update collateral_custody stats and sync to custody
    if position.side == Side::Long && !custody.is_virtual {
        // Track opening volume
        collateral_custody.volume_stats.open_position_usd = collateral_custody
            .volume_stats
            .open_position_usd
            .wrapping_add(size_usd);

        // Update open interest (increase by position size)
        if params.side == Side::Long {
            collateral_custody.trade_stats.oi_long_usd =
                math::checked_add(collateral_custody.trade_stats.oi_long_usd, size_usd)?;
        } else {
            collateral_custody.trade_stats.oi_short_usd =
                math::checked_add(collateral_custody.trade_stats.oi_short_usd, size_usd)?;
        }

        // Add position to custody tracking and update borrow rate
        collateral_custody.add_position(position, &token_ema_price, curtime, None)?;
        collateral_custody.update_borrow_rate(curtime)?;
        // Sync custody account with collateral_custody
        *custody = collateral_custody.clone();
    } else {
        // Update custody stats (position token custody)
        custody.volume_stats.open_position_usd = custody
            .volume_stats
            .open_position_usd
            .wrapping_add(size_usd);

        // Update open interest
        if params.side == Side::Long {
            custody.trade_stats.oi_long_usd =
                math::checked_add(custody.trade_stats.oi_long_usd, size_usd)?;
        } else {
            custody.trade_stats.oi_short_usd =
                math::checked_add(custody.trade_stats.oi_short_usd, size_usd)?;
        }

        // Add position to custody tracking (with collateral_custody reference)
        custody.add_position(
            position,
            &token_ema_price,
            curtime,
            Some(collateral_custody),
        )?;
        // Update borrow rate for collateral custody
        collateral_custody.update_borrow_rate(curtime)?;
    }

    Ok(())
}