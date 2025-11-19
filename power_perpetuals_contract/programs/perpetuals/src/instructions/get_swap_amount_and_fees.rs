//! GetSwapAmountAndFees instruction handler
//! 
//! This is a view/query instruction that calculates the output amount and fees
//! for swapping tokens within a pool. It allows users to preview the transaction
//! before executing it, helping them understand the costs and expected returns.

use {
    crate::state::{
        custody::Custody,
        oracle::OraclePrice,
        perpetuals::{Perpetuals, SwapAmountAndFees},
        pool::Pool,
    },
    anchor_lang::prelude::*,
    solana_program::program_error::ProgramError,
};

/// Accounts required for querying swap amount and fees
/// 
/// This instruction is read-only and doesn't modify any state.
/// It calculates output amount and fees that would apply if tokens were swapped.
#[derive(Accounts)]
pub struct GetSwapAmountAndFees<'info> {
    /// Main perpetuals program account (read-only)
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account to query (read-only)
    #[account(
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account for the token being received (input token)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 receiving_custody.mint.as_ref()],
        bump = receiving_custody.bump
    )]
    pub receiving_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the input token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = receiving_custody_oracle_account.key() == receiving_custody.oracle.oracle_account
    )]
    pub receiving_custody_oracle_account: AccountInfo<'info>,

    /// Custody account for the token being dispensed (output token)
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 dispensing_custody.mint.as_ref()],
        bump = dispensing_custody.bump
    )]
    pub dispensing_custody: Box<Account<'info, Custody>>,

    /// Oracle account for price feed of the output token
    /// 
    /// CHECK: Oracle account, validated by constraint
    #[account(
        constraint = dispensing_custody_oracle_account.key() == dispensing_custody.oracle.oracle_account
    )]
    pub dispensing_custody_oracle_account: AccountInfo<'info>,
}

/// Parameters for querying swap amount and fees
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct GetSwapAmountAndFeesParams {
    amount_in: u64,
}

/// Calculate swap output amount and fees (view function)
/// 
/// This function simulates a token swap without actually executing the transaction.
/// It calculates:
/// 1. Output amount of tokens that would be received
/// 2. Fees charged on input token
/// 3. Fees charged on output token
/// 
/// The swap uses oracle prices to determine the exchange rate between tokens.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts (read-only)
/// * `params` - Parameters including input token amount
/// 
/// # Returns
/// `Result<SwapAmountAndFees>` - Struct containing:
/// - `amount_out`: Output tokens that would be received (in output token decimals)
/// - `fee_in`: Fee charged on input token (in input token decimals)
/// - `fee_out`: Fee charged on output token (in output token decimals)
pub fn get_swap_amount_and_fees(
    ctx: Context<GetSwapAmountAndFees>,
    params: &GetSwapAmountAndFeesParams,
) -> Result<SwapAmountAndFees> {
    // Validate inputs
    msg!("Validate inputs");
    if params.amount_in == 0 {
        return Err(ProgramError::InvalidArgument.into());
    }
    // Ensure input and output tokens are different
    require_keys_neq!(
        ctx.accounts.receiving_custody.key(),
        ctx.accounts.dispensing_custody.key()
    );

    // Get current time and account references
    let curtime = ctx.accounts.perpetuals.get_time()?;
    let pool = &ctx.accounts.pool;
    let token_id_in = pool.get_token_id(&ctx.accounts.receiving_custody.key())?;
    let token_id_out = pool.get_token_id(&ctx.accounts.dispensing_custody.key())?;
    let receiving_custody = &ctx.accounts.receiving_custody;
    let dispensing_custody = &ctx.accounts.dispensing_custody;

    // Get input token prices from oracle (spot and EMA)
    let received_token_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .receiving_custody_oracle_account
            .to_account_info(),
        &receiving_custody.oracle,
        curtime,
        false,
    )?;

    let received_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .receiving_custody_oracle_account
            .to_account_info(),
        &receiving_custody.oracle,
        curtime,
        receiving_custody.pricing.use_ema,
    )?;

    // Get output token prices from oracle (spot and EMA)
    let dispensed_token_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .dispensing_custody_oracle_account
            .to_account_info(),
        &dispensing_custody.oracle,
        curtime,
        false,
    )?;

    let dispensed_token_ema_price = OraclePrice::new_from_oracle(
        &ctx.accounts
            .dispensing_custody_oracle_account
            .to_account_info(),
        &dispensing_custody.oracle,
        curtime,
        dispensing_custody.pricing.use_ema,
    )?;

    // Calculate output token amount based on oracle prices and swap algorithm
    let amount_out = pool.get_swap_amount(
        &received_token_price,
        &received_token_ema_price,
        &dispensed_token_price,
        &dispensed_token_ema_price,
        receiving_custody,
        dispensing_custody,
        params.amount_in,
    )?;

    // Calculate swap fees
    // Returns tuple of (fee_in, fee_out) - fees on input and output tokens
    let fees = pool.get_swap_fees(
        token_id_in,
        token_id_out,
        params.amount_in,
        amount_out,
        receiving_custody,
        &received_token_price,
        dispensing_custody,
        &dispensed_token_price,
    )?;

    // Return calculated output amount and fees
    Ok(SwapAmountAndFees {
        amount_out,
        fee_in: fees.0,
        fee_out: fees.1,
    })
}