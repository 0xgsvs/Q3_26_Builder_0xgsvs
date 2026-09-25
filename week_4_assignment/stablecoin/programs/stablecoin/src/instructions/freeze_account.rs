use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{freeze_account, FreezeAccount as FreezeAccountCpi, Token2022},
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::AccountFrozen, extensions::mint_view};

/// The mirror image of `ThawAccount`: the freeze authority can re-freeze a
/// single account (a sanctions hit, a compromised wallet) without changing the
/// mint's default account state or touching anybody else's account.
#[derive(Accounts)]
pub struct FreezeAccount<'info> {
    pub freeze_authority: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> FreezeAccount<'info> {
    pub fn freeze(&mut self) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.freeze_authority == Some(self.freeze_authority.key()),
            StablecoinError::NotFreezeAuthority
        );
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );

        freeze_account(CpiContext::new(
            self.token_program.key(),
            FreezeAccountCpi {
                account: self.account.to_account_info(),
                mint: self.mint.to_account_info(),
                authority: self.freeze_authority.to_account_info(),
            },
        ))?;

        emit!(AccountFrozen {
            mint: self.mint.key(),
            account: self.account.key(),
            freeze_authority: self.freeze_authority.key(),
        });

        Ok(())
    }
}
