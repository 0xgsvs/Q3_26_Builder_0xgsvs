use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const LP_SEED: &[u8] = b"lp";

/// Fee denominator: `fee` is expressed in basis points (1/100 of 1%).
#[constant]
pub const FEE_DENOMINATOR: u64 = 10_000;

/// Precision used by the constant-product-curve math (matches 6-decimal mints).
#[constant]
pub const CURVE_PRECISION: u8 = 6;
