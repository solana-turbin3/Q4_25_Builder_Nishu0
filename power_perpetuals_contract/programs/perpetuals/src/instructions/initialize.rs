use anchor_lang::prelude::*;
use crate::state::Pool;

/// Initialize a new power perpetuals pool
pub fn initialize(
    ctx: Context<Initialize>,
    oracle: Pubkey,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    let clock = Clock::get()?;

    pool.bump = ctx.bumps.pool;
    pool.authority = ctx.accounts.authority.key();
    pool.total_collateral = 0;
    pool.total_notional = 0;
    pool.oracle = oracle;
    pool.funding_rate = 0;
    pool.last_update = clock.unix_timestamp;

    msg!("Pool initialized with oracle: {}", oracle);
    msg!("Authority: {}", pool.authority);

    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = Pool::LEN,
        seeds = [b"pool"],
        bump
    )]
    pub pool: Account<'info, Pool>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

