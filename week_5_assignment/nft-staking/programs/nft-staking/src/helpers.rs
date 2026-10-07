use anchor_lang::prelude::*;
use mpl_core::accounts::{BaseAssetV1, BaseCollectionV1};

use crate::error::ErrorCode;

/// mpl-core account types are Borsh-only and do not implement Anchor's
/// `AccountSerialize`/`AccountDeserialize`/`Owner`, so they cannot be used
/// directly as `Account<'info, T>`. Deserialize them by hand instead.
pub fn load_collection(account: &UncheckedAccount) -> Result<BaseCollectionV1> {
    let data = account.try_borrow_data()?;
    require_keys_eq!(*account.owner, mpl_core::ID, ErrorCode::InvalidOwner);
    BaseCollectionV1::from_bytes(&data).map_err(|_| error!(ErrorCode::InvalidAccountData))
}

pub fn load_asset(account: &UncheckedAccount) -> Result<BaseAssetV1> {
    let data = account.try_borrow_data()?;
    require_keys_eq!(*account.owner, mpl_core::ID, ErrorCode::InvalidOwner);
    BaseAssetV1::from_bytes(&data).map_err(|_| error!(ErrorCode::InvalidAccountData))
}