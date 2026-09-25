use anchor_lang::prelude::*;

/// The mint-level token-2022 extensions this program knows about.
///
/// Used both to *observe* the extension set of a mint (read through
/// `StateWithExtensions`, never a raw unpack) and to *record* the extension sets
/// of the fee mint and its confidential re-issue on-chain.
#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    InitSpace,
)]
pub struct ExtensionSet {
    pub transfer_fee_config: bool,
    pub metadata_pointer: bool,
    pub default_account_state: bool,
    pub mint_close_authority: bool,
    pub permanent_delegate: bool,
    pub confidential_transfer_mint: bool,
}

impl ExtensionSet {
    /// Extension set of the fee-bearing mint: protocol fee, on-chain metadata,
    /// frozen-by-default accounts and a mint close authority.
    pub const FEE_MINT: ExtensionSet = ExtensionSet {
        transfer_fee_config: true,
        metadata_pointer: true,
        default_account_state: true,
        mint_close_authority: true,
        permanent_delegate: false,
        confidential_transfer_mint: false,
    };

    /// Extension set of the re-issued mint: the same metadata pointer, default
    /// account state and close authority, plus the seizure authority
    /// (`PermanentDelegate`) and confidential transfers.
    ///
    /// `transfer_fee_config` is absent on purpose — see `README.md`, the gap.
    pub const CONFIDENTIAL_MINT: ExtensionSet = ExtensionSet {
        transfer_fee_config: false,
        metadata_pointer: true,
        default_account_state: true,
        mint_close_authority: true,
        permanent_delegate: true,
        confidential_transfer_mint: true,
    };

    /// Extensions carried over unchanged from the fee mint to the re-issue.
    pub fn carried_forward(&self) -> ExtensionSet {
        ExtensionSet {
            transfer_fee_config: self.transfer_fee_config,
            metadata_pointer: self.metadata_pointer,
            default_account_state: self.default_account_state,
            mint_close_authority: self.mint_close_authority,
            permanent_delegate: false,
            confidential_transfer_mint: false,
        }
    }

    /// Extensions the re-issue adds on top of the fee mint.
    pub fn added(&self) -> ExtensionSet {
        ExtensionSet {
            transfer_fee_config: false,
            metadata_pointer: false,
            default_account_state: false,
            mint_close_authority: false,
            permanent_delegate: self.permanent_delegate,
            confidential_transfer_mint: self.confidential_transfer_mint,
        }
    }

    /// Extensions of the fee mint that cannot be carried into the re-issue.
    pub fn dropped(&self) -> ExtensionSet {
        ExtensionSet {
            transfer_fee_config: self.transfer_fee_config,
            metadata_pointer: false,
            default_account_state: false,
            mint_close_authority: false,
            permanent_delegate: false,
            confidential_transfer_mint: false,
        }
    }
}

/// Links a fee mint to the confidential mint that replaces it.
#[account]
#[derive(InitSpace)]
pub struct ReissueRecord {
    /// The fee-bearing mint that was re-issued.
    pub previous_mint: Pubkey,
    /// The confidential mint created to replace it.
    pub new_mint: Pubkey,
    /// Extension set observed on the previous mint.
    pub previous: ExtensionSet,
    /// Extension set of the new mint.
    pub new: ExtensionSet,
    /// Extensions kept from the previous mint.
    pub carried_forward: ExtensionSet,
    /// Extensions added by the re-issue.
    pub added: ExtensionSet,
    /// Extensions that could not be carried forward.
    pub dropped: ExtensionSet,
    pub bump: u8,
}
