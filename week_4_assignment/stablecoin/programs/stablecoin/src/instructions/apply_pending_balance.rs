use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::extension::confidential_transfer::{
            instruction::inner_apply_pending_balance, DecryptableBalance,
        },
        Token2022,
    },
    token_interface::{Mint, TokenAccount},
};

use crate::{
    error::StablecoinError, events::PendingBalanceApplied, extensions::confidential_ready,
};

/// Moves the pending balance into the available (spendable) balance (task 6).
///
/// `new_decryptable_available_balance` is the AES-encrypted sum the owner
/// expects to hold afterwards: the owner decrypts the pending ciphertexts
/// off-chain and re-encrypts the total, which is why this is an owner-only
/// instruction.
#[derive(Accounts)]
pub struct ApplyPendingBalance<'info> {
    pub owner: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> ApplyPendingBalance<'info> {
    pub fn apply(
        &mut self,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );
        confidential_ready(&self.account.to_account_info(), &self.owner.key())?;

        apply_pending_balance_cpi(
            &self.token_program.to_account_info(),
            &self.account.to_account_info(),
            &self.owner.to_account_info(),
            expected_pending_balance_credit_counter,
            new_decryptable_available_balance,
        )?;

        emit!(PendingBalanceApplied {
            mint: self.mint.key(),
            account: self.account.key(),
            expected_pending_balance_credit_counter,
        });

        Ok(())
    }
}

/// Shared by `ApplyPendingBalance` and `WithdrawConfidential`, which applies the
/// pending balance before converting anything back into public tokens.
pub(crate) fn apply_pending_balance_cpi<'info>(
    token_program: &AccountInfo<'info>,
    account: &AccountInfo<'info>,
    owner: &AccountInfo<'info>,
    expected_pending_balance_credit_counter: u64,
    new_decryptable_available_balance: [u8; 36],
) -> Result<()> {
    let apply = inner_apply_pending_balance(
        &token_program.key(),
        &account.key(),
        expected_pending_balance_credit_counter,
        &DecryptableBalance::from(new_decryptable_available_balance),
        &owner.key(),
        &[],
    )?;
    anchor_lang::solana_program::program::invoke(
        &apply,
        &[account.clone(), owner.clone(), token_program.clone()],
    )?;
    Ok(())
}
