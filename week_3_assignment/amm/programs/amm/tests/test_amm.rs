use amm::{accounts, events, instruction};
use anchor_lang::{
    AccountDeserialize, AnchorDeserialize, Discriminator, InstructionData, ToAccountMetas,
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
};
use anchor_spl::associated_token::get_associated_token_address_with_program_id;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
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

const SEED: u64 = 42;
const FEE_BPS: u16 = 30;
const DECIMALS: u8 = 6;
const USER_FUNDS: u64 = 1_000_000_000;

struct Setup {
    svm: LiteSVM,
    admin: Keypair,
    treasury: Keypair,
    mint_x: Address,
    mint_y: Address,
    config: Address,
    mint_lp: Address,
    vault_x: Address,
    vault_y: Address,
}

fn send(svm: &mut LiteSVM, payer: &Keypair, ixs: &[Instruction]) -> TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap();
    svm.send_transaction(tx)
}

fn token_balance(svm: &LiteSVM, ata: &Address) -> u64 {
    get_spl_account::<spl_token::state::Account>(svm, ata)
        .unwrap()
        .amount
}

/// Decode the first `Program data:` log whose 8-byte discriminator matches `T`.
fn parse_event<T>(res: &TransactionResult) -> T
where
    T: AnchorDeserialize + Discriminator,
{
    let logs = &res.as_ref().expect("transaction failed").logs;
    let discriminator: &[u8] = T::DISCRIMINATOR;
    for log in logs {
        if let Some(b64) = log.strip_prefix("Program data: ") {
            let bytes = B64.decode(b64.trim()).expect("valid base64 event");
            if bytes.len() >= 8 && bytes[..8] == *discriminator {
                return T::try_from_slice(&bytes[8..]).expect("event decodes");
            }
        }
    }
    panic!("event not found in logs");
}

fn treasury_x(setup: &Setup) -> Address {
    get_associated_token_address_with_program_id(&setup.treasury.pubkey(), &setup.mint_x, &TOKEN_ID)
}

fn treasury_y(setup: &Setup) -> Address {
    get_associated_token_address_with_program_id(&setup.treasury.pubkey(), &setup.mint_y, &TOKEN_ID)
}

fn setup() -> Setup {
    let program_id = amm::id();
    let admin = Keypair::new();
    let treasury = Keypair::new();

    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/amm.so"));
    svm.add_program(program_id, bytes).unwrap();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();

    let mint_x = CreateMint::new(&mut svm, &admin)
        .decimals(DECIMALS)
        .send()
        .unwrap();
    let mint_y = CreateMint::new(&mut svm, &admin)
        .decimals(DECIMALS)
        .send()
        .unwrap();

    let config = Pubkey::find_program_address(
        &[amm::constants::CONFIG_SEED, SEED.to_le_bytes().as_ref()],
        &program_id,
    )
    .0;
    let mint_lp =
        Pubkey::find_program_address(&[amm::constants::LP_SEED, config.as_ref()], &program_id).0;
    let vault_x = get_associated_token_address_with_program_id(&config, &mint_x, &TOKEN_ID);
    let vault_y = get_associated_token_address_with_program_id(&config, &mint_y, &TOKEN_ID);

    Setup {
        svm,
        admin,
        treasury,
        mint_x,
        mint_y,
        config,
        mint_lp,
        vault_x,
        vault_y,
    }
}

