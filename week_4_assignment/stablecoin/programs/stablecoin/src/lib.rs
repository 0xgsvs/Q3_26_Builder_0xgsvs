pub mod constants;
pub mod error;
pub mod events;
pub mod extensions;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("J4YPy3eboDrRJ1cnta5H7UZ36TVY9CwzFikCs1TGeau");

#[program]
pub mod stablecoin {
    use super::*;

    /// Creates the remittance mint with `TransferFeeConfig`, `MetadataPointer`
    /// (pointed at the mint itself), `DefaultAccountState` (frozen) and
    /// `MintCloseAuthority`, all initialized before `InitializeMint` (task 1).
    pub fn initialize_mint(
        ctx: Context<InitializeMint>,
        decimals: u8,
        name: String,
        symbol: String,
        uri: String,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        ctx.accounts.init(
            decimals,
            name,
            symbol,
            uri,
            transfer_fee_basis_points,
            maximum_fee,
        )
    }

    /// Opens a token account with room for the account-level extensions.
    pub fn create_token_account(ctx: Context<CreateTokenAccount>) -> Result<()> {
        ctx.accounts.create()
    }

    /// Issuance against the mint authority.
    pub fn mint_to(ctx: Context<MintTo>, amount: u64) -> Result<()> {
        ctx.accounts.mint(amount)
    }

    /// KYC path: the freeze authority thaws a single account (task 4).
    pub fn thaw_account(ctx: Context<ThawAccount>) -> Result<()> {
        ctx.accounts.thaw()
    }

    /// The freeze authority re-freezes a single account.
    pub fn freeze_account(ctx: Context<FreezeAccount>) -> Result<()> {
        ctx.accounts.freeze()
    }

    /// Public transfer that charges the protocol fee through
    /// `transfer_checked_with_fee` (task 2).
    pub fn transfer_with_fee(ctx: Context<TransferWithFee>, amount: u64) -> Result<()> {
        ctx.accounts.transfer(amount)
    }

    /// Sweeps withheld transfer fees to the issuer.
    pub fn collect_fees<'info>(ctx: Context<'info, CollectFees<'info>>) -> Result<()> {
        let sources = ctx.remaining_accounts.to_vec();
        ctx.accounts.collect(&sources)
    }

    /// Closes the mint through its `MintCloseAuthority`.
    pub fn close_mint(ctx: Context<CloseMint>) -> Result<()> {
        ctx.accounts.close()
    }

    /// Re-issues the mint for confidential transfers with a permanent delegate,
    /// recording the extension sets and the gap between them (task 5).
    pub fn reissue_mint(ctx: Context<ReissueMint>) -> Result<()> {
        ctx.accounts.reissue(ctx.bumps.reissue_record)
    }

    /// Seizure through the permanent delegate.
    pub fn seize(ctx: Context<Seize>, amount: u64) -> Result<()> {
        ctx.accounts.seize(amount)
    }

    /// Configures a token account for confidential transfers. Owner-only,
    /// unlike the permissionless creation of the account itself (task 6).
    pub fn configure_confidential_account(
        ctx: Context<ConfigureConfidentialAccount>,
        decryptable_zero_balance: [u8; 36],
        maximum_pending_balance_credit_counter: u64,
    ) -> Result<()> {
        ctx.accounts
            .configure(decryptable_zero_balance, maximum_pending_balance_credit_counter)
    }

    /// `approve_policy = manual`: the mint's confidential transfer authority
    /// clears a configured account.
    pub fn approve_confidential_account(ctx: Context<ApproveConfidentialAccount>) -> Result<()> {
        ctx.accounts.approve()
    }

    /// Public balance -> confidential pending balance.
    pub fn deposit_confidential(ctx: Context<DepositConfidential>, amount: u64) -> Result<()> {
        ctx.accounts.deposit(amount)
    }

    /// Pending balance -> available (spendable) balance.
    pub fn apply_pending_balance(
        ctx: Context<ApplyPendingBalance>,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        ctx.accounts.apply(
            expected_pending_balance_credit_counter,
            new_decryptable_available_balance,
        )
    }

    /// Confidential transfer, proven by context state accounts.
    pub fn confidential_transfer(
        ctx: Context<ConfidentialTransfer>,
        new_source_decryptable_available_balance: [u8; 36],
        transfer_amount_auditor_ciphertext_lo: [u8; 64],
        transfer_amount_auditor_ciphertext_hi: [u8; 64],
    ) -> Result<()> {
        ctx.accounts.transfer(
            new_source_decryptable_available_balance,
            transfer_amount_auditor_ciphertext_lo,
            transfer_amount_auditor_ciphertext_hi,
        )
    }

    /// Confidential balance -> public balance, applying the pending balance
    /// first.
    pub fn withdraw_confidential(
        ctx: Context<WithdrawConfidential>,
        amount: u64,
        expected_pending_balance_credit_counter: u64,
        decryptable_available_balance: [u8; 36],
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        ctx.accounts.withdraw(
            amount,
            expected_pending_balance_credit_counter,
            decryptable_available_balance,
            new_decryptable_available_balance,
        )
    }
}
