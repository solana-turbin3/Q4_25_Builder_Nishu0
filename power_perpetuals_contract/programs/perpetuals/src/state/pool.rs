//! Pool state and pricing logic for perpetuals
//! 
//! This module handles pool management, token pricing, fee calculations,
//! profit/loss calculations, leverage checks, and AUM (Assets Under Management) tracking.

use {
    crate::{
        error::PerpetualsError,
        math,
        state::{
            custody::{Custody, FeesMode},
            oracle::OraclePrice,
            perpetuals::Perpetuals,
            position::{Position, Side},
        },
    },
    anchor_lang::prelude::*,
    std::cmp::Ordering,
};

/// AUM (Assets Under Management) calculation mode
/// 
/// Determines which price to use when calculating pool value
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Debug)]
pub enum AumCalcMode {
    /// Use minimum price between spot and EMA
    Min,
    /// Use maximum price between spot and EMA
    Max,
    /// Use last spot price
    Last,
    /// Use EMA (Exponential Moving Average) price
    EMA,
}

/// Token ratio constraints for pool rebalancing
/// 
/// All ratios are in basis points (BPS), where 10,000 BPS = 100%
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct TokenRatios {
    /// Target ratio for this token in the pool (in BPS)
    pub target: u64,
    /// Minimum allowed ratio (in BPS)
    pub min: u64,
    /// Maximum allowed ratio (in BPS)
    pub max: u64,
}

/// Pool account - manages a multi-token liquidity pool
/// 
/// The pool tracks multiple token custodies, their target ratios,
/// and the total assets under management (AUM).
#[account]
#[derive(Default, Debug)]
pub struct Pool {
    /// Pool name (max 64 characters)
    pub name: String,
    /// List of custody account addresses for tokens in this pool
    pub custodies: Vec<Pubkey>,
    /// Token ratio constraints for each custody (parallel to custodies)
    pub ratios: Vec<TokenRatios>,
    /// Total assets under management in USD (scaled to USD_DECIMALS)
    pub aum_usd: u128,

    /// Bump seed for the pool PDA
    pub bump: u8,
    /// Bump seed for the LP token mint PDA
    pub lp_token_bump: u8,
    /// Pool creation timestamp
    pub inception_time: i64,
}

impl TokenRatios {
    /// Validate that ratio constraints are valid
    /// 
    /// Checks:
    /// - All ratios are <= 100% (BPS_POWER)
    /// - min <= target <= max
    /// 
    /// # Returns
    /// true if ratios are valid
    pub fn validate(&self) -> bool {
        (self.target as u128) <= Perpetuals::BPS_POWER
            && (self.min as u128) <= Perpetuals::BPS_POWER
            && (self.max as u128) <= Perpetuals::BPS_POWER
            && self.min <= self.target
            && self.target <= self.max
    }
}

/// Token Pool implementation
/// 
/// All returned prices are scaled to PRICE_DECIMALS.
/// All returned amounts are scaled to corresponding custody decimals.
impl Pool {
    /// Account size in bytes (8 byte discriminator + 64 byte string + data)
    pub const LEN: usize = 8 + 64 + std::mem::size_of::<Pool>();

    /// Validate pool configuration
    /// 
    /// Checks:
    /// - All token ratios are valid
    /// - Target ratios sum to 100%
    /// - Custody addresses are unique
    /// - Name is non-empty and <= 64 chars
    /// - Custodies and ratios arrays have matching lengths
    /// 
    /// # Returns
    /// true if pool configuration is valid
    pub fn validate(&self) -> bool {
        for ratio in &self.ratios {
            if !ratio.validate() {
                return false;
            }
        }

        // check target ratios add up to 1
        if !self.ratios.is_empty()
            && self
                .ratios
                .iter()
                .map(|&x| (x.target as u128))
                .sum::<u128>()
                != Perpetuals::BPS_POWER
        {
            return false;
        }

        // check custodies are unique
        for i in 1..self.custodies.len() {
            if self.custodies[i..].contains(&self.custodies[i - 1]) {
                return false;
            }
        }

        !self.name.is_empty() && self.name.len() <= 64 && self.custodies.len() == self.ratios.len()
    }

    /// Get the token ID (index) for a given custody address
    /// 
    /// # Arguments
    /// * `custody` - Custody account pubkey
    /// 
    /// # Returns
    /// Token ID (index in custodies array) or error if not found
    pub fn get_token_id(&self, custody: &Pubkey) -> Result<usize> {
        self.custodies
            .iter()
            .position(|&k| k == *custody)
            .ok_or_else(|| PerpetualsError::UnsupportedToken.into())
    }

    /// Calculate entry price for opening a position
    /// 
    /// Uses the maximum price (spot or EMA) for longs, applies trade spread.
    /// 
    /// # Arguments
    /// * `token_price` - Current spot price from oracle
    /// * `token_ema_price` - EMA price from oracle
    /// * `side` - Position side (Long or Short)
    /// * `custody` - Custody account for the token
    /// 
    /// # Returns
    /// Entry price scaled to PRICE_DECIMALS
    pub fn get_entry_price(
        &self,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        side: Side,
        custody: &Custody,
    ) -> Result<u64> {
        let price = self.get_price(
            token_price,
            token_ema_price,
            side,
            if side == Side::Long {
                custody.pricing.trade_spread_long
            } else {
                custody.pricing.trade_spread_short
            },
        )?;
        require_gt!(price.price, 0, PerpetualsError::MaxPriceSlippage);

        Ok(price
            .scale_to_exponent(-(Perpetuals::PRICE_DECIMALS as i32))?
            .price)
    }

