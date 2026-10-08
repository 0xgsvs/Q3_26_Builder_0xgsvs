//! Shared test harness for the nft-staking integration tests.
//!
//! Each integration test file compiles as its own crate, so every file that
//! wants these helpers adds `mod common;`.

#![allow(dead_code)]

use anchor_lang::{
    AccountDeserialize, InstructionData, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use litesvm::LiteSVM;
use mpl_core::{ID as MPL_CORE_ID, accounts::BaseAssetV1};
use solana_awesome::{
    keypair::Keypair,
    message::{Message, VersionedMessage},
    signer::Signer,
    transaction::versioned::VersionedTransaction,
};
use solana_clock::Clock;

pub const NAME: &str = "Collection";
pub const URI: &str = "https://example.com/c.json";
pub const ASSET_NAME: &str = "Asset";
pub const ASSET_URI: &str = "https://example.com/a.json";

pub fn pda(seeds: &[&[u8]], program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program_id).0
}

pub fn send_ok(
    svm: &mut LiteSVM,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> litesvm::types::TransactionMetadata {
    match send(svm, signers, ixs) {
        Ok(meta) => meta,
        Err(failed) => panic!("transaction failed:\n{}", failed.meta.pretty_logs()),
    }
}

/// Sends a transaction that is expected to fail and returns the failed meta.
pub fn send_err(
    svm: &mut LiteSVM,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> litesvm::types::FailedTransactionMetadata {
    match send(svm, signers, ixs) {
        Ok(meta) => panic!("transaction unexpectedly succeeded: {:?}", meta.logs),
        Err(failed) => failed,
    }
}

fn send(
    svm: &mut LiteSVM,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

pub fn warp(svm: &mut LiteSVM, unix_timestamp: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = unix_timestamp;
    svm.set_sysvar(&clock);
}

pub fn owner_ata(mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            owner.as_ref(),
            anchor_spl::token::ID.as_ref(),
            mint.as_ref(),
        ],
        &anchor_spl::associated_token::ID,
    )
    .0
}

/// SPL token account amount lives at offset 64, 8 bytes LE.
pub fn token_amount(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let account = svm.get_account(ata).expect("token account exists");
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

/// Reads the `total_staked` attribute from the collection.
pub fn read_total_staked(svm: &LiteSVM, collection: &Pubkey) -> u64 {
    let account = svm.get_account(collection).unwrap();
    let collection = mpl_core::Collection::from_bytes(&account.data).unwrap();
    collection
        .plugin_list
        .attributes
        .as_ref()
        .unwrap()
        .attributes
        .attribute_list
        .iter()
        .find(|a| a.key == "total_staked")
        .unwrap()
        .value
        .parse()
        .unwrap()
}

/// LiteSVM environment with the program, the mpl-core program, a funded payer
/// and the derived collection / update-authority PDAs.
pub struct Env {
    pub svm: LiteSVM,
    pub program_id: Pubkey,
    pub payer: Keypair,
    pub collection: Pubkey,
    pub update_authority: Pubkey,
    pub config: Pubkey,
    pub rewards_mint: Pubkey,
}

impl Env {
    pub fn new() -> Self {
        let program_id = nft_staking::id();
        let payer = Keypair::new();

        let mut svm = LiteSVM::new();
        svm.add_program(
            program_id,
            include_bytes!(concat!(
                env!("CARGO_TARGET_TMPDIR"),
                "/../deploy/nft_staking.so"
            )),
        )
        .unwrap();
        svm.add_program(
            MPL_CORE_ID,
            include_bytes!("../fixtures/mpl_core_program.so"),
        )
        .unwrap();
        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();

        let collection = pda(
            &[nft_staking::COLLECTION, payer.pubkey().as_ref()],
            &program_id,
        );
        let update_authority = pda(
            &[nft_staking::UPDATE_AUTHORITY, collection.as_ref()],
            &program_id,
        );
        let config = pda(&[nft_staking::CONFIG, collection.as_ref()], &program_id);
        let rewards_mint = pda(&[nft_staking::REWARDS_MINT, config.as_ref()], &program_id);

        Self {
            svm,
            program_id,
            payer,
            collection,
            update_authority,
            config,
            rewards_mint,
        }
    }

    pub fn stake_state(&self, asset: &Pubkey) -> Pubkey {
        pda(&[nft_staking::STAKE, asset.as_ref()], &self.program_id)
    }

    pub fn oracle(&self) -> Pubkey {
        pda(
            &[nft_staking::ORACLE, self.collection.as_ref()],
            &self.program_id,
        )
    }

    pub fn oracle_vault(&self) -> Pubkey {
        pda(
            &[nft_staking::ORACLE_VAULT, self.collection.as_ref()],
            &self.program_id,
        )
    }

    pub fn create_collection(&mut self) {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::CreateCollection {
                name: NAME.to_string(),
                uri: URI.to_string(),
            }
            .data(),
            nft_staking::accounts::CreateCollection {
                payer: self.payer.pubkey(),
                collection: self.collection,
                update_authority: self.update_authority,
                mpl_core_program: MPL_CORE_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn create_asset(&mut self, asset: &Keypair) {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::CreateAsset {
                name: ASSET_NAME.to_string(),
                uri: ASSET_URI.to_string(),
            }
            .data(),
            nft_staking::accounts::CreateAsset {
                payer: self.payer.pubkey(),
                asset: asset.pubkey(),
                collection: self.collection,
                update_authority: self.update_authority,
                mpl_core_program: MPL_CORE_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_ok(&mut self.svm, &[&self.payer, asset], &[ix]);
    }

    pub fn initialize(&mut self, reward_bps: u16) {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::Initialize { reward_bps }.data(),
            nft_staking::accounts::Initialize {
                admin: self.payer.pubkey(),
                config: self.config,
                collection: self.collection,
                update_authority: self.update_authority,
                rewards_mint: self.rewards_mint,
                system_program: system_program::ID,
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
        );
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn stake_ix(&self, asset: &Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::Stake {}.data(),
            nft_staking::accounts::Stake {
                owner: self.payer.pubkey(),
                stake_state: self.stake_state(asset),
                asset: *asset,
                collection: self.collection,
                update_authority: self.update_authority,
                mpl_core_program: MPL_CORE_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub fn unstake_ix(&self, asset: &Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::Unstake {}.data(),
            nft_staking::accounts::Unstake {
                owner: self.payer.pubkey(),
                stake_state: self.stake_state(asset),
                asset: *asset,
                collection: self.collection,
                update_authority: self.update_authority,
                mpl_core_program: MPL_CORE_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub fn stake(&mut self, asset: &Pubkey) {
        let ix = self.stake_ix(asset);
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn unstake(&mut self, asset: &Pubkey) {
        let ix = self.unstake_ix(asset);
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn stake_state_owner(&self, asset: &Pubkey) -> Pubkey {
        let account = self
            .svm
            .get_account(&self.stake_state(asset))
            .expect("stake state");
        let mut data: &[u8] = &account.data;
        nft_staking::state::StakeState::try_deserialize(&mut data)
            .unwrap()
            .owner
    }

    pub fn last_claim(&self, asset: &Pubkey) -> i64 {
        let account = self
            .svm
            .get_account(&self.stake_state(asset))
            .expect("stake state");
        let mut data: &[u8] = &account.data;
        nft_staking::state::StakeState::try_deserialize(&mut data)
            .unwrap()
            .last_claim
    }

    pub fn reward_carry(&self, asset: &Pubkey) -> u64 {
        let account = self
            .svm
            .get_account(&self.stake_state(asset))
            .expect("stake state");
        let mut data: &[u8] = &account.data;
        nft_staking::state::StakeState::try_deserialize(&mut data)
            .unwrap()
            .reward_carry
    }

    pub fn claim_ix(&self, asset: &Pubkey) -> Instruction {
        let ata = owner_ata(&self.rewards_mint, &self.payer.pubkey());
        Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::ClaimRewards {}.data(),
            nft_staking::accounts::ClaimRewards {
                owner: self.payer.pubkey(),
                stake_state: self.stake_state(asset),
                asset: *asset,
                config: self.config,
                collection: self.collection,
                rewards_mint: self.rewards_mint,
                owner_rewards_ata: ata,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub fn burn_ix(&self, asset: &Pubkey) -> Instruction {
        let ata = owner_ata(&self.rewards_mint, &self.payer.pubkey());
        Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::BurnStakedNft {}.data(),
            nft_staking::accounts::BurnStakedNft {
                owner: self.payer.pubkey(),
                stake_state: self.stake_state(asset),
                asset: *asset,
                collection: self.collection,
                update_authority: self.update_authority,
                config: self.config,
                rewards_mint: self.rewards_mint,
                owner_rewards_ata: ata,
                mpl_core_program: MPL_CORE_ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub fn owner_rewards_ata(&self) -> Pubkey {
        owner_ata(&self.rewards_mint, &self.payer.pubkey())
    }

    // --- oracle helpers ---

    pub fn init_oracle(&mut self) {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::InitOracle {}.data(),
            nft_staking::accounts::InitOracle {
                payer: self.payer.pubkey(),
                collection: self.collection,
                oracle: self.oracle(),
                vault: self.oracle_vault(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    /// Calls the crank with `crank` as the reward recipient. The fee payer stays
    /// `payer`, so the crank's balance delta is exactly the reward.
    pub fn update_oracle_as(&mut self, crank: &Pubkey) {
        let ix = Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::UpdateOracle {}.data(),
            nft_staking::accounts::UpdateOracle {
                crank: *crank,
                collection: self.collection,
                oracle: self.oracle(),
                vault: self.oracle_vault(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        );
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn update_oracle(&mut self) {
        let crank = self.payer.pubkey();
        self.update_oracle_as(&crank);
    }

    pub fn read_oracle(&self) -> nft_staking::state::Oracle {
        let account = self.svm.get_account(&self.oracle()).expect("oracle exists");
        let mut data: &[u8] = &account.data;
        nft_staking::state::Oracle::try_deserialize(&mut data).unwrap()
    }

    /// mpl-core reads the 5 `OracleValidation::V1` bytes at Anchor offset (8):
    /// [variant, create, transfer, burn, update].
    pub fn oracle_validation_bytes(&self) -> [u8; 5] {
        let account = self.svm.get_account(&self.oracle()).expect("oracle exists");
        account.data[8..13].try_into().unwrap()
    }

    /// Credits the oracle vault so it can pay crank rewards.
    pub fn fund_vault(&mut self, lamports: u64) {
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &self.payer.pubkey(),
            &self.oracle_vault(),
            lamports,
        );
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn transfer_asset_ix(&self, asset: &Pubkey, new_owner: &Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &nft_staking::instruction::TransferAsset {}.data(),
            nft_staking::accounts::TransferAsset {
                owner: self.payer.pubkey(),
                asset: *asset,
                collection: self.collection,
                update_authority: self.update_authority,
                oracle: self.oracle(),
                new_owner: *new_owner,
                mpl_core_program: MPL_CORE_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub fn transfer_asset(&mut self, asset: &Pubkey, new_owner: &Pubkey) {
        let ix = self.transfer_asset_ix(asset, new_owner);
        send_ok(&mut self.svm, &[&self.payer], &[ix]);
    }

    pub fn asset_owner(&self, asset: &Pubkey) -> Pubkey {
        let account = self.svm.get_account(asset).expect("asset account");
        BaseAssetV1::from_bytes(&account.data).unwrap().owner
    }

    pub fn asset_is_burned(&self, asset: &Pubkey) -> bool {
        match self.svm.get_account(asset) {
            None => true,
            // mpl-core clears the discriminator to a single zero byte on burn.
            Some(a) => a.data.is_empty() || a.data == [0],
        }
    }

    /// `frozen` flag of the asset's freeze delegate, or `None` if absent.
    pub fn asset_frozen(&self, asset: &Pubkey) -> Option<bool> {
        let account = self.svm.get_account(asset).expect("asset account");
        let asset = mpl_core::Asset::from_bytes(&account.data).unwrap();
        asset
            .plugin_list
            .freeze_delegate
            .as_ref()
            .map(|d| d.freeze_delegate.frozen)
    }
}
