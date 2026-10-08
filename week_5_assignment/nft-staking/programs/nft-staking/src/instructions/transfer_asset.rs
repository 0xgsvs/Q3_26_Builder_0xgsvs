use anchor_lang::prelude::*;
use mpl_core::instructions::TransferV1CpiBuilder;

use crate::{
    constants::*,
    error::ErrorCode,
    helpers::load_asset,
    state::{ORACLE_APPROVED, Oracle},
};

#[derive(Accounts)]
pub struct TransferAsset<'info> {
    pub owner: Signer<'info>,
    /// CHECK: mpl-core asset, deserialized in the handler.
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: mpl-core collection carrying the Oracle adapter.
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: PDA update authority that signs the mpl-core transfer.
    #[account(
        seeds = [UPDATE_AUTHORITY, collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    /// The oracle PDA read by mpl-core during the Transfer lifecycle. Refreshed
    /// from the current window before the CPI.
    #[account(
        mut,
        seeds = [ORACLE, collection.key().as_ref()],
        bump = oracle.bump,
    )]
    pub oracle: Account<'info, Oracle>,
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
    require_keys_eq!(
        asset.owner,
        ctx.accounts.owner.key(),
        ErrorCode::InvalidOwner
    );

    // Enforce the current window here: direct mpl-core transfers are blocked by
    // the asset's permanent transfer delegate, so this wrapper is the only path
    // and the stored oracle result cannot be trusted to have expired on its own.
    let hour = ((Clock::get()?.unix_timestamp.rem_euclid(86_400)) / 3_600) as u8;
    require!(
        (OPEN_HOUR..CLOSE_HOUR).contains(&hour),
        ErrorCode::TransferWindowClosed
    );

    // Keep the stored oracle result consistent with the live window, so the
    // adapter's reject-only check agrees with the wrapper.
    ctx.accounts.oracle.transfer = ORACLE_APPROVED;

    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        UPDATE_AUTHORITY,
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .new_owner(&ctx.accounts.new_owner.to_account_info())
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        // The oracle account must be visible to mpl-core's lifecycle check.
        .add_remaining_account(&ctx.accounts.oracle.to_account_info(), false, false)
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
