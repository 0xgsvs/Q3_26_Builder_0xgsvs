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