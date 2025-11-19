//! SetTestTime instruction handler
//! 
//! This instruction allows admins to set a custom inception time for testing purposes.
//! This is only available when the program is compiled with the "test" feature flag.
//! It requires multisig approval and updates the perpetuals account's inception_time,
//! which affects time-based calculations throughout the program.

use {
    crate::{
        error::PerpetualsError,
        state::{
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
        },
    },
    anchor_lang::prelude::*,
};

/// Accounts required for setting test time
#[derive(Accounts)]
pub struct SetTestTime<'info> {
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

    /// Main perpetuals program account (mutable, inception_time will be updated)
    #[account(
        mut,
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,
}

/// Parameters for setting test time
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SetTestTimeParams {
    /// Custom time value to set as inception_time (Unix timestamp)
    pub time: i64,
}

/// Set custom inception time for testing
/// 
/// This function allows admins to set a custom inception_time for testing purposes.
/// The process:
/// 1. Validates program is compiled with "test" feature flag
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Updates inception_time in perpetuals account
/// 
/// This instruction is only available in test builds and allows testers to control
/// time-based calculations by setting a custom inception time. This affects all
/// time-dependent operations in the program.
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including custom time value
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn set_test_time<'info>(
    ctx: Context<'_, '_, '_, 'info, SetTestTime<'info>>,
    params: &SetTestTimeParams,
) -> Result<u8> {
    // Validate program is compiled with "test" feature
    // This instruction should only be available in test builds
    if !cfg!(feature = "test") {
        return err!(PerpetualsError::InvalidEnvironment);
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::SetTestTime, params)?,
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

    // Update inception_time in perpetuals account
    // This affects all time-based calculations in the program
    ctx.accounts.perpetuals.inception_time = params.time;

    Ok(0)
}