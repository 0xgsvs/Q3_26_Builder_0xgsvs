use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub reward_bps: u16,
    pub rewards_bump: u8,
    pub freeze_period: u16,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct StakeState {
    pub owner: Pubkey,
    pub staked_at: i64,
    pub last_claim: i64,
    pub bump: u8,
}

/// The oracle account read by the mpl-core Oracle external plugin adapter with
/// `ValidationResultsOffset::Anchor`. The first five bytes after the Anchor
/// discriminator must be borsh `OracleValidation::V1 { create, transfer, burn,
/// update }`, so `variant` must stay 1 (V1) and the four results are raw
/// `ExternalValidationResult` bytes (0 = Approved, 1 = Rejected, 2 = Pass).
#[account]
#[derive(InitSpace)]
pub struct Oracle {
    /// `OracleValidation` variant tag. Must be 1 (V1).
    pub variant: u8,
    pub create: u8,
    /// The gated lifecycle result: 0 = Approved, 1 = Rejected.
    pub transfer: u8,
    pub burn: u8,
    pub update: u8,
    /// Slot of the last crank update.
    pub last_slot: u64,
    /// Absolute UTC hour recorded by the last crank.
    pub last_hour: u8,
    pub bump: u8,
}

/// `Oracle` account layout: Approved = 0, Rejected = 1, Pass = 2.
pub const ORACLE_APPROVED: u8 = 0;
pub const ORACLE_REJECTED: u8 = 1;
pub const ORACLE_PASS: u8 = 2;

/// Program-owned vault holding the lamports used to reward boundary cranks.
#[account]
#[derive(InitSpace)]
pub struct OracleVault {
    pub bump: u8,
}