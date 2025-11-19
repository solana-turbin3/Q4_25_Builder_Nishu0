//! AddCustody instruction handler
//! 
//! This instruction allows admins to add a new custody (token) to an existing pool.
//! A custody represents a token that can be traded or used as collateral in the pool.
//! This requires multisig approval and initializes the custody account with pricing,
//! fees, oracle configuration, and other parameters.

use {
    crate::{
        error::PerpetualsError,
        state::{
            custody::{BorrowRateParams, Custody, Fees, PricingParams},
            multisig::{AdminInstruction, Multisig},
            oracle::OracleParams,
            perpetuals::{Permissions, Perpetuals},
            pool::{Pool, TokenRatios},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Mint, Token, TokenAccount},
};

/// Accounts required for adding a new custody to a pool
#[derive(Accounts)]
pub struct AddCustody<'info> {
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

    /// Transfer authority PDA for token accounts
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

    /// Pool account (mutable, will be reallocated to accommodate new custody)
    /// Reallocation increases size to fit new custody pubkey and token ratios
    #[account(
        mut,
        realloc = Pool::LEN + (pool.custodies.len() + 1) * std::mem::size_of::<Pubkey>() +
                              (pool.ratios.len() + 1) * std::mem::size_of::<TokenRatios>(),
        realloc::payer = admin,
        realloc::zero = false,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// New custody account to be initialized (PDA derived from pool and token mint)
    #[account(
        init_if_needed,
        payer = admin,
        space = Custody::LEN,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody_token_mint.key().as_ref()],
        bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Token account for the custody (holds tokens for this custody)
    /// Initialized if needed, owned by transfer_authority PDA
    #[account(
        init_if_needed,
        payer = admin,
        token::mint = custody_token_mint,
        token::authority = transfer_authority,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody_token_mint.key().as_ref()],
        bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    /// Mint account for the token being added as custody
    #[account()]
    pub custody_token_mint: Box<Account<'info, Mint>>,

    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
    rent: Sysvar<'info, Rent>,
}

/// Parameters for adding a new custody to a pool
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AddCustodyParams {
    /// Whether this token is a stablecoin (affects price calculations)
    pub is_stable: bool,
    /// Whether this is a virtual custody (no actual tokens held)
    pub is_virtual: bool,
    /// Oracle configuration for price feeds
    pub oracle: OracleParams,
    /// Pricing parameters (spreads, EMA settings, etc.)
    pub pricing: PricingParams,
    /// Permission flags controlling allowed operations
    pub permissions: Permissions,
    /// Fee structure (open/close position fees, swap fees, etc.)
    pub fees: Fees,
    /// Borrow rate parameters for interest calculations
    pub borrow_rate: BorrowRateParams,
    /// Token ratios for pool rebalancing (must include ratio for new custody)
    pub ratios: Vec<TokenRatios>,
}

/// Add a new custody (token) to an existing pool
/// 
/// This function:
/// 1. Validates multisig signatures (requires enough admin signatures)
/// 2. Checks that the custody doesn't already exist
/// 3. Updates the pool to include the new custody
/// 4. Initializes the custody account with all configuration parameters
/// 5. Validates the custody and pool configurations
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including custody configuration (pricing, fees, oracle, etc.)
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn add_custody<'info>(
    ctx: Context<'_, '_, '_, 'info, AddCustody<'info>>,
    params: &AddCustodyParams,
) -> Result<u8> {
    // Validate inputs
    // Ratios must include one entry for each existing custody plus one for the new custody
    if params.ratios.len() != ctx.accounts.pool.ratios.len() + 1 {
        return Err(ProgramError::InvalidArgument.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::AddCustody, params)?,
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

    // Check if custody already exists in the pool
    let pool = ctx.accounts.pool.as_mut();
    if pool.get_token_id(&ctx.accounts.custody.key()).is_ok() {
        // Return error if custody is already initialized
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    // Update pool data to include new custody
    // Add custody pubkey to pool's custody list
    pool.custodies.push(ctx.accounts.custody.key());
    // Update token ratios (must include ratio for new custody)
    pool.ratios = params.ratios.clone();
    // Validate pool configuration after adding custody
    if !pool.validate() {
        return err!(PerpetualsError::InvalidPoolConfig);
    }

    // Initialize custody account with all configuration parameters
    let custody = ctx.accounts.custody.as_mut();
    custody.pool = pool.key();
    custody.mint = ctx.accounts.custody_token_mint.key();
    custody.token_account = ctx.accounts.custody_token_account.key();
    custody.decimals = ctx.accounts.custody_token_mint.decimals;
    custody.is_stable = params.is_stable;
    custody.is_virtual = params.is_virtual;
    custody.oracle = params.oracle;
    custody.pricing = params.pricing;
    custody.permissions = params.permissions;
    custody.fees = params.fees;
    custody.borrow_rate = params.borrow_rate;
    // Initialize borrow rate state with base rate
    custody.borrow_rate_state.current_rate = params.borrow_rate.base_rate;
    custody.borrow_rate_state.last_update = ctx.accounts.perpetuals.get_time()?;
    // Store PDA bumps for future account derivation
    custody.bump = *ctx.bumps.get("custody").ok_or(ProgramError::InvalidSeeds)?;
    custody.token_account_bump = *ctx
        .bumps
        .get("custody_token_account")
        .ok_or(ProgramError::InvalidSeeds)?;

    // Validate custody configuration
    // Return error if validation fails, otherwise return success (0 signatures left)
    if !custody.validate() {
        err!(PerpetualsError::InvalidCustodyConfig)
    } else {
        Ok(0)
    }
}