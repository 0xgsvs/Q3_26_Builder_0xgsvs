use anchor_lang::prelude::*;

use crate::{ESCROW_SEED, state::Escrow};

#[derive(Accounts)]
pub struct Update<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,
    #[account(
        mut,
        has_one = maker,
        seeds = [ESCROW_SEED, maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
    pub system_program: Program<'info, System>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, receive: u64, expiration: i64) -> Result<()> {
        self.escrow.receive = receive;
        self.escrow.expiration = expiration;
        Ok(())
    }
}
