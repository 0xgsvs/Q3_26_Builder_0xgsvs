use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{thaw_account, ThawAccount as ThawAccountCpi, Token2022},
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::AccountThawed, extensions::mint_view};

/// The KYC path (task 4).
///
/// New accounts are frozen by the mint's `DefaultAccountState`. Clearing KYC
/// thaws that one account with the mint's freeze authority; the mint-level
/// default account state is never touched, so accounts opened later still start
/// frozen.
#[derive(Accounts)]
pub struct ThawAccount<'info> {
    pub freeze_authority: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> ThawAccount<'info> {
    pub fn thaw(&mut self) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.freeze_authority == Some(self.freeze_authority.key()),
            StablecoinError::NotFreezeAuthority
        );
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );

        thaw_account(CpiContext::new(
            self.token_program.key(),
            ThawAccountCpi {
                account: self.account.to_account_info(),
                mint: self.mint.to_account_info(),
                authority: self.freeze_authority.to_account_info(),
            },
        ))?;

        emit!(AccountThawed {
            mint: self.mint.key(),
            account: self.account.key(),
            freeze_authority: self.freeze_authority.key(),
        });

        Ok(())
    }
}
