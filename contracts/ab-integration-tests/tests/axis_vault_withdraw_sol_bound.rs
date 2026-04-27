//! axis-vault WithdrawSol per-leg input-side bound coverage.
//!
//! Closes the audit-blocker gap noted on the `withdraw_sol` review:
//! every Jupiter CPI is signed by the vault PDA, so `route_bytes`
//! alone determines how many tokens Jupiter pulls from `vault[i]`.
//! Without an input-side bound, a withdrawer could burn 1 ETF
//! lamport but encode `inAmount = vault_balance` and walk away with
//! the full vault as wSOL — the only output check
//! (`total_wsol_out >= min_sol_out`) is also attacker-supplied and
//! trivially satisfied with `min_sol_out = 0`.
//!
//! The fix snapshots `vault_pre_cpi[i]` before the per-leg CPI loop
//! and rejects with `ExcessVaultDrain` (9036) whenever the realized
//! consumption exceeds `per_vault_amount[i] = vault_balance *
//! effective_burn / total_supply`. The check is `<=` (not `==`) so
//! Jupiter slippage / partial fills still go through; the user
//! simply gets less wSOL out and the aggregate slippage gate
//! catches anything below their own threshold.
//!
//! These tests use a drain-only mock Jupiter loaded at the
//! canonical Jupiter V6 program ID. The mock takes
//! `[in_amount: u64 LE][out_amount: u64 LE]` directly from
//! instruction data and CPIs SPL Token Transfer, so the test can
//! prove the bound regardless of any AMM math the real Jupiter
//! would normally do. End-to-end coverage with real Jupiter routes
//! lives in the (in-progress) mainnet-fork harness.
//!
//! Source for the mock lives under
//! `contracts/ab-integration-tests/mock-jupiter/`. Rebuild with
//! `cargo build-sbf --manifest-path
//! contracts/ab-integration-tests/mock-jupiter/Cargo.toml` and
//! re-copy the resulting `.so` into the fixtures directory.

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

const ERR_JUPITER_CPI_NO_OUTPUT: u32 = 9030;
const ERR_EXCESS_VAULT_DRAIN: u32 = 9036;

/// Canonical wrapped-SOL mint, byte-encoded.
const WSOL_MINT_BYTES: [u8; 32] = [
    0x06, 0x9b, 0x88, 0x57, 0xfe, 0xab, 0x81, 0x84,
    0xfb, 0x68, 0x7f, 0x63, 0x46, 0x18, 0xc0, 0x35,
    0xda, 0xc4, 0x39, 0xdc, 0x1a, 0xeb, 0x3b, 0x55,
    0x98, 0xa0, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x01,
];

fn assert_custom_err(err: &str, code: u32, label: &str) {
    let hex = format!("0x{:x}", code);
    let custom = format!("Custom({})", code);
    assert!(
        err.contains(&hex) || err.contains(&custom),
        "{label}: expected {code} ({hex}), got: {err}"
    );
}

fn assert_not_custom_err(err: &str, code: u32, label: &str) {
    let hex = format!("0x{:x}", code);
    let custom = format!("Custom({})", code);
    assert!(
        !err.contains(&hex) && !err.contains(&custom),
        "{label}: bound check fired unexpectedly ({code}/{hex}). Full error: {err}"
    );
}

/// Build the v3 EtfState blob — same offsets as
/// `axis_vault_coverage::build_etf_state`, duplicated to keep this
/// file standalone.
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
    let mut d = vec![0u8; 536];
    d[0..8].copy_from_slice(b"etfstat3");
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
    d[484..486].copy_from_slice(b"AX");
    d[512..514].copy_from_slice(&300u16.to_le_bytes()); // max_fee_bps
    d
}

fn build_mint_with_authority(mint_authority: &Address, decimals: u8) -> Vec<u8> {
    let mut d = vec![0u8; 82];
    d[0..4].copy_from_slice(&1u32.to_le_bytes());
    d[4..36].copy_from_slice(mint_authority.as_ref());
    d[44] = decimals;
    d[45] = 1;
    d
}

struct WithdrawSolFixture {
    svm: LiteSVM,
    withdrawer: Keypair,
    /// Tx-level signer used as `vault[0]`'s SPL Token authority in
    /// these tests. axis-vault doesn't read the SPL Token authority
    /// field — it only checks `vault.owner() == TOKEN_PROGRAM_ID`
    /// and the key match against `etf.token_vaults[i]`. Substituting
    /// a regular keypair for the production etf_state PDA lets the
    /// mock Jupiter sign the drain Transfer at tx level, which keeps
    /// the test focused on the bound logic without dragging in the
    /// PDA-signer-propagation path. The bound check itself reads
    /// vault[i] balance deltas only, so authority substitution is
    /// invisible to the code under test.
    drain_authority: Keypair,
    etf_state: Address,
    etf_mint: Address,
    treasury: Address,
    treasury_etf_ata: Address,
    user_etf_ata: Address,
    wsol_ata: Address,
    wsol_mint: Address,
    vaults: [Address; 3],
    drain_sink: Address,
    name: Vec<u8>,
}

