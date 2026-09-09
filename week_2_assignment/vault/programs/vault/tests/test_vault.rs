use anchor_lang::{
    InstructionData, Key, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use litesvm::LiteSVM;
use solana_awesome::{
    keypair::{Address, Keypair},
    message::{Message, VersionedMessage},
    signer::Signer,
    transaction::versioned::VersionedTransaction,
};
use vault::{accounts, instruction};

struct Setup {
    program_id: Address,
    svm: LiteSVM,
    user: Keypair,
    vault: Address,
    vault_state: Address,
}

impl Setup {
    fn send_transaction(
        &mut self,
        instructions: &[Instruction],
    ) -> litesvm::types::TransactionResult {
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(instructions, Some(&self.user.pubkey()), &blockhash);
        let tx =
            VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&self.user]).unwrap();
        self.svm.send_transaction(tx)
    }
}
fn setup() -> Setup {
    let program_id = vault::id();
    let user = Keypair::new();
    let vault_state = Pubkey::find_program_address(
        &[vault::constants::VAULT_STATE_SEED, user.pubkey().as_ref()],
        &program_id,
    )
    .0;
    let vault = Pubkey::find_program_address(
        &[vault::constants::VAULT_SEED, vault_state.key().as_ref()],
        &program_id,
    )
    .0;
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/vault.so"));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&user.pubkey(), 1_000_000_000).unwrap();
    Setup {
        program_id,
        svm,
        user,
        vault,
        vault_state,
    }
}

#[test]
fn vault_initialize() {
    let mut setup = setup();
    let instruction = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Initialize {}.data(),
        accounts::Initialize {
            user: setup.user.pubkey(),
            vault_state: setup.vault_state,
            vault: setup.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_transaction(&[instruction]);
    assert!(res.is_ok());
}

#[test]
fn vault_deposit() {
    let mut setup = setup();
    let init_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Initialize {}.data(),
        accounts::Initialize {
            user: setup.user.pubkey(),
            vault_state: setup.vault_state,
            vault: setup.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(setup.send_transaction(&[init_ix]).is_ok());

    let amount = 100_000_000u64;
    let deposit_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Deposit { amount }.data(),
        accounts::Deposit {
            user: setup.user.pubkey(),
            vault: setup.vault,
            vault_state: setup.vault_state,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_transaction(&[deposit_ix]);
    assert!(res.is_ok());

    assert_eq!(setup.svm.get_balance(&setup.vault).unwrap(), amount);
}

#[test]
fn vault_withdraw() {
    let mut setup = setup();
    let init_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Initialize {}.data(),
        accounts::Initialize {
            user: setup.user.pubkey(),
            vault_state: setup.vault_state,
            vault: setup.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(setup.send_transaction(&[init_ix]).is_ok());

    let deposit_amount = 100_000_000u64;
    let deposit_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Deposit {
            amount: deposit_amount,
        }
        .data(),
        accounts::Deposit {
            user: setup.user.pubkey(),
            vault: setup.vault,
            vault_state: setup.vault_state,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(setup.send_transaction(&[deposit_ix]).is_ok());

    let withdraw_amount = 40_000_000u64;
    let user_before = setup.svm.get_balance(&setup.user.pubkey()).unwrap();
    let withdraw_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Withdraw {
            amount: withdraw_amount,
        }
        .data(),
        accounts::Withdraw {
            user: setup.user.pubkey(),
            vault: setup.vault,
            vault_state: setup.vault_state,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_transaction(&[withdraw_ix]);
    assert!(res.is_ok());

    assert_eq!(
        setup.svm.get_balance(&setup.vault).unwrap(),
        deposit_amount - withdraw_amount
    );
    // User pays tx fee, so net gain is withdraw minus fee.
    assert!(setup.svm.get_balance(&setup.user.pubkey()).unwrap() > user_before);
}

#[test]
fn vault_close() {
    let mut setup = setup();
    let init_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Initialize {}.data(),
        accounts::Initialize {
            user: setup.user.pubkey(),
            vault_state: setup.vault_state,
            vault: setup.vault,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(setup.send_transaction(&[init_ix]).is_ok());

    let deposit_amount = 100_000_000u64;
    let deposit_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Deposit {
            amount: deposit_amount,
        }
        .data(),
        accounts::Deposit {
            user: setup.user.pubkey(),
            vault: setup.vault,
            vault_state: setup.vault_state,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(setup.send_transaction(&[deposit_ix]).is_ok());

    let user_before = setup.svm.get_balance(&setup.user.pubkey()).unwrap();
    let close_ix = Instruction::new_with_bytes(
        setup.program_id,
        &instruction::Close {}.data(),
        accounts::Close {
            user: setup.user.pubkey(),
            vault: setup.vault,
            vault_state: setup.vault_state,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );

    let res = setup.send_transaction(&[close_ix]);
    assert!(res.is_ok());

    assert_eq!(setup.svm.get_balance(&setup.vault).unwrap_or(0), 0);
    assert!(setup.svm.get_account(&setup.vault_state).is_none());
    assert!(setup.svm.get_balance(&setup.user.pubkey()).unwrap() > user_before);
}