    /// Calculate entry fee for opening a position
    /// 
    /// Uses the "optimal" fee algorithm with utilization-based fee adjustment.
    /// Fee increases when utilization exceeds optimal level.
    /// 
    /// Formula:
    /// - entry_fee = custody.fees.open_position * utilization_fee * size
    /// - utilization_fee = 1 + custody.fees.utilization_mult * (new_utilization - optimal_utilization) / (1 - optimal_utilization)
    /// 
    /// # Arguments
    /// * `base_fee` - Base fee rate (in BPS)
    /// * `size` - Position size in tokens
    /// * `locked_amount` - Amount that will be locked for this position
    /// * `collateral_custody` - Custody account for collateral token
    /// 
    /// # Returns
    /// Entry fee amount in tokens
    pub fn get_entry_fee(
        &self,
        base_fee: u64,
        size: u64,
        locked_amount: u64,
        collateral_custody: &Custody,
    ) -> Result<u64> {

        let mut size_fee = Self::get_fee_amount(base_fee, size)?;

        let new_utilization = if collateral_custody.assets.owned > 0 {
            // utilization = (assets_locked + locked_amount) / assets_owned
            std::cmp::min(
                Perpetuals::RATE_POWER,
                math::checked_div(
                    math::checked_mul(
                        math::checked_add(collateral_custody.assets.locked, locked_amount)? as u128,
                        Perpetuals::RATE_POWER,
                    )?,
                    collateral_custody.assets.owned as u128,
                )?,
            )
        } else {
            Perpetuals::RATE_POWER
        };

        if new_utilization > collateral_custody.borrow_rate.optimal_utilization as u128 {
            let utilization_fee = math::checked_add(
                Perpetuals::BPS_POWER,
                math::checked_div(
                    math::checked_mul(
                        collateral_custody.fees.utilization_mult as u128,
                        math::checked_sub(
                            new_utilization,
                            collateral_custody.borrow_rate.optimal_utilization as u128,
                        )?,
                    )?,
                    math::checked_sub(
                        Perpetuals::RATE_POWER,
                        collateral_custody.borrow_rate.optimal_utilization as u128,
                    )?,
                )?,
            )?;
            size_fee = math::checked_as_u64(math::checked_div(
                math::checked_mul(size_fee as u128, utilization_fee)?,
                Perpetuals::BPS_POWER,
            )?)?;
        }

        Ok(size_fee)
    }

    /// Calculate exit price for closing a position
    /// 
    /// Uses the minimum price (spot or EMA) for the opposite side,
    /// applies trade spread. For longs, uses short spread and vice versa.
    /// 
    /// # Arguments
    /// * `token_price` - Current spot price from oracle
    /// * `token_ema_price` - EMA price from oracle
    /// * `side` - Position side being closed (Long or Short)
    /// * `custody` - Custody account for the token
    /// 
    /// # Returns
    /// Exit price scaled to PRICE_DECIMALS
    pub fn get_exit_price(
        &self,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        side: Side,
        custody: &Custody,
    ) -> Result<u64> {
        let price = self.get_price(
            token_price,
            token_ema_price,
            if side == Side::Long {
                Side::Short
            } else {
                Side::Long
            },
            if side == Side::Long {
                custody.pricing.trade_spread_short
            } else {
                custody.pricing.trade_spread_long
            },
        )?;

        Ok(price
            .scale_to_exponent(-(Perpetuals::PRICE_DECIMALS as i32))?
            .price)
    }

    /// Calculate exit fee for closing a position
    /// 
    /// # Arguments
    /// * `size` - Position size in tokens
    /// * `custody` - Custody account for the token
    /// 
    /// # Returns
    /// Exit fee amount in tokens
    pub fn get_exit_fee(&self, size: u64, custody: &Custody) -> Result<u64> {
        Self::get_fee_amount(custody.fees.close_position, size)
    }

    /// Calculate close amount and PnL for closing a position
    /// 
    /// Returns the amount of collateral to return, fees, profit, and loss.
    /// 
    /// # Arguments
    /// * `position` - Position being closed
    /// * `token_price` - Current spot price for the position token
    /// * `token_ema_price` - EMA price for the position token
    /// * `custody` - Custody account for the position token
    /// * `collateral_token_price` - Current spot price for collateral token
    /// * `collateral_token_ema_price` - EMA price for collateral token
    /// * `collateral_custody` - Custody account for collateral token
    /// * `curtime` - Current timestamp
    /// * `liquidation` - Whether this is a liquidation (affects fee calculation)
    /// 
    /// # Returns
    /// Tuple of (close_amount, fee_amount, profit_usd, loss_usd)
    #[allow(clippy::too_many_arguments)]
    pub fn get_close_amount(
        &self,
        position: &Position,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        custody: &Custody,
        collateral_token_price: &OraclePrice,
        collateral_token_ema_price: &OraclePrice,
        collateral_custody: &Custody,
        curtime: i64,
        liquidation: bool,
    ) -> Result<(u64, u64, u64, u64)> {
        let (profit_usd, loss_usd, fee_amount) = self.get_pnl_usd(
            position,
            token_price,
            token_ema_price,
            custody,
            collateral_token_price,
            collateral_token_ema_price,
            collateral_custody,
            curtime,
            liquidation,
        )?;

        let available_amount_usd = if profit_usd > 0 {
            math::checked_add(position.collateral_usd, profit_usd)?
        } else if loss_usd < position.collateral_usd {
            math::checked_sub(position.collateral_usd, loss_usd)?
        } else {
            0
        };

        let max_collateral_price = if collateral_token_price > collateral_token_ema_price {
            collateral_token_price
        } else {
            collateral_token_ema_price
        };
        let close_amount = max_collateral_price
            .get_token_amount(available_amount_usd, collateral_custody.decimals)?;
        let max_amount = math::checked_add(
            position.locked_amount.saturating_sub(fee_amount),
            position.collateral_amount,
        )?;

        Ok((
            std::cmp::min(max_amount, close_amount),
            fee_amount,
            profit_usd,
            loss_usd,
        ))
    }

    /// Calculate swap price between two tokens
    /// 
    /// Uses minimum input price and maximum output price, then applies swap spread.
    /// This ensures the pool gets favorable pricing.
    /// 
    /// # Arguments
    /// * `token_in_price` - Spot price for input token
    /// * `token_in_ema_price` - EMA price for input token
    /// * `token_out_price` - Spot price for output token
    /// * `token_out_ema_price` - EMA price for output token
    /// * `custody_in` - Custody account for input token
    /// 
    /// # Returns
    /// Swap price as OraclePrice (output tokens per input token)
    pub fn get_swap_price(
        &self,
        token_in_price: &OraclePrice,
        token_in_ema_price: &OraclePrice,
        token_out_price: &OraclePrice,
        token_out_ema_price: &OraclePrice,
        custody_in: &Custody,
    ) -> Result<OraclePrice> {
        let min_price = if token_in_price < token_in_ema_price {
            token_in_price
        } else {
            token_in_ema_price
        };

        let max_price = if token_out_price > token_out_ema_price {
            token_out_price
        } else {
            token_out_ema_price
        };

        let pair_price = min_price.checked_div(max_price)?;

        self.get_price(
            &pair_price,
            &pair_price,
            Side::Short,
            custody_in.pricing.swap_spread,
        )
    }

