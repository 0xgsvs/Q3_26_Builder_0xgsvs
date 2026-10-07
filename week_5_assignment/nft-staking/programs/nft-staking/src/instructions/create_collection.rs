use anchor_lang::prelude::*;
use mpl_core::{
    instructions::CreateCollectionV2CpiBuilder,
    types::{
        Attribute, Attributes, ExternalCheckResult, ExternalPluginAdapterInitInfo,
        HookableLifecycleEvent, OracleInitInfo, PermanentFreezeDelegate, Plugin, PluginAuthority,
        PluginAuthorityPair, ValidationResultsOffset,
    },
};

use crate::constants::*;

#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: created by the mpl-core CPI; verified to be the collection PDA below.
    #[account(mut, seeds = [COLLECTION, payer.key().as_ref()], bump)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority that signs the mpl-core CPI.
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

pub fn handle_create_collection(
    ctx: Context<CreateCollection>,
    name: String,
    uri: String,
) -> Result<()> {
    let collection_key = ctx.accounts.collection.key();
    let payer_key = ctx.accounts.payer.key();
    let collection_seeds = &[COLLECTION, payer_key.as_ref(), &[ctx.bumps.collection]];
    let update_authority_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    // The collection owns a permanent freeze delegate, so the staking program can
    // freeze an asset on stake and thaw it on unstake. Its authority is this PDA.
    let plugins = vec![
        PluginAuthorityPair {
            plugin: Plugin::PermanentFreezeDelegate(PermanentFreezeDelegate { frozen: false }),
            authority: Some(PluginAuthority::UpdateAuthority),
        },
        PluginAuthorityPair {
            plugin: Plugin::Attributes(Attributes {
                attribute_list: vec![Attribute {
                    key: TOTAL_STAKED.to_string(),
                    value: "0".to_string(),
                }],
            }),
            authority: Some(PluginAuthority::UpdateAuthority),
        },
    ];

    // A reject-only Oracle external plugin adapter gates asset transfers on the
    // per-collection oracle PDA (created later by `init_oracle`, read at Anchor
    // offset 8).
    let (oracle, _) = Pubkey::find_program_address(&[ORACLE, collection_key.as_ref()], &crate::ID);
    let external_plugin_adapters = vec![ExternalPluginAdapterInitInfo::Oracle(OracleInitInfo {
        base_address: oracle,
        init_plugin_authority: Some(PluginAuthority::UpdateAuthority),
        lifecycle_checks: vec![(
            HookableLifecycleEvent::Transfer,
            ExternalCheckResult { flags: 0b100 }, // reject-only
        )],
        base_address_config: None,
        results_offset: Some(ValidationResultsOffset::Anchor),
    })];

    CreateCollectionV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .collection(&ctx.accounts.collection.to_account_info())
        .update_authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .payer(&ctx.accounts.payer.to_account_info())
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(name)
        .uri(uri)
        .plugins(plugins)
        .external_plugin_adapters(external_plugin_adapters)
        .invoke_signed(&[collection_seeds, update_authority_seeds])?;

    Ok(())
}
