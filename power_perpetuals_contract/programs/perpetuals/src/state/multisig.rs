//! Multisig state and routines
//! 
//! This module implements a multisignature scheme for admin operations.
//! Multiple admin signers must approve instructions before they are executed.

use {
    crate::{error::PerpetualsError, math},
    ahash::AHasher,
    anchor_lang::prelude::*,
    std::hash::Hasher,
};

/// Multisig account for collecting admin signatures
/// 
/// Stores signer addresses, signature status, and instruction metadata.
/// Uses zero_copy for efficient storage.
#[repr(C, packed)]
#[account(zero_copy)]
#[derive(Default)]
pub struct Multisig {
    /// Total number of signers in the multisig
    pub num_signers: u8,
    /// Number of signatures collected so far
    pub num_signed: u8,
    /// Minimum number of signatures required to execute
    pub min_signatures: u8,
    /// Length of instruction accounts array (for validation)
    pub instruction_accounts_len: u8,
    /// Length of instruction data (for validation)
    pub instruction_data_len: u16,
    /// Hash of instruction accounts and data (for validation)
    pub instruction_hash: u64,
    /// Array of signer public keys (up to MAX_SIGNERS)
    pub signers: [Pubkey; 6], // Multisig::MAX_SIGNERS
    /// Signature status array (1 = signed, 0 = not signed)
    pub signed: [u8; 6],      // Multisig::MAX_SIGNERS
    /// Bump seed for the multisig PDA
    pub bump: u8,
}

/// Admin instruction types requiring multisig approval
/// 
/// Each instruction type is encoded as a u8 for serialization.
#[derive(Debug, Clone, Copy)]
pub enum AdminInstruction {
    /// Add a new liquidity pool
    AddPool,
    /// Remove an existing pool
    RemovePool,
    /// Add a new custody (token) to a pool
    AddCustody,
    /// Remove a custody from a pool
    RemoveCustody,
    /// Update multisig signers and thresholds
    SetAdminSigners,
    /// Update custody configuration
    SetCustodyConfig,
    /// Update pool permissions
    SetPermissions,
    /// Update borrow rate parameters
    SetBorrowRate,
    /// Withdraw collected fees (tokens)
    WithdrawFees,
    /// Withdraw collected fees (SOL)
    WithdrawSolFees,
    /// Set custom oracle price (for testing)
    SetCustomOraclePrice,
    /// Set test time (for testing)
    SetTestTime,
    /// Upgrade custody account
    UpgradeCustody,
}

impl Multisig {
    /// Maximum number of signers allowed in multisig
    pub const MAX_SIGNERS: usize = 6;
    /// Account size in bytes (8 byte discriminator + data)
    pub const LEN: usize = 8 + std::mem::size_of::<Multisig>();

    /// Compute hash of instruction accounts and data
    /// 
    /// This hash is used to ensure all admins are signing the same instruction.
    /// Uses fast non-cryptographic hashing (AHasher) for performance.
    /// 
    /// # Arguments
    /// * `instruction_accounts` - Account infos for the instruction
    /// * `instruction_data` - Serialized instruction parameters
    /// 
    /// # Returns
    /// 64-bit hash value
    pub fn get_instruction_hash(
        instruction_accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> u64 {
        use core::hash::BuildHasher;
        let build_hasher = ahash::RandomState::with_seeds(697533735114380, 537268678243635, 0, 0);
        let mut hasher = build_hasher.build_hasher();
        for account in instruction_accounts {
            hasher.write(account.key.as_ref());
        }
        if !instruction_data.is_empty() {
            hasher.write(instruction_data);
        }
        hasher.finish()
    }

    /// Get all account infos from context (including remaining accounts)
    /// 
    /// Used to compute instruction hash for multisig validation.
    /// 
    /// # Arguments
    /// * `ctx` - Anchor context
    /// 
    /// # Returns
    /// Vector of all account infos
    pub fn get_account_infos<'info, T: ToAccountInfos<'info> + anchor_lang::Bumps>(
        ctx: &Context<'_, '_, '_, 'info, T>,
    ) -> Vec<AccountInfo<'info>> {
        let mut infos = ctx.accounts.to_account_infos();
        infos.extend_from_slice(ctx.remaining_accounts);
        infos
    }

    /// Serialize instruction type and parameters
    /// 
    /// Instruction type is appended as a u8 byte at the end.
    /// 
    /// # Arguments
    /// * `instruction_type` - Type of admin instruction
    /// * `params` - Instruction parameters to serialize
    /// 
    /// # Returns
    /// Serialized bytes: [params_bytes..., instruction_type as u8]
    pub fn get_instruction_data<T: AnchorSerialize>(
        instruction_type: AdminInstruction,
        params: &T,
    ) -> Result<Vec<u8>> {
        let mut res = vec![];
        AnchorSerialize::serialize(&params, &mut res)?;
        res.push(instruction_type as u8);
        Ok(res)
    }

