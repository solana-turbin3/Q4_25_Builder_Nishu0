//! SetAdminSigners instruction handler
//! 
//! This instruction allows admins to update the multisig configuration by setting
//! new admin signers and minimum signature requirements. This requires multisig
//! approval using the current signer configuration.

use {
    crate::state::multisig::{AdminInstruction, Multisig},
    anchor_lang::prelude::*,
};

/// Accounts required for setting admin signers
#[derive(Accounts)]
pub struct SetAdminSigners<'info> {
    /// Admin account that must sign (must be part of current multisig)
    #[account()]
    pub admin: Signer<'info>,

    /// Multisig account to update (mutable, signers will be changed)
    #[account(
        mut,
        seeds = [b"multisig"], 
        bump = multisig.load()?.bump
    )]
    pub multisig: AccountLoader<'info, Multisig>,
    // Remaining accounts (passed via ctx.remaining_accounts):
    //   1 to Multisig::MAX_SIGNERS admin signer accounts (read-only, unsigned)
    // These are the new admin signers that will replace the current ones
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SetAdminSignersParams {
    pub min_signatures: u8,
}

pub fn set_admin_signers<'info>(
    ctx: Context<'_, '_, '_, 'info, SetAdminSigners<'info>>,
    params: &SetAdminSignersParams,
) -> Result<u8> {
    // Validate multisig signatures using CURRENT signer configuration
    // This ensures the change is approved by current admins before applying new signers
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::SetAdminSigners, params)?,
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

    // Set new admin signers and minimum signature requirements
    // ctx.remaining_accounts contains the new admin signer accounts
    multisig.set_signers(ctx.remaining_accounts, params.min_signatures)?;

    Ok(0)
}