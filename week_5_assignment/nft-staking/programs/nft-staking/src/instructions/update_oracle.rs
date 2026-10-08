use anchor_lang::prelude::*;

use crate::{
    constants::*,
    state::{ORACLE_APPROVED, ORACLE_REJECTED, Oracle, OracleVault},
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
    let transfer = if open {
        ORACLE_APPROVED
    } else {
        ORACLE_REJECTED
    };

    ctx.accounts.oracle.transfer = transfer;

    // Pay the caller once per open/close boundary: only when the nearby
    // boundary is later than the last rewarded one.
    if let Some(boundary) = nearby_boundary(clock.unix_timestamp) {
        if boundary > ctx.accounts.oracle.last_rewarded_boundary {
            pay_reward(&ctx.accounts.vault, &ctx.accounts.crank)?;
            ctx.accounts.oracle.last_rewarded_boundary = boundary;
        }
    }

    Ok(())
}

/// Absolute unix timestamp of the open or close boundary within
/// `BOUNDARY_TOLERANCE` seconds of `unix_timestamp`, if any.
fn nearby_boundary(unix_timestamp: i64) -> Option<i64> {
    let day = unix_timestamp.div_euclid(86_400);
    let into_day = unix_timestamp.rem_euclid(86_400);
    let open = OPEN_HOUR as i64 * 3_600;
    let close = CLOSE_HOUR as i64 * 3_600;
    let offset = if (into_day - open).abs() <= BOUNDARY_TOLERANCE {
        open
    } else if (into_day - close).abs() <= BOUNDARY_TOLERANCE {
        close
    } else {
        return None;
    };
    Some(day * 86_400 + offset)
}

/// Moves `ORACLE_REWARD` lamports from the program-owned vault to the crank,
/// keeping at least `VAULT_MIN_LAMPORTS` in the vault so it stays rent-exempt.
fn pay_reward<'info>(
    vault: &Account<'info, OracleVault>,
    crank: &UncheckedAccount<'info>,
) -> Result<()> {
    let available = vault
        .to_account_info()
        .lamports()
        .saturating_sub(VAULT_MIN_LAMPORTS);
    let reward = ORACLE_REWARD.min(available);
    **vault.to_account_info().try_borrow_mut_lamports()? -= reward;
    **crank.to_account_info().try_borrow_mut_lamports()? += reward;
    Ok(())
}
