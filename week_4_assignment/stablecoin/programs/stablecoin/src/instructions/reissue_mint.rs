use anchor_lang::{
    prelude::*,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::{
    token_2022::{initialize_mint2, spl_token_2022::state::AccountState, InitializeMint2, Token2022},
    token_2022_extensions::{
        default_account_state_initialize, metadata_pointer_initialize,
        mint_close_authority_initialize, permanent_delegate_initialize,
        spl_token_metadata_interface::state::TokenMetadata, token_metadata_initialize,
        DefaultAccountStateInitialize, MetadataPointerInitialize, MintCloseAuthorityInitialize,
        PermanentDelegateInitialize, TokenMetadataInitialize,
    },
    token_interface::Mint,
};

use crate::{
    constants::REISSUE_SEED,
    error::StablecoinError,
    events::MintReissued,
    extensions::{
        mint_account_space, mint_rent_lamports, mint_view, token_metadata_of,
        CONFIDENTIAL_MINT_EXTENSIONS,
    },
    state::{ExtensionSet, ReissueRecord},
};

/// Re-issues the mint for the confidential era (task 5).
///
/// Token-2022 only accepts extensions that are present when a mint is created,
/// so adding confidentiality or a `PermanentDelegate` means building a new mint
/// from the extension set of the old one. This instruction reads the previous
/// mint through `StateWithExtensions`, carries the metadata pointer, the frozen
/// default account state and the close authority forward, re-publishes the same
/// token metadata, and adds:
///
/// - `PermanentDelegate` — the seizure authority regulators asked for; and
/// - `ConfidentialTransferMint` with `auto_approve_new_accounts = false`, i.e.
///   `approve_policy = manual`: the mint authority has to approve each
///   confidential account.
///
/// `TransferFeeConfig` is *not* carried forward. Token-2022 rejects
/// `TransferFeeConfig` + `ConfidentialTransferMint` unless the mint also has
/// `ConfidentialTransferFeeConfig` (see `ExtensionType::check_for_invalid_mint_extension_combinations`),
/// and a fee-bearing confidential mint can only move confidential amounts with
/// the `TransferWithFee` proof set. That is the gap; `ReissueRecord` writes both
/// extension sets on-chain so the difference is auditable.
#[derive(Accounts)]
pub struct ReissueMint<'info> {
    #[account(mut)]
    pub issuer: Signer<'info>,
    /// The fee mint being replaced. Only its extension set and its metadata are
    /// read, always through `StateWithExtensions`.
    pub previous_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub new_mint: Signer<'info>,
    /// CHECK: freeze authority (KYC) of the new mint. Only the key is stored.
    pub freeze_authority: UncheckedAccount<'info>,
    /// CHECK: close authority of the new mint. Only the key is stored.
    pub close_authority: UncheckedAccount<'info>,
    /// CHECK: permanent delegate — the seizure authority — of the new mint.
    /// Only the key is stored.
    pub permanent_delegate: UncheckedAccount<'info>,
    /// CHECK: confidential transfer authority of the new mint, which approves
    /// confidential accounts under `approve_policy = manual`. Only the key is
    /// stored.
    pub confidential_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = issuer,
        seeds = [REISSUE_SEED, previous_mint.key().as_ref()],
        bump,
        space = ReissueRecord::DISCRIMINATOR.len() + ReissueRecord::INIT_SPACE,
    )]
    pub reissue_record: Account<'info, ReissueRecord>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> ReissueMint<'info> {
    pub fn reissue(&mut self, bump: u8) -> Result<()> {
        let previous = mint_view(&self.previous_mint.to_account_info())?;
        require!(
            previous.extensions == ExtensionSet::FEE_MINT,
            StablecoinError::UnexpectedExtensionSet
        );

        // The metadata pointer of the previous mint points at the previous mint
        // itself, so its name / symbol / uri travel with the extension set.
        let previous_metadata = token_metadata_of(&self.previous_mint.to_account_info())?;
        let new_metadata = TokenMetadata {
            update_authority: Some(self.issuer.key())
                .try_into()
                .map_err(|_| StablecoinError::InvalidMetadata)?,
            mint: self.new_mint.key(),
            name: previous_metadata.name,
            symbol: previous_metadata.symbol,
            uri: previous_metadata.uri,
            additional_metadata: previous_metadata.additional_metadata,
        };

        let mint_len = mint_account_space(CONFIDENTIAL_MINT_EXTENSIONS)?;
        create_account(
            CpiContext::new(
                self.system_program.key(),
                CreateAccount {
                    from: self.issuer.to_account_info(),
                    to: self.new_mint.to_account_info(),
                },
            ),
            mint_rent_lamports(CONFIDENTIAL_MINT_EXTENSIONS, &new_metadata)?,
            mint_len as u64,
            &self.token_program.key(),
        )?;

        // ---- carried forward, then added, all before InitializeMint ----
        metadata_pointer_initialize(
            CpiContext::new(
                self.token_program.key(),
                MetadataPointerInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.new_mint.to_account_info(),
                },
            ),
            None,
            Some(self.new_mint.key()),
        )?;
        default_account_state_initialize(
            CpiContext::new(
                self.token_program.key(),
                DefaultAccountStateInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.new_mint.to_account_info(),
                },
            ),
            &AccountState::Frozen,
        )?;
        mint_close_authority_initialize(
            CpiContext::new(
                self.token_program.key(),
                MintCloseAuthorityInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.new_mint.to_account_info(),
                },
            ),
            Some(&self.close_authority.key()),
        )?;
        permanent_delegate_initialize(
            CpiContext::new(
                self.token_program.key(),
                PermanentDelegateInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.new_mint.to_account_info(),
                },
            ),
            &self.permanent_delegate.key(),
        )?;

        // Confidential transfers, manual approval: token-2022 records
        // `auto_approve_new_accounts = false` and this program enforces the
        // approval gate on every confidential instruction.
        let initialize_confidential_transfer_mint = anchor_spl::token_2022::spl_token_2022::extension::confidential_transfer::instruction::initialize_mint(
            &self.token_program.key(),
            &self.new_mint.key(),
            Some(self.confidential_authority.key()),
            false,
            None,
        )?;
        anchor_lang::solana_program::program::invoke(
            &initialize_confidential_transfer_mint,
            &[
                self.new_mint.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        initialize_mint2(
            CpiContext::new(
                self.token_program.key(),
                InitializeMint2 {
                    mint: self.new_mint.to_account_info(),
                },
            ),
            previous.decimals,
            &self.issuer.key(),
            Some(&self.freeze_authority.key()),
        )?;

        token_metadata_initialize(
            CpiContext::new(
                self.token_program.key(),
                TokenMetadataInitialize {
                    program_id: self.token_program.to_account_info(),
                    metadata: self.new_mint.to_account_info(),
                    update_authority: self.issuer.to_account_info(),
                    mint: self.new_mint.to_account_info(),
                    mint_authority: self.issuer.to_account_info(),
                },
            ),
            new_metadata.name,
            new_metadata.symbol,
            new_metadata.uri,
        )?;

        self.reissue_record.set_inner(ReissueRecord {
            previous_mint: self.previous_mint.key(),
            new_mint: self.new_mint.key(),
            previous: previous.extensions,
            new: ExtensionSet::CONFIDENTIAL_MINT,
            carried_forward: ExtensionSet::CONFIDENTIAL_MINT.carried_forward(),
            added: ExtensionSet::CONFIDENTIAL_MINT.added(),
            dropped: ExtensionSet::FEE_MINT.dropped(),
            bump,
        });

        emit!(MintReissued {
            previous_mint: self.previous_mint.key(),
            new_mint: self.new_mint.key(),
            permanent_delegate: self.permanent_delegate.key(),
            confidential_transfer_authority: self.confidential_authority.key(),
            previous: previous.extensions,
            new: ExtensionSet::CONFIDENTIAL_MINT,
            carried_forward: ExtensionSet::CONFIDENTIAL_MINT.carried_forward(),
            added: ExtensionSet::CONFIDENTIAL_MINT.added(),
            dropped: ExtensionSet::FEE_MINT.dropped(),
        });

        Ok(())
    }
}
