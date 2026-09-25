use anchor_lang::prelude::*;

use crate::state::ExtensionSet;

#[event]
pub struct MintInitialized {
    pub mint: Pubkey,
    pub mint_authority: Pubkey,
    pub freeze_authority: Pubkey,
    pub close_authority: Pubkey,
    pub withdraw_withheld_authority: Pubkey,
    pub decimals: u8,
    pub transfer_fee_basis_points: u16,
    pub maximum_fee: u64,
    pub extensions: ExtensionSet,
}

#[event]
pub struct TokenAccountCreated {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub owner: Pubkey,
    pub frozen: bool,
}

#[event]
pub struct AccountThawed {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub freeze_authority: Pubkey,
}

#[event]
pub struct AccountFrozen {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub freeze_authority: Pubkey,
}

#[event]
pub struct TokensMinted {
    pub mint: Pubkey,
    pub destination: Pubkey,
    pub mint_authority: Pubkey,
    pub amount: u64,
}

#[event]
pub struct TokensTransferred {
    pub mint: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
    pub authority: Pubkey,
    pub amount: u64,
    /// Fee the token-2022 program withheld on the destination account, computed
    /// from `calculate_epoch_fee(epoch, amount)`.
    pub fee: u64,
    pub epoch: u64,
}

#[event]
pub struct WithheldFeesCollected {
    pub mint: Pubkey,
    pub destination: Pubkey,
    pub authority: Pubkey,
    pub sources: u8,
}

#[event]
pub struct MintClosed {
    pub mint: Pubkey,
    pub destination: Pubkey,
    pub close_authority: Pubkey,
    pub lamports: u64,
}

#[event]
pub struct MintReissued {
    pub previous_mint: Pubkey,
    pub new_mint: Pubkey,
    pub permanent_delegate: Pubkey,
    pub confidential_transfer_authority: Pubkey,
    pub previous: ExtensionSet,
    pub new: ExtensionSet,
    pub carried_forward: ExtensionSet,
    pub added: ExtensionSet,
    pub dropped: ExtensionSet,
}

#[event]
pub struct FundsSeized {
    pub mint: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
    pub permanent_delegate: Pubkey,
    pub amount: u64,
}

#[event]
pub struct ConfidentialAccountConfigured {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub owner: Pubkey,
    pub maximum_pending_balance_credit_counter: u64,
}

#[event]
pub struct ConfidentialAccountApproved {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub authority: Pubkey,
}

#[event]
pub struct ConfidentialTokensDeposited {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub amount: u64,
}

#[event]
pub struct PendingBalanceApplied {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub expected_pending_balance_credit_counter: u64,
}

#[event]
pub struct ConfidentialTokensTransferred {
    pub mint: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
    pub authority: Pubkey,
}

#[event]
pub struct ConfidentialTokensWithdrawn {
    pub mint: Pubkey,
    pub account: Pubkey,
    pub amount: u64,
    pub pending_balance_applied: bool,
}
