use anchor_lang::{
    prelude::*,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::{
    token_2022::{spl_token_2022::state::AccountState, Token2022},
    token_2022_extensions::{
        default_account_state_initialize, metadata_pointer_initialize,
        mint_close_authority_initialize, spl_token_metadata_interface::state::TokenMetadata,
        token_metadata_initialize, transfer_fee_initialize, DefaultAccountStateInitialize,
        MetadataPointerInitialize, MintCloseAuthorityInitialize, TokenMetadataInitialize,
        TransferFeeInitialize,
    },
};

use crate::{
    constants::MAXIMUM_TRANSFER_FEE_BPS,
    error::StablecoinError,
    events::MintInitialized,
    extensions::{mint_account_space, mint_rent_lamports, FEE_MINT_EXTENSIONS},
    state::ExtensionSet,
};

/// Creates the remittance mint (task 1).
///
/// The mint stacks four extensions — `TransferFeeConfig` (issuer revenue on
/// every transfer), `MetadataPointer` (pointed at the mint itself so wallets
/// read metadata from the mint), `DefaultAccountState` (frozen, so brand new
/// accounts cannot move anything until KYC clears) and `MintCloseAuthority`
/// (decommissioning) — and every extension-initialize instruction runs *before*
/// `InitializeMint`.
#[derive(Accounts)]
pub struct InitializeMint<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// The mint account, created here. It signs so the system program can
    /// allocate it; the program never trusts it as a typed account.
    #[account(mut)]
    pub mint: Signer<'info>,
    /// CHECK: mint authority. Only the key is stored, in the mint.
    pub mint_authority: UncheckedAccount<'info>,
    /// CHECK: freeze authority (the KYC/compliance officer). Only the key is
    /// stored, in the mint.
    pub freeze_authority: UncheckedAccount<'info>,
    /// CHECK: mint close authority. Only the key is stored, in the mint.
    pub close_authority: UncheckedAccount<'info>,
    /// CHECK: recipient of the withheld transfer fees. Only the key is stored,
    /// in the transfer fee config.
    pub withdraw_withheld_authority: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeMint<'info> {
    pub fn init(
        &mut self,
        decimals: u8,
        name: String,
        symbol: String,
        uri: String,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        require!(decimals <= 9, StablecoinError::InvalidDecimals);
        require!(
            transfer_fee_basis_points > 0 && transfer_fee_basis_points < MAXIMUM_TRANSFER_FEE_BPS,
            StablecoinError::InvalidTransferFee
        );
        require!(
            !name.is_empty() && !symbol.is_empty(),
            StablecoinError::InvalidMetadata
        );

        let token_metadata = TokenMetadata {
            update_authority: Some(self.mint_authority.key())
                .try_into()
                .map_err(|_| StablecoinError::InvalidMetadata)?,
            mint: self.mint.key(),
            name: name.clone(),
            symbol: symbol.clone(),
            uri: uri.clone(),
            additional_metadata: Vec::new(),
        };

        // `ExtensionType::try_calculate_account_len` sizes the extension set
        // that is initialized before `InitializeMint` (the length has to match
        // exactly); the metadata written afterwards is paid for up front, since
        // token-2022 grows the account to fit it.
        let mint_len = mint_account_space(FEE_MINT_EXTENSIONS)?;
        create_account(
            CpiContext::new(
                self.system_program.key(),
                CreateAccount {
                    from: self.payer.to_account_info(),
                    to: self.mint.to_account_info(),
                },
            ),
            mint_rent_lamports(FEE_MINT_EXTENSIONS, &token_metadata)?,
            mint_len as u64,
            &self.token_program.key(),
        )?;

        // ---- extension initialization, strictly before InitializeMint ----
        //
        // 1. MetadataPointer, pointed at the mint itself and immutable.
        metadata_pointer_initialize(
            CpiContext::new(
                self.token_program.key(),
                MetadataPointerInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            None,
            Some(self.mint.key()),
        )?;

        // 2. DefaultAccountState = Frozen: every new account starts frozen.
        default_account_state_initialize(
            CpiContext::new(
                self.token_program.key(),
                DefaultAccountStateInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            &AccountState::Frozen,
        )?;

        // 3. MintCloseAuthority.
        mint_close_authority_initialize(
            CpiContext::new(
                self.token_program.key(),
                MintCloseAuthorityInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            Some(&self.close_authority.key()),
        )?;

        // 4. TransferFeeConfig: the protocol-level fee the issuer keeps.
        transfer_fee_initialize(
            CpiContext::new(
                self.token_program.key(),
                TransferFeeInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            Some(&self.mint_authority.key()),
            Some(&self.withdraw_withheld_authority.key()),
            transfer_fee_basis_points,
            maximum_fee,
        )?;

        // ---- only now the mint itself ----
        anchor_spl::token_2022::initialize_mint2(
            CpiContext::new(
                self.token_program.key(),
                anchor_spl::token_2022::InitializeMint2 {
                    mint: self.mint.to_account_info(),
                },
            ),
            decimals,
            &self.mint_authority.key(),
            Some(&self.freeze_authority.key()),
        )?;

        // On-chain metadata, written through the metadata pointer that points
        // back at the mint. Requires an initialized mint, so it comes last.
        token_metadata_initialize(
            CpiContext::new(
                self.token_program.key(),
                TokenMetadataInitialize {
                    program_id: self.token_program.to_account_info(),
                    metadata: self.mint.to_account_info(),
                    update_authority: self.mint_authority.to_account_info(),
                    mint: self.mint.to_account_info(),
                    mint_authority: self.mint_authority.to_account_info(),
                },
            ),
            name,
            symbol,
            uri,
        )?;

        emit!(MintInitialized {
            mint: self.mint.key(),
            mint_authority: self.mint_authority.key(),
            freeze_authority: self.freeze_authority.key(),
            close_authority: self.close_authority.key(),
            withdraw_withheld_authority: self.withdraw_withheld_authority.key(),
            decimals,
            transfer_fee_basis_points,
            maximum_fee,
            extensions: ExtensionSet::FEE_MINT,
        });

        Ok(())
    }
}
