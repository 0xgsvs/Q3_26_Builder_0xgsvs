use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid asset owner")]
    InvalidOwner,
    #[msg("Invalid update authority")]
    InvalidUpdateAuthority,
    #[msg("Invalid rewards bps")]
    InvalidRewardsBps,
    #[msg("Invalid account data")]
    InvalidAccountData,
    #[msg("Numerical overflow")]
    NumericalOverflow,
}
