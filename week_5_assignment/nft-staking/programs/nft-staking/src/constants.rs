use anchor_lang::prelude::*;

#[constant]
pub const CONFIG: &[u8] = b"config";

#[constant]
pub const COLLECTION: &[u8] = b"collection";

#[constant]
pub const UPDATE_AUTHORITY: &[u8] = b"update_authority";

#[constant]
pub const REWARDS_MINT: &[u8] = b"rewards_mint";

#[constant]
pub const STAKE: &[u8] = b"stake";

/// One-time bonus (in reward base units) minted when a staked NFT is burned.
#[constant]
pub const BURN_BONUS: u64 = 1_000_000;

/// Collection attribute key holding the number of currently staked assets.
#[constant]
pub const TOTAL_STAKED: &str = "total_staked";

/// PDA seed for the per-collection oracle account read by the mpl-core Oracle
/// external plugin adapter.
#[constant]
pub const ORACLE: &[u8] = b"oracle";

/// PDA seed for the oracle crank reward vault.
#[constant]
pub const ORACLE_VAULT: &[u8] = b"oracle_vault";

/// Open window for transfers, in whole hours UTC: [OPEN_HOUR, CLOSE_HOUR).
pub const OPEN_HOUR: u8 = 9;
pub const CLOSE_HOUR: u8 = 17;

/// A crank within this many seconds of an open/close boundary earns a reward.
pub const BOUNDARY_TOLERANCE: i64 = 60;

/// Reward (lamports) paid to the caller of a boundary crank.
pub const ORACLE_REWARD: u64 = 1_000_000;
