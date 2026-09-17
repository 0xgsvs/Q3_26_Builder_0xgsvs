use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub seed: u64,
    /// Optional admin: the only signer allowed to update fee / lock flag.
    pub authority: Option<Pubkey>,
    /// Protocol fee recipient. Swap fees are routed to its X/Y token accounts.
    pub treasury: Pubkey,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    /// Swap fee in basis points (0..=10_000).
    pub fee: u16,
    /// When true, deposit / withdraw / swap are all rejected.
    pub locked: bool,
    pub config_bump: u8,
    pub lp_bump: u8,
}