/// Seed a 3-leg ETF where only `vault[0]` carries balance. The
/// other two vaults have zero balance so `per_vault_amount[i] = 0`
/// for `i in {1, 2}` and the per-leg loop in `process_withdraw_sol`
/// short-circuits past them — only leg 0 runs through Jupiter, and
/// only leg 0 needs the bound check exercised.
///
/// Numbers picked so that with `burn_amount = 1_000`, fee_bps = 30,
/// and `vault[0] = 1_000_000`, total_supply = 1_000_000:
///
///     fee_amount       = 1_000 * 30 / 10_000 = 3
///     effective_burn   = 1_000 - 3           = 997
///     per_vault_amount = 1_000_000 * 997 / 1_000_000 = 997
fn seed() -> Option<WithdrawSolFixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_VAULT_SO).exists() {
        eprintln!("SKIP: axis_vault.so fixture missing");
        return None;
    }
    if !std::path::Path::new(MOCK_JUPITER_SO).exists() {
        eprintln!("SKIP: mock_jupiter.so fixture missing — rebuild via");
        eprintln!("  cargo build-sbf --manifest-path \\");
        eprintln!("    contracts/ab-integration-tests/mock-jupiter/Cargo.toml");
        return None;
    }
    svm.add_program_from_file(axis_vault_id(), AXIS_VAULT_SO).ok()?;
    // Mock Jupiter substitute, loaded at the canonical Jupiter V6
    // program ID so axis-vault's `JUPITER_PROGRAM_ID` check passes.
    svm.add_program_from_file(jupiter_id(), MOCK_JUPITER_SO).ok()?;

    let withdrawer = Keypair::new();
    svm.airdrop(&withdrawer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();
    let drain_authority = Keypair::new();
    svm.airdrop(&drain_authority.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let name = b"BOUND".to_vec();
    let (etf_state, bump) =
        Address::find_program_address(&[b"etf", withdrawer.pubkey().as_ref(), &name], &axis_vault_id());

    // ETF mint with mint_authority = etf_state PDA.
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

    // 3 basket mints + 3 vaults; only vault[0] carries balance.
    let basket_mints: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let vaults: [Address; 3] = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        create_mint(&mut svm, basket_mints[i], &withdrawer.pubkey(), 6);
    }
    // vault[0] uses `drain_authority` as the SPL Token authority so
    // the mock Jupiter can sign its Transfer with a tx-level keypair.
    // See WithdrawSolFixture.drain_authority docstring for why this
    // is a valid simplification for the bound test.
    create_token_account(&mut svm, vaults[0], &basket_mints[0], &drain_authority.pubkey(), 1_000_000);
    create_token_account(&mut svm, vaults[1], &basket_mints[1], &etf_state, 0);
    create_token_account(&mut svm, vaults[2], &basket_mints[2], &etf_state, 0);

    // Drain sink for the mock to dump tokens into.
    let drain_sink = Address::new_unique();
    create_token_account(&mut svm, drain_sink, &basket_mints[0], &withdrawer.pubkey(), 0);

    // wSOL mint + user wsol_ata.
    let wsol_mint = Address::from(WSOL_MINT_BYTES);
    create_mint(&mut svm, wsol_mint, &withdrawer.pubkey(), 9);
    let wsol_ata = Address::new_unique();
    create_token_account(&mut svm, wsol_ata, &wsol_mint, &withdrawer.pubkey(), 0);

    // Treasury + treasury_etf_ata. Treasury is a synthetic key; the
    // governance gate stays inert because PROTOCOL_TREASURY is still
    // the zero sentinel in this build.
    let treasury = Address::new_unique();
    let treasury_etf_ata = Address::new_unique();
    create_token_account(&mut svm, treasury_etf_ata, &etf_mint, &treasury, 0);

    // user_etf_ata holds the burn balance.
    let user_etf_ata = Address::new_unique();
    create_token_account(&mut svm, user_etf_ata, &etf_mint, &withdrawer.pubkey(), 1_000);

    // EtfState — total_supply = 1_000_000, fee_bps = 30 (from the
    // build helper), max_fee_bps = 300.
    let weights = [3334u16, 3333u16, 3333u16];
    let data = build_etf_state(
        &withdrawer.pubkey(),
        &etf_mint,
        3,
        &basket_mints,
        &vaults,
        &weights,
        1_000_000,
        &treasury,
        bump,
        &name,
    );
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

    Some(WithdrawSolFixture {
        svm,
        withdrawer,
        drain_authority,
        etf_state,
        etf_mint,
        treasury,
        treasury_etf_ata,
        user_etf_ata,
        wsol_ata,
        wsol_mint,
        vaults,
        drain_sink,
        name,
    })
}

