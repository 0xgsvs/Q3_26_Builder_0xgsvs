pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::*;
pub use instructions::*;
pub use state::*;

declare_id!("EGHTwkYmi2LZtfebzuhVhU4LFqhHyhy1vWdRFro9sYcu");

#[program]
pub mod nft_staking {
    use super::*;
}
