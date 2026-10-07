use anchor_lang::prelude::*;
use mpl_core::instructions::TransferV1CpiBuilder;

use crate::{constants::*, helpers::load_asset};

#[derive(Accounts)]
pub struct TransferAsset<'info> {
    pub owner: Signer<'info>,
    /// CHECK: mpl-core asset, deserialized in the handler.
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: mpl-core collection carrying the Oracle adapter.
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: the oracle PDA read by mpl-core during the Transfer lifecycle.
    #[account(seeds = [ORACLE, collection.key().as_ref()], bump)]
    pub oracle: UncheckedAccount<'info>,
    /// CHECK: the recipient; only its key is used by mpl-core.
    #[account(mut)]
    pub new_owner: UncheckedAccount<'info>,
    /// CHECK: the mpl-core program.
    #[account(address = mpl_core::ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_transfer_asset(ctx: Context<TransferAsset>) -> Result<()> {
    let asset = load_asset(&ctx.accounts.asset)?;
    require_keys_eq!(asset.owner, ctx.accounts.owner.key(), crate::error::ErrorCode::InvalidOwner);

    TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.owner.to_account_info()))
        .new_owner(&ctx.accounts.new_owner.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        // The oracle account must be visible to mpl-core's lifecycle check.
        .add_remaining_account(&ctx.accounts.oracle.to_account_info(), false, false)
        .invoke()?;

    Ok(())
}