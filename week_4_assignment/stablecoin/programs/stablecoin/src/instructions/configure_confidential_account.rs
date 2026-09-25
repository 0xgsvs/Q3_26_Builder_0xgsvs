use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::extension::confidential_transfer::{
            instruction::{inner_configure_account, PubkeyValidityProofData},
            DecryptableBalance,
        },
        Token2022,
    },
    token_interface::{Mint, TokenAccount},
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

use crate::{
    error::StablecoinError,
    events::ConfidentialAccountConfigured,
    extensions::mint_view,
};

/// Enables confidential transfers on one token account (task 6).
///
/// Creating the account is permissionless — `CreateTokenAccount` reserves the
/// space, exactly like the associated token account program — but *configuring*
/// it is owner-only: the signer has to be the account owner (a delegate is not
/// enough), and the signer also has to supply a proof of the AES key that
/// encrypts the account's confidential balance. Nobody can turn somebody else's
/// account into a confidential one.
///
/// The proof is referenced through a context state account verified by the ZK
/// ElGamal proof program, so the account is named here instead of being inlined
/// in the instruction data.
#[derive(Accounts)]
pub struct ConfigureConfidentialAccount<'info> {
    pub owner: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub account: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: a verified `PubkeyValidityProof` context state account owned by
    /// the ZK ElGamal proof program. Only its key is passed to token-2022,
    /// which checks the owner and the proof type itself.
    pub proof_account: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> ConfigureConfidentialAccount<'info> {
    pub fn configure(
        &mut self,
        decryptable_zero_balance: [u8; 36],
        maximum_pending_balance_credit_counter: u64,
    ) -> Result<()> {
        require!(
            self.account.owner == self.owner.key(),
            StablecoinError::NotAccountOwner
        );
        require!(
            self.account.mint == self.mint.key(),
            StablecoinError::IncompatibleMint
        );
        require!(
            maximum_pending_balance_credit_counter > 0,
            StablecoinError::InvalidAmount
        );
        // Cheap guard for a typo'd mint: a confidential account needs the mint's
        // `ConfidentialTransferMint` extension, written through
        // `StateWithExtensions`.
        require!(
            mint_view(&self.mint.to_account_info())?.extensions.confidential_transfer_mint,
            StablecoinError::MintNotConfiguredForConfidentialTransfers
        );

        let configure = inner_configure_account(
            &self.token_program.key(),
            &self.account.key(),
            &self.mint.key(),
            &DecryptableBalance::from(decryptable_zero_balance),
            maximum_pending_balance_credit_counter,
            &self.owner.key(),
            &[],
            ProofLocation::<PubkeyValidityProofData>::ContextStateAccount(&self.proof_account.key()),
        )?;
        anchor_lang::solana_program::program::invoke(
            &configure,
            &[
                self.account.to_account_info(),
                self.mint.to_account_info(),
                self.proof_account.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        emit!(ConfidentialAccountConfigured {
            mint: self.mint.key(),
            account: self.account.key(),
            owner: self.owner.key(),
            maximum_pending_balance_credit_counter,
        });

        Ok(())
    }
}
