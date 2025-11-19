//! Core perpetuals program state and utility functions
//! 
//! This module contains the main Perpetuals account structure and helper functions
//! for token transfers, account management, and permission controls.

use {
    anchor_lang::prelude::*,
    anchor_spl::token::{Burn, MintTo, Transfer},
};

/// Price and associated fee structure
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct PriceAndFee {
    /// Price value
    pub price: u64,
    /// Fee amount
    pub fee: u64,
}

/// Amount and associated fee structure
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct AmountAndFee {
    /// Amount value
    pub amount: u64,
    /// Fee amount
    pub fee: u64,
}

/// Price information for opening a new position
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct NewPositionPricesAndFee {
    /// Entry price for the position
    pub entry_price: u64,
    /// Liquidation price threshold
    pub liquidation_price: u64,
    /// Fee charged for opening the position
    pub fee: u64,
}

/// Swap result with input/output amounts and fees
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct SwapAmountAndFees {
    /// Output amount after swap
    pub amount_out: u64,
    /// Fee on input token
    pub fee_in: u64,
    /// Fee on output token
    pub fee_out: u64,
}

/// Profit and loss calculation result
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct ProfitAndLoss {
    /// Profit amount (if position is profitable)
    pub profit: u64,
    /// Loss amount (if position is at a loss)
    pub loss: u64,
}

/// Permission flags controlling which operations are allowed
#[derive(Copy, Clone, PartialEq, AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct Permissions {
    /// Allow token swaps
    pub allow_swap: bool,
    /// Allow adding liquidity to pools
    pub allow_add_liquidity: bool,
    /// Allow removing liquidity from pools
    pub allow_remove_liquidity: bool,
    /// Allow opening new positions
    pub allow_open_position: bool,
    /// Allow closing existing positions
    pub allow_close_position: bool,
    /// Allow withdrawing profit from positions
    pub allow_pnl_withdrawal: bool,
    /// Allow withdrawing collateral
    pub allow_collateral_withdrawal: bool,
    /// Allow changing position size
    pub allow_size_change: bool,
}

/// Main perpetuals program account
/// 
/// This is the root account that stores global program state,
/// permissions, and references to all pools.
#[account]
#[derive(Default, Debug)]
pub struct Perpetuals {
    /// Permission flags controlling allowed operations
    pub permissions: Permissions,
    /// List of pool account addresses managed by this program
    pub pools: Vec<Pubkey>,

    /// Bump seed for the transfer authority PDA
    pub transfer_authority_bump: u8,
    /// Bump seed for the perpetuals PDA
    pub perpetuals_bump: u8,
    /// Time of inception, also used as current wall clock time for testing
    pub inception_time: i64,
}

impl anchor_lang::Id for Perpetuals {
    fn id() -> Pubkey {
        crate::ID
    }
}

impl Perpetuals {
    /// Account size in bytes (8 byte discriminator + data)
    pub const LEN: usize = 8 + std::mem::size_of::<Perpetuals>();
    /// Basis points (BPS) decimal places (1 BPS = 0.01%)
    pub const BPS_DECIMALS: u8 = 4;
    /// Power of 10 for BPS calculations (10^4 = 10,000)
    pub const BPS_POWER: u128 = 10u64.pow(Self::BPS_DECIMALS as u32) as u128;
    /// Decimal places for price representation
    pub const PRICE_DECIMALS: u8 = 6;
    /// Decimal places for USD amounts
    pub const USD_DECIMALS: u8 = 6;
    /// Decimal places for LP (liquidity provider) tokens
    pub const LP_DECIMALS: u8 = Self::USD_DECIMALS;
    /// Decimal places for rate calculations (funding rates, etc.)
    pub const RATE_DECIMALS: u8 = 9;
    /// Power of 10 for rate calculations (10^9)
    pub const RATE_POWER: u128 = 10u64.pow(Self::RATE_DECIMALS as u32) as u128;

    /// Validate the perpetuals account state
    /// 
    /// # Returns
    /// true if valid
    pub fn validate(&self) -> bool {
        true
    }

    /// Get current time (test mode - uses inception_time)
    #[cfg(feature = "test")]
    pub fn get_time(&self) -> Result<i64> {
        Ok(self.inception_time)
    }

