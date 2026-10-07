use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub reward_bps: u16,
    pub rewards_bump: u8,
    pub freeze_period: u16,
    pub bump: u8,
}
