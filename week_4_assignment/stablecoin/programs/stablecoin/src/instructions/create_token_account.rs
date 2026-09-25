use anchor_lang::{
    prelude::*,
    solana_program::rent::Rent,
    system_program::{create_account, CreateAccount},
};
use anchor_spl::{
    token_2022::{
        initialize_account3,
        spl_token_2022::{extension::ExtensionType, state::Account as SplAccount},
        InitializeAccount3, Token2022,
    },
    token_interface::Mint,
};

use crate::{
    events::TokenAccountCreated,
    extensions::{mint_view, TOKEN_ACCOUNT_EXTENSIONS},
};

/// Opens a token account with room for the account-level extensions this
/// program uses (task 1 / task 6).
///
/// Creating the account is deliberately permissionless, exactly like the
/// associated token account program: anyone may pay for an account owned by
/// somebody else. What is *not* permissionless is turning that account into a
/// confidential one — see `ConfigureConfidentialAccount`, which needs the
/// owner's signature and a proof of the owner's AES key.
#[derive(Accounts)]
pub struct CreateTokenAccount<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub account: Signer<'info>,
    /// CHECK: owner of the new account. Anyone may create an account for any
    /// owner, so no relationship with the payer is required.
    pub owner: UncheckedAccount<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> CreateTokenAccount<'info> {
    pub fn create(&mut self) -> Result<()> {
        // `TransferFeeAmount` is required by a fee-bearing mint and is
        // initialized by `InitializeAccount3`; `ConfidentialTransferAccount` is
        // only reserved here so the owner can configure it later without a
        // realloc.
        let account_len =
            ExtensionType::try_calculate_account_len::<SplAccount>(TOKEN_ACCOUNT_EXTENSIONS)?;
        let frozen = mint_view(&self.mint.to_account_info())?.frozen_by_default;

        create_account(
            CpiContext::new(
                self.system_program.key(),
                CreateAccount {
                    from: self.payer.to_account_info(),
                    to: self.account.to_account_info(),
                },
            ),
            Rent::get()?.minimum_balance(account_len),
            account_len as u64,
            &self.token_program.key(),
        )?;

        initialize_account3(CpiContext::new(
            self.token_program.key(),
            InitializeAccount3 {
                account: self.account.to_account_info(),
                mint: self.mint.to_account_info(),
                authority: self.owner.to_account_info(),
            },
        ))?;

        emit!(TokenAccountCreated {
            mint: self.mint.key(),
            account: self.account.key(),
            owner: self.owner.key(),
            frozen,
        });

        Ok(())
    }
}
