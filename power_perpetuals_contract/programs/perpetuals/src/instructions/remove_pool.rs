//! RemovePool instruction handler
//! 
//! This instruction allows admins to remove a pool from the perpetuals program.
//! The pool can only be removed if it has no custodies (all tokens must be removed first).
//! This requires multisig approval and removes the pool from the program's pool list.

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
};

/// Accounts required for removing a pool
#[derive(Accounts)]
pub struct RemovePool<'info> {
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

    /// Transfer authority PDA for token accounts (mutable, will close pool account)
    /// 
    /// CHECK: Empty PDA, authority for token accounts
    #[account(
        mut,
        seeds = [b"transfer_authority"],
        bump = perpetuals.transfer_authority_bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    /// Main perpetuals program account (mutable, will be reallocated to remove pool)
    /// Reallocation decreases size to remove pool pubkey
    #[account(
        mut,
        realloc = Perpetuals::LEN + (perpetuals.pools.len() - 1) * 32,
        realloc::payer = admin,
        realloc::zero = false,
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account to be removed (mutable, will be closed)
    /// Rent is returned to transfer_authority PDA
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump,
        close = transfer_authority
    )]
    pub pool: Box<Account<'info, Pool>>,

    system_program: Program<'info, System>,
}

/// Parameters for removing a pool
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemovePoolParams {}

/// Remove a pool from the perpetuals program
/// 
/// This function allows admins to remove a pool. The process:
/// 1. Validates multisig signatures (requires enough admin signatures)
/// 2. Validates pool has no custodies (all tokens must be removed first)
/// 3. Removes pool from perpetuals program's pool list
/// 4. Pool account is closed and rent is returned
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn remove_pool<'info>(
    ctx: Context<'_, '_, '_, 'info, RemovePool<'info>>,
    params: &RemovePoolParams,
) -> Result<u8> {
    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::RemovePool, params)?,
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

    // Validate that pool has no custodies
    // All tokens must be removed before the pool can be deleted
    require!(
        ctx.accounts.pool.custodies.is_empty(),
        PerpetualsError::InvalidPoolState
    );

    // Remove pool from perpetuals program's pool list
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    // Find the index of the pool in the pools list
    let pool_idx = perpetuals
        .pools
        .iter()
        .position(|x| *x == ctx.accounts.pool.key())
        .ok_or(PerpetualsError::InvalidPoolState)?;
    // Remove the pool from the list
    perpetuals.pools.remove(pool_idx);

    Ok(0)
}