    /// Calculate output amount for a token swap
    /// 
    /// # Arguments
    /// * `token_in_price` - Spot price for input token
    /// * `token_in_ema_price` - EMA price for input token
    /// * `token_out_price` - Spot price for output token
    /// * `token_out_ema_price` - EMA price for output token
    /// * `custody_in` - Custody account for input token
    /// * `custody_out` - Custody account for output token
    /// * `amount_in` - Input amount in input token's native decimals
    /// 
    /// # Returns
    /// Output amount in output token's native decimals
    #[allow(clippy::too_many_arguments)]
    pub fn get_swap_amount(
        &self,
        token_in_price: &OraclePrice,
        token_in_ema_price: &OraclePrice,
        token_out_price: &OraclePrice,
        token_out_ema_price: &OraclePrice,
        custody_in: &Custody,
        custody_out: &Custody,
        amount_in: u64,
    ) -> Result<u64> {
        let swap_price = self.get_swap_price(
            token_in_price,
            token_in_ema_price,
            token_out_price,
            token_out_ema_price,
            custody_in,
        )?;

        math::checked_decimal_mul(
            amount_in,
            -(custody_in.decimals as i32),
            swap_price.price,
            swap_price.exponent,
            -(custody_out.decimals as i32),
        )
    }

    /// Calculate swap fees for both input and output tokens
    /// 
    /// Uses different fee rates for stablecoin swaps vs regular swaps.
    /// 
    /// # Arguments
    /// * `token_id_in` - Token ID for input token
    /// * `token_id_out` - Token ID for output token
    /// * `amount_in` - Input amount
    /// * `amount_out` - Output amount
    /// * `custody_in` - Custody account for input token
    /// * `token_price_in` - Price for input token
    /// * `custody_out` - Custody account for output token
    /// * `token_price_out` - Price for output token
    /// 
    /// # Returns
    /// Tuple of (fee_in, fee_out) in respective token amounts
    #[allow(clippy::too_many_arguments)]
    pub fn get_swap_fees(
        &self,
        token_id_in: usize,
        token_id_out: usize,
        amount_in: u64,
        amount_out: u64,
        custody_in: &Custody,
        token_price_in: &OraclePrice,
        custody_out: &Custody,
        token_price_out: &OraclePrice,
    ) -> Result<(u64, u64)> {
        let stable_swap = custody_in.is_stable && custody_out.is_stable;

        let swap_in_fee = self.get_fee(
            token_id_in,
            if stable_swap {
                custody_in.fees.stable_swap_in
            } else {
                custody_in.fees.swap_in
            },
            amount_in,
            0u64,
            custody_in,
            token_price_in,
        )?;

        let swap_out_fee = self.get_fee(
            token_id_out,
            if stable_swap {
                custody_out.fees.stable_swap_out
            } else {
                custody_out.fees.swap_out
            },
            0u64,
            amount_out,
            custody_out,
            token_price_out,
        )?;

        Ok((swap_in_fee, swap_out_fee))
    }

    /// Calculate fee for adding liquidity
    /// 
    /// # Arguments
    /// * `token_id` - Token ID being added
    /// * `amount` - Amount of tokens being added
    /// * `custody` - Custody account for the token
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// Fee amount in tokens
    pub fn get_add_liquidity_fee(
        &self,
        token_id: usize,
        amount: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        self.get_fee(
            token_id,
            custody.fees.add_liquidity,
            amount,
            0u64,
            custody,
            token_price,
        )
    }

    /// Calculate fee for removing liquidity
    /// 
    /// # Arguments
    /// * `token_id` - Token ID being removed
    /// * `amount` - Amount of tokens being removed
    /// * `custody` - Custody account for the token
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// Fee amount in tokens
    pub fn get_remove_liquidity_fee(
        &self,
        token_id: usize,
        amount: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        self.get_fee(
            token_id,
            custody.fees.remove_liquidity,
            0u64,
            amount,
            custody,
            token_price,
        )
    }

    /// Calculate liquidation fee
    /// 
    /// # Arguments
    /// * `size` - Position size in tokens
    /// * `custody` - Custody account for the token
    /// 
    /// # Returns
    /// Liquidation fee amount in tokens
    pub fn get_liquidation_fee(&self, size: u64, custody: &Custody) -> Result<u64> {
        Self::get_fee_amount(custody.fees.liquidation, size)
    }