    /// Get current time from Solana clock sysvar (production mode)
    #[cfg(not(feature = "test"))]
    pub fn get_time(&self) -> Result<i64> {
        let time = solana_program::sysvar::clock::Clock::get()?.unix_timestamp;
        if time > 0 {
            Ok(time)
        } else {
            Err(ProgramError::InvalidAccountData.into())
        }
    }

    /// Validate that the program upgrade authority matches expected authority
    /// 
    /// # Arguments
    /// * `expected_upgrade_authority` - Expected upgrade authority pubkey
    /// * `program_data` - Program data account info
    /// * `program` - Perpetuals program instance
    /// 
    /// # Returns
    /// Error if upgrade authority doesn't match
    pub fn validate_upgrade_authority(
        expected_upgrade_authority: Pubkey,
        program_data: &AccountInfo,
        program: &Program<crate::program::Perpetuals>,
    ) -> Result<()> {
        if let Some(programdata_address) = program.programdata_address()? {
            require_keys_eq!(
                programdata_address,
                program_data.key(),
                ErrorCode::InvalidProgramExecutable
            );
            let program_data: Account<ProgramData> = Account::try_from(program_data)?;
            if let Some(current_upgrade_authority) = program_data.upgrade_authority_address {
                if current_upgrade_authority != Pubkey::default() {
                    require_keys_eq!(
                        current_upgrade_authority,
                        expected_upgrade_authority,
                        ErrorCode::ConstraintOwner
                    );
                }
            }
        } // otherwise not upgradeable

        Ok(())
    }

    /// Transfer tokens using the program's transfer authority PDA
    /// 
    /// # Arguments
    /// * `from` - Source token account
    /// * `to` - Destination token account
    /// * `authority` - Transfer authority PDA
    /// * `token_program` - Token program account
    /// * `amount` - Amount of tokens to transfer
    pub fn transfer_tokens<'info>(
        &self,
        from: AccountInfo<'info>,
        to: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        let authority_seeds: &[&[&[u8]]] =
            &[&[b"transfer_authority", &[self.transfer_authority_bump]]];

        let context = CpiContext::new(
            token_program,
            Transfer {
                from,
                to,
                authority,
            },
        )
        .with_signer(authority_seeds);

