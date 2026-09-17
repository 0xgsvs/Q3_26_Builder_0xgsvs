use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, Transfer, transfer},
};
use constant_product_curve::{ConstantProduct, LiquidityPair};

use crate::{constants::*, error::AmmError, events::Swapped, state::Config};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    pub mint_x: Box<Account<'info, Mint>>,
    pub mint_y: Box<Account<'info, Mint>>,
    #[account(
        has_one = mint_x,
        has_one = mint_y,
        seeds = [CONFIG_SEED, config.seed.to_le_bytes().as_ref()],
        bump = config.config_bump,
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [LP_SEED, config.key().as_ref()],
        bump = config.lp_bump,
    )]
    pub mint_lp: Box<Account<'info, Mint>>,
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
    /// CHECK: must equal `config.treasury`; receives the protocol fee.
    #[account(constraint = treasury.key() == config.treasury @ AmmError::InvalidTreasury)]
    pub treasury: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_x,
        associated_token::authority = treasury,
    )]
    pub treasury_x: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint_y,
        associated_token::authority = treasury,
    )]
    pub treasury_y: Box<Account<'info, TokenAccount>>,
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
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

impl<'info> Swap<'info> {
    pub fn swap(&mut self, is_x: bool, amount_in: u64, min_amount_out: u64) -> Result<()> {
        require!(!self.config.locked, AmmError::PoolLocked);
        require_neq!(amount_in, 0, AmmError::InvalidAmount);

        // Protocol fee is taken off the top and routed to the treasury.
        // The pool quotes on the net amount with zero curve fee so the
        // fee is not charged twice.
        let fee_amount = (amount_in as u128)
            .checked_mul(self.config.fee as u128)
            .and_then(|v| v.checked_div(FEE_DENOMINATOR as u128))
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(AmmError::Overflow)?;
        let net_in = amount_in
            .checked_sub(fee_amount)
            .ok_or(AmmError::Underflow)?;
        require_neq!(net_in, 0, AmmError::InvalidAmount);

        let mut curve = ConstantProduct::init(
            self.vault_x.amount,
            self.vault_y.amount,
            self.mint_lp.supply,
            0,
            Some(CURVE_PRECISION),
        )
        .map_err(AmmError::from)?;

        let pair = match is_x {
            true => LiquidityPair::X,
            false => LiquidityPair::Y,
        };

        let result = curve
            .swap(pair, net_in, min_amount_out)
            .map_err(|_| AmmError::SlippageExceeded)?;

        self.transfer_in(is_x, result.deposit)?;
        self.transfer_fee(is_x, fee_amount)?;
        self.transfer_out(is_x, result.withdraw)?;

        emit!(Swapped {
            config: self.config.key(),
            user: self.user.key(),
            is_x,
            amount_in,
            fee_amount,
            amount_out: result.withdraw,
        });

        Ok(())
    }

    /// Net swap amount: user -> pool vault.
    fn transfer_in(&self, is_x: bool, amount: u64) -> Result<()> {
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

        transfer(
            CpiContext::new(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    /// Protocol fee: user -> treasury ATA. Skipped when the fee is zero.
    fn transfer_fee(&self, is_x: bool, amount: u64) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }

        let (from, to) = match is_x {
            true => (
                self.user_x.to_account_info(),
                self.treasury_x.to_account_info(),
            ),
            false => (
                self.user_y.to_account_info(),
                self.treasury_y.to_account_info(),
            ),
        };

        transfer(
            CpiContext::new(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.user.to_account_info(),
                },
            ),
            amount,
        )
    }

    /// Swap proceeds: pool vault -> user (opposite side of the deposit).
    fn transfer_out(&self, is_x: bool, amount: u64) -> Result<()> {
        let (from, to) = match is_x {
            true => (
                self.vault_y.to_account_info(),
                self.user_y.to_account_info(),
            ),
            false => (
                self.vault_x.to_account_info(),
                self.user_x.to_account_info(),
            ),
        };

        let seed_bytes = self.config.seed.to_le_bytes();
        let bump_seed = [self.config.config_bump];
        let signer_seeds: &[&[&[u8]]] =
            &[&[CONFIG_SEED, seed_bytes.as_slice(), bump_seed.as_slice()]];

        transfer(
            CpiContext::new_with_signer(
                self.token_program.key(),
                Transfer {
                    from,
                    to,
                    authority: self.config.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
        )
    }
}
