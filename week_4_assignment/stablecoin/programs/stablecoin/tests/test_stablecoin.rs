use anchor_lang::{
    prelude::Pubkey,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        rent::Rent,
        system_instruction, system_program,
    },
    AccountDeserialize, AnchorDeserialize, Discriminator, InstructionData, ToAccountMetas,
};
use anchor_spl::token_2022::{
    self,
    spl_token_2022::{
        extension::{
            confidential_transfer::{
                instruction as confidential_instruction, ConfidentialTransferAccount,
                ConfidentialTransferMint,
            },
            default_account_state::DefaultAccountState,
            metadata_pointer::MetadataPointer,
            mint_close_authority::MintCloseAuthority,
            permanent_delegate::PermanentDelegate,
            transfer_fee::{TransferFeeAmount, TransferFeeConfig},
            BaseStateWithExtensions, ExtensionType, StateWithExtensions,
        },
        state::{Account as SplTokenAccount, AccountState, Mint as SplMint},
    },
    ID as TOKEN_2022_ID,
};
use anchor_spl::token_2022_extensions::spl_token_metadata_interface::state::TokenMetadata;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use litesvm::{LiteSVM, types::TransactionResult};
use solana_awesome::{
    keypair::{Address, Keypair},
    message::{Message, VersionedMessage},
    signer::Signer,
    transaction::versioned::VersionedTransaction,
};
use solana_zk_elgamal_proof_interface::{
    instruction::{ContextStateInfo, ProofInstruction},
    proof_data::ZkProofData,
    state::ProofContextState,
    ID as PROOF_PROGRAM_ID,
};
use solana_zk_sdk::{
    encryption::{
        auth_encryption::{AeCiphertext, AeKey},
        elgamal::{ElGamalCiphertext, ElGamalKeypair},
    },
    zk_elgamal_proof_program::build_pubkey_validity_proof_data,
};
use spl_token_confidential_transfer_proof_generation::{
    transfer::transfer_split_proof_data, withdraw::withdraw_proof_data,
};
use stablecoin::{accounts, events, instruction, state::ReissueRecord, ExtensionSet, REISSUE_SEED};

const DECIMALS: u8 = 6;
const FEE_BPS: u16 = 30;
const MAXIMUM_FEE: u64 = 1_000_000;
const NAME: &str = "Remittance USD";
const SYMBOL: &str = "rUSD";
const URI: &str = "https://example.com/rusd.json";
const FUNDS: u64 = 10_000_000_000;

/// The extension set of the fee mint, as `InitializeMint` builds it.
fn fee_mint_extensions() -> Vec<ExtensionType> {
    vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::DefaultAccountState,
        ExtensionType::MintCloseAuthority,
    ]
}

struct Setup {
    svm: LiteSVM,
    issuer: Keypair,
    kyc: Keypair,
    closer: Keypair,
    user: Keypair,
    mint: Keypair,
}

#[allow(clippy::result_large_err)]
fn send(svm: &mut LiteSVM, signers: &[&Keypair], ixs: &[Instruction]) -> TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

fn send_ok(
    svm: &mut LiteSVM,
    signers: &[&Keypair],
    ixs: &[Instruction],
) -> litesvm::types::TransactionMetadata {
    match send(svm, signers, ixs) {
        Ok(meta) => meta,
        Err(failed) => panic!("transaction failed:\n{}", failed.meta.pretty_logs()),
    }
}

fn setup() -> Setup {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/stablecoin.so"
    ));
    svm.add_program(stablecoin::id(), bytes).unwrap();

    let issuer = Keypair::new();
    let kyc = Keypair::new();
    let closer = Keypair::new();
    let user = Keypair::new();
    for keypair in [&issuer, &kyc, &closer, &user] {
        svm.airdrop(&keypair.pubkey(), FUNDS).unwrap();
    }

    Setup {
        svm,
        issuer,
        kyc,
        closer,
        user,
        mint: Keypair::new(),
    }
}

