use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, MintTo, TokenAccount, TokenInterface, mint_to},
};

use crate::{
    constants::*,
    error::ErrorCode,
    state::{Config, StakeState},
};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        mut,
        seeds = [STAKE, asset.key().as_ref()],
        bump = stake_state.bump,
        has_one = owner @ ErrorCode::InvalidOwner,
    )]
    pub stake_state: Account<'info, StakeState>,
    /// CHECK: the staked asset; only used to derive the stake PDA.
    pub asset: UncheckedAccount<'info>,
    #[account(
        seeds = [CONFIG, collection.key().as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,
    /// CHECK: the collection, matched against the config seeds above.
    pub collection: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [REWARDS_MINT, config.key().as_ref()],
        bump = config.rewards_bump,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = rewards_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program,
    )]
    pub owner_rewards_ata: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Rewards accrue from the later of staked_at and the last claim.
    let from = ctx
        .accounts
        .stake_state
        .last_claim
        .max(ctx.accounts.stake_state.staked_at);
    let elapsed = now.saturating_sub(from) as u64;
    let amount = elapsed
        .checked_mul(ctx.accounts.config.reward_bps as u64)
        .ok_or(ErrorCode::NumericalOverflow)?;

    if amount > 0 {
        let collection_key = ctx.accounts.collection.key();
        let signer_seeds = &[CONFIG, collection_key.as_ref(), &[ctx.accounts.config.bump]];
        mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                MintTo {
                    mint: ctx.accounts.rewards_mint.to_account_info(),
                    to: ctx.accounts.owner_rewards_ata.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                &[signer_seeds],
            ),
            amount,
        )?;
    }

    ctx.accounts.stake_state.last_claim = now;

    Ok(())
}
