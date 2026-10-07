//! Collection creation through the mpl-core CPI.

mod common;

use {
    common::{Env, NAME, URI},
    mpl_core::{ID as MPL_CORE_ID, accounts::BaseCollectionV1},
};

#[test]
fn creates_collection_via_mpl_core_cpi() {
    let mut env = Env::new();
    env.create_collection();

    let account = env
        .svm
        .get_account(&env.collection)
        .expect("collection account exists");
    assert_eq!(account.owner, MPL_CORE_ID);

    let collection = BaseCollectionV1::from_bytes(&account.data).expect("valid BaseCollectionV1");
    assert_eq!(collection.name, NAME);
    assert_eq!(collection.uri, URI);
    assert_eq!(collection.update_authority, env.update_authority);
}
