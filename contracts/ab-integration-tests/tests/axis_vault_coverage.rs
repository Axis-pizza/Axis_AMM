//! axis-vault CreateEtf + Deposit validation coverage (issue #33).
//!
//! kidney's scenario table:
//!   - CreateEtf invalid token_count          → InvalidBasketSize (9002)
//!   - CreateEtf weights != 10_000            → WeightsMismatch   (9003)
//!   - CreateEtf invalid ticker               → InvalidTicker     (9019)
//!   - CreateEtf invalid name                 → InvalidName       (9020)
//!   - Deposit wrong mint on basket ATA       → MintMismatch or similar
//!
//! These sit in the validation prefix of CreateEtf, before any account
//! initialization CPIs run — we can hit every branch with a minimal
//! account set (just enough to pass the length check) and a hand-rolled
//! instruction-data buffer.

use ab_integration_tests::helpers::{svm_setup::*, token_factory::*};
use ab_integration_tests::require_fixture;
use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

// VaultError codes
const ERR_INVALID_BASKET_SIZE: u32 = 9002;
const ERR_WEIGHTS_MISMATCH: u32 = 9003;
const ERR_INVALID_TICKER: u32 = 9019;
const ERR_INVALID_NAME: u32 = 9020;

// ─── CreateEtf instruction data helpers ────────────────────────────────

fn create_etf_data(
    token_count: u8,
    weights_bps: &[u16],
    ticker: &[u8],
    name: &[u8],
) -> Vec<u8> {
    let mut data = vec![0u8]; // disc = 0 (CreateEtf)
    data.push(token_count);
    for w in weights_bps {
        data.extend_from_slice(&w.to_le_bytes());
    }
    data.push(ticker.len() as u8);
    data.extend_from_slice(ticker);
    data.push(name.len() as u8);
    data.extend_from_slice(name);
    data
}

/// Build the minimum account list CreateEtf expects (6 fixed + 2N).
/// For validation-only tests we just need N fresh unique keys — the
/// rejection fires on instruction-data validation before any account
/// deserialization, so the accounts don't need real mint/vault bytes.
fn create_etf_accounts(
    authority: Address,
    etf_state: Address,
    etf_mint: Address,
    treasury: Address,
    basket_mints: &[Address],
    basket_vaults: &[Address],
) -> Vec<AccountMeta> {
    let mut a = vec![
        AccountMeta::new(authority, true),
        AccountMeta::new(etf_state, false),
        AccountMeta::new(etf_mint, false),
        AccountMeta::new_readonly(treasury, false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
    ];
    for m in basket_mints {
        a.push(AccountMeta::new_readonly(*m, false));
    }
    for v in basket_vaults {
        a.push(AccountMeta::new(*v, false));
    }
    a
}

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    match svm.send_transaction(tx) {
        Ok(meta) => Ok(meta.compute_units_consumed),
        Err(e) => {
            let mut msg = format!("{:?}", e.err);
            for log in &e.meta.logs {
                msg.push_str(&format!("\n  {}", log));
            }
            Err(msg)
        }
    }
}

fn assert_custom_err(err: &str, code: u32, label: &str) {
    let hex = format!("0x{:x}", code);
    let custom = format!("Custom({})", code);
    assert!(
        err.contains(&hex) || err.contains(&custom),
        "{label}: expected {code} ({hex}), got: {err}"
    );
}

// ─── Fixture ────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    etf_state: Address,
    etf_mint: Address,
    treasury: Address,
    basket_mints: Vec<Address>,
    basket_vaults: Vec<Address>,
}

