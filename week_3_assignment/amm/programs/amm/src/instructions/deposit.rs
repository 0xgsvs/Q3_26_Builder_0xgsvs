use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, MintTo, Token, TokenAccount, Transfer, mint_to, transfer},
};
use constant_product_curve::ConstantProduct;

use crate::{constants::*, error::AmmError, events::LiquidityDeposited, state::Config};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,
    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [CONFIG_SEED, config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [LP_SEED, config.key().as_ref()],
        bump = config.lp_bump,
    )]
    pub mint_lp: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_x,
        associated_token::authority = user,
    )]
    pub user_x: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_y,
        associated_token::authority = user,
    )]
    pub user_y: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_lp,
        associated_token::authority = user,
    )]
    pub user_lp: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Deposit<'info> {
    pub fn deposit(
        &mut self,
        amount: u64, // Amount of LP tokens the user wants to claim
        max_x: u64,  // Max token X the user is willing to deposit
        max_y: u64,  // Max token Y the user is willing to deposit
    ) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require_neq!(amount, 0, AmmError::InvalidAmount);

        let (x, y) =
            if self.mint_lp.supply == 0 && self.vault_x.amount == 0 && self.vault_y.amount == 0 {
                (max_x, max_y)
            } else {
                let amounts = ConstantProduct::xy_deposit_amounts_from_l(
                    self.vault_x.amount,
                    self.vault_y.amount,
                    self.mint_lp.supply,
                    amount,
                    CURVE_PRECISION as u32,
                )
                .map_err(AmmError::from)?;

                require!(
                    amounts.x <= max_x && amounts.y <= max_y,
                    AmmError::SlippageExceeded
                );

                (amounts.x, amounts.y)
            };

        self.deposit_tokens(true, x)?;
        self.deposit_tokens(false, y)?;
        self.mint_lp_tokens(amount)?;

        emit!(LiquidityDeposited {
            config: self.config.key(),
            user: self.user.key(),
            lp_minted: amount,
            x_deposited: x,
            y_deposited: y,
        });

        Ok(())
    }

    pub fn deposit_tokens(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.vault_x.to_account_info(),
            ),
            false => (
                self.user_y.to_account_info(),
                self.vault_y.to_account_info(),
            ),
        };

        let ctx = CpiContext::new(
            self.token_program.key(),
            Transfer {
                from,
                to,
                authority: self.user.to_account_info(),
            },
        );

        transfer(ctx, amount)
    }

    pub fn mint_lp_tokens(&self, amount: u64) -> Result<()> {
        let seed_bytes = self.config.seed.to_le_bytes();
        let bump_seed = [self.config.config_bump];
        let signer_seeds: &[&[&[u8]]] =
            &[&[CONFIG_SEED, seed_bytes.as_slice(), bump_seed.as_slice()]];

        let ctx = CpiContext::new_with_signer(
            self.token_program.key(),
            MintTo {
                mint: self.mint_lp.to_account_info(),
                to: self.user_lp.to_account_info(),
                authority: self.config.to_account_info(),
            },
            signer_seeds,
        );

        mint_to(ctx, amount)
    }
}