/// Build the WithdrawSol ix data for a 3-leg ETF where only leg 0
/// carries a non-empty route. Legs 1 and 2 are declared with
/// `route_account_count = 0` and `route_len = 0` (they get skipped
/// in `process_withdraw_sol` because their `per_vault_amount = 0`,
/// but the parser still requires the per-leg header bytes).
fn withdraw_sol_data(
    burn_amount: u64,
    min_sol_out: u64,
    name: &[u8],
    leg0_in_amount: u64,
    leg0_out_amount: u64,
) -> Vec<u8> {
    let mut d = vec![6u8]; // disc = 6 (WithdrawSol)
    d.extend_from_slice(&burn_amount.to_le_bytes());
    d.extend_from_slice(&min_sol_out.to_le_bytes());
    d.push(name.len() as u8);
    d.extend_from_slice(name);
    d.push(3u8); // leg_count

    // Leg 0: route_account_count = 6, route_len = 16
    // (drain_authority signer, drain_sink, token_program,
    //  wsol_source = wsol_ata, wsol_source_auth = withdrawer,
    //  wsol_dest = wsol_ata)
    d.push(6u8);
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&leg0_in_amount.to_le_bytes());
    d.extend_from_slice(&leg0_out_amount.to_le_bytes());

    // Legs 1 & 2: empty (skipped in-program because per_vault_amount = 0).
    d.push(0u8);
    d.extend_from_slice(&0u32.to_le_bytes());
    d.push(0u8);
    d.extend_from_slice(&0u32.to_le_bytes());

    d
}

fn build_withdraw_sol_ix(f: &WithdrawSolFixture, in_amount: u64, out_amount: u64) -> Instruction {
    // jupiter_program slot is the canonical Jupiter V6 ID; the mock
    // is loaded there, and axis-vault's check at line 104 of
    // withdraw_sol.rs verifies the address — not the binary.
    let jupiter_program = jupiter_id();
    let system_program = system_program_id();

    let mut accounts = vec![
        AccountMeta::new(f.withdrawer.pubkey(), true),     // 0: withdrawer
        AccountMeta::new(f.etf_state, false),              // 1: etf_state PDA
        AccountMeta::new(f.etf_mint, false),               // 2: etf_mint
        AccountMeta::new(f.user_etf_ata, false),           // 3: withdrawer_etf_ata
        AccountMeta::new_readonly(token_program_id(), false), // 4
        AccountMeta::new(f.treasury_etf_ata, false),       // 5: treasury_etf_ata
        AccountMeta::new(f.wsol_ata, false),               // 6: wsol_ata
        AccountMeta::new_readonly(f.wsol_mint, false),     // 7: wsol_mint
        AccountMeta::new_readonly(jupiter_program, false), // 8: jupiter_program
        AccountMeta::new_readonly(system_program, false),  // 9
    ];
    // 10..13: vaults
    for v in &f.vaults {
        accounts.push(AccountMeta::new(*v, false));
    }
    // Leg 0 route accounts (cnt = 6):
    //   [0] source_authority — drain_authority keypair, signed at tx level.
    //       Used by the mock as the `authority` for the SPL Token
    //       Transfer that drains vault[0]. Substituted in for the
    //       etf_state PDA only for test ergonomics; the bound check
    //       under test reads vault[i] balance deltas only.
    //   [1] drain_sink
    //   [2] token_program — required for the mock's SPL Token CPI
    //   [3] wsol_source — reuse wsol_ata (mock skips when out_amount=0)
    //   [4] wsol_source_auth — reuse withdrawer keypair (signed at tx level)
    //   [5] wsol_destination — wsol_ata
    accounts.push(AccountMeta::new(f.drain_authority.pubkey(), true));
    accounts.push(AccountMeta::new(f.drain_sink, false));
    accounts.push(AccountMeta::new_readonly(token_program_id(), false));
    accounts.push(AccountMeta::new(f.wsol_ata, false));
    accounts.push(AccountMeta::new(f.withdrawer.pubkey(), false));
    accounts.push(AccountMeta::new(f.wsol_ata, false));

    Instruction {
        program_id: axis_vault_id(),
        accounts,
        data: withdraw_sol_data(1_000, 0, &f.name, in_amount, out_amount),
    }
}

