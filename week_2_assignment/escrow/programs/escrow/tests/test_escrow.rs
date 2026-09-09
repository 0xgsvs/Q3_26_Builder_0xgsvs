use anchor_lang::{
    AccountDeserialize, InstructionData, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use escrow::{ESCROW_SEED, accounts, instruction};
use litesvm::{LiteSVM, types::TransactionResult};
use litesvm_token::{
    CreateAssociatedTokenAccount, CreateMint, MintToChecked, TOKEN_ID, get_spl_account, spl_token,
};
use solana_awesome::{
    keypair::{Address, Keypair},
    message::{Message, VersionedMessage},
    signer::Signer,
    transaction::versioned::VersionedTransaction,
};

struct Setup {
    program_id: Address,
    svm: LiteSVM,
    maker: Keypair,
    taker: Keypair,
    mint_a: Address,
    mint_b: Address,
    maker_ata_a: Address,
    taker_ata_b: Address,
    escrow: Address,
    vault: Address,
    seed: u64,
    deposit: u64,
    receive: u64,
    expiration: i64,
}

impl Setup {
    fn send_as_maker(&mut self, instructions: &[Instruction]) -> TransactionResult {
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(instructions, Some(&self.maker.pubkey()), &blockhash);
        let tx =
            VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&self.maker]).unwrap();
        self.svm.send_transaction(tx)
    }

    fn send_as_taker(&mut self, instructions: &[Instruction]) -> TransactionResult {
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(instructions, Some(&self.taker.pubkey()), &blockhash);
        let tx =
            VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&self.taker]).unwrap();
        self.svm.send_transaction(tx)
    }

    fn make_ix(&self) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &instruction::Make {
                seed: self.seed,
                deposit: self.deposit,
                receive: self.receive,
                expiration: self.expiration,
            }
            .data(),
            accounts::Make {
                maker: self.maker.pubkey(),
                mint_a: self.mint_a,
                mint_b: self.mint_b,
                maker_ata_a: self.maker_ata_a,
                escrow: self.escrow,
                vault: self.vault,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: TOKEN_ID,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }
}

fn setup() -> Setup {
    let program_id = escrow::id();
    let maker = Keypair::new();
    let taker = Keypair::new();
    let seed = 42u64;
    let deposit = 1_000_000u64;
    let receive = 500_000u64;
    let expiration = 0i64;

    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/escrow.so"));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&maker.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&taker.pubkey(), 10_000_000_000).unwrap();

    let mint_a = CreateMint::new(&mut svm, &maker)
        .decimals(6)
        .send()
        .unwrap();
    let mint_b = CreateMint::new(&mut svm, &maker)
        .decimals(6)
        .send()
        .unwrap();

    let maker_ata_a = CreateAssociatedTokenAccount::new(&mut svm, &maker, &mint_a)
        .owner(&maker.pubkey())
        .send()
        .unwrap();
    let taker_ata_b = CreateAssociatedTokenAccount::new(&mut svm, &taker, &mint_b)
        .owner(&taker.pubkey())
        .send()
        .unwrap();

    MintToChecked::new(&mut svm, &maker, &mint_a, &maker_ata_a, 10_000_000)
        .decimals(6)
        .send()
        .unwrap();
    MintToChecked::new(&mut svm, &maker, &mint_b, &taker_ata_b, 10_000_000)
        .decimals(6)
        .send()
        .unwrap();

    let escrow = Pubkey::find_program_address(
        &[
            ESCROW_SEED,
            maker.pubkey().as_ref(),
            seed.to_le_bytes().as_ref(),
        ],
        &program_id,
    )
    .0;
    let vault = get_associated_token_address_with_program_id(&escrow, &mint_a, &TOKEN_ID);

    Setup {
        program_id,
        svm,
        maker,
        taker,
        mint_a,
        mint_b,
        maker_ata_a,
        taker_ata_b,
        escrow,
        vault,
        seed,
        deposit,
        receive,
        expiration,
    }
}

