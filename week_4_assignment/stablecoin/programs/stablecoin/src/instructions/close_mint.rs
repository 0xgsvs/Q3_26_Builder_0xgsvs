use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{close_account, CloseAccount, Token2022},
    token_interface::Mint,
};

use crate::{error::StablecoinError, events::MintClosed, extensions::mint_view};

/// Decommissions the mint: the close authority reclaims the mint account's
/// lamports, which requires a zero supply and the `MintCloseAuthority`
/// extension created back in `InitializeMint`.
#[derive(Accounts)]
pub struct CloseMint<'info> {
    pub close_authority: Signer<'info>,
    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: receives the reclaimed rent. Any writable account will do; the
    /// token program only moves lamports into it.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> CloseMint<'info> {
    pub fn close(&mut self) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.mint_close_authority == Some(self.close_authority.key()),
            StablecoinError::NotCloseAuthority
        );
        require!(view.supply == 0, StablecoinError::MintHasSupply);

        let lamports = self.mint.to_account_info().lamports();

        close_account(CpiContext::new(
            self.token_program.key(),
            CloseAccount {
                account: self.mint.to_account_info(),
                destination: self.destination.to_account_info(),
                authority: self.close_authority.to_account_info(),
            },
        ))?;

        emit!(MintClosed {
            mint: self.mint.key(),
            destination: self.destination.key(),
            close_authority: self.close_authority.key(),
            lamports,
        });

        Ok(())
    }
}
