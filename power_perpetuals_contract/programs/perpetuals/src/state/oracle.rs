//! Oracle price feed integration for power perpetuals
//! 
//! This module handles price feeds from various oracle providers (Pyth, Custom)
//! and provides utilities for price normalization, conversion, and validation.

use {
    crate::{error::PerpetualsError, math, state::perpetuals::Perpetuals},
    anchor_lang::prelude::*,
    core::cmp::Ordering,
};

/// Scale factor for oracle exponent calculations
const ORACLE_EXPONENT_SCALE: i32 = -9;
/// Scale factor for oracle price calculations (1 billion)
const ORACLE_PRICE_SCALE: u64 = 1_000_000_000;
/// Maximum price value that can be stored (2^28 - 1)
const ORACLE_MAX_PRICE: u64 = (1 << 28) - 1;

/// Supported oracle types for price feeds
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Debug)]
pub enum OracleType {
    /// No oracle configured
    None,
    /// Custom oracle implementation
    Custom,
    /// Pyth Network oracle
    Pyth,
}

impl Default for OracleType {
    fn default() -> Self {
        Self::None
    }
}

/// Oracle price representation with mantissa and exponent
/// 
/// Price = price * 10^exponent
/// Example: price=12300, exponent=-3 represents 12.3
#[derive(Copy, Clone, Eq, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct OraclePrice {
    /// Price mantissa (the significant digits)
    pub price: u64,
    /// Price exponent (power of 10)
    pub exponent: i32,
}

/// Configuration parameters for oracle price feeds
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct OracleParams {
    /// Public key of the oracle account
    pub oracle_account: Pubkey,
    /// Type of oracle (Pyth, Custom, etc.)
    pub oracle_type: OracleType,
    /// The oracle_authority pubkey is allowed to sign permissionless off-chain price updates.
    pub oracle_authority: Pubkey,
    /// Maximum acceptable price error in basis points (BPS)
    pub max_price_error: u64,
    /// Maximum age of price data in seconds before considered stale
    pub max_price_age_sec: u32,
}

/// Custom oracle account structure for storing price data on-chain
#[account]
#[derive(Default, Debug)]
pub struct CustomOracle {
    /// Current price mantissa
    pub price: u64,
    /// Price exponent
    pub expo: i32,
    /// Price confidence interval (uncertainty)
    pub conf: u64,
    /// Exponential moving average (EMA) price
    pub ema: u64,
    /// Unix timestamp when price was last published
    pub publish_time: i64,
}

impl CustomOracle {
    /// Account size in bytes (8 byte discriminator + data)
    pub const LEN: usize = 8 + std::mem::size_of::<CustomOracle>();

    /// Update all oracle price fields
    pub fn set(&mut self, price: u64, expo: i32, conf: u64, ema: u64, publish_time: i64) {
        self.price = price;
        self.expo = expo;
        self.conf = conf;
        self.ema = ema;
        self.publish_time = publish_time;
    }
}

impl PartialOrd for OraclePrice {
    fn partial_cmp(&self, other: &OraclePrice) -> Option<Ordering> {
        let (lhs, rhs) = if self.exponent == other.exponent {
            (self.price, other.price)
        } else if self.exponent < other.exponent {
            if let Ok(scaled_price) = other.scale_to_exponent(self.exponent) {
                (self.price, scaled_price.price)
            } else {
                return None;
            }
        } else if let Ok(scaled_price) = self.scale_to_exponent(other.exponent) {
            (scaled_price.price, other.price)
        } else {
            return None;
        };
        lhs.partial_cmp(&rhs)
    }
}

#[allow(dead_code)]
impl OraclePrice {
    /// Create a new OraclePrice from price and exponent
    pub fn new(price: u64, exponent: i32) -> Self {
        Self { price, exponent }
    }

    /// Create OraclePrice from token amount and decimals
    /// 
    /// # Arguments
    /// * `amount_and_decimals` - Tuple of (token_amount, decimals)
    pub fn new_from_token(amount_and_decimals: (u64, u8)) -> Self {
        Self {
            price: amount_and_decimals.0,
            exponent: -(amount_and_decimals.1 as i32),
        }
    }

    /// Fetch price from oracle account based on oracle type
    /// 
    /// # Arguments
    /// * `oracle_account` - Account info of the oracle
    /// * `oracle_params` - Oracle configuration parameters
    /// * `current_time` - Current Unix timestamp
    /// * `use_ema` - Whether to use EMA (exponential moving average) price instead of spot price
    /// 
    /// # Returns
    /// OraclePrice if successful, error otherwise
    pub fn new_from_oracle(
        oracle_account: &AccountInfo,
        oracle_params: &OracleParams,
        current_time: i64,
        use_ema: bool,
    ) -> Result<Self> {
        match oracle_params.oracle_type {
            OracleType::Custom => Self::get_custom_price(
                oracle_account,
                oracle_params.max_price_error,
                oracle_params.max_price_age_sec,
                current_time,
                use_ema,
            ),
            OracleType::Pyth => Self::get_pyth_price(
                oracle_account,
                oracle_params.max_price_error,
                oracle_params.max_price_age_sec,
                current_time,
                use_ema,
            ),
            _ => err!(PerpetualsError::UnsupportedOracle),
        }
    }

