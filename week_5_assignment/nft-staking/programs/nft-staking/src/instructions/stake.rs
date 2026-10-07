use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, FreezeDelegate, Plugin, UpdateAuthority},
};

use crate::{
    constants::*,
    error::ErrorCode,
    helpers::{load_asset, load_collection, load_collection_with_plugins, read_total_staked},
    state::StakeState,
};

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = StakeState::DISCRIMINATOR.len() + StakeState::INIT_SPACE,
        seeds = [STAKE, asset.key().as_ref()],
        bump,
    )]
    pub stake_state: Account<'info, StakeState>,
    /// CHECK: mpl-core asset, deserialized in the handler.
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: mpl-core collection, deserialized in the handler.
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

pub fn handle_stake(ctx: Context<Stake>) -> Result<()> {
    let asset = load_asset(&ctx.accounts.asset)?;
    let collection_key = ctx.accounts.collection.key();
    load_collection(&ctx.accounts.collection)?;

    // The asset must belong to the collection and be owned by the signer.
    require_keys_eq!(asset.owner, ctx.accounts.owner.key(), ErrorCode::InvalidOwner);
    require!(
        matches!(asset.update_authority, UpdateAuthority::Collection(c) if c == collection_key),
        ErrorCode::InvalidUpdateAuthority
    );

    let signer_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
        .invoke_signed(&[signer_seeds])?;

    ctx.accounts.stake_state.set_inner(StakeState {
        owner: ctx.accounts.owner.key(),
        staked_at: Clock::get()?.unix_timestamp,
        last_claim: 0,
        bump: ctx.bumps.stake_state,
    });

    // Collection-level stat: increment total_staked.
    let current = read_total_staked(&load_collection_with_plugins(&ctx.accounts.collection)?, TOTAL_STAKED)?;
    let next = current.checked_add(1).ok_or(ErrorCode::NumericalOverflow)?;
    update_collection_total_staked(ctx, next)?;

    Ok(())
}

/// Writes `total_staked = value` onto the collection's Attributes plugin.
fn update_collection_total_staked(ctx: Context<Stake>, value: u64) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    UpdateCollectionPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .collection(&ctx.accounts.collection.to_account_info())
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .plugin(Plugin::Attributes(Attributes {
            attribute_list: vec![Attribute {
                key: TOTAL_STAKED.to_string(),
                value: value.to_string(),
            }],
        }))
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}