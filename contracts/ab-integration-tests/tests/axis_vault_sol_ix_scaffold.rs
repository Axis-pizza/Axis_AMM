//! axis-vault DepositSol / WithdrawSol scaffolding tests (#36, PR #58).
//!
//! These tests lock down the scaffolding behavior so the dispatcher +
//! parameter validation stay correct while the Jupiter CPI body is
//! being designed in follow-up. Each case hits one of the two early
//! validation branches:
//!
//!   - sol_in / burn_amount == 0 → ZeroDeposit (9004)
//!   - leg_count == 0 or > 3    → BasketTooLargeForOnchainSol (9025)
//!   - otherwise                → NotYetImplemented (9024)
//!
//! When the real implementation lands, the `NotYetImplemented` branch
//! goes away and the happy-path tests move into the axis_vault_coverage
//! sibling file.

use ab_integration_tests::helpers::{svm_setup::*, token_factory::*};
use ab_integration_tests::require_fixture;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

const ERR_ZERO_DEPOSIT: u32 = 9004;
const ERR_NOT_YET_IMPLEMENTED: u32 = 9024;
const ERR_BASKET_TOO_LARGE: u32 = 9025;

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

fn bootstrap_svm() -> Option<(LiteSVM, Keypair)> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_VAULT_SO).exists() {
        eprintln!("SKIP: axis_vault.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_vault_id(), AXIS_VAULT_SO).ok()?;
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
    Some((svm, payer))
}

/// Build a placeholder DepositSol / WithdrawSol tx. The scaffolding
/// rejects on parameter-validation before touching accounts, so fresh
/// unique addresses are enough padding. 18 bytes = sol_in + min_out +
/// name_len(0) + leg_count.
fn make_data(disc: u8, amount: u64, min_out: u64, leg_count: u8) -> Vec<u8> {
    let mut d = Vec::with_capacity(18);
    d.push(disc);
    d.extend_from_slice(&amount.to_le_bytes());
    d.extend_from_slice(&min_out.to_le_bytes());
    d.push(0u8); // name_len = 0
    d.push(leg_count);
    d
}

fn make_accounts(payer: &Keypair) -> Vec<AccountMeta> {
    // Scaffolded body doesn't dereference any of these — but the ix
    // needs a non-trivial account vec so the runtime accepts the tx.
    vec![
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new(Address::new_unique(), false),
    ]
}

// ─── DepositSol ────────────────────────────────────────────────────────

#[test]
fn deposit_sol_rejects_zero_amount() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(5, 0, 0, 3) },
        &payer,
    ).err().expect("sol_in=0 must reject");
    assert_custom_err(&err, ERR_ZERO_DEPOSIT, "zero sol_in");
}

#[test]
fn deposit_sol_rejects_basket_too_large() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(5, 1_000_000, 0, 4) },
        &payer,
    ).err().expect("leg_count=4 must reject");
    assert_custom_err(&err, ERR_BASKET_TOO_LARGE, "4-leg deposit_sol");
}

#[test]
fn deposit_sol_placeholder_returns_not_yet_implemented() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    // sol_in > 0, leg_count within bounds → falls through parameter
    // validation and returns the placeholder.
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(5, 1_000_000, 0, 2) },
        &payer,
    ).err().expect("placeholder body must return NotYetImplemented");
    assert_custom_err(&err, ERR_NOT_YET_IMPLEMENTED, "deposit_sol placeholder");
}

// ─── WithdrawSol ───────────────────────────────────────────────────────

#[test]
fn withdraw_sol_rejects_zero_amount() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(6, 0, 0, 3) },
        &payer,
    ).err().expect("burn_amount=0 must reject");
    assert_custom_err(&err, ERR_ZERO_DEPOSIT, "zero burn_amount");
}

#[test]
fn withdraw_sol_rejects_basket_too_large() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(6, 500_000, 0, 5) },
        &payer,
    ).err().expect("leg_count=5 must reject");
    assert_custom_err(&err, ERR_BASKET_TOO_LARGE, "5-leg withdraw_sol");
}

#[test]
fn withdraw_sol_placeholder_returns_not_yet_implemented() {
    require_fixture!(AXIS_VAULT_SO);
    let (mut svm, payer) = match bootstrap_svm() { Some(x) => x, None => return };
    let err = send(
        &mut svm,
        Instruction { program_id: axis_vault_id(), accounts: make_accounts(&payer), data: make_data(6, 500_000, 0, 2) },
        &payer,
    ).err().expect("placeholder body must return NotYetImplemented");
    assert_custom_err(&err, ERR_NOT_YET_IMPLEMENTED, "withdraw_sol placeholder");
}
