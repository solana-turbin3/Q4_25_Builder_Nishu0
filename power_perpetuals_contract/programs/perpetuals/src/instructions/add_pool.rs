//! AddPool instruction handler
//! 
//! This instruction allows admins to create a new trading pool. A pool is a collection
//! of custodies (tokens) that can be traded against each other. Each pool has its own
//! LP token mint and maintains token ratios. This requires multisig approval.

use {
    crate::{
        error::PerpetualsError,
        state::{
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
            pool::Pool,
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Mint, Token},
};

/// Accounts required for creating a new pool
#[derive(Accounts)]
#[instruction(params: AddPoolParams)]
pub struct AddPool<'info> {
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

    /// Main perpetuals program account (mutable, will be reallocated to include new pool)
    /// Reallocation increases size to fit new pool pubkey
    #[account(
        mut,
        realloc = Perpetuals::LEN + (perpetuals.pools.len() + 1) * std::mem::size_of::<Pubkey>(),
        realloc::payer = admin,
        realloc::zero = false,
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// New pool account to be initialized (PDA derived from pool name)
    /// 
    /// Note: Uses init_if_needed instead of init because instruction can be called
    /// multiple times due to multisig. On first call, account is zero-initialized and
    /// filled out when all signatures are collected. When account is in zeroed state,
    /// it can't be used in other instructions because seeds are computed with pool name.
    /// Uniqueness is enforced manually in the instruction handler.
    #[account(
        init_if_needed,
        payer = admin,
        space = Pool::LEN,
        seeds = [b"pool",
                 params.name.as_bytes()],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// LP token mint for this pool (initialized if needed)
    /// Owned by transfer_authority PDA, with fixed decimals
    #[account(
        init_if_needed,
        payer = admin,
        mint::authority = transfer_authority,
        mint::freeze_authority = transfer_authority,
        mint::decimals = Perpetuals::LP_DECIMALS,
        seeds = [b"lp_token_mint",
                 pool.key().as_ref()],
        bump
    )]
    pub lp_token_mint: Box<Account<'info, Mint>>,

    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
    rent: Sysvar<'info, Rent>,
}

/// Parameters for creating a new pool
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct AddPoolParams {
    /// Pool name (max 64 characters, must be unique)
    pub name: String,
}

/// Create a new trading pool
/// 
/// This function allows admins to create a new pool with a unique name. The process:
/// 1. Validates pool name (non-empty, max 64 characters)
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Checks that pool doesn't already exist
/// 4. Initializes pool account with name, inception time, and bumps
/// 5. Validates pool configuration
/// 6. Adds pool to perpetuals program's pool list
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including the pool name
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn add_pool<'info>(
    ctx: Context<'_, '_, '_, 'info, AddPool<'info>>,
    params: &AddPoolParams,
) -> Result<u8> {
    // Validate inputs
    // Pool name must be non-empty and not exceed 64 characters
    if params.name.is_empty() || params.name.len() > 64 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::AddPool, params)?,
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

    // Initialize pool account with configuration data
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    let pool = ctx.accounts.pool.as_mut();

    // Check if pool is already initialized
    // inception_time != 0 indicates the pool has been set up
    if pool.inception_time != 0 {
        // Return error if pool is already initialized
        return Err(anchor_lang::error::ErrorCode::ConstraintMut.into());
    }
    
    msg!("Record pool: {}", params.name);
    // Set pool inception time to current time
    pool.inception_time = perpetuals.get_time()?;
    // Set pool name
    pool.name = params.name.clone();
    // Store PDA bumps for future account derivation
    pool.bump = ctx.bumps.pool;
    pool.lp_token_bump = ctx.bumps.lp_token_mint;

    // Validate pool configuration
    if !pool.validate() {
        return err!(PerpetualsError::InvalidPoolConfig);
    }

    // Add pool to perpetuals program's pool list
    perpetuals.pools.push(ctx.accounts.pool.key());

    Ok(0)
}