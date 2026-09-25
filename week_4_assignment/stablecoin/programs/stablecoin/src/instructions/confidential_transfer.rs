use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{
        spl_token_2022::{
            extension::confidential_transfer::{
                instruction::{
                    inner_transfer, BatchedGroupedCiphertext3HandlesValidityProofData,
                    BatchedRangeProofU128Data, CiphertextCommitmentEqualityProofData,
                },
                DecryptableBalance,
            },
            solana_zk_sdk::encryption::pod::elgamal::PodElGamalCiphertext,
        },
        Token2022,
    },
    token_interface::{Mint, TokenAccount},
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

use crate::{
    error::StablecoinError,
    events::ConfidentialTokensTransferred,
    extensions::{confidential_approved, confidential_ready, mint_view},
};

/// A confidential transfer: the amount is hidden, and the source, destination
/// and auditor see only ElGamal ciphertexts (task 6).
///
/// The transfer is accompanied by three zero-knowledge proofs — equality of the
/// remaining balance, validity of the grouped ciphertexts, and a range proof
/// over the remaining balance and the two halves of the amount. Each is
/// verified by the ZK ElGamal proof program into its own context state account
/// and referenced from here, which keeps this transaction small.
///
/// The mint must not charge a transfer fee. token-2022 requires the
/// `TransferWithFee` proof set on a fee-bearing mint (and a
/// `ConfidentialTransferFeeConfig` extension, which a mint that is already live
/// cannot gain), so this program refuses instead of building a transfer the
/// mint would reject. That refusal is the gap between the fee mint and its
/// confidential re-issue.
#[derive(Accounts)]
pub struct ConfidentialTransfer<'info> {
    pub owner: Signer<'info>,
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
    /// CHECK: verified `CiphertextCommitmentEqualityProof` context state account
    /// owned by the ZK ElGamal proof program.
    pub equality_proof: UncheckedAccount<'info>,
    /// CHECK: verified `BatchedGroupedCiphertext3HandlesValidityProof` context
    /// state account owned by the ZK ElGamal proof program.
    pub ciphertext_validity_proof: UncheckedAccount<'info>,
    /// CHECK: verified `BatchedRangeProofU128` context state account owned by
    /// the ZK ElGamal proof program.
    pub range_proof: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
}

impl<'info> ConfidentialTransfer<'info> {
    pub fn transfer(
        &mut self,
        new_source_decryptable_available_balance: [u8; 36],
        transfer_amount_auditor_ciphertext_lo: [u8; 64],
        transfer_amount_auditor_ciphertext_hi: [u8; 64],
    ) -> Result<()> {
        let view = mint_view(&self.mint.to_account_info())?;
        require!(
            view.extensions.confidential_transfer_mint,
            StablecoinError::MintNotConfiguredForConfidentialTransfers
        );
        require!(
            !view.extensions.transfer_fee_config,
            StablecoinError::ConfidentialTransferUnavailableWithTransferFee
        );

        confidential_ready(&self.source.to_account_info(), &self.owner.key())?;
        // A manual-approval mint should not let value land in an account the
        // issuer has not cleared yet.
        confidential_approved(&self.destination.to_account_info())?;

        let transfer = inner_transfer(
            &self.token_program.key(),
            &self.source.key(),
            &self.mint.key(),
            &self.destination.key(),
            &DecryptableBalance::from(new_source_decryptable_available_balance),
            &PodElGamalCiphertext::from(transfer_amount_auditor_ciphertext_lo),
            &PodElGamalCiphertext::from(transfer_amount_auditor_ciphertext_hi),
            &self.owner.key(),
            &[],
            ProofLocation::<CiphertextCommitmentEqualityProofData>::ContextStateAccount(
                &self.equality_proof.key(),
            ),
            ProofLocation::<BatchedGroupedCiphertext3HandlesValidityProofData>::ContextStateAccount(
                &self.ciphertext_validity_proof.key(),
            ),
            ProofLocation::<BatchedRangeProofU128Data>::ContextStateAccount(&self.range_proof.key()),
        )?;
        anchor_lang::solana_program::program::invoke(
            &transfer,
            &[
                self.source.to_account_info(),
                self.mint.to_account_info(),
                self.destination.to_account_info(),
                self.equality_proof.to_account_info(),
                self.ciphertext_validity_proof.to_account_info(),
                self.range_proof.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        emit!(ConfidentialTokensTransferred {
            mint: self.mint.key(),
            source: self.source.key(),
            destination: self.destination.key(),
            authority: self.owner.key(),
        });

        Ok(())
    }
}