/// Minimal setup for CreateEtf validation tests. Doesn't pre-create
/// any SPL state — these tests exercise instruction-data rejection
/// which fires before any account reads.
fn seed(n: usize, name: &[u8]) -> Option<Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_VAULT_SO).exists() {
        eprintln!("SKIP: axis_vault.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_vault_id(), AXIS_VAULT_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let etf_mint = Address::new_unique();
    let treasury = Address::new_unique();

    // Fund the etf_mint and vaults as empty System-owned accounts so
    // the runtime accepts the tx; CreateEtf's validation will reject
    // before it tries to CreateAccount / InitializeMint on them.
    let basket_mints: Vec<Address> = (0..n).map(|_| Address::new_unique()).collect();
    let basket_vaults: Vec<Address> = (0..n).map(|_| Address::new_unique()).collect();

    let (etf_state, _bump) = Address::find_program_address(
        &[b"etf", payer.pubkey().as_ref(), name],
        &axis_vault_id(),
    );

    // Empty accounts: 0 data, system-owned, 0 lamports — enough for the
    // runtime to serialize them into the tx. Program rejects before
    // reading their data.
    for k in basket_mints.iter().chain(basket_vaults.iter()).chain([etf_mint, treasury, etf_state].iter()) {
        svm.set_account(
            *k,
            Account {
                lamports: 0,
                data: vec![],
                owner: Address::from([0u8; 32]),
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
    }

    Some(Fixture {
        svm, payer, etf_state, etf_mint, treasury,
        basket_mints, basket_vaults,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn create_etf_rejects_invalid_basket_size() {
    require_fixture!(AXIS_VAULT_SO);
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(1, b"TEST") { Some(f) => f, None => return };

    // token_count = 1 is below the MIN_BASKET_TOKENS (2).
    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(1, &[10_000], b"AX", b"TEST"),
        },
        &payer,
    )
    .err()
    .expect("tc=1 should reject");
    assert_custom_err(&err, ERR_INVALID_BASKET_SIZE, "tc too small");
}

#[test]
fn create_etf_rejects_weights_mismatch() {
    require_fixture!(AXIS_VAULT_SO);
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(3, b"TEST") { Some(f) => f, None => return };

    // Weights sum to 9_999 (off by 1), not 10_000.
    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(3, &[3334, 3333, 3332], b"AX", b"TEST"),
        },
        &payer,
    )
    .err()
    .expect("weight sum 9999 should reject");
    assert_custom_err(&err, ERR_WEIGHTS_MISMATCH, "weights mismatch");
}

#[test]
fn create_etf_rejects_invalid_ticker() {
    require_fixture!(AXIS_VAULT_SO);
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(3, b"TEST") { Some(f) => f, None => return };

    // Ticker contains lowercase — rejected (ASCII upper + digits only).
    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(3, &[3334, 3333, 3333], b"ax", b"TEST"),
        },
        &payer,
    )
    .err()
    .expect("lowercase ticker should reject");
    assert_custom_err(&err, ERR_INVALID_TICKER, "ticker lowercase");
}

#[test]
fn create_etf_rejects_invalid_ticker_too_short() {
    require_fixture!(AXIS_VAULT_SO);
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(3, b"TEST") { Some(f) => f, None => return };

    // Ticker is 1 char — below the 2..=16 bound.
    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(3, &[3334, 3333, 3333], b"X", b"TEST"),
        },
        &payer,
    )
    .err()
    .expect("1-char ticker should reject");
    assert_custom_err(&err, ERR_INVALID_TICKER, "ticker too short");
}

#[test]
fn create_etf_rejects_empty_name() {
    require_fixture!(AXIS_VAULT_SO);
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(3, b"") { Some(f) => f, None => return };

    // Name is empty — rejected (1..=32 bytes required).
    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(3, &[3334, 3333, 3333], b"AX", b""),
        },
        &payer,
    )
    .err()
    .expect("empty name should reject");
    assert_custom_err(&err, ERR_INVALID_NAME, "empty name");
}

// ─── Deposit rejection paths ────────────────────────────────────────────
// Pre-seed an EtfState PDA + a full set of supporting SPL accounts so
// Deposit's pre-CPI validation branches fire.

const ERR_MINT_MISMATCH: u32 = 9009;
const ERR_VAULT_MISMATCH: u32 = 9013;

/// EtfState offsets (verified via offset_of! probe against the real
/// struct — 520 bytes total):
///   0    discriminator [u8;8]   = "etfstat2"
///   8    authority [u8;32]
///   40   etf_mint  [u8;32]
///   72   token_count u8
///   73   token_mints [[u8;32];5]
///   233  token_vaults [[u8;32];5]
///   394  weights_bps [u16;5]
///   408  total_supply u64
///   416  treasury [u8;32]
///   448  fee_bps u16
///   450  paused u8
///   451  bump u8
///   452  name [u8;32]
///   484  ticker [u8;16]
///   504  created_at_slot u64
///   512  _padding [u8;4]
#[allow(clippy::too_many_arguments)]
fn build_etf_state(
    authority: &Address,
    etf_mint: &Address,
    token_count: u8,
    token_mints: &[Address],
    token_vaults: &[Address],
    weights_bps: &[u16],
    total_supply: u64,
    treasury: &Address,
    bump: u8,
    name: &[u8],
) -> Vec<u8> {
    let mut d = vec![0u8; 520];
    d[0..8].copy_from_slice(b"etfstat2");
    d[8..40].copy_from_slice(authority.as_ref());
    d[40..72].copy_from_slice(etf_mint.as_ref());
    d[72] = token_count;
    for i in 0..token_count as usize {
        d[73 + i * 32..73 + (i + 1) * 32].copy_from_slice(token_mints[i].as_ref());
        d[233 + i * 32..233 + (i + 1) * 32].copy_from_slice(token_vaults[i].as_ref());
        d[394 + i * 2..394 + (i + 1) * 2].copy_from_slice(&weights_bps[i].to_le_bytes());
    }
    d[408..416].copy_from_slice(&total_supply.to_le_bytes());
    d[416..448].copy_from_slice(treasury.as_ref());
    d[448..450].copy_from_slice(&30u16.to_le_bytes()); // fee_bps = 30
    d[450] = 0; // paused
    d[451] = bump;
    d[452..452 + name.len()].copy_from_slice(name);
    d[484..484 + 2].copy_from_slice(b"AX");
    d
}

