//! SetPermissions instruction handler
//! 
//! This instruction allows admins to update global permissions for the perpetuals program.
//! Permissions control which operations are allowed across the entire program. This requires
//! multisig approval and validates the perpetuals configuration after updates.

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

/// Accounts required for setting global permissions
#[derive(Accounts)]
pub struct SetPermissions<'info> {
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

    /// Main perpetuals program account (mutable, permissions will be updated)
    #[account(
        mut,
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,
}

/// Parameters for setting global permissions
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SetPermissionsParams {
    /// Allow swap operations
    pub allow_swap: bool,
    /// Allow adding liquidity to pools
    pub allow_add_liquidity: bool,
    /// Allow removing liquidity from pools
    pub allow_remove_liquidity: bool,
    /// Allow opening new positions
    pub allow_open_position: bool,
    /// Allow closing existing positions
    pub allow_close_position: bool,
    /// Allow withdrawing profit/loss from positions
    pub allow_pnl_withdrawal: bool,
    /// Allow withdrawing collateral from positions
    pub allow_collateral_withdrawal: bool,
    /// Allow changing position size
    pub allow_size_change: bool,
}

/// Update global permissions for the perpetuals program
/// 
/// This function allows admins to change which operations are allowed across the entire
/// program. The process:
/// 1. Validates multisig signatures (requires enough admin signatures)
/// 2. Updates all permission flags
/// 3. Validates perpetuals configuration remains valid
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - New permission flags
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn set_permissions<'info>(
    ctx: Context<'_, '_, '_, 'info, SetPermissions<'info>>,
    params: &SetPermissionsParams,
) -> Result<u8> {
    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::SetPermissions, params)?,
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

    // Update permissions
    // Apply all new permission flags to the perpetuals account
    let perpetuals = ctx.accounts.perpetuals.as_mut();
    perpetuals.permissions.allow_swap = params.allow_swap;
    perpetuals.permissions.allow_add_liquidity = params.allow_add_liquidity;
    perpetuals.permissions.allow_remove_liquidity = params.allow_remove_liquidity;
    perpetuals.permissions.allow_open_position = params.allow_open_position;
    perpetuals.permissions.allow_close_position = params.allow_close_position;
    perpetuals.permissions.allow_pnl_withdrawal = params.allow_pnl_withdrawal;
    perpetuals.permissions.allow_collateral_withdrawal = params.allow_collateral_withdrawal;
    perpetuals.permissions.allow_size_change = params.allow_size_change;

    // Validate perpetuals configuration after updates
    // Ensure all parameters are within acceptable ranges
    if !perpetuals.validate() {
        err!(PerpetualsError::InvalidPerpetualsConfig)
    } else {
        Ok(0)
    }
}