//! Init instruction handler
//! 
//! This instruction initializes the perpetuals program. It creates and initializes
//! the multisig account, transfer_authority PDA, and perpetuals account. This must
//! be called once before any other operations can be performed. The upgrade_authority
//! must be the program's upgrade authority, and initial admin signers are set up.

use {
    crate::{
        error::PerpetualsError,
        state::{multisig::Multisig, perpetuals::Perpetuals},
    },
    anchor_lang::prelude::*,
    anchor_spl::token::Token,
};

/// Accounts required for initializing the perpetuals program
#[derive(Accounts)]
pub struct Init<'info> {
    /// Upgrade authority that must sign (must be the program's upgrade authority)
    /// Pays for account creation
    #[account(mut)]
    pub upgrade_authority: Signer<'info>,

    /// Multisig account to be initialized (will be created)
    /// Stores admin signers and minimum signature requirements
    #[account(
        init,
        payer = upgrade_authority,
        space = Multisig::LEN,
        seeds = [b"multisig"],
        bump
    )]
    pub multisig: AccountLoader<'info, Multisig>,

    /// Transfer authority PDA to be initialized (will be created)
    /// Empty PDA used as authority for token accounts and SOL fee storage
    /// 
    /// CHECK: Empty PDA, will be set as authority for token accounts
    #[account(
        init,
        payer = upgrade_authority,
        space = 0,
        seeds = [b"transfer_authority"],
        bump
    )]
    pub transfer_authority: AccountInfo<'info>,

    /// Main perpetuals program account to be initialized (will be created)
    /// Stores global program state and permissions
    #[account(
        init,
        payer = upgrade_authority,
        space = Perpetuals::LEN,
        seeds = [b"perpetuals"],
        bump
    )]
    pub perpetuals: Box<Account<'info, Perpetuals>>,

    /// ProgramData account for upgrade authority validation
    /// 
    /// CHECK: ProgramData account, doesn't work in tests
    #[account()]
    pub perpetuals_program_data: AccountInfo<'info /*, ProgramData*/>,

    /// Perpetuals program account (for upgrade authority validation)
    pub perpetuals_program: Program<'info, crate::program::Perpetuals>,

    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
    // remaining accounts: 1 to Multisig::MAX_SIGNERS admin signers (read-only, unsigned)
}

/// Parameters for initializing the perpetuals program
#[derive(AnchorSerialize, AnchorDeserialize, Copy, Clone)]
pub struct InitParams {
    /// Minimum number of signatures required for multisig operations
    pub min_signatures: u8,
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

/// Initialize the perpetuals program
/// 
/// This function sets up the entire perpetuals program. The process:
/// 1. Validates upgrade authority matches program's upgrade authority
/// 2. Initializes multisig account with admin signers and minimum signatures
/// 3. Records multisig PDA bump
/// 4. Initializes perpetuals account with permissions and PDA bumps
/// 5. Records transfer_authority and perpetuals PDA bumps
/// 6. Sets inception_time to current time
/// 7. Validates perpetuals configuration
/// 
/// This must be called exactly once before any other operations. All accounts
/// are created as PDAs using seeds, ensuring deterministic addresses.
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Initialization parameters including permissions and multisig config
/// 
/// # Returns
/// `Result<()>` - Success if initialization completed successfully
pub fn init<'info>(ctx: Context<'_, 'info, '_, 'info, Init<'info>>, params: &InitParams) -> Result<()> {
    // Validate upgrade authority
    // Ensures only the program's upgrade authority can initialize
    Perpetuals::validate_upgrade_authority(
        ctx.accounts.upgrade_authority.key(),
        &ctx.accounts.perpetuals_program_data,
        &ctx.accounts.perpetuals_program,
    )?;

    // Initialize multisig account
    // This will fail if account is already initialized (prevents re-initialization)
    let mut multisig = ctx.accounts.multisig.load_init()?;

    // Set initial admin signers and minimum signature requirement
    // ctx.remaining_accounts contains the admin signer accounts
    multisig.set_signers(ctx.remaining_accounts, params.min_signatures)?;

    // Record multisig PDA bump
    // This is needed for future account derivations
    multisig.bump = ctx.bumps.multisig;

    // Initialize perpetuals account
    let perpetuals = ctx.accounts.perpetuals.as_mut();

    // Set all permission flags from parameters
    perpetuals.permissions.allow_swap = params.allow_swap;
    perpetuals.permissions.allow_add_liquidity = params.allow_add_liquidity;
    perpetuals.permissions.allow_remove_liquidity = params.allow_remove_liquidity;
    perpetuals.permissions.allow_open_position = params.allow_open_position;
    perpetuals.permissions.allow_close_position = params.allow_close_position;
    perpetuals.permissions.allow_pnl_withdrawal = params.allow_pnl_withdrawal;
    perpetuals.permissions.allow_collateral_withdrawal = params.allow_collateral_withdrawal;
    perpetuals.permissions.allow_size_change = params.allow_size_change;
    
    // Record transfer_authority PDA bump
    // This is needed for token account authority derivations
    perpetuals.transfer_authority_bump = ctx.bumps.transfer_authority;
    
    // Record perpetuals PDA bump
    // This is needed for future account derivations
    perpetuals.perpetuals_bump = ctx.bumps.perpetuals;
    
    // Set inception time to current time
    // This is used as a reference point for time-based calculations
    perpetuals.inception_time = perpetuals.get_time()?;

    // Validate perpetuals configuration
    // Ensures all parameters are within acceptable ranges
    if !perpetuals.validate() {
        return err!(PerpetualsError::InvalidPerpetualsConfig);
    }

    Ok(())
}