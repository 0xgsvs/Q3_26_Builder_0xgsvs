use anchor_lang::prelude::*;
use mpl_core::accounts::{BaseAssetV1, BaseCollectionV1};
use mpl_core::Collection;

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

/// Full collection, including its plugins (needed to read the `total_staked`
/// attribute).
pub fn load_collection_with_plugins(account: &UncheckedAccount) -> Result<Box<Collection>> {
    let data = account.try_borrow_data()?;
    require_keys_eq!(*account.owner, mpl_core::ID, ErrorCode::InvalidOwner);
    Collection::from_bytes(&data).map_err(|_| error!(ErrorCode::InvalidAccountData))
}

/// Reads the `total_staked` attribute from a collection, defaulting to 0.
pub fn read_total_staked(collection: &Box<Collection>, key: &str) -> Result<u64> {
    let Some(attributes) = collection.plugin_list.attributes.as_ref() else {
        return Ok(0);
    };
    let value = attributes
        .attributes
        .attribute_list
        .iter()
        .find(|a| a.key == key)
        .map(|a| a.value.clone())
        .unwrap_or_else(|| "0".to_string());
    value.parse::<u64>().map_err(|_| error!(ErrorCode::InvalidAccountData))
}