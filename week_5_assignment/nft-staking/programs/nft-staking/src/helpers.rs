use anchor_lang::prelude::*;
use mpl_core::{
    Collection,
    accounts::{BaseAssetV1, BaseCollectionV1},
};

use crate::{constants::TOTAL_STAKED, error::ErrorCode};

/// mpl-core account types are Borsh-only and do not implement Anchor's
/// `AccountSerialize`/`AccountDeserialize`/`Owner`, so they cannot be used
/// directly as `Account<'info, T>`. Deserialize them by hand instead.
fn load_borsh<T>(
    account: &UncheckedAccount,
    from_bytes: impl FnOnce(&[u8]) -> std::result::Result<T, std::io::Error>,
) -> Result<T> {
    let data = account.try_borrow_data()?;
    require_keys_eq!(*account.owner, mpl_core::ID, ErrorCode::InvalidOwner);
    from_bytes(&data).map_err(|_| error!(ErrorCode::InvalidAccountData))
}

pub fn load_collection(account: &UncheckedAccount) -> Result<BaseCollectionV1> {
    load_borsh(account, BaseCollectionV1::from_bytes)
}

pub fn load_asset(account: &UncheckedAccount) -> Result<BaseAssetV1> {
    load_borsh(account, BaseAssetV1::from_bytes)
}

/// Full collection, including its plugins (needed to read the `total_staked`
/// attribute).
pub fn load_collection_with_plugins(account: &UncheckedAccount) -> Result<Box<Collection>> {
    load_borsh(account, Collection::from_bytes)
}

/// Reads the `total_staked` attribute from a collection, defaulting to 0.
pub fn read_total_staked(collection: &Collection) -> Result<u64> {
    let Some(attributes) = collection.plugin_list.attributes.as_ref() else {
        return Ok(0);
    };
    let value = attributes
        .attributes
        .attribute_list
        .iter()
        .find(|a| a.key == TOTAL_STAKED)
        .map(|a| a.value.as_str())
        .unwrap_or("0");
    value
        .parse::<u64>()
        .map_err(|_| error!(ErrorCode::InvalidAccountData))
}