    /// Initialize multisig with a new set of signers
    /// 
    /// Validates signers and sets up the multisig account.
    /// Resets all signature tracking.
    /// 
    /// # Arguments
    /// * `admin_signers` - Array of admin signer account infos
    /// * `min_signatures` - Minimum signatures required to execute
    /// 
    /// # Returns
    /// Error if validation fails (empty signers, invalid count, duplicates)
    pub fn set_signers(&mut self, admin_signers: &[AccountInfo], min_signatures: u8) -> Result<()> {
        if admin_signers.is_empty() || min_signatures == 0 {
            msg!("Error: At least one signer is required");
            return Err(ProgramError::MissingRequiredSignature.into());
        }
        if (min_signatures as usize) > admin_signers.len() {
            msg!(
                "Error: Number of min signatures ({}) exceeded number of signers ({})",
                min_signatures,
                admin_signers.len(),
            );
            return Err(ProgramError::InvalidArgument.into());
        }
        if admin_signers.len() > Multisig::MAX_SIGNERS {
            msg!(
                "Error: Number of signers ({}) exceeded max ({})",
                admin_signers.len(),
                Multisig::MAX_SIGNERS
            );
            return Err(ProgramError::InvalidArgument.into());
        }

        let mut signers: [Pubkey; Multisig::MAX_SIGNERS] = Default::default();
        let mut signed: [u8; Multisig::MAX_SIGNERS] = Default::default();

        for idx in 0..admin_signers.len() {
            if signers.contains(admin_signers[idx].key) {
                msg!("Error: Duplicate signer {}", admin_signers[idx].key);
                return Err(ProgramError::InvalidArgument.into());
            }
            signers[idx] = *admin_signers[idx].key;
            signed[idx] = 0;
        }

        *self = Multisig {
            num_signers: admin_signers.len() as u8,
            num_signed: 0,
            min_signatures,
            instruction_accounts_len: 0,
            instruction_data_len: 0,
            instruction_hash: 0,
            signers,
            signed,
            bump: self.bump,
        };

        Ok(())
    }

    /// Sign the multisig instruction
    /// 
    /// Validates the signer, checks instruction hash, and records the signature.
    /// If this is a new instruction, resets signature tracking.
    /// 
    /// # Arguments
    /// * `signer_account` - Account info of the signer
    /// * `instruction_accounts` - All account infos for the instruction
    /// * `instruction_data` - Serialized instruction data
    /// 
    /// # Returns
    /// * `Ok(0)` - Enough signatures collected, instruction can proceed
    /// * `Ok(n)` - More signatures needed (n = signatures_left)
    /// * `Err` - Invalid signer, duplicate signature, or already executed
    pub fn sign_multisig(
        &mut self,
        signer_account: &AccountInfo,
        instruction_accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> Result<u8> {
        // return early if not a signer
        if !signer_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature.into());
        }

        // find index of current signer or return error if not found
        let signer_idx = if let Ok(idx) = self.get_signer_index(signer_account.key) {
            idx
        } else {
            return err!(PerpetualsError::MultisigAccountNotAuthorized);
        };

        // if single signer return Ok to continue
        if self.num_signers <= 1 {
            return Ok(0);
        }

        let instruction_hash =
            Multisig::get_instruction_hash(instruction_accounts, instruction_data);
        if instruction_hash != self.instruction_hash
            || instruction_accounts.len() != self.instruction_accounts_len as usize
            || instruction_data.len() != self.instruction_data_len as usize
        {
            // if this is a new instruction reset the data
            self.num_signed = 1;
            self.instruction_accounts_len = instruction_accounts.len() as u8;
            self.instruction_data_len = instruction_data.len() as u16;
            self.instruction_hash = instruction_hash;
            self.signed.fill(0);
            self.signed[signer_idx] = 1;
            //multisig.pack(*multisig_account.try_borrow_mut_data()?)?;

            math::checked_sub(self.min_signatures, 1)
        } else if self.signed[signer_idx] == 1 {
            err!(PerpetualsError::MultisigAlreadySigned)
        } else if self.num_signed < self.min_signatures {
            // count the signature in
            self.num_signed = math::checked_add(self.num_signed, 1)?;
            self.signed[signer_idx] = 1;

            if self.num_signed == self.min_signatures {
                Ok(0)
            } else {
                math::checked_sub(self.min_signatures, self.num_signed)
            }
        } else {
            err!(PerpetualsError::MultisigAlreadyExecuted)
        }
    }

    /// Remove a signature from the multisig
    /// 
    /// Allows an admin to revoke their signature before execution.
    /// Useful if instruction parameters need to change.
    /// 
    /// # Arguments
    /// * `signer_account` - Account info of the signer removing their signature
    /// 
    /// # Returns
    /// Error if signer is not authorized or not found
    pub fn unsign_multisig(&mut self, signer_account: &AccountInfo) -> Result<()> {
        // return early if not a signer
        if !signer_account.is_signer {
            return Err(ProgramError::MissingRequiredSignature.into());
        }

        // if single signer return
        if self.num_signers <= 1 || self.num_signed == 0 {
            return Ok(());
        }

        // find index of current signer or return error if not found
        let signer_idx = if let Ok(idx) = self.get_signer_index(signer_account.key) {
            idx
        } else {
            return err!(PerpetualsError::MultisigAccountNotAuthorized);
        };

        // if not signed by this account return
        if self.signed[signer_idx] == 0 {
            return Ok(());
        }

        // remove signature
        self.num_signed = math::checked_sub(self.num_signed, 1)?;
        self.signed[signer_idx] = 0;

        Ok(())
    }

    /// Get the array index of a signer
    /// 
    /// # Arguments
    /// * `signer` - Public key of the signer
    /// 
    /// # Returns
    /// Index in the signers array, or error if not found
    pub fn get_signer_index(&self, signer: &Pubkey) -> Result<usize> {
        for i in 0..self.num_signers as usize {
            if &self.signers[i] == signer {
                return Ok(i);
            }
        }
        err!(PerpetualsError::MultisigAccountNotAuthorized)
    }

    /// Check if an account is one of the multisig signers
    /// 
    /// # Arguments
    /// * `key` - Public key to check
    /// 
    /// # Returns
    /// true if the key is a signer, false otherwise
    pub fn is_signer(&self, key: &Pubkey) -> Result<bool> {
        Ok(self.get_signer_index(key).is_ok())
    }
}