    /// Converts token amount to USD value using oracle price
    /// 
    /// # Arguments
    /// * `token_amount` - Amount of tokens
    /// * `token_decimals` - Number of decimals for the token
    /// 
    /// # Returns
    /// USD value with Perpetuals::USD_DECIMALS decimals
    pub fn get_asset_amount_usd(&self, token_amount: u64, token_decimals: u8) -> Result<u64> {
        if token_amount == 0 || self.price == 0 {
            return Ok(0);
        }
        math::checked_decimal_mul(
            token_amount,
            -(token_decimals as i32),
            self.price,
            self.exponent,
            -(Perpetuals::USD_DECIMALS as i32),
        )
    }

    /// Converts USD amount to token amount using oracle price
    /// 
    /// # Arguments
    /// * `asset_amount_usd` - USD amount with Perpetuals::USD_DECIMALS decimals
    /// * `token_decimals` - Number of decimals for the token
    /// 
    /// # Returns
    /// Token amount
    pub fn get_token_amount(&self, asset_amount_usd: u64, token_decimals: u8) -> Result<u64> {
        if asset_amount_usd == 0 || self.price == 0 {
            return Ok(0);
        }
        math::checked_decimal_div(
            asset_amount_usd,
            -(Perpetuals::USD_DECIMALS as i32),
            self.price,
            self.exponent,
            -(token_decimals as i32),
        )
    }

    /// Normalizes price mantissa to be less than ORACLE_MAX_PRICE
    /// 
    /// Adjusts exponent accordingly to maintain the same value.
    /// This prevents overflow in calculations.
    /// 
    /// # Returns
    /// Normalized OraclePrice with same value but smaller mantissa
    pub fn normalize(&self) -> Result<OraclePrice> {
        let mut p = self.price;
        let mut e = self.exponent;

        while p > ORACLE_MAX_PRICE {
            p = math::checked_div(p, 10)?;
            e = math::checked_add(e, 1)?;
        }

        Ok(OraclePrice {
            price: p,
            exponent: e,
        })
    }

    /// Divide two oracle prices with overflow protection
    /// 
    /// # Returns
    /// Result of self / other
    pub fn checked_div(&self, other: &OraclePrice) -> Result<OraclePrice> {
        let base = self.normalize()?;
        let other = other.normalize()?;

        Ok(OraclePrice {
            price: math::checked_div(
                math::checked_mul(base.price, ORACLE_PRICE_SCALE)?,
                other.price,
            )?,
            exponent: math::checked_sub(
                math::checked_add(base.exponent, ORACLE_EXPONENT_SCALE)?,
                other.exponent,
            )?,
        })
    }

    /// Multiply two oracle prices with overflow protection
    /// 
    /// # Returns
    /// Result of self * other
    pub fn checked_mul(&self, other: &OraclePrice) -> Result<OraclePrice> {
        Ok(OraclePrice {
            price: math::checked_mul(self.price, other.price)?,
            exponent: math::checked_add(self.exponent, other.exponent)?,
        })
    }

    /// Scale price to a different exponent while maintaining the same value
    /// 
    /// # Arguments
    /// * `target_exponent` - Desired exponent
    /// 
    /// # Returns
    /// OraclePrice with same value but different exponent
    pub fn scale_to_exponent(&self, target_exponent: i32) -> Result<OraclePrice> {
        if target_exponent == self.exponent {
            return Ok(*self);
        }
        let delta = math::checked_sub(target_exponent, self.exponent)?;
        if delta > 0 {
            Ok(OraclePrice {
                price: math::checked_div(self.price, math::checked_pow(10, delta as usize)?)?,
                exponent: target_exponent,
            })
        } else {
            Ok(OraclePrice {
                price: math::checked_mul(self.price, math::checked_pow(10, (-delta) as usize)?)?,
                exponent: target_exponent,
            })
        }
    }

    /// Convert OraclePrice to f64 floating point representation
    /// 
    /// # Returns
    /// Price as f64 value
    pub fn checked_as_f64(&self) -> Result<f64> {
        math::checked_float_mul(
            math::checked_as_f64(self.price)?,
            math::checked_powi(10.0, self.exponent)?,
        )
    }

