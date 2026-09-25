use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_2022_extensions::{transfer_checked_with_fee, TransferCheckedWithFee},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    error::StablecoinError,
    events::TokensTransferred,
    extensions::{mint_view, transfer_fee_for},
};

/// The public transfer path (task 2).
///
/// The fee is read from the mint's live `TransferFeeConfig` with
/// `calculate_epoch_fee(current_epoch, amount)` — the epoch is taken from the
/// clock sysvar on every call, so a fee that changes mid-epoch is never cached
/// — and the transfer goes out through `transfer_checked_with_fee`. Plain
/// `transfer` / `transfer_checked` are never used: both would move tokens
/// without the fee the issuer is owed.
#[derive(Accounts)]
pub struct TransferWithFee<'info> {
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

impl<'info> TransferWithFee<'info> {
    pub fn transfer(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, StablecoinError::InvalidAmount);
        let view = mint_view(&self.mint.to_account_info())?;

        // KYC gate: an account that has not been thawed cannot move anything.
        require!(!self.source.is_frozen(), StablecoinError::AccountFrozen);

        let epoch = Clock::get()?.epoch;
        let fee = transfer_fee_for(&self.mint.to_account_info(), epoch, amount)?;

        transfer_checked_with_fee(
            CpiContext::new(
                self.token_program.key(),
                TransferCheckedWithFee {
                    token_program_id: self.token_program.to_account_info(),
                    source: self.source.to_account_info(),
                    mint: self.mint.to_account_info(),
                    destination: self.destination.to_account_info(),
                    authority: self.authority.to_account_info(),
                },
            ),
            amount,
            view.decimals,
            fee,
        )?;

        emit!(TokensTransferred {
            mint: self.mint.key(),
            source: self.source.key(),
            destination: self.destination.key(),
            authority: self.authority.key(),
            amount,
            fee,
            epoch,
        });

        Ok(())
    }
}