/// Build an SPL mint account blob (82 bytes) with a specific
/// `mint_authority`.
fn build_mint_with_authority(mint_authority: &Address, decimals: u8) -> Vec<u8> {
    let mut d = vec![0u8; 82];
    d[0..4].copy_from_slice(&1u32.to_le_bytes()); // COption::Some tag
    d[4..36].copy_from_slice(mint_authority.as_ref());
    d[44] = decimals;
    d[45] = 1; // is_initialized
    d
}

struct DepositFixture {
    svm: LiteSVM,
    payer: Keypair,
    etf_state: Address,
    etf_mint: Address,
    treasury: Address,
    basket_mints: Vec<Address>,
    vaults: Vec<Address>,
    user_basket_atas: Vec<Address>,
    user_etf_ata: Address,
    treasury_etf_ata: Address,
    name: Vec<u8>,
}

fn seed_deposit(n: usize, paused: bool, total_supply: u64) -> Option<DepositFixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_VAULT_SO).exists() {
        eprintln!("SKIP: axis_vault.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_vault_id(), AXIS_VAULT_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let name = b"TESTETF".to_vec();

    let (etf_state, bump) = Address::find_program_address(
        &[b"etf", payer.pubkey().as_ref(), &name],
        &axis_vault_id(),
    );

    // ETF mint — owned by Token Program, mint_authority = etf_state PDA.
    let etf_mint = Address::new_unique();
    svm.set_account(
        etf_mint,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: build_mint_with_authority(&etf_state, 6),
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let treasury = Address::new_unique();

    // Basket mints + pool-PDA-owned vaults + user ATAs for each.
    let mut basket_mints = Vec::with_capacity(n);
    let mut vaults = Vec::with_capacity(n);
    let mut user_basket_atas = Vec::with_capacity(n);
    for _ in 0..n {
        let mint = Address::new_unique();
        create_mint(&mut svm, mint, &payer.pubkey(), 6);
        basket_mints.push(mint);

        let vault = Address::new_unique();
        // Seed vault with a sensible balance so per-vault mint math
        // runs cleanly (divide-by-zero otherwise).
        create_token_account(&mut svm, vault, &mint, &etf_state, 1_000_000);
        vaults.push(vault);

        let user = Address::new_unique();
        create_token_account(&mut svm, user, &mint, &payer.pubkey(), 1_000_000_000);
        user_basket_atas.push(user);
    }

    // User + treasury ETF ATAs.
    let user_etf_ata = Address::new_unique();
    create_token_account(&mut svm, user_etf_ata, &etf_mint, &payer.pubkey(), 0);
    let treasury_etf_ata = Address::new_unique();
    create_token_account(&mut svm, treasury_etf_ata, &etf_mint, &treasury, 0);

    // EtfState.
    let weights_bps: Vec<u16> = (0..n).map(|_| (10_000 / n as u16)).collect();
    let mut weights = weights_bps.clone();
    // Fix residual so sum = 10_000
    let sum: u16 = weights.iter().sum();
    if sum != 10_000 {
        *weights.last_mut().unwrap() += 10_000 - sum;
    }
    let mut data = build_etf_state(
        &payer.pubkey(), &etf_mint, n as u8,
        &basket_mints, &vaults, &weights,
        total_supply, &treasury, bump, &name,
    );
    if paused {
        data[450] = 1;
    }

    svm.set_account(
        etf_state,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data,
            owner: axis_vault_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    Some(DepositFixture {
        svm, payer, etf_state, etf_mint, treasury,
        basket_mints, vaults, user_basket_atas,
        user_etf_ata, treasury_etf_ata, name,
    })
}

#[allow(clippy::too_many_arguments)]
fn deposit_ix(
    depositor: Address,
    etf_state: Address,
    etf_mint: Address,
    user_etf_ata: Address,
    treasury_etf_ata: Address,
    user_basket_atas: &[Address],
    vaults: &[Address],
    name: &[u8],
    amount: u64,
) -> Instruction {
    let mut data = vec![1u8]; // Deposit
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_mint_out
    data.push(name.len() as u8);
    data.extend_from_slice(name);

    let mut accts = vec![
        AccountMeta::new(depositor, true),
        AccountMeta::new(etf_state, false),
        AccountMeta::new(etf_mint, false),
        AccountMeta::new(user_etf_ata, false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new(treasury_etf_ata, false),
    ];
    for a in user_basket_atas {
        accts.push(AccountMeta::new(*a, false));
    }
    for v in vaults {
        accts.push(AccountMeta::new(*v, false));
    }

    Instruction { program_id: axis_vault_id(), accounts: accts, data }
}

fn send_tx(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        svm.latest_blockhash(),
    );
    match svm.send_transaction(tx) {
        Ok(meta) => Ok(meta.compute_units_consumed),
        Err(e) => {
            let mut msg = format!("{:?}", e.err);
            for log in &e.meta.logs {
                msg.push_str(&format!("\n  {}", log));
            }
            Err(msg)
        }
    }
}

#[test]
fn deposit_rejects_wrong_etf_mint() {
    require_fixture!(AXIS_VAULT_SO);
    let DepositFixture {
        mut svm, payer, etf_state, etf_mint: _, treasury: _,
        basket_mints: _, vaults, user_basket_atas,
        user_etf_ata, treasury_etf_ata, name,
    } = match seed_deposit(3, false, 0) {
        Some(f) => f, None => return,
    };

    // Pass a rogue etf_mint — real SPL mint but not the one stored
    // on the EtfState. Pre-fix this would have let a depositor mint
    // against an attacker-controlled mint.
    let rogue_mint = Address::new_unique();
    svm.set_account(
        rogue_mint,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: build_mint_with_authority(&payer.pubkey(), 6),
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    ).unwrap();

    let err = send_tx(
        &mut svm,
        deposit_ix(
            payer.pubkey(), etf_state, rogue_mint,
            user_etf_ata, treasury_etf_ata,
            &user_basket_atas, &vaults, &name, 10_000_000,
        ),
        &payer,
    )
    .err()
    .expect("wrong etf_mint should reject");
    assert_custom_err(&err, ERR_MINT_MISMATCH, "deposit wrong mint");
}

#[test]
fn deposit_rejects_wrong_vault() {
    require_fixture!(AXIS_VAULT_SO);
    let DepositFixture {
        mut svm, payer, etf_state, etf_mint, treasury: _,
        basket_mints, mut vaults, user_basket_atas,
        user_etf_ata, treasury_etf_ata, name,
    } = match seed_deposit(3, false, 0) {
        Some(f) => f, None => return,
    };

    // Replace vaults[0] with a rogue token account of the correct
    // mint but not the program-registered vault.
    let rogue_vault = Address::new_unique();
    create_token_account(&mut svm, rogue_vault, &basket_mints[0], &etf_state, 0);
    vaults[0] = rogue_vault;

    let err = send_tx(
        &mut svm,
        deposit_ix(
            payer.pubkey(), etf_state, etf_mint,
            user_etf_ata, treasury_etf_ata,
            &user_basket_atas, &vaults, &name, 10_000_000,
        ),
        &payer,
    )
    .err()
    .expect("wrong vault should reject");
    assert_custom_err(&err, ERR_VAULT_MISMATCH, "deposit wrong vault");
}

#[test]
fn create_etf_rejects_invalid_utf8_name() {
    require_fixture!(AXIS_VAULT_SO);
    // Use a name that's not valid UTF-8 (stray 0xFF byte).
    let bad_name = &[0xFFu8, 0xFE, 0xFD];
    let Fixture { mut svm, payer, etf_state, etf_mint, treasury, basket_mints, basket_vaults } =
        match seed(3, bad_name) { Some(f) => f, None => return };

    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_vault_id(),
            accounts: create_etf_accounts(
                payer.pubkey(), etf_state, etf_mint, treasury,
                &basket_mints, &basket_vaults,
            ),
            data: create_etf_data(3, &[3334, 3333, 3333], b"AX", bad_name),
        },
        &payer,
    )
    .err()
    .expect("non-UTF-8 name should reject");
    assert_custom_err(&err, ERR_INVALID_NAME, "non-UTF-8 name");
}
