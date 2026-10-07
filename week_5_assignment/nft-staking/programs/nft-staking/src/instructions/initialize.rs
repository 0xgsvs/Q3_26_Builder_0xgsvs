use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

use crate::{constants::*, error::ErrorCode, helpers::load_collection, state::Config};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [CONFIG, collection.key().as_ref()],
        bump,
    )]
    pub config: Account<'info, Config>,
    /// CHECK: mpl-core collection, deserialized in the handler.
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority of the collection.
    #[account(
        seeds = [UPDATE_AUTHORITY, collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = config,
        seeds = [REWARDS_MINT, config.key().as_ref()],
        bump,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_initialize(ctx: Context<Initialize>, reward_bps: u16) -> Result<()> {
    // Sanity bound: a reward rate above 100% per period is a misconfiguration.
    require!(reward_bps <= 10_000, ErrorCode::InvalidRewardsBps);

    let collection = load_collection(&ctx.accounts.collection)?;
    require_keys_eq!(
        collection.update_authority,
        ctx.accounts.update_authority.key(),
        ErrorCode::InvalidUpdateAuthority
    );

    ctx.accounts.config.set_inner(Config {
        reward_bps,
        rewards_bump: ctx.bumps.rewards_mint,
        bump: ctx.bumps.config,
    });

    Ok(())
}
