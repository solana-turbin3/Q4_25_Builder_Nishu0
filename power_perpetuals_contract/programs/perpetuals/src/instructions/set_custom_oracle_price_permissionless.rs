//! SetCustomOraclePricePermissionless instruction handler
//! 
//! This instruction allows anyone to update custom oracle prices without admin approval,
//! as long as they provide a valid Ed25519 signature from the oracle authority. The oracle
//! account must first be initialized by an admin. This enables permissionless price updates
//! while maintaining security through cryptographic signatures.

use {
    crate::{
        error::PerpetualsError,
        state::{custody::Custody, oracle::CustomOracle, perpetuals::Perpetuals, pool::Pool},
    },
    anchor_lang::prelude::*,
    solana_program::{ed25519_program, instruction::Instruction, sysvar},
};

/// Accounts required for permissionless custom oracle price update
#[derive(Accounts)]
#[instruction(params: SetCustomOraclePricePermissionlessParams)]
pub struct SetCustomOraclePricePermissionless<'info> {
    /// Main perpetuals program account
    #[account(
        seeds = [b"perpetuals"],
        bump = perpetuals.perpetuals_bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// Pool account
    #[account(
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Custody account for which oracle price is being updated
    /// Must match the custody_account in params
    #[account(
        seeds = [b"custody",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        constraint = custody.key() == params.custody_account,
        bump = custody.bump
    )]
    pub custody: Box<Account<'info, Custody>>,

    /// Custom oracle account (mutable, price will be updated)
    /// Custom oracle must first be initialized by authority before permissionless updates.
    #[account(
        // Custom oracle must first be initialized by authority before permissionless updates.
        mut,
        seeds = [b"oracle_account",
                 pool.key().as_ref(),
                 custody.mint.as_ref()],
        bump
    )]
    pub oracle_account: Box<Account<'info, CustomOracle>>,

    /// Instructions sysvar account for Ed25519 signature verification
    /// 
    /// CHECK: Needed for ed25519 signature verification, to inspect all instructions in this transaction.
    #[account(address = sysvar::instructions::ID)]
    pub ix_sysvar: AccountInfo<'info>,
}

/// Parameters for permissionless custom oracle price update
#[derive(AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq)]
pub struct SetCustomOraclePricePermissionlessParams {
    /// Custody account pubkey (for validation)
    pub custody_account: Pubkey,
    /// Price value (scaled by exponent)
    pub price: u64,
    /// Price exponent (for decimal scaling)
    pub expo: i32,
    /// Price confidence interval
    pub conf: u64,
    /// Exponential moving average price
    pub ema: u64,
    /// Timestamp when price was published (must be newer than current publish_time)
    pub publish_time: i64,
}

/// Update custom oracle price permissionlessly with Ed25519 signature verification
/// 
/// This function allows anyone to update oracle prices without admin approval, as long
/// as they provide a valid Ed25519 signature from the oracle authority. The process:
/// 1. Validates publish_time is newer than current (prevents stale updates)
/// 2. Loads Ed25519 signature verification instruction from transaction
/// 3. Validates signature matches oracle authority and message matches params
/// 4. Updates oracle account with new price data
/// 
/// This enables permissionless price updates while maintaining security through
/// cryptographic signatures. The oracle account must first be initialized by an admin.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Oracle price parameters (must match signed message)
/// 
/// # Returns
/// `Result<()>` - Success if price was updated, or error
pub fn set_custom_oracle_price_permissionless(
    ctx: Context<SetCustomOraclePricePermissionless>,
    params: &SetCustomOraclePricePermissionlessParams,
) -> Result<()> {
    // Validate publish_time is newer than current
    // Prevents replay attacks and ensures only newer prices are accepted
    if params.publish_time <= ctx.accounts.oracle_account.publish_time {
        msg!("Custom oracle price did not update because the requested publish time is stale.");
        return Ok(());
    }
    
    // Get Ed25519Program signature verification instruction from transaction
    // This instruction should be at index 0 and contain the signature
    let signature_ix: Instruction =
        sysvar::instructions::load_instruction_at_checked(0, &ctx.accounts.ix_sysvar)?;

    // Validate Ed25519 signature
    // Ensures signature is from oracle authority and message matches params
    validate_ed25519_signature_instruction(
        &signature_ix,
        &ctx.accounts.custody.oracle.oracle_authority,
        params,
    )?;

    // Update oracle account with new price data
    // Only reached if signature validation passes
    ctx.accounts.oracle_account.set(
        params.price,
        params.expo,
        params.conf,
        params.ema,
        params.publish_time,
    );
    Ok(())
}

/// Validate Ed25519 signature instruction format and content
/// 
/// This function validates that:
/// 1. Instruction is from Ed25519Program
/// 2. Instruction format matches expected structure (no accounts, single signature, correct data length)
/// 3. Signer pubkey matches expected oracle authority
/// 4. Signed message matches the provided parameters
/// 
/// The Ed25519 instruction format follows Solana's specification:
/// - data[0] = number of signatures (must be 0x01 for single signature)
/// - data[16..48] = signer pubkey (32 bytes)
/// - data[112..] = signed message (instruction params)
/// 
/// # Arguments
/// * `signature_ix` - Ed25519 signature verification instruction from transaction
/// * `expected_pubkey` - Expected oracle authority pubkey
/// * `expected_params` - Expected instruction parameters (must match signed message)
/// 
/// # Returns
/// `Result<()>` - Success if signature is valid, or error
fn validate_ed25519_signature_instruction(
    signature_ix: &Instruction,
    expected_pubkey: &Pubkey,
    expected_params: &SetCustomOraclePricePermissionlessParams,
) -> Result<()> {
    // Validate instruction is from Ed25519Program
    require_eq!(
        signature_ix.program_id,
        ed25519_program::ID,
        PerpetualsError::PermissionlessOracleMissingSignature
    );
    
    // Validate instruction format matches expected structure
    // Must have no accounts, single signature (0x01), and exact data length (180 bytes)
    require!(
        signature_ix.accounts.is_empty() /* no accounts touched */
            && signature_ix.data[0] == 0x01 /* only one ed25519 signature */
            && signature_ix.data.len() == 180, /* data len matches exactly the expected */
        PerpetualsError::PermissionlessOracleMalformedEd25519Data
    );

    // Extract signer pubkey and message from instruction data
    // Manually access offsets for signer pubkey and message data according to:
    // https://docs.solana.com/developing/runtime-facilities/programs#ed25519-program
    let signer_pubkey = &signature_ix.data[16..16 + 32];
    let mut verified_message = &signature_ix.data[112..];

    // Deserialize signed message to get instruction parameters
    let deserialized_instruction_params =
        SetCustomOraclePricePermissionlessParams::deserialize(&mut verified_message)?;

    // Validate signer pubkey matches expected oracle authority
    require!(
        signer_pubkey == expected_pubkey.to_bytes(),
        PerpetualsError::PermissionlessOracleSignerMismatch
    );
    
    // Validate signed message matches provided parameters
    // This ensures the signature was created for exactly these parameters
    require!(
        deserialized_instruction_params == *expected_params,
        PerpetualsError::PermissionlessOracleMessageMismatch
    );
    Ok(())
}