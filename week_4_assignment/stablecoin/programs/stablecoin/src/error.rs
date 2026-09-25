use anchor_lang::prelude::*;

#[error_code]
pub enum StablecoinError {
    #[msg("Decimals must be between 0 and 9.")]
    InvalidDecimals,
    #[msg("Transfer fee must be between 1 and 10,000 basis points.")]
    InvalidTransferFee,
    #[msg("Name and symbol must not be empty.")]
    InvalidMetadata,
    #[msg("Amount must be greater than zero.")]
    InvalidAmount,
    #[msg("The mint has no transfer fee config extension.")]
    TransferFeeNotConfigured,
    #[msg("Signer is not the mint authority.")]
    NotMintAuthority,
    #[msg("Signer is not the freeze authority.")]
    NotFreezeAuthority,
    #[msg("Signer is not the mint close authority.")]
    NotCloseAuthority,
    #[msg("Signer is not the withdraw withheld authority.")]
    NotWithdrawWithheldAuthority,
    #[msg("Signer is not the permanent delegate (seizure authority).")]
    NotPermanentDelegate,
    #[msg("Signer is not the confidential transfer mint authority.")]
    NotConfidentialTransferAuthority,
    #[msg("The mint still has supply.")]
    MintHasSupply,
    #[msg("The account is frozen. The freeze authority must thaw it first.")]
    AccountFrozen,
    #[msg("Signer is not the owner of the token account.")]
    NotAccountOwner,
    #[msg("The token account has no confidential transfer extension.")]
    ConfidentialAccountNotConfigured,
    #[msg("The confidential transfer account has not been approved by the mint authority.")]
    ConfidentialAccountNotApproved,
    #[msg("The pending balance is full. Apply it before crediting the account again.")]
    PendingBalanceFull,
    #[msg("The mint is not configured for confidential transfers.")]
    MintNotConfiguredForConfidentialTransfers,
    #[msg("Confidential transfers are not available on a mint that charges a transfer fee.")]
    ConfidentialTransferUnavailableWithTransferFee,
    #[msg("The previous mint does not carry the extension set this program re-issues.")]
    UnexpectedExtensionSet,
    #[msg("The two mints are not compatible.")]
    IncompatibleMint,
    #[msg("Arithmetic overflow.")]
    Overflow,
}
