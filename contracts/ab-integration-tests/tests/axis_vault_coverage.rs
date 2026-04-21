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
