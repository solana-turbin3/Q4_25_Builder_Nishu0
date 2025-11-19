//! RemoveCustody instruction handler
//! 
//! This instruction allows admins to remove a custody (token) from an existing pool.
//! The custody can only be removed if its token account is empty (no tokens held).
//! This requires multisig approval and updates the pool's custody list and token ratios.

use {
    crate::{
        error::PerpetualsError,
        state::{
            custody::Custody,
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
            pool::{Pool, TokenRatios},
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Token, TokenAccount},
};

/// Accounts required for removing a custody from a pool
#[derive(Accounts)]
pub struct RemoveCustody<'info> {
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

    /// Transfer authority PDA for token accounts (mutable, will close token account)
    /// 
    /// CHECK: Empty PDA, authority for token accounts
    #[account(
        mut,
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

    /// Pool account (mutable, will be reallocated to remove custody)
    /// Reallocation decreases size to remove custody pubkey and token ratios
    #[account(
        mut,
        realloc = Pool::LEN + (pool.custodies.len() - 1) * std::mem::size_of::<Pubkey>() +
                              (pool.ratios.len() - 1) * std::mem::size_of::<TokenRatios>(),
        realloc::payer = admin,
        realloc::zero = false,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account to be removed (mutable, will be closed)
    /// Rent is returned to transfer_authority
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.bump,
        close = transfer_authority
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Token account for the custody (mutable, will be closed)
    /// Must be empty (amount == 0) before removal
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.token_account_bump,
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
}

/// Parameters for removing a custody from a pool
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RemoveCustodyParams {
    /// Updated token ratios for remaining custodies (must exclude ratio for removed custody)
    pub ratios: Vec<TokenRatios>,
}

/// Remove a custody (token) from an existing pool
/// 
/// This function allows admins to remove a custody from a pool. The process:
/// 1. Validates input ratios (must exclude ratio for removed custody)
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Validates custody token account is empty (no tokens held)
/// 4. Removes custody from pool's custody list
/// 5. Updates token ratios
/// 6. Validates pool configuration
/// 7. Closes custody token account
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including updated token ratios
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn remove_custody<'info>(
    ctx: Context<'_, '_, '_, 'info, RemoveCustody<'info>>,
    params: &RemoveCustodyParams,
) -> Result<u8> {
    // Validate inputs
    // Ratios must not be empty and must have one less entry than current ratios
    if ctx.accounts.pool.ratios.is_empty()
        || params.ratios.len() != ctx.accounts.pool.ratios.len() - 1
    {
        return Err(ProgramError::InvalidArgument.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::RemoveCustody, params)?,
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

    // Validate that custody token account is empty
    // Cannot remove custody if it still holds tokens
    require!(
        ctx.accounts.custody_token_account.amount == 0,
        PerpetualsError::InvalidCustodyState
    );

    // Remove custody from pool's custody list
    let pool = ctx.accounts.pool.as_mut();
    let token_id = pool.get_token_id(&ctx.accounts.custody.key())?;
    pool.custodies.remove(token_id);
    // Update token ratios (must exclude ratio for removed custody)
    pool.ratios = params.ratios.clone();
    // Validate pool configuration after removing custody
    if !pool.validate() {
        return err!(PerpetualsError::InvalidPoolConfig);
    }

    // Close custody token account
    // Returns rent to transfer_authority PDA
    Perpetuals::close_token_account(
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        &[&[
            b"transfer_authority",
            &[ctx.accounts.perpetuals.transfer_authority_bump],
        ]],
    )?;

    Ok(0)
}