    /// Check if a liquidity operation maintains valid token ratio
    /// 
    /// Allows operations that improve ratio even if they temporarily go outside bounds,
    /// as long as the new ratio is better than current ratio.
    /// 
    /// # Arguments
    /// * `token_id` - Token ID being modified
    /// * `amount_add` - Amount being added (0 if removing)
    /// * `amount_remove` - Amount being removed (0 if adding)
    /// * `custody` - Custody account for the token
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// true if ratio constraints are satisfied
    pub fn check_token_ratio(
        &self,
        token_id: usize,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<bool> {
        let new_ratio = self.get_new_ratio(amount_add, amount_remove, custody, token_price)?;

        if new_ratio < self.ratios[token_id].min {
            Ok(new_ratio >= self.get_current_ratio(custody, token_price)?)
        } else if new_ratio > self.ratios[token_id].max {
            Ok(new_ratio <= self.get_current_ratio(custody, token_price)?)
        } else {
            Ok(true)
        }
    }

    /// Check if sufficient tokens are available for withdrawal
    /// 
    /// Available = owned + collateral - locked
    /// 
    /// # Arguments
    /// * `amount` - Amount requested
    /// * `custody` - Custody account to check
    /// 
    /// # Returns
    /// true if amount is available
    pub fn check_available_amount(&self, amount: u64, custody: &Custody) -> Result<bool> {
        let available_amount = math::checked_sub(
            math::checked_add(custody.assets.owned, custody.assets.collateral)?,
            custody.assets.locked,
        )?;
        Ok(available_amount >= amount)
    }

    /// Calculate current leverage for a position
    /// 
    /// Leverage = size_usd / current_margin_usd
    /// where current_margin includes unrealized PnL
    /// 
    /// # Arguments
    /// * `position` - Position to calculate leverage for
    /// * `token_price` - Current spot price for position token
    /// * `token_ema_price` - EMA price for position token
    /// * `custody` - Custody account for position token
    /// * `collateral_token_price` - Current spot price for collateral
    /// * `collateral_token_ema_price` - EMA price for collateral
    /// * `collateral_custody` - Custody account for collateral
    /// * `curtime` - Current timestamp
    /// 
    /// # Returns
    /// Leverage in BPS (e.g., 40000 = 4x leverage)
    #[allow(clippy::too_many_arguments)]
    pub fn get_leverage(
        &self,
        position: &Position,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        custody: &Custody,
        collateral_token_price: &OraclePrice,
        collateral_token_ema_price: &OraclePrice,
        collateral_custody: &Custody,
        curtime: i64,
    ) -> Result<u64> {
        let (profit_usd, loss_usd, _) = self.get_pnl_usd(
            position,
            token_price,
            token_ema_price,
            custody,
            collateral_token_price,
            collateral_token_ema_price,
            collateral_custody,
            curtime,
            false,
        )?;

        let current_margin_usd = if profit_usd > 0 {
            math::checked_add(position.collateral_usd, profit_usd)?
        } else if loss_usd <= position.collateral_usd {
            math::checked_sub(position.collateral_usd, loss_usd)?
        } else {
            0
        };

        if current_margin_usd > 0 {
            math::checked_as_u64(math::checked_div(
                math::checked_mul(position.size_usd as u128, Perpetuals::BPS_POWER)?,
                current_margin_usd as u128,
            )?)
        } else {
            Ok(u64::MAX)
        }
    }

    /// Check if position leverage is within allowed limits
    /// 
    /// For initial positions, also checks min/max initial leverage constraints.
    /// 
    /// # Arguments
    /// * `position` - Position to check
    /// * `token_price` - Current spot price for position token
    /// * `token_ema_price` - EMA price for position token
    /// * `custody` - Custody account for position token
    /// * `collateral_token_price` - Current spot price for collateral
    /// * `collateral_token_ema_price` - EMA price for collateral
    /// * `collateral_custody` - Custody account for collateral
    /// * `curtime` - Current timestamp
    /// * `initial` - Whether this is a new position (affects leverage constraints)
    /// 
    /// # Returns
    /// true if leverage is within allowed limits
    #[allow(clippy::too_many_arguments)]
    pub fn check_leverage(
        &self,
        position: &Position,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        custody: &Custody,
        collateral_token_price: &OraclePrice,
        collateral_token_ema_price: &OraclePrice,
        collateral_custody: &Custody,
        curtime: i64,
        initial: bool,
    ) -> Result<bool> {
        let current_leverage = self.get_leverage(
            position,
            token_price,
            token_ema_price,
            custody,
            collateral_token_price,
            collateral_token_ema_price,
            collateral_custody,
            curtime,
        )?;

        Ok(current_leverage <= custody.pricing.max_leverage
            && (!initial
                || (current_leverage >= custody.pricing.min_initial_leverage
                    && current_leverage <= custody.pricing.max_initial_leverage)))
    }

    /// Calculate liquidation price for a position
    /// 
    /// Liquidation occurs when:
    /// margin + unrealized_profit - unrealized_loss - exit_fee - interest - size/max_leverage <= 0
    /// 
    /// Formula:
    /// liq_price = pos_price ± (margin - size/max_leverage - exit_fee - interest) * pos_price / size
    /// 
    /// # Arguments
    /// * `position` - Position to calculate liquidation price for
    /// * `token_ema_price` - EMA price for position token
    /// * `custody` - Custody account for position token
    /// * `collateral_custody` - Custody account for collateral
    /// * `curtime` - Current timestamp
    /// 
    /// # Returns
    /// Liquidation price scaled to PRICE_DECIMALS (0 if already liquidated)
    pub fn get_liquidation_price(
        &self,
        position: &Position,
        token_ema_price: &OraclePrice,
        custody: &Custody,
        collateral_custody: &Custody,
        curtime: i64,
    ) -> Result<u64> {

        if position.size_usd == 0 || position.price == 0 {
            return Ok(0);
        }

        let size = token_ema_price.get_token_amount(position.size_usd, custody.decimals)?;
        let exit_fee_tokens = self.get_exit_fee(size, custody)?;
        let exit_fee_usd =
            token_ema_price.get_asset_amount_usd(exit_fee_tokens, custody.decimals)?;
        let interest_usd = collateral_custody.get_interest_amount_usd(position, curtime)?;
        let unrealized_loss_usd = math::checked_add(
            math::checked_add(exit_fee_usd, interest_usd)?,
            position.unrealized_loss_usd,
        )?;

        let max_loss_usd = math::checked_as_u64(math::checked_div(
            math::checked_mul(position.size_usd as u128, Perpetuals::BPS_POWER)?,
            custody.pricing.max_leverage as u128,
        )?)?;
        let max_loss_usd = math::checked_add(max_loss_usd, unrealized_loss_usd)?;

        let margin_usd =
            math::checked_add(position.collateral_usd, position.unrealized_profit_usd)?;

        let max_price_diff = if max_loss_usd >= margin_usd {
            math::checked_sub(max_loss_usd, margin_usd)?
        } else {
            math::checked_sub(margin_usd, max_loss_usd)?
        };

        let position_price = math::scale_to_exponent(
            position.price,
            -(Perpetuals::PRICE_DECIMALS as i32),
            -(Perpetuals::USD_DECIMALS as i32),
        )?;

        let max_price_diff = math::checked_as_u64(math::checked_div(
            math::checked_mul(max_price_diff as u128, position_price as u128)?,
            position.size_usd as u128,
        )?)?;

        let max_price_diff = math::scale_to_exponent(
            max_price_diff,
            -(Perpetuals::USD_DECIMALS as i32),
            -(Perpetuals::PRICE_DECIMALS as i32),
        )?;

        if position.side == Side::Long {
            if max_loss_usd >= margin_usd {
                math::checked_add(position.price, max_price_diff)
            } else if position.price > max_price_diff {
                math::checked_sub(position.price, max_price_diff)
            } else {
                Ok(0)
            }
        } else if max_loss_usd >= margin_usd {
            if position.price > max_price_diff {
                math::checked_sub(position.price, max_price_diff)
            } else {
                Ok(0)
            }
        } else {
            math::checked_add(position.price, max_price_diff)
        }
    }

    /// Calculate profit and loss for a position in USD
    /// 
    /// Accounts for:
    /// - Price difference from entry price
    /// - Unrealized profit/loss already accrued
    /// - Exit fees
    /// - Interest accrued
    /// - Collateral price changes (for profit calculation)
    /// 
    /// # Arguments
    /// * `position` - Position to calculate PnL for
    /// * `token_price` - Current spot price for position token
    /// * `token_ema_price` - EMA price for position token
    /// * `custody` - Custody account for position token
    /// * `collateral_token_price` - Current spot price for collateral
    /// * `collateral_token_ema_price` - EMA price for collateral
    /// * `collateral_custody` - Custody account for collateral
    /// * `curtime` - Current timestamp
    /// * `liquidation` - Whether this is a liquidation (affects fee)
    /// 
    /// # Returns
    /// Tuple of (profit_usd, loss_usd, fee_amount)
    #[allow(clippy::too_many_arguments)]
    pub fn get_pnl_usd(
        &self,
        position: &Position,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        custody: &Custody,
        collateral_token_price: &OraclePrice,
        collateral_token_ema_price: &OraclePrice,
        collateral_custody: &Custody,
        curtime: i64,
        liquidation: bool,
    ) -> Result<(u64, u64, u64)> {
        if position.size_usd == 0 || position.price == 0 {
            return Ok((0, 0, 0));
        }

        let exit_price =
            self.get_exit_price(token_price, token_ema_price, position.side, custody)?;

        let size = token_ema_price.get_token_amount(position.size_usd, custody.decimals)?;

        let exit_fee = if liquidation {
            self.get_liquidation_fee(size, custody)?
        } else {
            self.get_exit_fee(size, custody)?
        };

        let exit_fee_usd = token_ema_price.get_asset_amount_usd(exit_fee, custody.decimals)?;
        let interest_usd = collateral_custody.get_interest_amount_usd(position, curtime)?;
        let unrealized_loss_usd = math::checked_add(
            math::checked_add(exit_fee_usd, interest_usd)?,
            position.unrealized_loss_usd,
        )?;

        let (price_diff_profit, price_diff_loss) = if position.side == Side::Long {
            if exit_price > position.price {
                (math::checked_sub(exit_price, position.price)?, 0u64)
            } else {
                (0u64, math::checked_sub(position.price, exit_price)?)
            }
        } else if exit_price < position.price {
            (math::checked_sub(position.price, exit_price)?, 0u64)
        } else {
            (0u64, math::checked_sub(exit_price, position.price)?)
        };

        let position_price = math::scale_to_exponent(
            position.price,
            -(Perpetuals::PRICE_DECIMALS as i32),
            -(Perpetuals::USD_DECIMALS as i32),
        )?;

        if price_diff_profit > 0 {
            let potential_profit_usd = math::checked_as_u64(math::checked_div(
                math::checked_mul(position.size_usd as u128, price_diff_profit as u128)?,
                position_price as u128,
            )?)?;

            let potential_profit_usd =
                math::checked_add(potential_profit_usd, position.unrealized_profit_usd)?;

            if potential_profit_usd >= unrealized_loss_usd {
                let cur_profit_usd = math::checked_sub(potential_profit_usd, unrealized_loss_usd)?;
                let min_collateral_price = if collateral_custody.is_virtual {
                    // if collateral_custody is virtual it means this function is called from get_assets_under_management_usd()
                    // (to calculate unrealized pnl of all open positions) and actual collateral custody is a stablecoin.
                    // we need to use 1USD reference price for such positions
                    OraclePrice {
                        price: 10u64.pow(Perpetuals::USD_DECIMALS as u32),
                        exponent: -(Perpetuals::USD_DECIMALS as i32),
                    }
                } else {
                    collateral_token_price
                        .get_min_price(collateral_token_ema_price, collateral_custody.is_stable)?
                };
                let max_profit_usd = if curtime <= position.open_time {
                    0
                } else {
                    min_collateral_price
                        .get_asset_amount_usd(position.locked_amount, collateral_custody.decimals)?
                };
                Ok((
                    std::cmp::min(max_profit_usd, cur_profit_usd),
                    0u64,
                    exit_fee,
                ))
            } else {
                Ok((
                    0u64,
                    math::checked_sub(unrealized_loss_usd, potential_profit_usd)?,
                    exit_fee,
                ))
            }
        } else {
            let potential_loss_usd = math::checked_as_u64(math::checked_ceil_div(
                math::checked_mul(position.size_usd as u128, price_diff_loss as u128)?,
                position_price as u128,
            )?)?;

            let potential_loss_usd = math::checked_add(potential_loss_usd, unrealized_loss_usd)?;

            if potential_loss_usd >= position.unrealized_profit_usd {
                Ok((
                    0u64,
                    math::checked_sub(potential_loss_usd, position.unrealized_profit_usd)?,
                    exit_fee,
                ))
            } else {
                let cur_profit_usd =
                    math::checked_sub(position.unrealized_profit_usd, potential_loss_usd)?;
                let min_collateral_price = if collateral_custody.is_virtual {
                    OraclePrice {
                        price: 10u64.pow(Perpetuals::USD_DECIMALS as u32),
                        exponent: -(Perpetuals::USD_DECIMALS as i32),
                    }
                } else {
                    collateral_token_price
                        .get_min_price(collateral_token_ema_price, collateral_custody.is_stable)?
                };
                let max_profit_usd = if curtime <= position.open_time {
                    0
                } else {
                    min_collateral_price
                        .get_asset_amount_usd(position.locked_amount, collateral_custody.decimals)?
                };
                Ok((
                    std::cmp::min(max_profit_usd, cur_profit_usd),
                    0u64,
                    exit_fee,
                ))
            }
        }
    }

    /// Calculate total Assets Under Management (AUM) in USD
    /// 
    /// Sums up all token values in the pool, optionally including unrealized PnL.
    /// 
    /// # Arguments
    /// * `aum_calc_mode` - Which price to use (Min/Max/Last/EMA)
    /// * `accounts` - Account infos array: [custody0, custody1, ..., oracle0, oracle1, ...]
    /// * `curtime` - Current timestamp
    /// 
    /// # Returns
    /// Total AUM in USD (scaled to USD_DECIMALS)
    pub fn get_assets_under_management_usd<'a>(
        &self,
        aum_calc_mode: AumCalcMode,
        accounts: &'a [AccountInfo<'a>],
        curtime: i64,
    ) -> Result<u128> {
        let mut pool_amount_usd: u128 = 0;
        for (idx, &custody) in self.custodies.iter().enumerate() {
            let oracle_idx = idx + self.custodies.len();
            if oracle_idx >= accounts.len() {
                return Err(PerpetualsError::UnsupportedOracle.into());
            }

            require_keys_eq!(accounts[idx].key(), custody);
            let custody = Account::<Custody>::try_from(&accounts[idx])?;

            require_keys_eq!(accounts[oracle_idx].key(), custody.oracle.oracle_account);

            let token_price = OraclePrice::new_from_oracle(
                &accounts[oracle_idx],
                &custody.oracle,
                curtime,
                false,
            )?;

            let token_ema_price = OraclePrice::new_from_oracle(
                &accounts[oracle_idx],
                &custody.oracle,
                curtime,
                custody.pricing.use_ema,
            )?;

            let aum_token_price = match aum_calc_mode {
                AumCalcMode::Last => token_price,
                AumCalcMode::EMA => token_ema_price,
                AumCalcMode::Min => {
                    if token_price < token_ema_price {
                        token_price
                    } else {
                        token_ema_price
                    }
                }
                AumCalcMode::Max => {
                    if token_price > token_ema_price {
                        token_price
                    } else {
                        token_ema_price
                    }
                }
            };

            let token_amount_usd =
                aum_token_price.get_asset_amount_usd(custody.assets.owned, custody.decimals)?;

            pool_amount_usd = math::checked_add(pool_amount_usd, token_amount_usd as u128)?;

            if custody.pricing.use_unrealized_pnl_in_aum {
                if custody.is_stable {
                    // compute accumulated interest
                    let collective_position = custody.get_collective_position(Side::Long)?;
                    let interest_usd =
                        custody.get_interest_amount_usd(&collective_position, curtime)?;
                    pool_amount_usd = math::checked_add(pool_amount_usd, interest_usd as u128)?;

                    let collective_position = custody.get_collective_position(Side::Short)?;
                    let interest_usd =
                        custody.get_interest_amount_usd(&collective_position, curtime)?;
                    pool_amount_usd = math::checked_add(pool_amount_usd, interest_usd as u128)?;
                } else {
                    // compute aggregate unrealized pnl
                    let (long_profit, long_loss, _) = self.get_pnl_usd(
                        &custody.get_collective_position(Side::Long)?,
                        &token_price,
                        &token_ema_price,
                        &custody,
                        &token_price,
                        &token_ema_price,
                        &custody,
                        curtime,
                        false,
                    )?;
                    let (short_profit, short_loss, _) = self.get_pnl_usd(
                        &custody.get_collective_position(Side::Short)?,
                        &token_price,
                        &token_ema_price,
                        &custody,
                        &token_price,
                        &token_ema_price,
                        &custody,
                        curtime,
                        false,
                    )?;

                    // adjust pool amount by collective profit/loss
                    pool_amount_usd = math::checked_add(pool_amount_usd, long_loss as u128)?;
                    pool_amount_usd = math::checked_add(pool_amount_usd, short_loss as u128)?;
                    pool_amount_usd = pool_amount_usd.saturating_sub(long_profit as u128);
                    pool_amount_usd = pool_amount_usd.saturating_sub(short_profit as u128);
                }
            }
        }

        Ok(pool_amount_usd)
    }

    /// Calculate fee amount from fee rate and amount
    /// 
    /// Uses ceiling division to ensure fees round up.
    /// 
    /// # Arguments
    /// * `fee` - Fee rate in BPS (basis points)
    /// * `amount` - Amount to calculate fee for
    /// 
    /// # Returns
    /// Fee amount (0 if fee or amount is 0)
    pub fn get_fee_amount(fee: u64, amount: u64) -> Result<u64> {
        if fee == 0 || amount == 0 {
            return Ok(0);
        }
        math::checked_as_u64(math::checked_ceil_div(
            math::checked_mul(amount as u128, fee as u128)?,
            Perpetuals::BPS_POWER,
        )?)
    }

    // ========== Private Helper Functions ==========
    
    /// Get current token ratio in the pool
    /// 
    /// # Arguments
    /// * `custody` - Custody account for the token
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// Current ratio in BPS (0 if AUM is 0 or token is virtual)
    fn get_current_ratio(&self, custody: &Custody, token_price: &OraclePrice) -> Result<u64> {
        if self.aum_usd == 0 || custody.is_virtual {
            return Ok(0);
        }
        let ratio = math::checked_as_u64(math::checked_div(
            math::checked_mul(
                token_price.get_asset_amount_usd(custody.assets.owned, custody.decimals)? as u128,
                Perpetuals::BPS_POWER,
            )?,
            self.aum_usd,
        )?)?;
        Ok(std::cmp::min(ratio, Perpetuals::BPS_POWER as u64))
    }

    /// Calculate new token ratio after adding/removing liquidity
    /// 
    /// # Arguments
    /// * `amount_add` - Amount being added (0 if removing)
    /// * `amount_remove` - Amount being removed (0 if adding)
    /// * `custody` - Custody account for the token
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// New ratio in BPS (0 if pool would be empty or token is virtual)
    fn get_new_ratio(
        &self,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        if custody.is_virtual {
            return Ok(0);
        }
        let (new_token_aum_usd, new_pool_aum_usd) = if amount_add > 0 && amount_remove > 0 {
            return Err(PerpetualsError::InvalidPositionState.into());
        } else if amount_add == 0 && amount_remove == 0 {
            (
                token_price.get_asset_amount_usd(custody.assets.owned, custody.decimals)? as u128,
                self.aum_usd,
            )
        } else if amount_add > 0 {
            let added_aum_usd =
                token_price.get_asset_amount_usd(amount_add, custody.decimals)? as u128;

            (
                token_price.get_asset_amount_usd(
                    math::checked_add(custody.assets.owned, amount_add)?,
                    custody.decimals,
                )? as u128,
                math::checked_add(self.aum_usd, added_aum_usd)?,
            )
        } else {
            let removed_aum_usd =
                token_price.get_asset_amount_usd(amount_remove, custody.decimals)? as u128;

            if removed_aum_usd >= self.aum_usd || amount_remove >= custody.assets.owned {
                (0, 0)
            } else {
                (
                    token_price.get_asset_amount_usd(
                        math::checked_sub(custody.assets.owned, amount_remove)?,
                        custody.decimals,
                    )? as u128,
                    math::checked_sub(self.aum_usd, removed_aum_usd)?,
                )
            }
        };
        if new_token_aum_usd == 0 || new_pool_aum_usd == 0 {
            return Ok(0);
        }

        let ratio = math::checked_as_u64(math::checked_div(
            math::checked_mul(new_token_aum_usd, Perpetuals::BPS_POWER)?,
            new_pool_aum_usd,
        )?)?;
        Ok(std::cmp::min(ratio, Perpetuals::BPS_POWER as u64))
    }

    /// Apply spread to price based on trade side
    /// 
    /// For longs: uses max(spot, EMA) and adds spread
    /// For shorts: uses min(spot, EMA) and subtracts spread
    /// 
    /// # Arguments
    /// * `token_price` - Current spot price
    /// * `token_ema_price` - Current EMA price
    /// * `side` - Trade side (Long or Short)
    /// * `spread` - Spread in BPS
    /// 
    /// # Returns
    /// Price with spread applied
    fn get_price(
        &self,
        token_price: &OraclePrice,
        token_ema_price: &OraclePrice,
        side: Side,
        spread: u64,
    ) -> Result<OraclePrice> {
        if side == Side::Long {
            let max_price = if token_price > token_ema_price {
                token_price
            } else {
                token_ema_price
            };

            Ok(OraclePrice {
                price: math::checked_add(
                    max_price.price,
                    math::checked_decimal_ceil_mul(
                        max_price.price,
                        max_price.exponent,
                        spread,
                        -(Perpetuals::BPS_DECIMALS as i32),
                        max_price.exponent,
                    )?,
                )?,
                exponent: max_price.exponent,
            })
        } else {
            let min_price = if token_price < token_ema_price {
                token_price
            } else {
                token_ema_price
            };

            let spread = math::checked_decimal_mul(
                min_price.price,
                min_price.exponent,
                spread,
                -(Perpetuals::BPS_DECIMALS as i32),
                min_price.exponent,
            )?;

            let price = if spread < min_price.price {
                math::checked_sub(min_price.price, spread)?
            } else {
                0
            };

            Ok(OraclePrice {
                price,
                exponent: min_price.exponent,
            })
        }
    }

    /// Calculate fee based on fee mode
    /// 
    /// Routes to appropriate fee calculation:
    /// - Fixed: simple percentage fee
    /// - Linear: fee varies linearly with ratio deviation
    /// - Optimal: fee varies optimally with ratio deviation
    /// 
    /// # Arguments
    /// * `token_id` - Token ID
    /// * `base_fee` - Base fee rate in BPS
    /// * `amount_add` - Amount being added
    /// * `amount_remove` - Amount being removed
    /// * `custody` - Custody account
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// Fee amount in tokens
    fn get_fee(
        &self,
        token_id: usize,
        base_fee: u64,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        require!(!custody.is_virtual, PerpetualsError::InstructionNotAllowed);

        if custody.fees.mode == FeesMode::Fixed {
            Self::get_fee_amount(base_fee, std::cmp::max(amount_add, amount_remove))
        } else if custody.fees.mode == FeesMode::Linear {
            self.get_fee_linear(
                token_id,
                base_fee,
                amount_add,
                amount_remove,
                custody,
                token_price,
            )
        } else {
            self.get_fee_optimal(
                token_id,
                base_fee,
                amount_add,
                amount_remove,
                custody,
                token_price,
            )
        }
    }

    /// Calculate fee using linear fee model
    /// 
    /// Fee adjusts based on how much the operation improves or worsens token ratio.
    /// 
    /// Algorithm:
    /// - If ratio improves: fee = base_fee / ratio_fee (lower fee)
    /// - If ratio worsens: fee = base_fee * ratio_fee (higher fee)
    /// 
    /// Ratio fee calculation:
    /// - If new_ratio < target: ratio_fee = 1 + ratio_mult * (target - new_ratio) / (target - min)
    /// - If new_ratio > target: ratio_fee = 1 + ratio_mult * (new_ratio - target) / (max - target)
    /// 
    /// # Arguments
    /// * `token_id` - Token ID
    /// * `base_fee` - Base fee rate in BPS
    /// * `amount_add` - Amount being added
    /// * `amount_remove` - Amount being removed
    /// * `custody` - Custody account
    /// * `token_price` - Current token price
    /// 
    /// # Returns
    /// Fee amount in tokens
    fn get_fee_linear(
        &self,
        token_id: usize,
        base_fee: u64,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {

        let ratios = &self.ratios[token_id];
        let current_ratio = self.get_current_ratio(custody, token_price)?;
        let new_ratio = self.get_new_ratio(amount_add, amount_remove, custody, token_price)?;

        let improved = match new_ratio.cmp(&ratios.target) {
            Ordering::Less => {
                new_ratio > current_ratio
                    || (current_ratio > ratios.target
                        && current_ratio - ratios.target > ratios.target - new_ratio)
            }
            Ordering::Greater => {
                new_ratio < current_ratio
                    || (current_ratio < ratios.target
                        && ratios.target - current_ratio > new_ratio - ratios.target)
            }
            Ordering::Equal => current_ratio != ratios.target,
        };

        let ratio_fee = if new_ratio <= ratios.target {
            if ratios.target == ratios.min {
                Perpetuals::BPS_POWER
            } else {
                math::checked_add(
                    Perpetuals::BPS_POWER,
                    math::checked_div(
                        math::checked_mul(
                            custody.fees.ratio_mult as u128,
                            math::checked_sub(ratios.target, new_ratio)? as u128,
                        )?,
                        math::checked_sub(ratios.target, ratios.min)? as u128,
                    )?,
                )?
            }
        } else if ratios.target == ratios.max {
            Perpetuals::BPS_POWER
        } else {
            math::checked_add(
                Perpetuals::BPS_POWER,
                math::checked_div(
                    math::checked_mul(
                        custody.fees.ratio_mult as u128,
                        math::checked_sub(new_ratio, ratios.target)? as u128,
                    )?,
                    math::checked_sub(ratios.max, ratios.target)? as u128,
                )?,
            )?
        };

        let fee = if improved {
            math::checked_div(
                math::checked_mul(base_fee as u128, Perpetuals::BPS_POWER)?,
                ratio_fee,
            )?
        } else {
            math::checked_div(
                math::checked_mul(base_fee as u128, ratio_fee)?,
                Perpetuals::BPS_POWER,
            )?
        };

        Self::get_fee_amount(
            math::checked_as_u64(fee)?,
            std::cmp::max(amount_add, amount_remove),
        )
    }

    fn get_fee_optimal(
        &self,
        token_id: usize,
        base_fee: u64,
        amount_add: u64,
        amount_remove: u64,
        custody: &Custody,
        token_price: &OraclePrice,
    ) -> Result<u64> {
        // Fee calculations must temporarily be in i64 because of negative slope.
        let fee_max: i64 = custody.fees.fee_max as i64;
        let fee_optimal: i64 = custody.fees.fee_optimal as i64;

        let target_ratio: i64 = self.ratios[token_id].target as i64;
        let min_ratio: i64 = self.ratios[token_id].min as i64;
        let max_ratio: i64 = self.ratios[token_id].max as i64;
        let post_lp_ratio: i64 =
            self.get_new_ratio(amount_add, amount_remove, custody, token_price)? as i64;

        let base_fee: i64 = base_fee as i64;

        let slope_denominator: i64 = if post_lp_ratio > target_ratio {
            math::checked_sub(max_ratio, target_ratio)?
        } else {
            math::checked_sub(target_ratio, min_ratio)?
        };

        let slope_numerator: i64 = if amount_add != 0 {
            if post_lp_ratio > max_ratio {
                return err!(PerpetualsError::TokenRatioOutOfRange);
            }
            fee_max - fee_optimal
        } else {
            if post_lp_ratio < min_ratio {
                return err!(PerpetualsError::TokenRatioOutOfRange);
            }
            fee_optimal - fee_max
        };

        // Delay applying slope_denominator until the very end to avoid losing precision.
        // b = fee_optimal - target_ratio * slope
        // lp_fee = slope * post_lp_ratio + b
        let b: i64 = math::checked_sub(
            math::checked_mul(fee_optimal, slope_denominator)?,
            math::checked_mul(target_ratio, slope_numerator)?,
        )?;
        let lp_fee: i64 = math::checked_div(
            math::checked_add(math::checked_mul(slope_numerator, post_lp_ratio)?, b)?,
            slope_denominator,
        )?;

        Self::get_fee_amount(
            math::checked_as_u64(math::checked_add(lp_fee, base_fee)?)?,
            std::cmp::max(amount_add, amount_remove),
        )
    }
}

