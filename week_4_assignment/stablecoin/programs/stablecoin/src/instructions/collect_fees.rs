use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::{extension::StateWithExtensions, state::Account as SplAccount},
        Token2022,
    },
    token_2022_extensions::{
        harvest_withheld_tokens_to_mint, withdraw_withheld_tokens_from_mint,
        HarvestWithheldTokensToMint, WithdrawWithheldTokensFromMint,
    },
    token_interface::{Mint, TokenAccount},
};

use crate::{error::StablecoinError, events::WithheldFeesCollected, extensions::mint_view};

/// Turns the fee the issuer is owed into an actual balance.
///
/// A transfer fee is withheld in the *recipient* account's `TransferFeeAmount`
/// extension, so it is only available to the issuer once it is harvested into
/// the mint and then withdrawn to the issuer's token account. Pass the accounts
/// whose withheld fees should be swept as remaining accounts.
#[derive(Accounts)]
pub struct CollectFees<'info> {
    pub withdraw_withheld_authority: Signer<'info>,
    /// The withheld amounts are harvested into the mint, so it is writable.
    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        constraint = destination.mint == mint.key() @ StablecoinError::IncompatibleMint,
    )]
    pub destination: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> CollectFees<'info> {
    /// `sources` are the token accounts whose withheld fees should be swept
    /// into the mint before the withdrawal.
    pub fn collect(&mut self, sources: &[AccountInfo<'info>]) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.withdraw_withheld_authority == Some(self.withdraw_withheld_authority.key()),
            StablecoinError::NotWithdrawWithheldAuthority
        );

        if !sources.is_empty() {
            for source in sources.iter() {
                // Read through `StateWithExtensions` to reject accounts that do
                // not belong to this mint.
                let data = source.try_borrow_data()?;
                let account = StateWithExtensions::<SplAccount>::unpack(&data)?;
                require!(
                    account.base.mint == self.mint.key(),
                    StablecoinError::IncompatibleMint
                );
            }
            harvest_withheld_tokens_to_mint(
                CpiContext::new(
                    self.token_program.key(),
                    HarvestWithheldTokensToMint {
                        token_program_id: self.token_program.to_account_info(),
                        mint: self.mint.to_account_info(),
                    },
                ),
                sources.to_vec(),
            )?;
        }

        withdraw_withheld_tokens_from_mint(CpiContext::new(
            self.token_program.key(),
            WithdrawWithheldTokensFromMint {
                token_program_id: self.token_program.to_account_info(),
                mint: self.mint.to_account_info(),
                destination: self.destination.to_account_info(),
                authority: self.withdraw_withheld_authority.to_account_info(),
            },
        ))?;

        emit!(WithheldFeesCollected {
            mint: self.mint.key(),
            destination: self.destination.key(),
            authority: self.withdraw_withheld_authority.key(),
            sources: sources.len() as u8,
        });

        Ok(())
    }
}
