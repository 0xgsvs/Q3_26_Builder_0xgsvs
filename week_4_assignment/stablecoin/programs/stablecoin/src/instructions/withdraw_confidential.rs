use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::extension::confidential_transfer::{
            instruction::{
                inner_withdraw, BatchedRangeProofU64Data, CiphertextCommitmentEqualityProofData,
            },
            DecryptableBalance,
        },
        Token2022,
    },
    token_interface::{Mint, TokenAccount},
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

use crate::{
    error::StablecoinError,
    events::ConfidentialTokensWithdrawn,
    extensions::{confidential_ready, mint_view},
    instructions::apply_pending_balance::apply_pending_balance_cpi,
};

/// Converts confidential tokens back into a public balance (task 6).
///
/// A withdrawal can only spend the *available* balance, so when the account has
/// a pending balance this instruction applies it first — inside the same
/// instruction — and then withdraws. Two proofs are needed: ciphertext /
/// commitment equality for the remaining balance, and a `BatchedRangeProofU64`
/// that proves the account stays solvent.
///
/// Both decryptable balances are supplied by the owner: `decryptable_available_balance`
/// is what the owner expects to hold once the pending balance lands, and
/// `new_decryptable_available_balance` is what is left after the withdrawal.
#[derive(Accounts)]
pub struct WithdrawConfidential<'info> {
    pub owner: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: verified `CiphertextCommitmentEqualityProof` context state account
    /// owned by the ZK ElGamal proof program.
    pub equality_proof: UncheckedAccount<'info>,
    /// CHECK: verified `BatchedRangeProofU64` context state account owned by the
    /// ZK ElGamal proof program.
    pub range_proof: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> WithdrawConfidential<'info> {
    pub fn withdraw(
        &mut self,
        amount: u64,
        expected_pending_balance_credit_counter: u64,
        decryptable_available_balance: [u8; 36],
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        require!(amount > 0, StablecoinError::InvalidAmount);
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );

        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.extensions.confidential_transfer_mint,
            StablecoinError::MintNotConfiguredForConfidentialTransfers
        );
        require!(
            !view.extensions.transfer_fee_config,
            StablecoinError::ConfidentialTransferUnavailableWithTransferFee
        );
        let confidential = confidential_ready(&self.account.to_account_info(), &self.owner.key())?;

        // Withdrawing spends the available balance only, so anything still
        // sitting in the pending balance has to be applied first.
        let pending_balance_applied = confidential.pending_balance_credit_counter > 0;
        if pending_balance_applied {
            apply_pending_balance_cpi(
                &self.token_program.to_account_info(),
                &self.account.to_account_info(),
                &self.owner.to_account_info(),
                expected_pending_balance_credit_counter,
                decryptable_available_balance,
            )?;
        }

        let withdraw = inner_withdraw(
            &self.token_program.key(),
            &self.account.key(),
            &self.mint.key(),
            amount,
            view.decimals,
            &DecryptableBalance::from(new_decryptable_available_balance),
            &self.owner.key(),
            &[],
            ProofLocation::<CiphertextCommitmentEqualityProofData>::ContextStateAccount(
                &self.equality_proof.key(),
            ),
            ProofLocation::<BatchedRangeProofU64Data>::ContextStateAccount(&self.range_proof.key()),
        )?;
        anchor_lang::solana_program::program::invoke(
            &withdraw,
            &[
                self.account.to_account_info(),
                self.mint.to_account_info(),
                self.equality_proof.to_account_info(),
                self.range_proof.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        emit!(ConfidentialTokensWithdrawn {
            mint: self.mint.key(),
            account: self.account.key(),
            amount,
            pending_balance_applied,
        });

        Ok(())
    }
}
