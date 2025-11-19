//! WithdrawFees instruction handler
//! 
//! This instruction allows admins to withdraw protocol fees collected from a custody.
//! Protocol fees are a portion of trading fees that accumulate in the custody's
//! protocol_fees account. This requires multisig approval and transfers tokens from
//! the custody's token account to a receiving account.

use {
    crate::{
        math,
        state::{
            custody::Custody,
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
            pool::Pool,
        },
    },
    anchor_lang::prelude::*,
    anchor_spl::token::{Token, TokenAccount},
};

/// Accounts required for withdrawing protocol fees
#[derive(Accounts)]
pub struct WithdrawFees<'info> {
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

    /// Transfer authority PDA for token transfers
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

    /// Pool account
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account (mutable, protocol_fees will be decremented)
    #[account(
        mut,
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.key().as_ref()],
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Pool's token account where protocol fees are stored (mutable, tokens will be transferred out)
    #[account(
        mut,
        seeds = [b"custody_token_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump = custody.token_account_bump
    )]
    pub custody_token_account: Box<Account<'info, TokenAccount>>,

    /// Receiving token account where fees will be transferred
    /// Must have the same mint as the custody token account
    #[account(
        mut,
        constraint = receiving_token_account.mint == custody_token_account.mint
    )]
    pub receiving_token_account: Box<Account<'info, TokenAccount>>,

    token_program: Program<'info, Token>,
}

/// Parameters for withdrawing protocol fees
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawFeesParams {
    /// Amount of tokens to withdraw (in token decimals)
    pub amount: u64,
}

/// Withdraw protocol fees from a custody
/// 
/// This function allows admins to withdraw accumulated protocol fees from a custody.
/// Protocol fees are a portion of trading fees that accumulate over time. The process:
/// 1. Validates input amount is greater than zero
/// 2. Validates multisig signatures (requires enough admin signatures)
/// 3. Validates sufficient protocol fees are available
/// 4. Decrements protocol fees from custody
/// 5. Transfers tokens from custody token account to receiving account
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters including withdrawal amount
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn withdraw_fees<'info>(
    ctx: Context<'_, '_, '_, 'info, WithdrawFees<'info>>,
    params: &WithdrawFeesParams,
) -> Result<u8> {
    // Validate inputs
    // Amount must be greater than zero
    if params.amount == 0 {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }

    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::WithdrawFees, params)?,
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

    // Transfer token fees from the custody to the receiver
    let custody = ctx.accounts.custody.as_mut();

    // Log withdrawal details for debugging
    msg!(
        "Withdraw token fees: {} / {}",
        params.amount,
        custody.assets.protocol_fees
    );

    // Validate sufficient protocol fees are available
    if custody.assets.protocol_fees < params.amount {
        return Err(anchor_lang::error::ErrorCode::ConstraintRaw.into());
    }
    
    // Decrement protocol fees from custody
    custody.assets.protocol_fees = math::checked_sub(custody.assets.protocol_fees, params.amount)?;

    // Transfer tokens from custody token account to receiving account
    ctx.accounts.perpetuals.transfer_tokens(
        ctx.accounts.custody_token_account.to_account_info(),
        ctx.accounts.receiving_token_account.to_account_info(),
        ctx.accounts.transfer_authority.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        params.amount,
    )?;

    Ok(0)
}