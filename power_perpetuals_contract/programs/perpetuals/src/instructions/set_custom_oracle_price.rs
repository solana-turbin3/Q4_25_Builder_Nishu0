//! SetCustomOraclePrice instruction handler
//! 
//! This instruction allows admins to set or update custom oracle prices for a custody.
//! The oracle account is created if it doesn't exist (init_if_needed). This requires
//! multisig approval and is used for admin-controlled price feeds.

use {
    crate::state::{
        custody::Custody,
        multisig::{AdminInstruction, Multisig},
        oracle::CustomOracle,
        perpetuals::Perpetuals,
        pool::Pool,
    },
    anchor_lang::prelude::*,
};

/// Accounts required for setting custom oracle price
#[derive(Accounts)]
pub struct SetCustomOraclePrice<'info> {
    /// Admin account that must sign (must be part of multisig)
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Multisig account for admin instruction approval
    #[account(
        mut,
        seeds = [b"multisig"],
        bump = multisig.load()?.bump
    )]
    pub multisig: AccountLoader<'info, Multisig>,

    /// Main perpetuals program account
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account
    #[account(
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account for which oracle price is being set
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Custom oracle account (will be created if it doesn't exist)
    /// Admin pays for account creation if needed
    #[account(
        init_if_needed,
        payer = admin,
        space = CustomOracle::LEN,
        //constraint = oracle_account.key() == custody.oracle.oracle_account,
        seeds = [b"oracle_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump
    )]
    pub oracle_account: Box<Account<'info, CustomOracle>>,

    system_program: Program<'info, System>,
}

/// Parameters for setting custom oracle price
#[derive(AnchorSerialize, AnchorDeserialize, Copy, Clone)]
pub struct SetCustomOraclePriceParams {
    /// Price value (scaled by exponent)
    pub price: u64,
    /// Price exponent (for decimal scaling)
    pub expo: i32,
    /// Price confidence interval
    pub conf: u64,
    /// Exponential moving average price
    pub ema: u64,
    /// Timestamp when price was published
    pub publish_time: i64,
}

/// Set or update custom oracle price for a custody
/// 
/// This function allows admins to set custom oracle prices. The process:
/// 1. Validates multisig signatures (requires enough admin signatures)
/// 2. Updates oracle account with new price data
/// 
/// The oracle account is created if it doesn't exist (init_if_needed).
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Oracle price parameters
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn set_custom_oracle_price<'info>(
    ctx: Context<'_, '_, '_, 'info, SetCustomOraclePrice<'info>>,
    params: &SetCustomOraclePriceParams,
) -> Result<u8> {
    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::SetCustomOraclePrice, params)?,
    )?;
    
    // If more signatures are required, return early with count
    // The instruction can be called again with additional signatures
    if signatures_left > 0 {
        msg!(
            "Instruction has been signed but more signatures are required: {}",
            signatures_left
        );
        return Ok(signatures_left);
    }

    // Update oracle data
    // Set all price-related fields in the custom oracle account
    ctx.accounts.oracle_account.set(
        params.price,
        params.expo,
        params.conf,
        params.ema,
        params.publish_time,
    );
    Ok(0)
}