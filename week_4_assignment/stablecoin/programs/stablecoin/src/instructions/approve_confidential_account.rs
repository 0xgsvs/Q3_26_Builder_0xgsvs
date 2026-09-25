use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::extension::confidential_transfer::instruction::approve_account, Token2022,
    },
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::ConfidentialAccountApproved, extensions::mint_view};

/// The `approve_policy = manual` half of task 5.
///
/// The re-issued mint sets `auto_approve_new_accounts = false`, so a freshly
/// configured confidential account starts out unapproved and the mint's
/// confidential transfer authority has to approve it — a second, issuer-side
/// KYC step after the owner proved possession of their keys.
///
/// token-2022 v11 records that flag at `ConfigureAccount` and sets it here, but
/// no token-2022 instruction reads it back, so a mint that wants manual
/// approval has to enforce it in its own program. This program does: see the
/// approval check in `DepositConfidential`, `ConfidentialTransfer` and
/// `WithdrawConfidential`.
#[derive(Accounts)]
pub struct ApproveConfidentialAccount<'info> {
    pub authority: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> ApproveConfidentialAccount<'info> {
    pub fn approve(&mut self) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.confidential_transfer_authority == Some(self.authority.key()),
            StablecoinError::NotConfidentialTransferAuthority
        );
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );

        let approve = approve_account(
            &self.token_program.key(),
            &self.account.key(),
            &self.mint.key(),
            &self.authority.key(),
            &[],
        )?;
        anchor_lang::solana_program::program::invoke(
            &approve,
            &[
                self.account.to_account_info(),
                self.mint.to_account_info(),
                self.authority.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        emit!(ConfidentialAccountApproved {
            mint: self.mint.key(),
            account: self.account.key(),
            authority: self.authority.key(),
        });

        Ok(())
    }
}
