use anchor_lang::prelude::*;

use crate::{constants::*, error::AmmError, events::PoolUpdated, state::Config};

#[derive(Accounts)]
pub struct Update<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED, config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, fee: u16, locked: bool) -> Result<()> {
        require!(self.config.authority.is_some(), AmmError::NoAuthoritySet);
        require!(
            self.config.authority == Some(self.authority.key()),
            AmmError::InvalidAuthority
        );
        require!(fee <= FEE_DENOMINATOR as u16, AmmError::FeePercentErr);

        self.config.fee = fee;
        self.config.locked = locked;

        emit!(PoolUpdated {
            config: self.config.key(),
            authority: self.authority.key(),
            fee,
            locked,
        });

        Ok(())
    }
}