fn initialize_mint_ix(setup: &Setup, fee_bps: u16) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::InitializeMint {
            decimals: DECIMALS,
            name: NAME.to_string(),
            symbol: SYMBOL.to_string(),
            uri: URI.to_string(),
            transfer_fee_basis_points: fee_bps,
            maximum_fee: MAXIMUM_FEE,
        }
        .data(),
        accounts::InitializeMint {
            payer: setup.issuer.pubkey(),
            mint: setup.mint.pubkey(),
            mint_authority: setup.issuer.pubkey(),
            freeze_authority: setup.kyc.pubkey(),
            close_authority: setup.closer.pubkey(),
            withdraw_withheld_authority: setup.issuer.pubkey(),
            token_program: TOKEN_2022_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// The fee mint, created through `InitializeMint` (task 1).
fn with_fee_mint() -> Setup {
    let mut setup = setup();
    let ix = initialize_mint_ix(&setup, FEE_BPS);
    send_ok(
        &mut setup.svm,
        &[&setup.issuer, &setup.mint],
        &[ix],
    );
    setup
}

fn create_token_account_ix(
    payer: &Pubkey,
    account: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::CreateTokenAccount {}.data(),
        accounts::CreateTokenAccount {
            payer: *payer,
            account: *account,
            owner: *owner,
            mint: *mint,
            token_program: TOKEN_2022_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn mint_to_ix(
    authority: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::MintTo { amount }.data(),
        accounts::MintTo {
            mint_authority: *authority,
            mint: *mint,
            destination: *destination,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn thaw_ix(freeze_authority: &Pubkey, mint: &Pubkey, account: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ThawAccount {}.data(),
        accounts::ThawAccount {
            freeze_authority: *freeze_authority,
            mint: *mint,
            account: *account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn freeze_ix(freeze_authority: &Pubkey, mint: &Pubkey, account: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::FreezeAccount {}.data(),
        accounts::FreezeAccount {
            freeze_authority: *freeze_authority,
            mint: *mint,
            account: *account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn transfer_with_fee_ix(
    authority: &Pubkey,
    mint: &Pubkey,
    source: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::TransferWithFee { amount }.data(),
        accounts::TransferWithFee {
            authority: *authority,
            mint: *mint,
            source: *source,
            destination: *destination,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn collect_fees_ix(
    withdraw_withheld_authority: &Pubkey,
    mint: &Pubkey,
    destination: &Pubkey,
    sources: &[Pubkey],
) -> Instruction {
    let mut metas = accounts::CollectFees {
        withdraw_withheld_authority: *withdraw_withheld_authority,
        mint: *mint,
        destination: *destination,
        token_program: TOKEN_2022_ID,
    }
    .to_account_metas(None);
    metas.extend(
        sources
            .iter()
            .map(|source| AccountMeta::new(*source, false)),
    );
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::CollectFees {}.data(),
        metas,
    )
}

fn close_mint_ix(close_authority: &Pubkey, mint: &Pubkey, destination: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::CloseMint {}.data(),
        accounts::CloseMint {
            close_authority: *close_authority,
            mint: *mint,
            destination: *destination,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn reissue_ix(
    setup: &Setup,
    previous_mint: &Pubkey,
    new_mint: &Pubkey,
    permanent_delegate: &Pubkey,
    confidential_authority: &Pubkey,
) -> Instruction {
    let record = Pubkey::find_program_address(
        &[REISSUE_SEED, previous_mint.as_ref()],
        &stablecoin::id(),
    )
    .0;
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ReissueMint {}.data(),
        accounts::ReissueMint {
            issuer: setup.issuer.pubkey(),
            previous_mint: *previous_mint,
            new_mint: *new_mint,
            freeze_authority: setup.kyc.pubkey(),
            close_authority: setup.closer.pubkey(),
            permanent_delegate: *permanent_delegate,
            confidential_authority: *confidential_authority,
            reissue_record: record,
            token_program: TOKEN_2022_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn seize_ix(
    authority: &Pubkey,
    mint: &Pubkey,
    source: &Pubkey,
    destination: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::Seize { amount }.data(),
        accounts::Seize {
            authority: *authority,
            mint: *mint,
            source: *source,
            destination: *destination,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn record_address(previous_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[REISSUE_SEED, previous_mint.as_ref()], &stablecoin::id()).0
}

fn reissue_setup() -> (Setup, Keypair, Pubkey) {
    let mut setup = with_fee_mint();
    let confidential_mint = Keypair::new();
    let ix = reissue_ix(
        &setup,
        &setup.mint.pubkey(),
        &confidential_mint.pubkey(),
        &setup.issuer.pubkey(),
        &setup.issuer.pubkey(),
    );
    send_ok(
        &mut setup.svm,
        &[&setup.issuer, &confidential_mint],
        &[ix],
    );
    let record = record_address(&setup.mint.pubkey());
    (setup, confidential_mint, record)
}

// ---- state readers ----

fn token_account(svm: &LiteSVM, address: &Address) -> SplTokenAccount {
    let account = svm.get_account(address).expect("token account exists");
    StateWithExtensions::<SplTokenAccount>::unpack(&account.data)
        .expect("token account unpacks")
        .base
}

fn amount(svm: &LiteSVM, address: &Address) -> u64 {
    token_account(svm, address).amount
}

fn withheld(svm: &LiteSVM, address: &Address) -> u64 {
    let account = svm.get_account(address).expect("token account exists");
    let state = StateWithExtensions::<SplTokenAccount>::unpack(&account.data).unwrap();
    state
        .get_extension::<TransferFeeAmount>()
        .map(|extension| u64::from(extension.withheld_amount))
        .unwrap_or_default()
}

/// Which extensions a mint actually carries, read through `StateWithExtensions`.
fn extension_set(svm: &LiteSVM, address: &Address) -> ExtensionSet {
    let account = svm.get_account(address).expect("mint exists");
    let mint = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    ExtensionSet {
        transfer_fee_config: mint.get_extension::<TransferFeeConfig>().is_ok(),
        metadata_pointer: mint
            .get_extension::<MetadataPointer>()
            .map(|pointer| {
                Option::<Pubkey>::from(pointer.metadata_address) == Some(*address)
            })
            .unwrap_or(false),
        default_account_state: mint
            .get_extension::<DefaultAccountState>()
            .map(|state| {
                AccountState::try_from(state.state).unwrap() == AccountState::Frozen
            })
            .unwrap_or(false),
        mint_close_authority: mint
            .get_extension::<MintCloseAuthority>()
            .map(|extension| Option::<Pubkey>::from(extension.close_authority).is_some())
            .unwrap_or(false),
        permanent_delegate: mint
            .get_extension::<PermanentDelegate>()
            .map(|extension| Option::<Pubkey>::from(extension.delegate).is_some())
            .unwrap_or(false),
        confidential_transfer_mint: mint
            .get_extension::<ConfidentialTransferMint>()
            .is_ok(),
    }
}

fn metadata_of(svm: &LiteSVM, address: &Address) -> TokenMetadata {
    let account = svm.get_account(address).expect("mint exists");
    let mint = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    mint.get_variable_len_extension::<TokenMetadata>().unwrap()
}

fn parse_event<T>(meta: &litesvm::types::TransactionMetadata) -> T
where
    T: AnchorDeserialize + Discriminator,
{
    let discriminator: &[u8] = T::DISCRIMINATOR;
    for log in meta.logs.iter() {
        if let Some(b64) = log.strip_prefix("Program data: ") {
            let bytes = B64.decode(b64.trim()).expect("valid base64 event");
            if bytes.len() >= 8 && bytes[..8] == *discriminator {
                return T::try_from_slice(&bytes[8..]).expect("event decodes");
            }
        }
    }
    panic!("event not found in logs");
}

fn assert_failed_with(res: &TransactionResult, needle: &str) {
    match res {
        Ok(meta) => panic!(
            "expected failure, transaction succeeded:\n{}",
            meta.pretty_logs()
        ),
        Err(failed) => {
            let logs = failed.meta.logs.join("\n");
            assert!(
                logs.contains(needle),
                "expected `{needle}` in program logs:\n{logs}"
            );
        }
    }
}

/// Opens a token account for `owner` (anyone may pay) and clears its KYC.
fn open_account(
    svm: &mut LiteSVM,
    payer: &Keypair,
    kyc: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Address {
    let account = Keypair::new();
    let ix = create_token_account_ix(&payer.pubkey(), &account.pubkey(), owner, mint);
    send_ok(svm, &[payer, &account], &[ix]);
    let ix = thaw_ix(&kyc.pubkey(), mint, &account.pubkey());
    send_ok(svm, &[kyc], &[ix]);
    account.pubkey()
}

/// A thawed account holding `funds` tokens.
fn funded_account(
    svm: &mut LiteSVM,
    issuer: &Keypair,
    kyc: &Keypair,
    mint: &Pubkey,
    owner: &Pubkey,
    funds: u64,
) -> Address {
    let account = open_account(svm, issuer, kyc, mint, owner);
    if funds > 0 {
        let ix = mint_to_ix(&issuer.pubkey(), mint, &account, funds);
        send_ok(svm, &[issuer], &[ix]);
    }
    account
}

// ---- task 1 ----

#[test]
fn initialize_mint_stacks_expected_extensions() {
    let mut setup = setup();
    let ix = initialize_mint_ix(&setup, FEE_BPS);
    let res = send_ok(
        &mut setup.svm,
        &[&setup.issuer, &setup.mint],
        &[ix],
    );
    let mint_address = setup.mint.pubkey();
    let account = setup.svm.get_account(&mint_address).unwrap();

    assert_eq!(account.owner, TOKEN_2022_ID);
    let mint = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    assert_eq!(mint.base.decimals, DECIMALS);
    assert_eq!(mint.base.supply, 0);
    assert_eq!(
        Option::<Pubkey>::from(mint.base.mint_authority),
        Some(setup.issuer.pubkey())
    );
    assert_eq!(
        Option::<Pubkey>::from(mint.base.freeze_authority),
        Some(setup.kyc.pubkey())
    );
    assert!(mint.base.is_initialized);

    // `ExtensionType::try_calculate_account_len` sized the account, and the
    // token metadata appended after InitializeMint grew it further.
    let extension_len =
        ExtensionType::try_calculate_account_len::<SplMint>(&fee_mint_extensions()).unwrap();
    assert!(account.data.len() > extension_len);

    // TransferFeeConfig: the protocol-level fee on every transfer.
    let fee = mint.get_extension::<TransferFeeConfig>().unwrap();
    assert_eq!(
        u16::from(fee.newer_transfer_fee.transfer_fee_basis_points),
        FEE_BPS
    );
    assert_eq!(u64::from(fee.newer_transfer_fee.maximum_fee), MAXIMUM_FEE);
    assert_eq!(
        Option::<Pubkey>::from(fee.transfer_fee_config_authority),
        Some(setup.issuer.pubkey())
    );
    assert_eq!(
        Option::<Pubkey>::from(fee.withdraw_withheld_authority),
        Some(setup.issuer.pubkey())
    );

    // MetadataPointer, pointed at the mint itself and immutable.
    let pointer = mint.get_extension::<MetadataPointer>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(pointer.metadata_address),
        Some(mint_address)
    );
    assert_eq!(Option::<Pubkey>::from(pointer.authority), None);

    // DefaultAccountState: frozen until KYC clears.
    let default_state = mint.get_extension::<DefaultAccountState>().unwrap();
    assert_eq!(
        AccountState::try_from(default_state.state).unwrap(),
        AccountState::Frozen
    );

    // MintCloseAuthority: decommissioning.
    let close = mint.get_extension::<MintCloseAuthority>().unwrap();
    assert_eq!(
        Option::<Pubkey>::from(close.close_authority),
        Some(setup.closer.pubkey())
    );

    // Only the four extensions of the fee mint.
    assert_eq!(extension_set(&setup.svm, &mint_address), ExtensionSet::FEE_MINT);
    assert!(mint.get_extension::<PermanentDelegate>().is_err());

    // Metadata lives on the mint itself, readable without an off-chain registry.
    let metadata = mint.get_variable_len_extension::<TokenMetadata>().unwrap();
    assert_eq!(metadata.name, NAME);
    assert_eq!(metadata.symbol, SYMBOL);
    assert_eq!(metadata.uri, URI);
    assert_eq!(metadata.mint, mint_address);

    // The event reports the same extension set.
    let event: events::MintInitialized = parse_event(&res);
    assert_eq!(event.mint, mint_address);
    assert_eq!(event.extensions, ExtensionSet::FEE_MINT);
    assert_eq!(event.transfer_fee_basis_points, FEE_BPS);
    assert_eq!(event.maximum_fee, MAXIMUM_FEE);
    assert_eq!(event.freeze_authority, setup.kyc.pubkey());
}

#[test]
fn initialize_mint_rejects_zero_fee() {
    let mut setup = setup();
    let ix = initialize_mint_ix(&setup, 0);
    let res = send(&mut setup.svm, &[&setup.issuer, &setup.mint], &[ix]);
    assert_failed_with(&res, "InvalidTransferFee");
}

// ---- task 4: frozen by default, thaw per account ----

#[test]
fn new_accounts_start_frozen_and_thaw_only_affects_that_account() {
    let mut setup = setup();
    let ix = initialize_mint_ix(&setup, FEE_BPS);
    send_ok(
        &mut setup.svm,
        &[&setup.issuer, &setup.mint],
        &[ix],
    );

    let mint = setup.mint.pubkey();
    let account = Keypair::new();
    let ix = create_token_account_ix(
        &setup.issuer.pubkey(),
        &account.pubkey(),
        &setup.user.pubkey(),
        &mint,
    );
    let res = send_ok(&mut setup.svm, &[&setup.issuer, &account], &[ix]);
    let event: events::TokenAccountCreated = parse_event(&res);
    assert!(event.frozen);

    // New accounts inherit the mint's frozen default.
    assert_eq!(
        token_account(&setup.svm, &account.pubkey()).state,
        AccountState::Frozen
    );

    // A frozen account cannot move anything.
    let destination = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.issuer.pubkey(),
    );
    let ix = transfer_with_fee_ix(
        &setup.user.pubkey(),
        &mint,
        &account.pubkey(),
        &destination,
        1,
    );
    let res = send(&mut setup.svm, &[&setup.user], &[ix]);
    assert_failed_with(&res, "AccountFrozen");

    // KYC clears: the freeze authority thaws this one account.
    let ix = thaw_ix(&setup.kyc.pubkey(), &mint, &account.pubkey());
    let res = send_ok(&mut setup.svm, &[&setup.kyc], &[ix]);
    let event: events::AccountThawed = parse_event(&res);
    assert_eq!(event.account, account.pubkey());
    assert_eq!(
        token_account(&setup.svm, &account.pubkey()).state,
        AccountState::Initialized
    );

    // ... and the mint-level default state is untouched: the account opened
    // afterwards still starts frozen.
    let account = setup.svm.get_account(&mint).unwrap();
    let mint_state = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    assert_eq!(
        AccountState::try_from(
            mint_state
                .get_extension::<DefaultAccountState>()
                .unwrap()
                .state
        )
        .unwrap(),
        AccountState::Frozen
    );
    let second = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.user.pubkey(),
    );
    assert_eq!(
        token_account(&setup.svm, &second).state,
        AccountState::Initialized
    );
}

// ---- task 2: the fee on every transfer ----

#[test]
fn transfer_with_fee_charges_the_epoch_fee() {
    let mut setup = with_fee_mint();
    let mint = setup.mint.pubkey();
    let user_account = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.user.pubkey(),
        1_000_000,
    );
    let treasury = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.issuer.pubkey(),
    );

    // The fee the mint owes for this epoch, straight from the extension.
    let account = setup.svm.get_account(&mint).unwrap();
    let mint_state = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    let epoch = 0;
    let expected_fee = mint_state
        .get_extension::<TransferFeeConfig>()
        .unwrap()
        .calculate_epoch_fee(epoch, 250_000)
        .unwrap();
    assert_eq!(expected_fee, 750); // 30 bps of 250,000

    let ix = transfer_with_fee_ix(
        &setup.user.pubkey(),
        &mint,
        &user_account,
        &treasury,
        250_000,
    );
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);

    assert_eq!(amount(&setup.svm, &user_account), 750_000);
    assert_eq!(amount(&setup.svm, &treasury), 250_000 - expected_fee);
    assert_eq!(withheld(&setup.svm, &treasury), expected_fee);

    let event: events::TokensTransferred = parse_event(&res);
    assert_eq!(event.mint, mint);
    assert_eq!(event.source, user_account);
    assert_eq!(event.destination, treasury);
    assert_eq!(event.authority, setup.user.pubkey());
    assert_eq!(event.amount, 250_000);
    assert_eq!(event.fee, expected_fee);
    assert_eq!(event.epoch, epoch);
}

#[test]
fn collect_fees_sweeps_withheld_fees_to_the_issuer() {
    let mut setup = with_fee_mint();
    let mint = setup.mint.pubkey();
    let user_account = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.user.pubkey(),
        1_000_000,
    );
    let treasury = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.issuer.pubkey(),
    );

    let ix = transfer_with_fee_ix(
        &setup.user.pubkey(),
        &mint,
        &user_account,
        &treasury,
        250_000,
    );
    send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let fee = withheld(&setup.svm, &treasury);

    let before = amount(&setup.svm, &treasury);
    let ix = collect_fees_ix(&setup.issuer.pubkey(), &mint, &treasury, &[treasury]);
    let res = send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);

    assert_eq!(amount(&setup.svm, &treasury), before + fee);
    assert_eq!(withheld(&setup.svm, &treasury), 0);

    let event: events::WithheldFeesCollected = parse_event(&res);
    assert_eq!(event.authority, setup.issuer.pubkey());
    assert_eq!(event.destination, treasury);
    assert_eq!(event.sources, 1);
}

// ---- mint close authority ----

#[test]
fn close_mint_returns_rent_when_supply_is_zero() {
    let mut setup = with_fee_mint();
    let destination = Keypair::new();
    setup
        .svm
        .airdrop(&destination.pubkey(), 1_000_000_000)
        .unwrap();
    let before = setup.svm.get_balance(&destination.pubkey()).unwrap();

    let ix = close_mint_ix(&setup.closer.pubkey(), &setup.mint.pubkey(), &destination.pubkey());
    let res = send_ok(&mut setup.svm, &[&setup.closer], &[ix]);

    assert!(setup.svm.get_account(&setup.mint.pubkey()).is_none());
    assert!(setup.svm.get_balance(&destination.pubkey()).unwrap() > before);

    let event: events::MintClosed = parse_event(&res);
    assert_eq!(event.mint, setup.mint.pubkey());
    assert_eq!(event.close_authority, setup.closer.pubkey());
    assert!(event.lamports > 0);
}

#[test]
fn close_mint_rejects_mint_with_supply() {
    let mut setup = with_fee_mint();
    let mint = setup.mint.pubkey();
    funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.user.pubkey(),
        1_000,
    );

    let destination = Keypair::new();
    let ix = close_mint_ix(&setup.closer.pubkey(), &mint, &destination.pubkey());
    let res = send(&mut setup.svm, &[&setup.closer], &[ix]);
    assert_failed_with(&res, "MintHasSupply");
}

// ---- task 5: re-issue ----

#[test]
fn reissue_carries_forward_the_extension_set_and_drops_the_fee() {
    let (setup, confidential_mint, record) = reissue_setup();
    let previous_mint = setup.mint.pubkey();
    let new_mint = confidential_mint.pubkey();

    // The on-chain record of what moved and what did not.
    let account = setup.svm.get_account(&record).expect("record exists");
    let record = ReissueRecord::try_deserialize(&mut &account.data[..]).unwrap();
    assert_eq!(record.previous_mint, previous_mint);
    assert_eq!(record.new_mint, new_mint);
    assert_eq!(record.previous, ExtensionSet::FEE_MINT);
    assert_eq!(record.new, ExtensionSet::CONFIDENTIAL_MINT);
    assert_eq!(
        record.carried_forward,
        ExtensionSet::CONFIDENTIAL_MINT.carried_forward()
    );
    assert_eq!(record.added, ExtensionSet::CONFIDENTIAL_MINT.added());
    assert_eq!(record.dropped, ExtensionSet::FEE_MINT.dropped());
    assert!(record.carried_forward.metadata_pointer);
    assert!(record.carried_forward.default_account_state);
    assert!(record.carried_forward.mint_close_authority);
    assert!(record.added.permanent_delegate);
    assert!(record.added.confidential_transfer_mint);
    assert!(record.dropped.transfer_fee_config);

    // The re-issued mint carries exactly that set.
    assert_eq!(
        extension_set(&setup.svm, &new_mint),
        ExtensionSet::CONFIDENTIAL_MINT
    );
    let account = setup.svm.get_account(&new_mint).unwrap();
    let mint_with_extensions = StateWithExtensions::<SplMint>::unpack(&account.data).unwrap();
    // No transfer fee on the re-issue.
    assert!(mint_with_extensions
        .get_extension::<TransferFeeConfig>()
        .is_err());
    // Same decimals, issued to the issuer, frozen by default, closable.
    assert_eq!(mint_with_extensions.base.decimals, DECIMALS);
    assert_eq!(
        Option::<Pubkey>::from(mint_with_extensions.base.mint_authority),
        Some(setup.issuer.pubkey())
    );
    assert_eq!(
        Option::<Pubkey>::from(mint_with_extensions.base.freeze_authority),
        Some(setup.kyc.pubkey())
    );
    let delegate = mint_with_extensions
        .get_extension::<PermanentDelegate>()
        .unwrap();
    assert_eq!(
        Option::<Pubkey>::from(delegate.delegate),
        Some(setup.issuer.pubkey())
    );
    let confidential = mint_with_extensions
        .get_extension::<ConfidentialTransferMint>()
        .unwrap();
    assert_eq!(
        Option::<Pubkey>::from(confidential.authority),
        Some(setup.issuer.pubkey())
    );
    // approve_policy = manual.
    assert!(!bool::from(confidential.auto_approve_new_accounts));
    assert_eq!(
        AccountState::try_from(
            mint_with_extensions
                .get_extension::<DefaultAccountState>()
                .unwrap()
                .state
        )
        .unwrap(),
        AccountState::Frozen
    );
    assert_eq!(
        Option::<Pubkey>::from(
            mint_with_extensions
                .get_extension::<MetadataPointer>()
                .unwrap()
                .metadata_address
        ),
        Some(new_mint)
    );

    // On-chain metadata travelled with the extension set.
    let metadata = metadata_of(&setup.svm, &new_mint);
    assert_eq!(metadata.name, NAME);
    assert_eq!(metadata.symbol, SYMBOL);
    assert_eq!(metadata.uri, URI);
    assert_eq!(metadata.mint, new_mint);
}

#[test]
fn reissue_rejects_a_mint_without_the_fee_extension_set() {
    let (setup, confidential_mint, _) = reissue_setup();
    // The fresh confidential mint is not the fee mint: re-issuing from it must
    // be refused rather than silently producing a second mint.
    let second = Keypair::new();
    let ix = reissue_ix(
        &setup,
        &confidential_mint.pubkey(),
        &second.pubkey(),
        &setup.issuer.pubkey(),
        &setup.issuer.pubkey(),
    );
    let mut svm = setup.svm;
    let res = send(&mut svm, &[&setup.issuer, &second], &[ix]);
    assert_failed_with(&res, "UnexpectedExtensionSet");
}

#[test]
fn token_2022_refuses_transfer_fee_plus_confidential_transfers() {
    // The gap, demonstrated against token-2022 itself: a mint cannot stack
    // `TransferFeeConfig` and `ConfidentialTransferMint` without the
    // `ConfidentialTransferFeeConfig` extension, so the fee mint cannot simply
    // gain confidentiality.
    let mut setup = setup();
    let bearer = setup.mint.pubkey();

    let extensions = vec![
        ExtensionType::TransferFeeConfig,
        ExtensionType::ConfidentialTransferMint,
    ];
    let len = ExtensionType::try_calculate_account_len::<SplMint>(&extensions).unwrap();
    let create = system_instruction::create_account(
        &setup.issuer.pubkey(),
        &bearer,
        Rent::default().minimum_balance(len),
        len as u64,
        &TOKEN_2022_ID,
    );
    let fee = token_2022::spl_token_2022::extension::transfer_fee::instruction::initialize_transfer_fee_config(
        &TOKEN_2022_ID,
        &bearer,
        None,
        None,
        FEE_BPS,
        MAXIMUM_FEE,
    )
    .unwrap();
    let confidential = confidential_instruction::initialize_mint(
        &TOKEN_2022_ID,
        &bearer,
        Some(setup.issuer.pubkey()),
        false,
        None,
    )
    .unwrap();
    let initialize = token_2022::spl_token_2022::instruction::initialize_mint2(
        &TOKEN_2022_ID,
        &bearer,
        &setup.issuer.pubkey(),
        None,
        DECIMALS,
    )
    .unwrap();

    let res = send(
        &mut setup.svm,
        &[&setup.issuer, &setup.mint],
        &[create, fee, confidential, initialize],
    );
    assert_failed_with(&res, "invalid combination of extensions");
}

// ---- seizure ----

#[test]
fn seize_moves_funds_without_the_owner_and_refuses_frozen_accounts() {
    let (mut setup, confidential_mint, _) = reissue_setup();
    let mint = confidential_mint.pubkey();

    let sanctioned = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.user.pubkey(),
        500_000,
    );
    let treasury = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &setup.issuer.pubkey(),
    );

    // The permanent delegate signs; the account owner does not.
    let ix = seize_ix(
        &setup.issuer.pubkey(),
        &mint,
        &sanctioned,
        &treasury,
        200_000,
    );
    let res = send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);
    assert_eq!(amount(&setup.svm, &sanctioned), 300_000);
    assert_eq!(amount(&setup.svm, &treasury), 200_000);

    let event: events::FundsSeized = parse_event(&res);
    assert_eq!(event.permanent_delegate, setup.issuer.pubkey());
    assert_eq!(event.amount, 200_000);

    // token-2022 rejects a frozen source for *every* transfer, the delegate
    // included: a sanctioned account is frozen after the seizure.
    let ix = freeze_ix(&setup.kyc.pubkey(), &mint, &sanctioned);
    send_ok(&mut setup.svm, &[&setup.kyc], &[ix]);
    let ix = seize_ix(
        &setup.issuer.pubkey(),
        &mint,
        &sanctioned,
        &treasury,
        100_000,
    );
    let res = send(&mut setup.svm, &[&setup.issuer], &[ix]);
    assert_failed_with(&res, "AccountFrozen");
}

// ---- task 6: the confidential lifecycle ----
//
// A confidential account needs a zero-knowledge proof of the AES key that
// encrypts its balance (`PubkeyValidity`), and every balance-moving instruction
// needs proofs: equality + range for a withdrawal, equality + ciphertext
// validity + range for a transfer. Each proof is verified by the ZK ElGamal
// proof program into its own context state account, which the instruction then
// references by address.

const MAXIMUM_PENDING_CREDITS: u64 = 65_536;

/// Creates an account owned by the ZK ElGamal proof program and verifies
/// `proof_data` into it. Returns the context state account.
fn verify_proof<T, U>(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: ProofInstruction,
    proof_data: &T,
) -> Address
where
    T: bytemuck::Pod + ZkProofData<U>,
    U: bytemuck::Pod,
{
    let context_state = Keypair::new();
    let len = std::mem::size_of::<ProofContextState<U>>();
    let create = system_instruction::create_account(
        &payer.pubkey(),
        &context_state.pubkey(),
        Rent::default().minimum_balance(len),
        len as u64,
        &PROOF_PROGRAM_ID,
    );
    let verify = instruction.encode_verify_proof::<T, U>(
        Some(ContextStateInfo {
            context_state_account: &context_state.pubkey(),
            context_state_authority: &payer.pubkey(),
        }),
        proof_data,
    );
    send_ok(svm, &[payer, &context_state], &[create, verify]);
    context_state.pubkey()
}

fn configure_confidential_account_ix(
    owner: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
    proof_account: &Pubkey,
    decryptable_zero_balance: [u8; 36],
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ConfigureConfidentialAccount {
            decryptable_zero_balance,
            maximum_pending_balance_credit_counter: MAXIMUM_PENDING_CREDITS,
        }
        .data(),
        accounts::ConfigureConfidentialAccount {
            owner: *owner,
            mint: *mint,
            account: *account,
            proof_account: *proof_account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn approve_confidential_account_ix(
    authority: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ApproveConfidentialAccount {}.data(),
        accounts::ApproveConfidentialAccount {
            authority: *authority,
            mint: *mint,
            account: *account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn deposit_confidential_ix(
    owner: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::DepositConfidential { amount }.data(),
        accounts::DepositConfidential {
            owner: *owner,
            mint: *mint,
            account: *account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

fn apply_pending_balance_ix(
    owner: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
    expected_pending_balance_credit_counter: u64,
    new_decryptable_available_balance: [u8; 36],
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ApplyPendingBalance {
            expected_pending_balance_credit_counter,
            new_decryptable_available_balance,
        }
        .data(),
        accounts::ApplyPendingBalance {
            owner: *owner,
            mint: *mint,
            account: *account,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

#[allow(clippy::too_many_arguments)]
fn confidential_transfer_ix(
    owner: &Pubkey,
    mint: &Pubkey,
    source: &Pubkey,
    destination: &Pubkey,
    new_source_decryptable_available_balance: [u8; 36],
    auditor_ciphertext_lo: [u8; 64],
    auditor_ciphertext_hi: [u8; 64],
    equality_proof: &Pubkey,
    ciphertext_validity_proof: &Pubkey,
    range_proof: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::ConfidentialTransfer {
            new_source_decryptable_available_balance,
            transfer_amount_auditor_ciphertext_lo: auditor_ciphertext_lo,
            transfer_amount_auditor_ciphertext_hi: auditor_ciphertext_hi,
        }
        .data(),
        accounts::ConfidentialTransfer {
            owner: *owner,
            mint: *mint,
            source: *source,
            destination: *destination,
            equality_proof: *equality_proof,
            ciphertext_validity_proof: *ciphertext_validity_proof,
            range_proof: *range_proof,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

#[allow(clippy::too_many_arguments)]
fn withdraw_confidential_ix(
    owner: &Pubkey,
    mint: &Pubkey,
    account: &Pubkey,
    amount: u64,
    expected_pending_balance_credit_counter: u64,
    decryptable_available_balance: [u8; 36],
    new_decryptable_available_balance: [u8; 36],
    equality_proof: &Pubkey,
    range_proof: &Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin::id(),
        &instruction::WithdrawConfidential {
            amount,
            expected_pending_balance_credit_counter,
            decryptable_available_balance,
            new_decryptable_available_balance,
        }
        .data(),
        accounts::WithdrawConfidential {
            owner: *owner,
            mint: *mint,
            account: *account,
            equality_proof: *equality_proof,
            range_proof: *range_proof,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    )
}

// ---- confidential state readers ----

fn confidential_extension(svm: &LiteSVM, address: &Address) -> ConfidentialTransferAccount {
    let account = svm.get_account(address).expect("token account exists");
    let state = StateWithExtensions::<SplTokenAccount>::unpack(&account.data).unwrap();
    *state
        .get_extension::<ConfidentialTransferAccount>()
        .expect("account is configured for confidential transfers")
}

fn ciphertext(bytes: &[u8]) -> ElGamalCiphertext {
    ElGamalCiphertext::from_bytes(bytes).expect("valid ElGamal ciphertext")
}

/// The available confidential balance, decrypted with the owner's ElGamal key.
fn available_balance(svm: &LiteSVM, address: &Address, keypair: &ElGamalKeypair) -> u64 {
    let extension = confidential_extension(svm, address);
    ciphertext(bytemuck::bytes_of(&extension.available_balance))
        .decrypt_u32(keypair.secret())
        .expect("available balance decrypts")
}

/// The pending confidential balance: the low 16 bits plus the high bits, both
/// ElGamal-encrypted under the owner's key.
fn pending_balance(svm: &LiteSVM, address: &Address, keypair: &ElGamalKeypair) -> u64 {
    let extension = confidential_extension(svm, address);
    let lo = ciphertext(bytemuck::bytes_of(&extension.pending_balance_lo))
        .decrypt_u32(keypair.secret())
        .unwrap_or_default();
    let hi = ciphertext(bytemuck::bytes_of(&extension.pending_balance_hi))
        .decrypt_u32(keypair.secret())
        .unwrap_or_default();
    lo + (hi << 16)
}

/// The available balance as a ciphertext *including* whatever is pending, the
/// way a client computes the balance a withdrawal will spend from.
fn available_with_pending(svm: &LiteSVM, address: &Address) -> ElGamalCiphertext {
    let extension = confidential_extension(svm, address);
    let available = ciphertext(bytemuck::bytes_of(&extension.available_balance));
    let lo = ciphertext(bytemuck::bytes_of(&extension.pending_balance_lo));
    let hi = ciphertext(bytemuck::bytes_of(&extension.pending_balance_hi));
    let pending = spl_token_confidential_transfer_proof_generation::try_combine_lo_hi_ciphertexts(
        &lo, &hi, 16,
    )
    .expect("pending ciphertexts combine");
    available + pending
}

fn decryptable_balance(svm: &LiteSVM, address: &Address) -> AeCiphertext {
    let extension = confidential_extension(svm, address);
    AeCiphertext::from_bytes(bytemuck::bytes_of(&extension.decryptable_available_balance))
        .expect("valid decryptable balance")
}

fn pending_credit_counter(svm: &LiteSVM, address: &Address) -> u64 {
    confidential_extension(svm, address)
        .pending_balance_credit_counter
        .into()
}

#[test]
fn configure_confidential_account_is_owner_only() {
    let (mut setup, confidential_mint, _) = reissue_setup();
    let mint = confidential_mint.pubkey();
    let owner = setup.user.pubkey();
    let account = open_account(&mut setup.svm, &setup.issuer, &setup.kyc, &mint, &owner);

    // Anyone may open the account, and anyone may pay for it — but the owner has
    // to be the one that configures it.
    let attacker = Keypair::new();
    setup
        .svm
        .airdrop(&attacker.pubkey(), FUNDS)
        .unwrap();
    let keypair = ElGamalKeypair::new_rand();
    let proof = build_pubkey_validity_proof_data(&keypair).unwrap();
    let proof_account = verify_proof(
        &mut setup.svm,
        &setup.issuer,
        ProofInstruction::VerifyPubkeyValidity,
        &proof,
    );

    let keys = AeKey::new_rand();
    let ix = configure_confidential_account_ix(
        &attacker.pubkey(),
        &mint,
        &account,
        &proof_account,
        keys.encrypt(0).to_bytes(),
    );
    let res = send(&mut setup.svm, &[&attacker], &[ix]);
    assert_failed_with(&res, "NotAccountOwner");

    // The owner of the account can.
    let proof = build_pubkey_validity_proof_data(&keypair).unwrap();
    let proof_account = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyPubkeyValidity,
        &proof,
    );
    let ix = configure_confidential_account_ix(
        &owner,
        &mint,
        &account,
        &proof_account,
        keys.encrypt(0).to_bytes(),
    );
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let event: events::ConfidentialAccountConfigured = parse_event(&res);
    assert_eq!(event.account, account);
    assert_eq!(
        event.maximum_pending_balance_credit_counter,
        MAXIMUM_PENDING_CREDITS
    );

    // Manual approval: the account exists but is not approved yet.
    let extension = confidential_extension(&setup.svm, &account);
    assert!(!bool::from(extension.approved));

    // The issuer approves it.
    let ix = approve_confidential_account_ix(&setup.issuer.pubkey(), &mint, &account);
    let res = send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);
    let event: events::ConfidentialAccountApproved = parse_event(&res);
    assert_eq!(event.authority, setup.issuer.pubkey());
    assert!(bool::from(confidential_extension(&setup.svm, &account).approved));
}

#[test]
fn manual_approval_gates_confidential_deposits() {
    let (mut setup, confidential_mint, _) = reissue_setup();
    let mint = confidential_mint.pubkey();
    let owner = setup.user.pubkey();
    let account = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &owner,
        500_000,
    );

    let keypair = ElGamalKeypair::new_rand();
    let keys = AeKey::new_rand();
    let proof = build_pubkey_validity_proof_data(&keypair).unwrap();
    let proof_account = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyPubkeyValidity,
        &proof,
    );
    let ix = configure_confidential_account_ix(
        &owner,
        &mint,
        &account,
        &proof_account,
        keys.encrypt(0).to_bytes(),
    );
    send_ok(&mut setup.svm, &[&setup.user], &[ix]);

    // approve_policy = manual: a deposit before approval is refused.
    let ix = deposit_confidential_ix(&owner, &mint, &account, 500_000);
    let res = send(&mut setup.svm, &[&setup.user], &[ix]);
    assert_failed_with(&res, "ConfidentialAccountNotApproved");

    let ix = approve_confidential_account_ix(&setup.issuer.pubkey(), &mint, &account);
    send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);

    let ix = deposit_confidential_ix(&owner, &mint, &account, 500_000);
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let event: events::ConfidentialTokensDeposited = parse_event(&res);
    assert_eq!(event.amount, 500_000);

    // The public balance moved into the pending confidential balance.
    assert_eq!(amount(&setup.svm, &account), 0);
    assert_eq!(pending_balance(&setup.svm, &account, &keypair), 500_000);
    assert_eq!(available_balance(&setup.svm, &account, &keypair), 0);
    assert_eq!(pending_credit_counter(&setup.svm, &account), 1);
}

#[test]
fn confidential_lifecycle_end_to_end() {
    let (mut setup, confidential_mint, _) = reissue_setup();
    let mint = confidential_mint.pubkey();

    // Alice and Bob, each with an AES key and an ElGamal key pair.
    let alice = setup.user.pubkey();
    let alice_elgamal = ElGamalKeypair::new_rand();
    let alice_keys = AeKey::new_rand();

    let bob = Keypair::new();
    setup.svm.airdrop(&bob.pubkey(), FUNDS).unwrap();
    let bob_elgamal = ElGamalKeypair::new_rand();
    let bob_keys = AeKey::new_rand();

    // Public balances to move into the confidential world.
    let alice_account = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &alice,
        1_000_000,
    );
    let bob_account = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &bob.pubkey(),
    );

    // 1. ConfigureAccount for both — owner-only, each with a proof of their own
    //    AES key, followed by the issuer's manual approval.
    for (keypair, keys, account, owner) in [
        (&alice_elgamal, &alice_keys, alice_account, &setup.user),
        (&bob_elgamal, &bob_keys, bob_account, &bob),
    ] {
        let proof = build_pubkey_validity_proof_data(keypair).unwrap();
        let proof_account = verify_proof(
            &mut setup.svm,
            owner,
            ProofInstruction::VerifyPubkeyValidity,
            &proof,
        );
        let ix = configure_confidential_account_ix(
            &owner.pubkey(),
            &mint,
            &account,
            &proof_account,
            keys.encrypt(0).to_bytes(),
        );
        send_ok(&mut setup.svm, &[owner], &[ix]);

        let ix = approve_confidential_account_ix(&setup.issuer.pubkey(), &mint, &account);
        send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);
    }

    // 2. DepositConfidentialTokens: the public balance becomes a pending
    //    confidential balance, invisible on-chain.
    let ix = deposit_confidential_ix(&alice, &mint, &alice_account, 1_000_000);
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    assert!(parse_event::<events::ConfidentialTokensDeposited>(&res).amount == 1_000_000);
    assert_eq!(amount(&setup.svm, &alice_account), 0);
    assert_eq!(
        pending_balance(&setup.svm, &alice_account, &alice_elgamal),
        1_000_000
    );

    // 3. ApplyPendingBalance: pending -> available, spendable balance.
    let ix = apply_pending_balance_ix(
        &alice,
        &mint,
        &alice_account,
        pending_credit_counter(&setup.svm, &alice_account),
        alice_keys.encrypt(1_000_000).to_bytes(),
    );
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let event: events::PendingBalanceApplied = parse_event(&res);
    assert_eq!(event.account, alice_account);
    assert_eq!(
        available_balance(&setup.svm, &alice_account, &alice_elgamal),
        1_000_000
    );
    assert_eq!(
        pending_balance(&setup.svm, &alice_account, &alice_elgamal),
        0
    );
    assert_eq!(
        decryptable_balance(&setup.svm, &alice_account)
            .decrypt(&alice_keys)
            .unwrap(),
        1_000_000
    );

    // 4. Confidential transfer: 400,000 from Alice to Bob, with the amount
    //    hidden. Both sides sign nothing but Alice's transaction fee.
    let transfer_amount = 400_000u64;
    let proofs = transfer_split_proof_data(
        &ciphertext(bytemuck::bytes_of(
            &confidential_extension(&setup.svm, &alice_account).available_balance,
        )),
        &decryptable_balance(&setup.svm, &alice_account),
        transfer_amount,
        &alice_elgamal,
        &alice_keys,
        bob_elgamal.pubkey(),
        None,
    )
    .unwrap();
    let equality = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &proofs.equality_proof_data,
    );
    let validity = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyBatchedGroupedCiphertext3HandlesValidity,
        &proofs.ciphertext_validity_proof_data_with_ciphertext.proof_data,
    );
    let range = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyBatchedRangeProofU128,
        &proofs.range_proof_data,
    );
    let auditor_lo: [u8; 64] = bytemuck::bytes_of(
        &proofs
            .ciphertext_validity_proof_data_with_ciphertext
            .ciphertext_lo,
    )
    .try_into()
    .unwrap();
    let auditor_hi: [u8; 64] = bytemuck::bytes_of(
        &proofs
            .ciphertext_validity_proof_data_with_ciphertext
            .ciphertext_hi,
    )
    .try_into()
    .unwrap();

    let ix = confidential_transfer_ix(
        &alice,
        &mint,
        &alice_account,
        &bob_account,
        alice_keys.encrypt(1_000_000 - transfer_amount).to_bytes(),
        auditor_lo,
        auditor_hi,
        &equality,
        &validity,
        &range,
    );
    let res = send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let event: events::ConfidentialTokensTransferred = parse_event(&res);
    assert_eq!(event.source, alice_account);
    assert_eq!(event.destination, bob_account);

    assert_eq!(
        available_balance(&setup.svm, &alice_account, &alice_elgamal),
        600_000
    );
    // Bob's balance lands in his pending balance until he applies it.
    assert_eq!(
        pending_balance(&setup.svm, &bob_account, &bob_elgamal),
        400_000
    );
    assert_eq!(available_balance(&setup.svm, &bob_account, &bob_elgamal), 0);
    // Nothing about the amount is public: both public balances are zero.
    assert_eq!(amount(&setup.svm, &alice_account), 0);
    assert_eq!(amount(&setup.svm, &bob_account), 0);

    // 5. WithdrawConfidentialTokens, which applies Bob's pending balance first
    //    and then converts 300,000 back into a public balance.
    let withdraw_amount = 300_000u64;
    let spendable = available_with_pending(&setup.svm, &bob_account);
    let spendable_plain = spendable
        .decrypt_u32(bob_elgamal.secret())
        .expect("spendable balance decrypts");
    assert_eq!(spendable_plain, 400_000);

    let proofs = withdraw_proof_data(
        &spendable,
        spendable_plain,
        withdraw_amount,
        &bob_elgamal,
    )
    .unwrap();
    let equality = verify_proof(
        &mut setup.svm,
        &bob,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &proofs.equality_proof_data,
    );
    let range = verify_proof(
        &mut setup.svm,
        &bob,
        ProofInstruction::VerifyBatchedRangeProofU64,
        &proofs.range_proof_data,
    );

    let ix = withdraw_confidential_ix(
        &bob.pubkey(),
        &mint,
        &bob_account,
        withdraw_amount,
        pending_credit_counter(&setup.svm, &bob_account),
        bob_keys.encrypt(spendable_plain).to_bytes(),
        bob_keys
            .encrypt(spendable_plain - withdraw_amount)
            .to_bytes(),
        &equality,
        &range,
    );
    let res = send_ok(&mut setup.svm, &[&bob], &[ix]);
    let event: events::ConfidentialTokensWithdrawn = parse_event(&res);
    assert_eq!(event.amount, withdraw_amount);
    assert!(event.pending_balance_applied);

    assert_eq!(amount(&setup.svm, &bob_account), 300_000);
    assert_eq!(
        available_balance(&setup.svm, &bob_account, &bob_elgamal),
        100_000
    );
    assert_eq!(pending_balance(&setup.svm, &bob_account, &bob_elgamal), 0);
    assert_eq!(
        decryptable_balance(&setup.svm, &bob_account)
            .decrypt(&bob_keys)
            .unwrap(),
        100_000
    );
    // Alice is untouched by Bob's withdrawal.
    assert_eq!(
        available_balance(&setup.svm, &alice_account, &alice_elgamal),
        600_000
    );
}

#[test]
fn confidential_transfer_rejects_an_unapproved_destination() {
    let (mut setup, confidential_mint, _) = reissue_setup();
    let mint = confidential_mint.pubkey();
    let alice = setup.user.pubkey();

    let keypair = ElGamalKeypair::new_rand();
    let keys = AeKey::new_rand();
    let alice_account = funded_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &alice,
        100_000,
    );
    let bob_account = open_account(
        &mut setup.svm,
        &setup.issuer,
        &setup.kyc,
        &mint,
        &alice,
    );

    // Alice is configured, approved and funded confidentially.
    let proof = build_pubkey_validity_proof_data(&keypair).unwrap();
    let proof_account = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyPubkeyValidity,
        &proof,
    );
    let ix = configure_confidential_account_ix(
        &alice,
        &mint,
        &alice_account,
        &proof_account,
        keys.encrypt(0).to_bytes(),
    );
    send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let ix = approve_confidential_account_ix(&setup.issuer.pubkey(), &mint, &alice_account);
    send_ok(&mut setup.svm, &[&setup.issuer], &[ix]);
    let ix = deposit_confidential_ix(&alice, &mint, &alice_account, 100_000);
    send_ok(&mut setup.svm, &[&setup.user], &[ix]);
    let ix = apply_pending_balance_ix(
        &alice,
        &mint,
        &alice_account,
        pending_credit_counter(&setup.svm, &alice_account),
        keys.encrypt(100_000).to_bytes(),
    );
    send_ok(&mut setup.svm, &[&setup.user], &[ix]);

    // The destination has not been configured at all.
    let proofs = transfer_split_proof_data(
        &ciphertext(bytemuck::bytes_of(
            &confidential_extension(&setup.svm, &alice_account).available_balance,
        )),
        &decryptable_balance(&setup.svm, &alice_account),
        10_000,
        &keypair,
        &keys,
        ElGamalKeypair::new_rand().pubkey(),
        None,
    )
    .unwrap();
    let equality = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyCiphertextCommitmentEquality,
        &proofs.equality_proof_data,
    );
    let validity = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyBatchedGroupedCiphertext3HandlesValidity,
        &proofs.ciphertext_validity_proof_data_with_ciphertext.proof_data,
    );
    let range = verify_proof(
        &mut setup.svm,
        &setup.user,
        ProofInstruction::VerifyBatchedRangeProofU128,
        &proofs.range_proof_data,
    );
    let ix = confidential_transfer_ix(
        &alice,
        &mint,
        &alice_account,
        &bob_account,
        keys.encrypt(90_000).to_bytes(),
        bytemuck::bytes_of(&proofs.ciphertext_validity_proof_data_with_ciphertext.ciphertext_lo)
            .try_into()
            .unwrap(),
        bytemuck::bytes_of(&proofs.ciphertext_validity_proof_data_with_ciphertext.ciphertext_hi)
            .try_into()
            .unwrap(),
        &equality,
        &validity,
        &range,
    );
    let res = send(&mut setup.svm, &[&setup.user], &[ix]);
    assert_failed_with(&res, "ConfidentialAccountNotConfigured");

    // ... and nothing moved.
    assert_eq!(
        available_balance(&setup.svm, &alice_account, &keypair),
        100_000
    );
}
