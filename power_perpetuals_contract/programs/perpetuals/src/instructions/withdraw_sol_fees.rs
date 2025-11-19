//! WithdrawSolFees instruction handler
//! 
//! This instruction allows admins to withdraw SOL fees collected by the program.
//! SOL fees accumulate in the transfer_authority PDA account from various operations.
//! This requires multisig approval and transfers SOL from the transfer_authority PDA
//! to a receiving account, ensuring the PDA maintains its minimum rent-exempt balance.

use {
    crate::{
        math,
        state::{
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
        },
    },
    anchor_lang::prelude::*,
    solana_program::sysvar,
};

/// Accounts required for withdrawing SOL fees
#[derive(Accounts)]
pub struct WithdrawSolFees<'info> {
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

    /// Transfer authority PDA where SOL fees are stored (mutable, SOL will be transferred out)
    /// 
    /// CHECK: Empty PDA, authority for token accounts and SOL fee storage
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

    /// Receiving account where SOL will be transferred (mutable)
    /// Must be an empty account (no data)
    /// 
    /// CHECK: SOL fees receiving account, validated by constraint
    #[account(
        mut,
        constraint = receiving_account.data_is_empty()
    )]
    pub receiving_account: AccountInfo<'info>,
}

/// Parameters for withdrawing SOL fees
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawSolFeesParams {
    /// Amount of SOL to withdraw (in lamports)
    pub amount: u64,
}

/// Withdraw SOL fees from the transfer authority PDA
/// 
/// This function allows admins to withdraw accumulated SOL fees from the transfer_authority
/// PDA account. SOL fees accumulate from various program operations. The process:
/// 1. Validates input amount is greater than zero
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Calculates available balance (total balance minus minimum rent-exempt balance)
/// 4. Validates sufficient SOL is available for withdrawal
/// 5. Transfers SOL from transfer_authority PDA to receiving account
/// 
/// The transfer_authority PDA must maintain its minimum rent-exempt balance, so only
/// the excess balance above the minimum can be withdrawn.
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including withdrawal amount
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn withdraw_sol_fees<'info>(
    ctx: Context<'_, '_, '_, 'info, WithdrawSolFees<'info>>,
    params: &WithdrawSolFeesParams,
) -> Result<u8> {
    // Validate inputs
    // Amount must be greater than zero
    if params.amount == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::WithdrawSolFees, params)?,
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

    // Calculate available balance for withdrawal
    // Get current balance of transfer_authority PDA
    let balance = ctx.accounts.transfer_authority.try_lamports()?;
    
    // Get minimum rent-exempt balance (required to keep account alive)
    let min_balance = sysvar::rent::Rent::get()?.minimum_balance(0);
    
    // Calculate available balance (excess above minimum rent-exempt balance)
    // Only this excess can be withdrawn, PDA must maintain minimum balance
    let available_balance = if balance > min_balance {
        math::checked_sub(balance, min_balance)?
    } else {
        0
    };

    // Log withdrawal details for debugging
    msg!(
        "Withdraw SOL fees: {} / {}",
        params.amount,
        available_balance
    );

    // Validate sufficient SOL is available for withdrawal
    if available_balance < params.amount {
        return Err(ProgramError::InsufficientFunds.into());
    }

    // Transfer SOL from transfer_authority PDA to receiving account
    Perpetuals::transfer_sol_from_owned(
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.receiving_account.to_account_info(),
        params.amount,
    )?;

    Ok(0)
}