fn send(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    extra_signer: &Keypair,
) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer, extra_signer],
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

// ─── Tests ─────────────────────────────────────────────────────────────

/// **The audit-blocker case.** A withdrawer burns 1_000 ETF tokens
/// (`per_vault_amount[0] = 997`) but encodes `in_amount = 998` in
/// the route bytes. The mock Jupiter happily drains 998 from
/// `vault[0]` because the program-signed PDA propagates as the
/// transfer authority. Without the input-side bound, axis-vault
/// would proceed to the wsol_post check (also user-controlled via
/// `min_sol_out = 0`) and let the over-drain settle.
///
/// With the fix, `process_withdraw_sol` rejects with
/// `ExcessVaultDrain` (9036) immediately after the per-leg loop.
#[test]
fn withdraw_sol_rejects_drain_above_burn_share() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed() {
        Some(x) => x,
        None => return,
    };

    // per_vault_amount[0] = 997, mock drains 998 → must reject.
    let ix = build_withdraw_sol_ix(&f, 998, 0);
    let err = send(&mut f.svm, ix, &f.withdrawer, &f.drain_authority).err().expect(
        "drain > per_vault_amount must reject — without the bound, vault drain is unconstrained",
    );
    assert_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "drain above burn-share");
}

/// **Even-larger drain.** Flexes the same code path as the prior
/// test but with the malicious `in_amount` set to the entire
/// vault balance (`1_000_000`), which is the realistic worst-case
/// of an attacker who knows the vault snapshot. Same expected
/// rejection: `ExcessVaultDrain`.
#[test]
fn withdraw_sol_rejects_full_vault_drain() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed() {
        Some(x) => x,
        None => return,
    };

    let ix = build_withdraw_sol_ix(&f, 1_000_000, 0);
    let err = send(&mut f.svm, ix, &f.withdrawer, &f.drain_authority)
        .err()
        .expect("draining the whole vault on a tiny burn must reject");
    assert_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "full vault drain");
}

/// **Negative test for over-rejection.** If `in_amount` matches the
/// burn share exactly, the bound check must NOT fire — otherwise
/// we'd brick legitimate withdrawals. The transaction still fails
/// downstream (with `JupiterCpiNoOutput` because the mock here
/// emits no wSOL), but specifically NOT with `ExcessVaultDrain`.
#[test]
fn withdraw_sol_at_burn_share_does_not_trigger_drain_error() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed() {
        Some(x) => x,
        None => return,
    };

    // per_vault_amount[0] = 997, mock drains exactly 997 — bound
    // passes. Mock leaves wsol_ata balance at 0, so
    // process_withdraw_sol bails on JupiterCpiNoOutput (9030),
    // not on ExcessVaultDrain (9036).
    let ix = build_withdraw_sol_ix(&f, 997, 0);
    let err = send(&mut f.svm, ix, &f.withdrawer, &f.drain_authority)
        .err()
        .expect("the mock emits 0 wSOL so the slippage gate trips");
    assert_not_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "drain == burn-share");
    assert_custom_err(&err, ERR_JUPITER_CPI_NO_OUTPUT, "expected wSOL slippage trip");
}

/// **Below-share consumption is also fine.** Jupiter slippage /
/// partial fills can leave less consumed than the burn-share allows;
/// that's a user-side cost, not a protocol-level violation. The
/// bound is `<=`, not `==`.
#[test]
fn withdraw_sol_below_burn_share_does_not_trigger_drain_error() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed() {
        Some(x) => x,
        None => return,
    };

    let ix = build_withdraw_sol_ix(&f, 500, 0);
    let err = send(&mut f.svm, ix, &f.withdrawer, &f.drain_authority)
        .err()
        .expect("slippage gate still trips because mock emits 0 wSOL");
    assert_not_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "drain < burn-share");
    assert_custom_err(&err, ERR_JUPITER_CPI_NO_OUTPUT, "expected wSOL slippage trip");
}

// Suppress unused-field warnings on fixture sub-keys we keep for
// future negative tests (e.g. wrong_treasury, frozen vault).
#[allow(dead_code)]
fn _fixture_field_uses(f: &WithdrawSolFixture) {
    let _ = (
        &f.etf_state,
        &f.etf_mint,
        &f.treasury,
        &f.treasury_etf_ata,
        &f.user_etf_ata,
        &f.wsol_ata,
        &f.wsol_mint,
        &f.vaults,
        &f.drain_sink,
        &f.name,
    );
}
