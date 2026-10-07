use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, FreezeDelegate, Plugin},
};

use crate::{
    constants::*,
    error::ErrorCode,
    helpers::{load_collection_with_plugins, read_total_staked},
    state::StakeState,
};

#[derive(Accounts)]
pub struct Unstake<'info> {
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
    /// CHECK: mpl-core asset.
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
    /// CHECK: the mpl-core program.
    #[account(address = mpl_core::ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_unstake(ctx: Context<Unstake>) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let current = read_total_staked(&*load_collection_with_plugins(&ctx.accounts.collection)?)?;
    let next = current.saturating_sub(1);

    let signer_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    // Thaw the asset.
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[signer_seeds])?;

    // Collection-level stat: decrement total_staked.
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
