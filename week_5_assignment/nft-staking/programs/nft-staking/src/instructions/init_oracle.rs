use anchor_lang::prelude::*;

use crate::{
    constants::*,
    helpers::load_collection,
    state::{ORACLE_PASS, ORACLE_REJECTED, Oracle, OracleVault},
};

#[derive(Accounts)]
pub struct InitOracle<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: mpl-core collection; verified by deserialization.
    pub collection: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = Oracle::DISCRIMINATOR.len() + Oracle::INIT_SPACE,
        seeds = [ORACLE, collection.key().as_ref()],
        bump,
    )]
    pub oracle: Account<'info, Oracle>,
    #[account(
        init,
        payer = payer,
        space = OracleVault::DISCRIMINATOR.len() + OracleVault::INIT_SPACE,
        seeds = [ORACLE_VAULT, collection.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, OracleVault>,
    pub system_program: Program<'info, System>,
}

pub fn handle_init_oracle(ctx: Context<InitOracle>) -> Result<()> {
    load_collection(&ctx.accounts.collection)?;

    // Start closed (Rejected) so no transfer can happen before the first crank.
    ctx.accounts.oracle.set_inner(Oracle {
        variant: 1,
        create: ORACLE_PASS,
        transfer: ORACLE_REJECTED,
        burn: ORACLE_PASS,
        update: ORACLE_PASS,
        bump: ctx.bumps.oracle,
    });
    ctx.accounts.vault.set_inner(OracleVault {
        bump: ctx.bumps.vault,
    });

    Ok(())
}
