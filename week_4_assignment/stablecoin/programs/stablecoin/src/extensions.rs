//! Every read of mint or token-account state goes through
//! `StateWithExtensions`, never a raw `unpack` that would silently ignore
//! extensions (task 3). The helpers below are the single place where token-2022
//! state is parsed.

use anchor_lang::{
    prelude::*,
    solana_program::rent::Rent,
};
use anchor_spl::{
    token_2022::spl_token_2022::{
        extension::{
            confidential_transfer::{ConfidentialTransferAccount, ConfidentialTransferMint},
            default_account_state::DefaultAccountState,
            metadata_pointer::MetadataPointer,
            mint_close_authority::MintCloseAuthority,
            permanent_delegate::PermanentDelegate,
            transfer_fee::TransferFeeConfig,
            BaseStateWithExtensions, ExtensionType, StateWithExtensions,
        },
        state::{Account as SplTokenAccount, AccountState, Mint as MintState},
    },
    token_2022_extensions::{
        spl_token_metadata_interface, spl_token_metadata_interface::state::TokenMetadata,
    },
};

use crate::{constants::TLV_ENTRY_HEADER_LEN, error::StablecoinError, state::ExtensionSet};

/// Mint-level extensions of the remittance mint: protocol fee, on-chain
/// metadata, frozen-by-default accounts, mint close authority (task 1).
pub const FEE_MINT_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::TransferFeeConfig,
    ExtensionType::MetadataPointer,
    ExtensionType::DefaultAccountState,
    ExtensionType::MintCloseAuthority,
];

/// Mint-level extensions of the re-issued mint: the same metadata pointer,
/// default account state and close authority, plus the seizure authority
/// (`PermanentDelegate`) and confidential transfers (task 5).
///
/// `TransferFeeConfig` is deliberately absent: token-2022 rejects that
/// combination unless `ConfidentialTransferFeeConfig` is present too.
pub const CONFIDENTIAL_MINT_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::MetadataPointer,
    ExtensionType::DefaultAccountState,
    ExtensionType::MintCloseAuthority,
    ExtensionType::PermanentDelegate,
    ExtensionType::ConfidentialTransferMint,
];

/// Account-level extensions reserved when a token account is created:
/// `TransferFeeAmount` (required by a fee-bearing mint) and
/// `ConfidentialTransferAccount` (reserved so the account can later be
/// configured for confidential transfers without a realloc).
pub const TOKEN_ACCOUNT_EXTENSIONS: &[ExtensionType] = &[
    ExtensionType::TransferFeeAmount,
    ExtensionType::ConfidentialTransferAccount,
];

/// Space to allocate for a mint: `ExtensionType::try_calculate_account_len` over
/// the extensions that are initialized before `InitializeMint`.
///
/// The length has to be exact — token-2022's `InitializeMint` compares the
/// account length against the extension set it finds — so the variable-length
/// token metadata that `Initialize` appends *after* `InitializeMint` is not
/// allocated here, only funded (see `mint_rent_lamports`).
pub fn mint_account_space(extensions: &[ExtensionType]) -> Result<usize> {
    ExtensionType::try_calculate_account_len::<MintState>(extensions).map_err(Into::into)
}

/// Lamports a mint needs at creation: rent for the extension set plus rent for
/// the token-metadata entry, which token-2022 `resize`s onto the account when
/// the metadata is initialized.
pub fn mint_rent_lamports(
    extensions: &[ExtensionType],
    metadata: &TokenMetadata,
) -> Result<u64> {
    let metadata_len = spl_token_metadata_interface::borsh::to_vec(metadata)
        .map_err(|_| StablecoinError::InvalidMetadata)?
        .len()
        .checked_add(TLV_ENTRY_HEADER_LEN)
        .ok_or(StablecoinError::Overflow)?;
    let total = mint_account_space(extensions)?
        .checked_add(metadata_len)
        .ok_or(StablecoinError::Overflow)?;
    Ok(Rent::get()?.minimum_balance(total))
}

/// Facts read from a mint, all of it through `StateWithExtensions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintView {
    pub decimals: u8,
    pub supply: u64,
    pub mint_authority: Option<Pubkey>,
    pub freeze_authority: Option<Pubkey>,
    pub extensions: ExtensionSet,
    pub mint_close_authority: Option<Pubkey>,
    pub permanent_delegate: Option<Pubkey>,
    pub confidential_transfer_authority: Option<Pubkey>,
    pub withdraw_withheld_authority: Option<Pubkey>,
    pub frozen_by_default: bool,
}

