use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::extension::confidential_transfer::instruction::deposit, Token2022,
    },
    token_interface::{Mint, TokenAccount},
};

use crate::{
    error::StablecoinError,
    events::ConfidentialTokensDeposited,
    extensions::{confidential_ready, mint_view},
};

/// Moves a public balance into the confidential pending balance (task 6).
///
/// The account must have been configured and approved: with
/// `approve_policy = manual` the issuer has to approve the account before any
/// confidential operation is allowed.
#[derive(Accounts)]
pub struct DepositConfidential<'info> {
    pub owner: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> DepositConfidential<'info> {
    pub fn deposit(&mut self, amount: u64) -> Result<()> {
        require!(amount > 0, StablecoinError::InvalidAmount);
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );
        let view = mint_view(&self.mint.to_account_info())?;
        let confidential = confidential_ready(&self.account.to_account_info(), &self.owner.key())?;

        // The pending balance only tolerates so many credits before
        // `ApplyPendingBalance` has to run; surface that as a program error
        // rather than letting the deposit fail deep inside token-2022.
        require!(
            confidential.pending_balance_credit_counter
                < confidential.maximum_pending_balance_credit_counter,
            StablecoinError::PendingBalanceFull
        );

        let deposit_tokens = deposit(
            &self.token_program.key(),
            &self.account.key(),
            &self.mint.key(),
            amount,
            view.decimals,
            &self.owner.key(),
            &[],
        )?;
        anchor_lang::solana_program::program::invoke(
            &deposit_tokens,
            &[
                self.account.to_account_info(),
                self.mint.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        emit!(ConfidentialTokensDeposited {
            mint: self.mint.key(),
            account: self.account.key(),
            amount,
        });

        Ok(())
    }
}
