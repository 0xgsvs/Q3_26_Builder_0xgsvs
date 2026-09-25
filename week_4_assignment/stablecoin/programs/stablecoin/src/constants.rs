use anchor_lang::prelude::*;

/// Seed for the `ReissueRecord` PDA that links a fee mint to its confidential re-issue.
#[constant]
pub const REISSUE_SEED: &[u8] = b"reissue";

/// Fee denominator: fee rates are expressed in basis points (1/100 of 1%).
#[constant]
pub const FEE_DENOMINATOR: u16 = 10_000;

/// Largest value the transfer-fee extension accepts.
#[constant]
pub const MAXIMUM_TRANSFER_FEE_BPS: u16 = 10_000;

/// Decimals of the remittance stablecoin.
#[constant]
pub const DECIMALS: u8 = 6;

/// TLV entry header (2 byte type + 2 byte length) that precedes an extension payload.
pub const TLV_ENTRY_HEADER_LEN: usize = 4;