    /// Get the minimum price between two prices
    /// 
    /// For stablecoins, ensures price doesn't exceed 1 USD.
    /// 
    /// # Arguments
    /// * `other` - Other price to compare
    /// * `is_stable` - Whether this is a stablecoin (caps at 1.0)
    /// 
    /// # Returns
    /// Minimum price
    pub fn get_min_price(&self, other: &OraclePrice, is_stable: bool) -> Result<OraclePrice> {
        let min_price = if self < other { self } else { other };
        if is_stable {
            if min_price.exponent > 0 {
                if min_price.price == 0 {
                    return Ok(*min_price);
                } else {
                    return Ok(OraclePrice {
                        price: 1000000u64,
                        exponent: -6,
                    });
                }
            }
            let one_usd = math::checked_pow(10u64, (-min_price.exponent) as usize)?;
            if min_price.price > one_usd {
                Ok(OraclePrice {
                    price: one_usd,
                    exponent: min_price.exponent,
                })
            } else {
                Ok(*min_price)
            }
        } else {
            Ok(*min_price)
        }
    }

    // ========== Private Helper Functions ==========
    
    /// Fetch price from custom oracle account
    /// 
    /// Validates price freshness and confidence interval.
    /// 
    /// # Arguments
    /// * `custom_price_info` - Account info of custom oracle
    /// * `max_price_error` - Maximum acceptable price error (BPS)
    /// * `max_price_age_sec` - Maximum age before price is stale
    /// * `current_time` - Current Unix timestamp
    /// * `use_ema` - Use EMA price if true, spot price otherwise
    fn get_custom_price(
        custom_price_info: &AccountInfo,
        max_price_error: u64,
        max_price_age_sec: u32,
        current_time: i64,
        use_ema: bool,
    ) -> Result<OraclePrice> {
        require!(
            !Perpetuals::is_empty_account(custom_price_info)?,
            PerpetualsError::InvalidOracleAccount
        );

        let oracle_acc = Account::<CustomOracle>::try_from(custom_price_info)?;

        let last_update_age_sec = math::checked_sub(current_time, oracle_acc.publish_time)?;
        if last_update_age_sec > max_price_age_sec as i64 {
            msg!("Error: Custom oracle price is stale");
            return err!(PerpetualsError::StaleOraclePrice);
        }
        let price = if use_ema {
            oracle_acc.ema
        } else {
            oracle_acc.price
        };

        if price == 0
            || math::checked_div(
                math::checked_mul(oracle_acc.conf as u128, Perpetuals::BPS_POWER)?,
                price as u128,
            )? > max_price_error as u128
        {
            msg!("Error: Custom oracle price is out of bounds");
            return err!(PerpetualsError::InvalidOraclePrice);
        }

        Ok(OraclePrice {
            // price is i64 and > 0 per check above
            price,
            exponent: oracle_acc.expo,
        })
    }

    /// Fetch price from Pyth Network oracle
    /// 
    /// Validates price freshness and confidence interval.
    /// 
    /// # Arguments
    /// * `pyth_price_info` - Account info of Pyth price feed
    /// * `max_price_error` - Maximum acceptable price error (BPS)
    /// * `max_price_age_sec` - Maximum age before price is stale
    /// * `current_time` - Current Unix timestamp
    /// * `use_ema` - Use EMA price if true, spot price otherwise
    fn get_pyth_price(
        pyth_price_info: &AccountInfo,
        max_price_error: u64,
        max_price_age_sec: u32,
        current_time: i64,
        use_ema: bool,
    ) -> Result<OraclePrice> {
        require!(
            !Perpetuals::is_empty_account(pyth_price_info)?,
            PerpetualsError::InvalidOracleAccount
        );
        let price_feed = pyth_sdk_solana::load_price_feed_from_account_info(pyth_price_info)
            .map_err(|_| PerpetualsError::InvalidOracleAccount)?;
        let pyth_price = if use_ema {
            price_feed.get_ema_price_unchecked()
        } else {
            price_feed.get_price_unchecked()
        };

        let last_update_age_sec = math::checked_sub(current_time, pyth_price.publish_time)?;
        if last_update_age_sec > max_price_age_sec as i64 {
            msg!("Error: Pyth oracle price is stale");
            return err!(PerpetualsError::StaleOraclePrice);
        }

        if pyth_price.price <= 0
            || math::checked_div(
                math::checked_mul(pyth_price.conf as u128, Perpetuals::BPS_POWER)?,
                pyth_price.price as u128,
            )? > max_price_error as u128
        {
            msg!("Error: Pyth oracle price is out of bounds");
            return err!(PerpetualsError::InvalidOraclePrice);
        }

        Ok(OraclePrice {
            // price is i64 and > 0 per check above
            price: pyth_price.price as u64,
            exponent: pyth_price.expo,
        })
    }
}