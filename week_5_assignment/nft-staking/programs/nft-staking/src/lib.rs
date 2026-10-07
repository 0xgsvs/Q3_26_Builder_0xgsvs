pub mod constants;
pub mod error;
pub mod helpers;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("EGHTwkYmi2LZtfebzuhVhU4LFqhHyhy1vWdRFro9sYcu");

#[program]
pub mod nft_staking {
    use super::*;

    pub fn create_collection(
        ctx: Context<CreateCollection>,
        name: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_collection::handle_create_collection(ctx, name, uri)
    }

    pub fn create_asset(
        ctx: Context<CreateAsset>,
        name: String,
        uri: String,
    ) -> Result<()> {
        instructions::create_asset::handle_create_asset(ctx, name, uri)
    }

    pub fn initialize(
        ctx: Context<Initialize>,
        reward_bps: u16,
        freeze_period: u16,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, reward_bps, freeze_period)
    }

    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        instructions::stake::handle_stake(ctx)
    }

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim_rewards::handle_claim_rewards(ctx)
    }

    pub fn burn_staked_nft(ctx: Context<BurnStakedNft>) -> Result<()> {
        instructions::burn_staked_nft::handle_burn_staked_nft(ctx)
    }

    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        instructions::unstake::handle_unstake(ctx)
    }

    pub fn init_oracle(ctx: Context<InitOracle>) -> Result<()> {
        instructions::init_oracle::handle_init_oracle(ctx)
    }

    pub fn update_oracle(ctx: Context<UpdateOracle>) -> Result<()> {
        instructions::update_oracle::handle_update_oracle(ctx)
    }

    pub fn transfer_asset(ctx: Context<TransferAsset>) -> Result<()> {
        instructions::transfer_asset::handle_transfer_asset(ctx)
    }
}