fn token_balance(svm: &LiteSVM, ata: &Address) -> u64 {
    get_spl_account::<spl_token::state::Account>(svm, ata)
        .unwrap()
        .amount
}

#[test]
fn escrow_make() {
    let mut setup = setup();
    let ix = setup.make_ix();
    let res = setup.send_as_maker(&[ix]);
    assert!(res.is_ok());

    let escrow_account = setup.svm.get_account(&setup.escrow).unwrap();
    let mut data: &[u8] = &escrow_account.data;
    let state = escrow::state::Escrow::try_deserialize(&mut data).unwrap();
    assert_eq!(state.seed, setup.seed);
    assert_eq!(state.maker, setup.maker.pubkey());
    assert_eq!(state.mint_a, setup.mint_a);
    assert_eq!(state.mint_b, setup.mint_b);
    assert_eq!(state.receive, setup.receive);
    assert_eq!(token_balance(&setup.svm, &setup.vault), setup.deposit);
}

#[test]
fn escrow_update() {
    let mut setup = setup();
    let make_ix = setup.make_ix();
    assert!(setup.send_as_maker(&[make_ix]).is_ok());

    let new_receive = 750_000u64;
    let new_expiration = 1_234_567_890i64;
    let update_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Update {
            receive: new_receive,
            expiration: new_expiration,
        }
        .data(),
        accounts::Update {
            maker: setup.maker.pubkey(),
            escrow: setup.escrow,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_as_maker(&[update_ix]);
    assert!(res.is_ok());

    let escrow_account = setup.svm.get_account(&setup.escrow).unwrap();
    let mut data: &[u8] = &escrow_account.data;
    let state = escrow::state::Escrow::try_deserialize(&mut data).unwrap();
    assert_eq!(state.receive, new_receive);
    assert_eq!(state.expiration, new_expiration);
}

#[test]
fn escrow_take() {
    let mut setup = setup();
    let make_ix = setup.make_ix();
    assert!(setup.send_as_maker(&[make_ix]).is_ok());

    let maker_ata_b = get_associated_token_address_with_program_id(
        &setup.maker.pubkey(),
        &setup.mint_b,
        &TOKEN_ID,
    );
    let taker_ata_a = get_associated_token_address_with_program_id(
        &setup.taker.pubkey(),
        &setup.mint_a,
        &TOKEN_ID,
    );

    let take_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Take {}.data(),
        accounts::Take {
            maker: setup.maker.pubkey(),
            taker: setup.taker.pubkey(),
            mint_a: setup.mint_a,
            mint_b: setup.mint_b,
            maker_ata_b,
            taker_ata_a,
            taker_ata_b: setup.taker_ata_b,
            escrow: setup.escrow,
            vault: setup.vault,
            associated_token_program: anchor_spl::associated_token::ID,
            token_program: TOKEN_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_as_taker(&[take_ix]);
    assert!(res.is_ok());

    assert_eq!(token_balance(&setup.svm, &taker_ata_a), setup.deposit);
    assert_eq!(token_balance(&setup.svm, &maker_ata_b), setup.receive);
    assert!(setup.svm.get_account(&setup.escrow).is_none());
}

#[test]
fn escrow_refund() {
    let mut setup = setup();
    let make_ix = setup.make_ix();
    assert!(setup.send_as_maker(&[make_ix]).is_ok());

    let refund_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Refund {}.data(),
        accounts::Refund {
            maker: setup.maker.pubkey(),
            mint_a: setup.mint_a,
            maker_ata_a: setup.maker_ata_a,
            escrow: setup.escrow,
            vault: setup.vault,
            token_program: TOKEN_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_as_maker(&[refund_ix]);
    assert!(res.is_ok());

    assert_eq!(token_balance(&setup.svm, &setup.maker_ata_a), 10_000_000);
    assert!(setup.svm.get_account(&setup.escrow).is_none());
}
