use crate::{constants::*, state::*};
use anchor_lang::{
    prelude::*,
    system_program::{Transfer, transfer},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(
    mut,
    seeds = [VAULT_SEED, vault_state.key().as_ref()],
    bump = vault_state.vault_bump,
  )]
    pub vault: SystemAccount<'info>,
    #[account(
    seeds = [VAULT_STATE_SEED, user.key().as_ref()],
    bump = vault_state.state_bump
  )]
    pub vault_state: Account<'info, VaultState>,
    system_program: Program<'info, System>,
}

impl<'info> Withdraw<'info> {
    pub fn withdraw(&mut self, amount: u64) -> Result<()> {
        let cpi_accounts = Transfer {
            from: self.vault.to_account_info(),
            to: self.user.to_account_info(),
        };
        let seeds = &[
            VAULT_SEED,
            self.vault_state.to_account_info().key.as_ref(),
            &[self.vault_state.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];
        let cpi_ctx = CpiContext::new_with_signer(System::id(), cpi_accounts, signer_seeds);
        transfer(cpi_ctx, amount)?;

        Ok(())
    }
}