fn init_ix(setup: &Setup, fee: u16) -> Instruction {
    Instruction::new_with_bytes(
        amm::id(),
        &instruction::Initialize {
            seed: SEED,
            fee,
            authority: Some(setup.admin.pubkey()),
        }
        .data(),
        accounts::Initialize {
            initializer: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_y,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            treasury: setup.treasury.pubkey(),
            config: setup.config,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Creates + funds the admin's X/Y token accounts. Returns (user_x, user_y).
fn fund_user(setup: &mut Setup) -> (Address, Address) {
    let user = setup.admin.pubkey();
    let user_x = CreateAssociatedTokenAccount::new(&mut setup.svm, &setup.admin, &setup.mint_x)
        .owner(&user)
        .send()
        .unwrap();
    MintToChecked::new(
        &mut setup.svm,
        &setup.admin,
        &setup.mint_x,
        &user_x,
        USER_FUNDS,
    )
    .decimals(DECIMALS)
    .send()
    .unwrap();

    let user_y = CreateAssociatedTokenAccount::new(&mut setup.svm, &setup.admin, &setup.mint_y)
        .owner(&user)
        .send()
        .unwrap();
    MintToChecked::new(
        &mut setup.svm,
        &setup.admin,
        &setup.mint_y,
        &user_y,
        USER_FUNDS,
    )
    .decimals(DECIMALS)
    .send()
    .unwrap();

    (user_x, user_y)
}

fn deposit_ix(
    setup: &Setup,
    user_x: Address,
    user_y: Address,
    user_lp: Address,
    amount: u64,
    max_x: u64,
    max_y: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        amm::id(),
        &instruction::Deposit {
            amount,
            max_x,
            max_y,
        }
        .data(),
        accounts::Deposit {
            user: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_y,
            config: setup.config,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn withdraw_ix(
    setup: &Setup,
    user_x: Address,
    user_y: Address,
    user_lp: Address,
    amount: u64,
    min_x: u64,
    min_y: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        amm::id(),
        &instruction::Withdraw {
            amount,
            min_x,
            min_y,
        }
        .data(),
        accounts::Withdraw {
            user: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_y,
            config: setup.config,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            user_x,
            user_y,
            user_lp,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn swap_ix(
    setup: &Setup,
    user_x: Address,
    user_y: Address,
    is_x: bool,
    amount_in: u64,
    min_out: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        amm::id(),
        &instruction::Swap {
            is_x,
            amount_in,
            min_amount_out: min_out,
        }
        .data(),
        accounts::Swap {
            user: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_y,
            config: setup.config,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            treasury: setup.treasury.pubkey(),
            treasury_x: treasury_x(setup),
            treasury_y: treasury_y(setup),
            user_x,
            user_y,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn update_ix(setup: &Setup, signer: &Keypair, fee: u16, locked: bool) -> Instruction {
    Instruction::new_with_bytes(
        amm::id(),
        &instruction::Update { fee, locked }.data(),
        accounts::Update {
            authority: signer.pubkey(),
            config: setup.config,
        }
        .to_account_metas(None),
    )
}

fn user_lp_ata(setup: &Setup) -> Address {
    get_associated_token_address_with_program_id(&setup.admin.pubkey(), &setup.mint_lp, &TOKEN_ID)
}

/// Fresh pool with a first deposit: vaults hold 200M X / 200M Y, LP supply 100M.
fn setup_with_liquidity() -> (Setup, Address, Address) {
    let mut setup = setup();
    let ix = init_ix(&setup, FEE_BPS);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    let (user_x, user_y) = fund_user(&mut setup);
    let user_lp = user_lp_ata(&setup);
    let ix = deposit_ix(
        &setup,
        user_x,
        user_y,
        user_lp,
        100_000_000,
        200_000_000,
        200_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    (setup, user_x, user_y)
}

#[test]
fn initialize_ok() {
    let mut setup = setup();
    let ix = init_ix(&setup, FEE_BPS);
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_ok());

    let raw = setup.svm.get_account(&setup.config).unwrap();
    let mut data: &[u8] = &raw.data;
    let config = amm::state::Config::try_deserialize(&mut data).unwrap();
    assert_eq!(config.seed, SEED);
    assert_eq!(config.fee, FEE_BPS);
    assert_eq!(config.authority, Some(setup.admin.pubkey()));
    assert_eq!(config.treasury, setup.treasury.pubkey());
    assert_eq!(config.mint_x, setup.mint_x);
    assert_eq!(config.mint_y, setup.mint_y);
    assert!(!config.locked);

    let event: events::PoolInitialized = parse_event(&res);
    assert_eq!(event.config, setup.config);
    assert_eq!(event.mint_x, setup.mint_x);
    assert_eq!(event.mint_y, setup.mint_y);
    assert_eq!(event.mint_lp, setup.mint_lp);
    assert_eq!(event.vault_x, setup.vault_x);
    assert_eq!(event.vault_y, setup.vault_y);
    assert_eq!(event.treasury, setup.treasury.pubkey());
    assert_eq!(event.seed, SEED);
    assert_eq!(event.fee, FEE_BPS);

    let lp = get_spl_account::<spl_token::state::Mint>(&setup.svm, &setup.mint_lp).unwrap();
    assert_eq!(lp.supply, 0);
    assert_eq!(lp.decimals, DECIMALS);
    assert_eq!(lp.mint_authority.unwrap(), setup.config);

    for (vault, mint) in [(setup.vault_x, setup.mint_x), (setup.vault_y, setup.mint_y)] {
        let v = get_spl_account::<spl_token::state::Account>(&setup.svm, &vault).unwrap();
        assert_eq!(v.amount, 0);
        assert_eq!(v.mint, mint);
        assert_eq!(v.owner, setup.config);
    }
}

#[test]
fn initialize_rejects_fee_above_10000() {
    let mut setup = setup();
    let ix = init_ix(&setup, 10_001);
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_err());
}

#[test]
fn initialize_rejects_same_mint() {
    let mut setup = setup();
    let ix = Instruction::new_with_bytes(
        amm::id(),
        &instruction::Initialize {
            seed: SEED,
            fee: FEE_BPS,
            authority: None,
        }
        .data(),
        accounts::Initialize {
            initializer: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_x,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            treasury: setup.treasury.pubkey(),
            config: setup.config,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn deposit_first_and_second() {
    let mut setup = setup();
    let ix = init_ix(&setup, FEE_BPS);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    let (user_x, user_y) = fund_user(&mut setup);
    let user_lp = user_lp_ata(&setup);

    // First deposit seeds the pool at whatever ratio the depositor picks.
    let ix = deposit_ix(
        &setup,
        user_x,
        user_y,
        user_lp,
        100_000_000,
        200_000_000,
        200_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    assert_eq!(token_balance(&setup.svm, &setup.vault_x), 200_000_000);
    assert_eq!(token_balance(&setup.svm, &setup.vault_y), 200_000_000);
    assert_eq!(token_balance(&setup.svm, &user_lp), 100_000_000);

    // Second deposit is proportional: 50M LP against 100M supply => 100M X + 100M Y.
    let ix = deposit_ix(
        &setup,
        user_x,
        user_y,
        user_lp,
        50_000_000,
        100_000_000,
        100_000_000,
    );
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_ok());
    assert_eq!(token_balance(&setup.svm, &setup.vault_x), 300_000_000);
    assert_eq!(token_balance(&setup.svm, &setup.vault_y), 300_000_000);
    assert_eq!(token_balance(&setup.svm, &user_lp), 150_000_000);

    let event: events::LiquidityDeposited = parse_event(&res);
    assert_eq!(event.config, setup.config);
    assert_eq!(event.user, setup.admin.pubkey());
    assert_eq!(event.lp_minted, 50_000_000);
    assert_eq!(event.x_deposited, 100_000_000);
    assert_eq!(event.y_deposited, 100_000_000);
}

#[test]
fn deposit_slippage_fail() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let user_lp = user_lp_ata(&setup);
    // 50M LP needs 100M X + 100M Y; max of 10M must fail.
    let ix = deposit_ix(
        &setup, user_x, user_y, user_lp, 50_000_000, 10_000_000, 10_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn deposit_zero_amount_fail() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let user_lp = user_lp_ata(&setup);
    let ix = deposit_ix(&setup, user_x, user_y, user_lp, 0, 100_000_000, 100_000_000);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn withdraw_ok() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let user_lp = user_lp_ata(&setup);

    // Top up to 150M LP / 300M reserves, then burn 50M LP => 100M X + 100M Y back.
    let ix = deposit_ix(
        &setup,
        user_x,
        user_y,
        user_lp,
        50_000_000,
        100_000_000,
        100_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());

    let user_x_before = token_balance(&setup.svm, &user_x);
    let ix = withdraw_ix(
        &setup,
        user_x,
        user_y,
        user_lp,
        50_000_000,
        100_000_000,
        100_000_000,
    );
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_ok());

    assert_eq!(token_balance(&setup.svm, &setup.vault_x), 200_000_000);
    assert_eq!(token_balance(&setup.svm, &setup.vault_y), 200_000_000);
    assert_eq!(token_balance(&setup.svm, &user_lp), 100_000_000);
    assert_eq!(
        token_balance(&setup.svm, &user_x),
        user_x_before + 100_000_000
    );

    let event: events::LiquidityWithdrawn = parse_event(&res);
    assert_eq!(event.config, setup.config);
    assert_eq!(event.user, setup.admin.pubkey());
    assert_eq!(event.lp_burned, 50_000_000);
    assert_eq!(event.x_withdrawn, 100_000_000);
    assert_eq!(event.y_withdrawn, 100_000_000);
}

#[test]
fn withdraw_slippage_fail() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let user_lp = user_lp_ata(&setup);
    // 10M LP of 100M supply against 200M reserves => 20M each; asking 50M must fail.
    let ix = withdraw_ix(
        &setup, user_x, user_y, user_lp, 10_000_000, 50_000_000, 50_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn swap_x_for_y_routes_fee_to_treasury() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();

    let vault_x_before = token_balance(&setup.svm, &setup.vault_x);
    let user_y_before = token_balance(&setup.svm, &user_y);

    // 30 bps of 10M = 30_000 to treasury_x; net 9_970_000 into the pool.
    let ix = swap_ix(&setup, user_x, user_y, true, 10_000_000, 1);
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_ok());

    assert_eq!(token_balance(&setup.svm, &treasury_x(&setup)), 30_000);
    assert_eq!(
        token_balance(&setup.svm, &setup.vault_x),
        vault_x_before + 9_970_000
    );
    assert!(token_balance(&setup.svm, &user_y) > user_y_before);
    assert!(token_balance(&setup.svm, &setup.vault_y) < 200_000_000);

    let event: events::Swapped = parse_event(&res);
    assert_eq!(event.config, setup.config);
    assert_eq!(event.user, setup.admin.pubkey());
    assert!(event.is_x);
    assert_eq!(event.amount_in, 10_000_000);
    assert_eq!(event.fee_amount, 30_000);
    assert_eq!(
        event.amount_out,
        token_balance(&setup.svm, &user_y) - user_y_before
    );
}

#[test]
fn swap_y_for_x_routes_fee_to_treasury() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();

    let user_x_before = token_balance(&setup.svm, &user_x);

    let ix = swap_ix(&setup, user_x, user_y, false, 10_000_000, 1);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());

    assert_eq!(token_balance(&setup.svm, &treasury_y(&setup)), 30_000);
    assert!(token_balance(&setup.svm, &user_x) > user_x_before);
    assert!(token_balance(&setup.svm, &setup.vault_x) < 200_000_000);
}

#[test]
fn swap_slippage_fail() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let ix = swap_ix(&setup, user_x, user_y, true, 10_000_000, 100_000_000);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn swap_wrong_treasury_fail() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let attacker = Keypair::new();
    let bad_treasury_x =
        get_associated_token_address_with_program_id(&attacker.pubkey(), &setup.mint_x, &TOKEN_ID);
    let bad_treasury_y =
        get_associated_token_address_with_program_id(&attacker.pubkey(), &setup.mint_y, &TOKEN_ID);
    let ix = Instruction::new_with_bytes(
        amm::id(),
        &instruction::Swap {
            is_x: true,
            amount_in: 10_000_000,
            min_amount_out: 1,
        }
        .data(),
        accounts::Swap {
            user: setup.admin.pubkey(),
            mint_x: setup.mint_x,
            mint_y: setup.mint_y,
            config: setup.config,
            mint_lp: setup.mint_lp,
            vault_x: setup.vault_x,
            vault_y: setup.vault_y,
            treasury: attacker.pubkey(),
            treasury_x: bad_treasury_x,
            treasury_y: bad_treasury_y,
            user_x,
            user_y,
            token_program: TOKEN_ID,
            associated_token_program: anchor_spl::associated_token::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}

#[test]
fn update_fee_and_lock() {
    let (mut setup, user_x, user_y) = setup_with_liquidity();
    let user_lp = user_lp_ata(&setup);

    // Authority raises the fee.
    let ix = update_ix(&setup, &setup.admin, 100, false);
    let res = send(&mut setup.svm, &setup.admin, &[ix]);
    assert!(res.is_ok());
    let raw = setup.svm.get_account(&setup.config).unwrap();
    let mut data: &[u8] = &raw.data;
    let config = amm::state::Config::try_deserialize(&mut data).unwrap();
    assert_eq!(config.fee, 100);
    assert!(!config.locked);

    let event: events::PoolUpdated = parse_event(&res);
    assert_eq!(event.config, setup.config);
    assert_eq!(event.authority, setup.admin.pubkey());
    assert_eq!(event.fee, 100);
    assert!(!event.locked);

    // Authority locks the pool: deposit / withdraw / swap all fail.
    let ix = update_ix(&setup, &setup.admin, 100, true);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    let bad_deposit = deposit_ix(
        &setup, user_x, user_y, user_lp, 1_000_000, 5_000_000, 5_000_000,
    );
    assert!(send(&mut setup.svm, &setup.admin, &[bad_deposit]).is_err());
    let bad_withdraw = withdraw_ix(&setup, user_x, user_y, user_lp, 1_000_000, 1, 1);
    assert!(send(&mut setup.svm, &setup.admin, &[bad_withdraw]).is_err());
    let bad_swap = swap_ix(&setup, user_x, user_y, true, 1_000_000, 1);
    assert!(send(&mut setup.svm, &setup.admin, &[bad_swap]).is_err());

    // Unlock restores normal operation.
    let ix = update_ix(&setup, &setup.admin, 100, false);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    let ok_swap = swap_ix(&setup, user_x, user_y, true, 1_000_000, 1);
    assert!(send(&mut setup.svm, &setup.admin, &[ok_swap]).is_ok());
    // 100 bps of 1M = 10_000 to the treasury.
    assert_eq!(token_balance(&setup.svm, &treasury_x(&setup)), 10_000);
}

#[test]
fn update_unauthorized_fail() {
    let mut setup = setup();
    let ix = init_ix(&setup, FEE_BPS);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());

    let attacker = Keypair::new();
    setup
        .svm
        .airdrop(&attacker.pubkey(), 1_000_000_000)
        .unwrap();
    let ix = update_ix(&setup, &attacker, 500, false);
    assert!(send(&mut setup.svm, &attacker, &[ix]).is_err());
}

#[test]
fn update_invalid_fee_fail() {
    let mut setup = setup();
    let ix = init_ix(&setup, FEE_BPS);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_ok());
    let ix = update_ix(&setup, &setup.admin, 10_001, false);
    assert!(send(&mut setup.svm, &setup.admin, &[ix]).is_err());
}
