//! UpgradeCustody instruction handler
//! 
//! This instruction allows admins to upgrade a deprecated custody account to the current
//! custody format. This is used for migrating custody accounts after protocol upgrades.
//! The deprecated custody data is loaded, converted to the new format, and the account
//! is resized and reinitialized with the new structure.

use {
    crate::{
        error::PerpetualsError,
        state::{
            custody::{Custody, DeprecatedCustody},
            multisig::{AdminInstruction, Multisig},
            perpetuals::Perpetuals,
            pool::Pool,
        },
    },
    anchor_lang::prelude::*,
    std::{
        cmp,
        io::{self, Write},
    },
};

/// BPF-compatible writer for serializing data to account memory
/// 
/// This writer is used to write serialized custody data directly to account memory
/// in the BPF environment, where standard I/O operations are restricted.
#[derive(Debug, Default)]
pub struct BpfWriter<T> {
    /// Inner buffer to write to
    inner: T,
    /// Current write position
    pos: u64,
}

impl<T> BpfWriter<T> {
    /// Create a new BpfWriter with the given inner buffer
    pub fn new(inner: T) -> Self {
        Self { inner, pos: 0 }
    }
}

/// Implementation of Write trait for BPF environment
/// 
/// Uses sol_memcpy for memory operations which is compatible with BPF restrictions.
impl Write for BpfWriter<&mut [u8]> {
    /// Write bytes to the buffer
    /// Returns the number of bytes written (0 if buffer is full)
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pos >= self.inner.len() as u64 {
            return Ok(0);
        }

        // Calculate how many bytes can be written
        let amt = cmp::min(
            self.inner.len().saturating_sub(self.pos as usize),
            buf.len(),
        );
        // Use sol_memcpy for BPF-compatible memory copy
        anchor_lang::solana_program::program_memory::sol_memcpy(&mut self.inner[(self.pos as usize)..], buf, amt);
        self.pos += amt as u64;
        Ok(amt)
    }

    /// Write all bytes from the buffer
    /// Returns error if not all bytes could be written
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        if self.write(buf)? == buf.len() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "failed to write whole buffer",
            ))
        }
    }

    /// Flush is a no-op in BPF environment
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Accounts required for upgrading a deprecated custody account
#[derive(Accounts)]
pub struct UpgradeCustody<'info> {
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

    /// Pool account (mutable, may be used for validation)
    #[account(
        mut,
        seeds = [b"pool",
                 pool.name.as_bytes()],
        bump = pool.bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Deprecated custody account to upgrade (mutable, will be resized and reinitialized)
    /// 
    /// CHECK: Deprecated custody account, validated in function
    #[account(mut)]
    pub custody: AccountInfo<'info>,

    system_program: Program<'info, System>,
}

/// Parameters for upgrading custody account
/// 
/// Currently empty, but kept for consistency with other instructions.
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpgradeCustodyParams {}

/// Upgrade a deprecated custody account to the current format
/// 
/// This function migrates a deprecated custody account to the current custody structure.
/// The process:
/// 1. Validates multisig signatures (requires enough admin signatures)
/// 2. Validates the deprecated custody account (owner and data length)
/// 3. Loads deprecated custody data
/// 4. Converts deprecated custody data to new format (sets is_virtual to false)
/// 5. Validates new custody configuration
/// 6. Resizes account to new custody length
/// 7. Serializes new custody data to account memory
/// 
/// Returns the number of signatures still required (0 if fully signed and executed).
/// 
/// # Arguments
/// * `ctx` - Context containing all required accounts
/// * `params` - Parameters (currently unused)
/// 
/// # Returns
/// `Result<u8>` - Number of signatures still required (0 if complete), or error
pub fn upgrade_custody<'info>(
    ctx: Context<'_, '_, '_, 'info, UpgradeCustody<'info>>,
    params: &UpgradeCustodyParams,
) -> Result<u8> {
    // Validate multisig signatures
    // This instruction requires multisig approval from admins
    let mut multisig = ctx.accounts.multisig.load_mut()?;

    let signatures_left = multisig.sign_multisig(
        &ctx.accounts.admin,
        &Multisig::get_account_infos(&ctx)[1..],
        &Multisig::get_instruction_data(AdminInstruction::UpgradeCustody, params)?,
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

    // Load deprecated custody data
    msg!("Load deprecated custody");
    let custody_account = &ctx.accounts.custody;
    
    // Validate account owner is the perpetuals program
    if custody_account.owner != &crate::ID {
        return Err(anchor_lang::error::ErrorCode::ConstraintOwner.into());
    }
    
    // Validate account data length matches deprecated custody size
    if custody_account.try_data_len()? != DeprecatedCustody::LEN {
        return Err(anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into());
    }
    
    // Deserialize deprecated custody data
    // Use ptr::read to avoid lifetime issues with try_from_unchecked
    let deprecated_custody_data = {
        let data = custody_account.try_borrow_data()?;
        DeprecatedCustody::try_deserialize(&mut &data[..])?
    };

    // Convert deprecated custody data to new custody format
    // Most fields are copied directly, but is_virtual is set to false
    let custody_data = Custody {
        pool: deprecated_custody_data.pool,
        mint: deprecated_custody_data.mint,
        token_account: deprecated_custody_data.token_account,
        decimals: deprecated_custody_data.decimals,
        is_stable: deprecated_custody_data.is_stable,
        is_virtual: false, // Always set to false for upgraded custodies
        oracle: deprecated_custody_data.oracle,
        pricing: deprecated_custody_data.pricing,
        permissions: deprecated_custody_data.permissions,
        fees: deprecated_custody_data.fees,
        borrow_rate: deprecated_custody_data.borrow_rate,
        assets: deprecated_custody_data.assets,
        collected_fees: deprecated_custody_data.collected_fees,
        volume_stats: deprecated_custody_data.volume_stats,
        trade_stats: deprecated_custody_data.trade_stats,
        long_positions: deprecated_custody_data.long_positions,
        short_positions: deprecated_custody_data.short_positions,
        borrow_rate_state: deprecated_custody_data.borrow_rate_state,
        bump: deprecated_custody_data.bump,
        token_account_bump: deprecated_custody_data.token_account_bump,
    };

    // Validate new custody configuration
    if !custody_data.validate() {
        return err!(PerpetualsError::InvalidCustodyConfig);
    }

    // Resize custody account to new length
    msg!("Resize custody account");
    Perpetuals::realloc(
        ctx.accounts.admin.to_account_info(),
        ctx.accounts.custody.clone(),
        ctx.accounts.system_program.to_account_info(),
        Custody::LEN,
        true, // zero = true, initialize new space to zero
    )?;

    // Re-initialize the custody with new data
    msg!("Re-initialize the custody");
    // Verify account was resized correctly
    if custody_account.try_data_len()? != Custody::LEN {
        return Err(anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into());
    }
    
    // Get mutable reference to account data
    let mut data = custody_account.try_borrow_mut_data()?;
    let dst: &mut [u8] = &mut data;
    
    // Serialize new custody data to account memory using BPF-compatible writer
    let mut writer = BpfWriter::new(dst);
    custody_data.try_serialize(&mut writer)?;

    Ok(0)
}