#[cfg(test)]
mod test {
    use {
        super::*,
        crate::state::{
            custody::{BorrowRateParams, Fees, PricingParams},
            oracle::{OracleParams, OracleType},
            perpetuals::Permissions,
        },
    };

    fn get_fixture() -> (Pool, Custody, Position, OraclePrice, OraclePrice) {
        let ratios = TokenRatios {
            target: 5_000,
            min: 1_000,
            max: 9_000,
        };

        let oracle = OracleParams {
            oracle_account: Pubkey::default(),
            oracle_type: OracleType::Custom,
            oracle_authority: Pubkey::default(),
            max_price_error: 100,
            max_price_age_sec: 1,
        };

        let pricing = PricingParams {
            use_ema: true,
            use_unrealized_pnl_in_aum: true,
            trade_spread_long: 100,
            trade_spread_short: 100,
            swap_spread: 300,
            min_initial_leverage: 10_000,
            max_initial_leverage: 100_000,
            max_leverage: 100_000,
            max_payoff_mult: 10_000,
            max_utilization: 0,
            max_position_locked_usd: 0,
            max_total_locked_usd: 0,
        };

        let permissions = Permissions {
            allow_swap: true,
            allow_add_liquidity: true,
            allow_remove_liquidity: true,
            allow_open_position: true,
            allow_close_position: true,
            allow_pnl_withdrawal: true,
            allow_collateral_withdrawal: true,
            allow_size_change: true,
        };

        let fees = Fees {
            mode: FeesMode::Linear,
            ratio_mult: 20_000,
            utilization_mult: 20_000,
            swap_in: 100,
            swap_out: 100,
            stable_swap_in: 100,
            stable_swap_out: 100,
            add_liquidity: 0,
            remove_liquidity: 0,
            open_position: 100,
            close_position: 0,
            liquidation: 50,
            protocol_share: 25,
            fee_max: 0,
            fee_optimal: 0,
        };

        let custody = Custody {
            token_account: Pubkey::default(),
            mint: Pubkey::default(),
            decimals: 9,
            oracle,
            pricing,
            permissions,
            fees,
            ..Custody::default()
        };

        let position = Position {
            side: Side::Long,
            price: scale(25_000, Perpetuals::PRICE_DECIMALS),
            // x4 leverage
            size_usd: scale(100_000, Perpetuals::USD_DECIMALS),
            borrow_size_usd: scale(100_000, Perpetuals::USD_DECIMALS),
            collateral_usd: scale(25_000, Perpetuals::USD_DECIMALS),
            locked_amount: scale(4, 9),
            collateral_amount: scale(1, 9),
            ..Position::default()
        };

        let token_price = OraclePrice {
            price: 25_000_000,
            exponent: -3,
        };
        let token_ema_price = OraclePrice {
            price: 25_300_000,
            exponent: -3,
        };

        (
            Pool {
                name: "Test Pool".to_string(),
                ratios: vec![ratios, ratios],
                ..Default::default()
            },
            custody,
            position,
            token_price,
            token_ema_price,
        )
    }

    fn scale(amount: u64, decimals: u8) -> u64 {
        math::checked_mul(amount, 10u64.pow(decimals as u32)).unwrap()
    }

    fn scale_f64(amount: f64, decimals: u8) -> u64 {
        math::checked_as_u64(
            math::checked_float_mul(amount, 10u64.pow(decimals as u32) as f64).unwrap(),
        )
        .unwrap()
    }
}