use anchor_lang::prelude::*;
use mpl_core::{
    instructions::CreateV2CpiBuilder,
    types::{BurnDelegate, FreezeDelegate, Plugin, PluginAuthority, PluginAuthorityPair},
};

use crate::{constants::*, helpers::load_collection};

#[derive(Accounts)]
pub struct CreateAsset<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: created by the mpl-core CPI.
    #[account(mut)]
    pub asset: Signer<'info>,
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

pub fn handle_create_asset(ctx: Context<CreateAsset>, name: String, uri: String) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    load_collection(&ctx.accounts.collection)?;

    let signer_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    // The asset carries a freeze delegate owned by our PDA so staking can freeze
    // it. It starts thawed.
    let plugins = vec![
        PluginAuthorityPair {
            plugin: Plugin::FreezeDelegate(FreezeDelegate { frozen: false }),
            authority: Some(PluginAuthority::UpdateAuthority),
        },
        PluginAuthorityPair {
            plugin: Plugin::BurnDelegate(BurnDelegate {}),
            authority: Some(PluginAuthority::UpdateAuthority),
        },
    ];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .payer(&ctx.accounts.payer.to_account_info())
        .owner(Some(&ctx.accounts.payer.to_account_info()))
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(name)
        .uri(uri)
        .plugins(plugins)
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
