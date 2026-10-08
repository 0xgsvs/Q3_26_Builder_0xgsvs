use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, MintTo, TokenAccount, TokenInterface, mint_to},
};
use mpl_core::{
    instructions::{
        BurnV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder,
    },
    types::{Attribute, Attributes, FreezeDelegate, Plugin},
};

use crate::{
    constants::*,
    error::ErrorCode,
    helpers::{load_collection, load_collection_with_plugins, read_total_staked},
    state::{Config, StakeState},
};

#[derive(Accounts)]
pub struct BurnStakedNft<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        mut,
        close = owner,
        seeds = [STAKE, asset.key().as_ref()],
        bump = stake_state.bump,
        has_one = owner @ ErrorCode::InvalidOwner,
    )]
    pub stake_state: Account<'info, StakeState>,
    /// CHECK: mpl-core asset being burned.
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: mpl-core collection.
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority of the collection.
    #[account(
        seeds = [UPDATE_AUTHORITY, collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        seeds = [CONFIG, collection.key().as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,
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
    /// CHECK: the mpl-core program.
    #[account(address = mpl_core::ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_burn_staked_nft(ctx: Context<BurnStakedNft>) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    load_collection(&ctx.accounts.collection)?;
    let signer_seeds = &[UPDATE_AUTHORITY, collection_key.as_ref(), &[ctx
        .bumps
        .update_authority]];

    // A frozen asset cannot be burned; thaw it first through the freeze delegate.
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[signer_seeds])?;

    // Burn the NFT through the burn delegate held by the staking PDA.
    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(&[signer_seeds])?;

    // One-time bonus to the owner.
    let config_seeds = &[CONFIG, collection_key.as_ref(), &[ctx.accounts.config.bump]];
    mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            MintTo {
                mint: ctx.accounts.rewards_mint.to_account_info(),
                to: ctx.accounts.owner_rewards_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            &[config_seeds],
        ),
        BURN_BONUS,
    )?;

    // Collection-level stat: the asset leaves the staked set.
    let current = read_total_staked(&*load_collection_with_plugins(&ctx.accounts.collection)?)?;
    let next = current
        .checked_sub(1)
        .ok_or(ErrorCode::TotalStakedUnderflow)?;
    UpdateCollectionPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .collection(&ctx.accounts.collection.to_account_info())
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::Attributes(Attributes {
            attribute_list: vec![Attribute {
                key: TOTAL_STAKED.to_string(),
                value: next.to_string(),
            }],
        }))
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
