use anchor_lang::prelude::*;

#[event]
pub struct PoolInitialized {
    pub config: Pubkey,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    pub mint_lp: Pubkey,
    pub vault_x: Pubkey,
    pub vault_y: Pubkey,
    pub treasury: Pubkey,
    pub seed: u64,
    pub fee: u16,
}

#[event]
pub struct LiquidityDeposited {
    pub config: Pubkey,
    pub user: Pubkey,
    pub lp_minted: u64,
    pub x_deposited: u64,
    pub y_deposited: u64,
}

#[event]
pub struct LiquidityWithdrawn {
    pub config: Pubkey,
    pub user: Pubkey,
    pub lp_burned: u64,
    pub x_withdrawn: u64,
    pub y_withdrawn: u64,
}

#[event]
pub struct Swapped {
    pub config: Pubkey,
    pub user: Pubkey,
    pub is_x: bool,
    pub amount_in: u64,
    pub fee_amount: u64,
    pub amount_out: u64,
}

#[event]
pub struct PoolUpdated {
    pub config: Pubkey,
    pub authority: Pubkey,
    pub fee: u16,
    pub locked: bool,
}
