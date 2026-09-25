use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked, Token2022},
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::FundsSeized, extensions::mint_view};

/// Seizure, using the `PermanentDelegate` extension of the re-issued mint.
///
/// The permanent delegate can move tokens out of an account whose owner never
/// signed, which is the whole point of the extension. Two things are worth
/// noting and are covered by the tests:
///
/// - even the permanent delegate must sign, so the seizure is attributable; and
/// - token-2022 rejects a *frozen* source account for every transfer, including
///   this one. A sanctioned account is therefore thawed, seized and (if it
///   should stay blocked) re-frozen.
#[derive(Accounts)]
pub struct Seize<'info> {
    pub authority: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        constraint = source.mint == mint.key() @ StablecoinError::IncompatibleMint,
    )]
    pub source: InterfaceAccount<'info, TokenAccount>,
    #[account(
        mut,
        constraint = destination.mint == mint.key() @ StablecoinError::IncompatibleMint,
    )]
    pub destination: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> Seize<'info> {
    pub fn seize(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, StablecoinError::InvalidAmount);

        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.permanent_delegate == Some(self.authority.key()),
            StablecoinError::NotPermanentDelegate
        );
        require!(!self.source.is_frozen(), StablecoinError::AccountFrozen);

        transfer_checked(
            CpiContext::new(
                self.token_program.key(),
                TransferChecked {
                    from: self.source.to_account_info(),
                    mint: self.mint.to_account_info(),
                    to: self.destination.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
            view.decimals,
        )?;

        emit!(FundsSeized {
            mint: self.mint.key(),
            source: self.source.key(),
            destination: self.destination.key(),
            permanent_delegate: self.authority.key(),
            amount,
        });

        Ok(())
    }
}