/// Reads a mint without ever unpacking raw bytes: the base fields and every
/// extension this program cares about come out of one `StateWithExtensions`.
pub fn mint_view(mint_info: &AccountInfo) -> Result<MintView> {
    let data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)?;

    let metadata_pointer_points_at_self = mint
        .get_extension::<MetadataPointer>()
        .map(|pointer| {
            Option::<Pubkey>::from(pointer.metadata_address) == Some(*mint_info.key)
        })
        .unwrap_or(false);
    let frozen_by_default = mint
        .get_extension::<DefaultAccountState>()
        .map(|state| {
            AccountState::try_from(state.state)
                .map(|state| state == AccountState::Frozen)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let close_authority = mint
        .get_extension::<MintCloseAuthority>()
        .map(|extension| Option::<Pubkey>::from(extension.close_authority))
        .unwrap_or(None);
    let delegate = mint
        .get_extension::<PermanentDelegate>()
        .map(|extension| Option::<Pubkey>::from(extension.delegate))
        .unwrap_or(None);
    let confidential_authority = mint
        .get_extension::<ConfidentialTransferMint>()
        .ok()
        .and_then(|extension| Option::<Pubkey>::from(extension.authority));
    let withdraw_withheld_authority = mint
        .get_extension::<TransferFeeConfig>()
        .ok()
        .and_then(|config| Option::<Pubkey>::from(config.withdraw_withheld_authority));

    Ok(MintView {
        decimals: mint.base.decimals,
        supply: mint.base.supply,
        mint_authority: mint.base.mint_authority.into(),
        freeze_authority: mint.base.freeze_authority.into(),
        extensions: ExtensionSet {
            transfer_fee_config: mint.get_extension::<TransferFeeConfig>().is_ok(),
            metadata_pointer: metadata_pointer_points_at_self,
            default_account_state: frozen_by_default,
            mint_close_authority: close_authority.is_some(),
            permanent_delegate: delegate.is_some(),
            confidential_transfer_mint: mint
                .get_extension::<ConfidentialTransferMint>()
                .is_ok(),
        },
        mint_close_authority: close_authority,
        permanent_delegate: delegate,
        confidential_transfer_authority: confidential_authority,
        withdraw_withheld_authority,
        frozen_by_default,
    })
}

/// Fee the mint charges for `amount` in `epoch`, computed from the live
/// `TransferFeeConfig` extension rather than a cached rate (task 2).
pub fn transfer_fee_for(mint_info: &AccountInfo, epoch: u64, amount: u64) -> Result<u64> {
    let data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)?;
    let config = mint
        .get_extension::<TransferFeeConfig>()
        .map_err(|_| StablecoinError::TransferFeeNotConfigured)?;
    config
        .calculate_epoch_fee(epoch, amount)
        .ok_or_else(|| StablecoinError::Overflow.into())
}

/// Reads the on-chain token metadata of a mint. The metadata pointer of our
/// mints points at the mint itself, so the mint account carries the
/// `TokenMetadata` extension. Read through `StateWithExtensions`.
pub fn token_metadata_of(mint_info: &AccountInfo) -> Result<TokenMetadata> {
    let data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)?;
    mint.get_variable_len_extension::<TokenMetadata>()
        .map_err(|_| StablecoinError::UnexpectedExtensionSet.into())
}

/// A freshly created token account has no confidential transfer extension until
/// `ConfigureAccount` runs, so absence is reported separately from an
/// unapproved account.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfidentialAccountView {
    pub owner: Pubkey,
    pub approved: bool,
    pub pending_balance_credit_counter: u64,
    pub maximum_pending_balance_credit_counter: u64,
}

/// The policy shared by every confidential instruction that moves a balance:
/// the signer must be the account owner (never a delegate) and the account must
/// have been configured *and* approved by the mint's confidential transfer
/// authority. Both checks read state through `StateWithExtensions`.
pub fn confidential_ready(
    account_info: &AccountInfo,
    owner: &Pubkey,
) -> Result<ConfidentialAccountView> {
    let data = account_info.try_borrow_data()?;
    let account = StateWithExtensions::<SplTokenAccount>::unpack(&data)?;
    require!(
        account.base.owner == *owner,
        StablecoinError::NotAccountOwner
    );
    drop(data);
    confidential_approved(account_info)
}

/// The issuer-side half of the policy: an account has to be configured for
/// confidential transfers and approved by the mint's confidential transfer
/// authority. token-2022 v11 writes the `approved` flag but never reads it, so
/// the `approve_policy = manual` of the re-issued mint is enforced here.
pub fn confidential_approved(account_info: &AccountInfo) -> Result<ConfidentialAccountView> {
    let data = account_info.try_borrow_data()?;
    let account = StateWithExtensions::<SplTokenAccount>::unpack(&data)?;
    let extension = account
        .get_extension::<ConfidentialTransferAccount>()
        .map_err(|_| StablecoinError::ConfidentialAccountNotConfigured)?;
    let view = ConfidentialAccountView {
        owner: account.base.owner,
        approved: bool::from(extension.approved),
        pending_balance_credit_counter: u64::from(extension.pending_balance_credit_counter),
        maximum_pending_balance_credit_counter: u64::from(
            extension.maximum_pending_balance_credit_counter,
        ),
    };
    require!(
        view.approved,
        StablecoinError::ConfidentialAccountNotApproved
    );
    Ok(view)
}
