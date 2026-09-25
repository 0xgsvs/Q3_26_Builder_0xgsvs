use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{mint_to_checked, MintToChecked, Token2022},
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::TokensMinted, extensions::mint_view};

/// Issuance. The mint authority signs; the destination may still be frozen
/// until KYC clears, and token-2022 rejects frozen destinations.
#[derive(Accounts)]
pub struct MintTo<'info> {
    pub mint_authority: Signer<'info>,
    /// The supply lives on the mint, so it is writable.
    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub destination: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> MintTo<'info> {
    pub fn mint(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, StablecoinError::InvalidAmount);

        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.mint_authority == Some(self.mint_authority.key()),
            StablecoinError::NotMintAuthority
        );
        require!(
            self.destination.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );

        mint_to_checked(
            CpiContext::new(
                self.token_program.key(),
                MintToChecked {
                    mint: self.mint.to_account_info(),
                    to: self.destination.to_account_info(),
                    authority: self.mint_authority.to_account_info(),
                },
            ),
            amount,
            view.decimals,
        )?;

        emit!(TokensMinted {
            mint: self.mint.key(),
            destination: self.destination.key(),
            mint_authority: self.mint_authority.key(),
            amount,
        });

        Ok(())
    }
}
