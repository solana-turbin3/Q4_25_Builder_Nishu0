//! SetCustodyConfig instruction handler
//! 
//! This instruction allows admins to update custody configuration parameters including
//! oracle settings, pricing parameters, permissions, fees, borrow rates, and token ratios.
//! This requires multisig approval and validates both pool and custody configurations
//! after updates to ensure system integrity.

use {
    crate::{
        error::PerpetualsError,
        state::{
            custody::{BorrowRateParams, Custody, Fees, PricingParams},
            multisig::{AdminInstruction, Multisig},
            oracle::OracleParams,
            perpetuals::Permissions,
            pool::{Pool, TokenRatios},
        },
    },
    anchor_lang::prelude::*,
};

/// Accounts required for setting custody configuration
#[derive(Accounts)]
pub struct SetCustodyConfig<'info> {
    /// Admin account that must sign (must be part of multisig)
    #[account()]
    pub admin: Signer<'info>,

    /// Multisig account for admin instruction approval
    #[account(
        mut,
        seeds = [b"multisig"],
        bump = multisig.load()?.bump
    )]
    pub multisig: AccountLoader<'info, Multisig>,

    /// Pool account (mutable, token ratios will be updated)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account to update (mutable, configuration will be changed)
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump
    )]
    pub custody: Box<Account<'info, Custody>>,
}

/// Parameters for setting custody configuration
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SetCustodyConfigParams {
    /// Whether this custody represents a stablecoin
    pub is_stable: bool,
    /// Whether this custody is virtual (not backed by real tokens)
    pub is_virtual: bool,
    /// Oracle configuration parameters
    pub oracle: OracleParams,
    /// Pricing parameters (EMA settings, etc.)
    pub pricing: PricingParams,
    /// Permission flags for various operations
    pub permissions: Permissions,
    /// Fee structure for this custody
    pub fees: Fees,
    /// Borrow rate parameters
    pub borrow_rate: BorrowRateParams,
    /// Token ratios for this custody (must match pool's ratio count)
    pub ratios: Vec<TokenRatios>,
}

/// Update custody configuration parameters
/// 
/// This function allows admins to change custody settings. The process:
/// 1. Validates input parameters (ratios count must match pool)
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Updates pool token ratios and validates pool configuration
/// 4. Updates custody configuration parameters
/// 5. Validates custody configuration
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - New configuration parameters
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn set_custody_config<'info>(
    ctx: Context<'_, '_, '_, 'info, SetCustodyConfig<'info>>,
    params: &SetCustodyConfigParams,
) -> Result<u8> {
    // Validate inputs
    // Ratios count must match pool's ratio count to maintain consistency
    if params.ratios.len() != ctx.accounts.pool.ratios.len() {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::SetCustodyConfig, params)?,
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

    // Update pool data
    // Update token ratios and validate pool configuration remains valid
    let pool = ctx.accounts.pool.as_mut();
    pool.ratios = params.ratios.clone();
    if !pool.validate() {
        return err!(PerpetualsError::InvalidPoolConfig);
    }

    // Update custody data
    // Apply all new configuration parameters to the custody account
    let custody = ctx.accounts.custody.as_mut();
    custody.is_stable = params.is_stable;
    custody.is_virtual = params.is_virtual;
    custody.oracle = params.oracle;
    custody.pricing = params.pricing;
    custody.permissions = params.permissions;
    custody.fees = params.fees;
    custody.borrow_rate = params.borrow_rate;

    // Validate custody configuration after updates
    // Ensure all parameters are within acceptable ranges
    if !custody.validate() {
        err!(PerpetualsError::InvalidCustodyConfig)
    } else {
        Ok(0)
    }
}