        anchor_spl::token::transfer(context, amount)
    }

    /// Transfer tokens from a user account (user signs the transaction)
    /// 
    /// # Arguments
    /// * `from` - Source token account (user-owned)
    /// * `to` - Destination token account
    /// * `authority` - User's authority (signer)
    /// * `token_program` - Token program account
    /// * `amount` - Amount of tokens to transfer
    pub fn transfer_tokens_from_user<'info>(
        &self,
        from: AccountInfo<'info>,
        to: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        let context = CpiContext::new(
            token_program,
            Transfer {
                from,
                to,
                authority,
            },
        );
        anchor_spl::token::transfer(context, amount)
    }

    /// Mint tokens using the program's transfer authority PDA
    /// 
    /// # Arguments
    /// * `mint` - Token mint account
    /// * `to` - Destination token account
    /// * `authority` - Transfer authority PDA
    /// * `token_program` - Token program account
    /// * `amount` - Amount of tokens to mint
    pub fn mint_tokens<'info>(
        &self,
        mint: AccountInfo<'info>,
        to: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        let authority_seeds: &[&[&[u8]]] =
            &[&[b"transfer_authority", &[self.transfer_authority_bump]]];

        let context = CpiContext::new(
            token_program,
            MintTo {
                mint,
                to,
                authority,
            },
        )
        .with_signer(authority_seeds);

        anchor_spl::token::mint_to(context, amount)
    }

    /// Burn tokens from an account
    /// 
    /// # Arguments
    /// * `mint` - Token mint account
    /// * `from` - Token account to burn from
    /// * `authority` - Authority that owns the token account
    /// * `token_program` - Token program account
    /// * `amount` - Amount of tokens to burn
    pub fn burn_tokens<'info>(
        &self,
        mint: AccountInfo<'info>,
        from: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        amount: u64,
    ) -> Result<()> {
        let context = CpiContext::new(
            token_program,
            Burn {
                mint,
                from,
                authority,
            },
        );

        anchor_spl::token::burn(context, amount)
    }

    /// Check if an account is empty (no data or zero lamports)
    /// 
    /// # Arguments
    /// * `account_info` - Account to check
    /// 
    /// # Returns
    /// true if account is empty
    pub fn is_empty_account(account_info: &AccountInfo) -> Result<bool> {
        Ok(account_info.try_data_is_empty()? || account_info.try_lamports()? == 0)
    }

    /// Close a token account and transfer remaining lamports to receiver
    /// 
    /// # Arguments
    /// * `receiver` - Account to receive the closed account's lamports
    /// * `token_account` - Token account to close
    /// * `token_program` - Token program account
    /// * `authority` - Authority PDA for the token account
    /// * `seeds` - Seeds for signing the authority PDA
    pub fn close_token_account<'info>(
        receiver: AccountInfo<'info>,
        token_account: AccountInfo<'info>,
        token_program: AccountInfo<'info>,
        authority: AccountInfo<'info>,
        seeds: &[&[&[u8]]],
    ) -> Result<()> {
        let cpi_accounts = anchor_spl::token::CloseAccount {
            account: token_account,
            destination: receiver,
            authority,
        };
        let cpi_context = anchor_lang::context::CpiContext::new(token_program, cpi_accounts);

        anchor_spl::token::close_account(cpi_context.with_signer(seeds))
    }

    /// Transfer SOL from a program-owned account (direct lamport manipulation)
    /// 
    /// # Arguments
    /// * `program_owned_source_account` - Source account owned by the program
    /// * `destination_account` - Destination account
    /// * `amount` - Amount of SOL (lamports) to transfer
    pub fn transfer_sol_from_owned<'a>(
        program_owned_source_account: AccountInfo<'a>,
        destination_account: AccountInfo<'a>,
        amount: u64,
    ) -> Result<()> {
        **destination_account.try_borrow_mut_lamports()? = destination_account
            .try_lamports()?
            .checked_add(amount)
            .ok_or(ProgramError::InsufficientFunds)?;

        let source_balance = program_owned_source_account.try_lamports()?;
        **program_owned_source_account.try_borrow_mut_lamports()? = source_balance
            .checked_sub(amount)
            .ok_or(ProgramError::InsufficientFunds)?;

        Ok(())
    }

    /// Transfer SOL using system program CPI
    /// 
    /// # Arguments
    /// * `source_account` - Source account (must be signer)
    /// * `destination_account` - Destination account
    /// * `system_program` - System program account
    /// * `amount` - Amount of SOL (lamports) to transfer
    pub fn transfer_sol<'a>(
        source_account: AccountInfo<'a>,
        destination_account: AccountInfo<'a>,
        system_program: AccountInfo<'a>,
        amount: u64,
    ) -> Result<()> {
        let cpi_accounts = anchor_lang::system_program::Transfer {
            from: source_account,
            to: destination_account,
        };
        let cpi_context = anchor_lang::context::CpiContext::new(system_program, cpi_accounts);

        anchor_lang::system_program::transfer(cpi_context, amount)
    }

    /// Reallocate an account to a new size
    /// 
    /// Transfers additional lamports if needed to cover rent for the new size.
    /// 
    /// # Arguments
    /// * `funding_account` - Account to fund the reallocation
    /// * `target_account` - Account to reallocate
    /// * `system_program` - System program account
    /// * `new_len` - New account size in bytes
    /// * `zero_init` - Whether to zero-initialize the new space
    pub fn realloc<'a>(
        funding_account: AccountInfo<'a>,
        target_account: AccountInfo<'a>,
        system_program: AccountInfo<'a>,
        new_len: usize,
        zero_init: bool,
    ) -> Result<()> {
        let new_minimum_balance = Rent::get()?.minimum_balance(new_len);
        let lamports_diff = new_minimum_balance.saturating_sub(target_account.try_lamports()?);

        Perpetuals::transfer_sol(
            funding_account,
            target_account.clone(),
            system_program,
            lamports_diff,
        )?;

        target_account
            .realloc(new_len, zero_init)
            .map_err(|_| ProgramError::InvalidRealloc.into())
    }
}