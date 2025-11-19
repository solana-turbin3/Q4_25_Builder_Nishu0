use anchor_lang::prelude::*;

pub mod instructions;
pub mod state;

use instructions::initialize::*;

declare_id!("GxegSBD3PQFQjzYui524RWraUM7SzQsBZpWnwBbpLQbk");

#[program]
pub mod perpetuals {
    use super::*;

    /// Initialize a new power perpetuals pool
    pub fn initialize(ctx: Context<Initialize>, oracle: Pubkey) -> Result<()> {
        instructions::initialize::initialize(ctx, oracle)
    }
}
