use anchor_lang::prelude::*;

use crate::{
    constants::*,
    state::{Oracle, OracleVault, ORACLE_APPROVED, ORACLE_REJECTED},
};

#[derive(Accounts)]
pub struct UpdateOracle<'info> {
    /// CHECK: permissionless crank; receives the reward when due.
    #[account(mut)]
    pub crank: UncheckedAccount<'info>,
    /// CHECK: mpl-core collection; only its key is used.
    pub collection: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [ORACLE, collection.key().as_ref()],
        bump = oracle.bump,
    )]
    pub oracle: Account<'info, Oracle>,
    #[account(
        mut,
        seeds = [ORACLE_VAULT, collection.key().as_ref()],
        bump = vault.bump,
    )]
    pub vault: Account<'info, OracleVault>,
    pub system_program: Program<'info, System>,
}

/// UTC hour (0..24) for a unix timestamp.
fn utc_hour(unix_timestamp: i64) -> u8 {
    let seconds = unix_timestamp.rem_euclid(86_400);
    (seconds / 3_600) as u8
}

pub fn handle_update_oracle(ctx: Context<UpdateOracle>) -> Result<()> {
    let clock = Clock::get()?;
    let hour = utc_hour(clock.unix_timestamp);
    let open = (OPEN_HOUR..CLOSE_HOUR).contains(&hour);
    let transfer = if open { ORACLE_APPROVED } else { ORACLE_REJECTED };

    ctx.accounts.oracle.transfer = transfer;
    ctx.accounts.oracle.last_slot = clock.slot;
    ctx.accounts.oracle.last_hour = hour;

    Ok(())
}