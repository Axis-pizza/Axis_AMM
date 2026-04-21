//! axis-g3m regression coverage (issue #33).
//!
//! Covers kidney's scenario-table rows for axis-g3m that were untested:
//!   - Swap slippage exceeded           → SlippageExceeded (7004)
//!   - Swap zero input                  → ZeroAmount       (7003)
//!   - Swap same in+out index           → InvalidTokenIndex (7007)
//!   - Swap on paused pool              → PoolPaused       (7010)
//!   - Rebalance during cooldown        → CooldownActive   (7009)
//!   - Rebalance wrong authority        → Unauthorized     (7020)
//!   - InitializePool invalid token_count → InvalidTokenCount (7001)
//!   - InitializePool weights != 10_000  → WeightsMismatch (7002)
//!   - InitializePool duplicate init     → AlreadyInitialized (7015)
//!
//! The setup path drives a real `InitializePool` transaction so the
//! pool account lives in exactly the shape pinocchio + axis-g3m expect
//! (correct layout, correct realloc headroom). For rejection scenarios
//! we mutate the committed state post-init (flip the paused byte,
//! warp the clock) before issuing the instruction under test.

use ab_integration_tests::helpers::{svm_setup::*, token_factory::*};
use ab_integration_tests::require_fixture;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

// G3mError codes (mirror of contracts/axis-g3m/src/error.rs)
const ERR_INVALID_TOKEN_COUNT: u32 = 7001;
const ERR_WEIGHTS_MISMATCH: u32 = 7002;
const ERR_ZERO_AMOUNT: u32 = 7003;
const ERR_SLIPPAGE_EXCEEDED: u32 = 7004;
const ERR_INVALID_TOKEN_INDEX: u32 = 7007;
const ERR_COOLDOWN_ACTIVE: u32 = 7009;
const ERR_POOL_PAUSED: u32 = 7010;
const ERR_ALREADY_INITIALIZED: u32 = 7015;
const ERR_UNAUTHORIZED: u32 = 7020;

// ─── Instruction builders ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn g3m_init_ix(
    program: Address,
    authority: Address,
    pool_pda: Address,
    user_tokens: &[Address],
    vaults: &[Address],
    tc: u8,
    fee_bps: u16,
    drift_bps: u16,
    cooldown: u64,
    weights: &[u16],
    reserves: &[u64],
) -> Instruction {
    let mut data = vec![0u8];
    data.push(tc);
    data.extend_from_slice(&fee_bps.to_le_bytes());
    data.extend_from_slice(&drift_bps.to_le_bytes());
    data.extend_from_slice(&cooldown.to_le_bytes());
    for w in weights {
        data.extend_from_slice(&w.to_le_bytes());
    }
    for r in reserves {
        data.extend_from_slice(&r.to_le_bytes());
    }

    let mut accounts = vec![
        AccountMeta::new(authority, true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
    ];
    for t in user_tokens {
        accounts.push(AccountMeta::new(*t, false));
    }
    for v in vaults {
        accounts.push(AccountMeta::new(*v, false));
    }

    Instruction { program_id: program, accounts, data }
}

#[allow(clippy::too_many_arguments)]
fn g3m_swap_ix(
    program: Address,
    authority: Address,
    pool_pda: Address,
    user_in: Address,
    user_out: Address,
    vault_in: Address,
    vault_out: Address,
    in_idx: u8,
    out_idx: u8,
    amount_in: u64,
    min_out: u64,
) -> Instruction {
    let mut data = vec![1u8];
    data.push(in_idx);
    data.push(out_idx);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&min_out.to_le_bytes());
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(user_in, false),
            AccountMeta::new(user_out, false),
            AccountMeta::new(vault_in, false),
            AccountMeta::new(vault_out, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data,
    }
}

fn g3m_rebalance_ix(
    program: Address,
    authority: Address,
    pool_pda: Address,
    reserves: &[u64],
) -> Instruction {
    let mut data = vec![3u8];
    for r in reserves {
        data.extend_from_slice(&r.to_le_bytes());
    }
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(pool_pda, false),
        ],
        data,
    }
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

// ─── Fixture: real init path ────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    vaults: [Address; 2],
    user_tokens: [Address; 2],
}

/// Uses `create_token_account` (not padded) to match the pattern in
/// `ab_comparison.rs::test_g3m_init_swap_checkdrift` — the token-program
/// Transfer path is happy with unpadded token accounts, and the swap
/// CPI in axis-g3m does not trip realloc on a freshly init'd pool.
fn init_pool(cooldown: u64) -> Option<Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_G3M_SO).exists() {
        eprintln!("SKIP: axis_g3m.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_g3m_id(), AXIS_G3M_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let mints = [Address::new_unique(), Address::new_unique()];
    for &m in &mints {
        create_mint(&mut svm, m, &payer.pubkey(), 6);
    }

    let (pool, _bump) = Address::find_program_address(
        &[b"g3m_pool", payer.pubkey().as_ref()],
        &axis_g3m_id(),
    );

    let vaults = [Address::new_unique(), Address::new_unique()];
    let user_tokens = [Address::new_unique(), Address::new_unique()];
    for i in 0..2 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 0);
        create_token_account(&mut svm, user_tokens[i], &mints[i], &payer.pubkey(), 50_000_000);
    }

    send(
        &mut svm,
        g3m_init_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &user_tokens,
            &vaults,
            2,
            100,        // fee_rate_bps
            500,        // drift_threshold_bps
            cooldown,
            &[5000, 5000],
            &[10_000_000, 10_000_000],
        ),
        &payer,
    )
    .expect("init setup");

    Some(Fixture { svm, payer, pool, vaults, user_tokens })
}

/// Toggle `paused` on a post-init pool. G3mPoolState has 8-byte
/// alignment and ends with `paused(u8) + bump(u8) + _padding[4]` after
/// a u64-aligned `rebalance_cooldown` — but repr(C) also inserts u64
/// alignment padding before `last_rebalance_slot`. Verified via an
/// `offset_of!` probe against the actual struct: total size = 464,
/// `paused` sits at offset 456 (== `len - 8`).
fn set_paused(svm: &mut LiteSVM, pool: Address, paused: bool) {
    let mut acc = svm.get_account(&pool).expect("pool exists");
    let n = acc.data.len();
    acc.data[n - 8] = if paused { 1 } else { 0 };
    svm.set_account(pool, acc).unwrap();
}

fn warp(svm: &mut LiteSVM, slot: u64) {
    svm.set_sysvar(&Clock {
        slot,
        epoch_start_timestamp: 0,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: slot as i64,
    });
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn swap_rejects_when_paused() {
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens } =
        match init_pool(0) { Some(f) => f, None => return };

    set_paused(&mut svm, pool, true);

    let err = send(
        &mut svm,
        g3m_swap_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            user_tokens[0],
            user_tokens[1],
            vaults[0],
            vaults[1],
            0,
            1,
            1_000,
            0,
        ),
        &payer,
    )
    .err()
    .expect("paused swap should reject");
    assert_custom_err(&err, ERR_POOL_PAUSED, "paused swap");
}

#[test]
fn swap_rejects_zero_input() {
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens } =
        match init_pool(0) { Some(f) => f, None => return };

    let err = send(
        &mut svm,
        g3m_swap_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            user_tokens[0],
            user_tokens[1],
            vaults[0],
            vaults[1],
            0,
            1,
            0,           // zero amount_in
            0,
        ),
        &payer,
    )
    .err()
    .expect("zero-input swap should reject");
    assert_custom_err(&err, ERR_ZERO_AMOUNT, "zero amount");
}

#[test]
fn swap_rejects_same_in_out_index() {
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens } =
        match init_pool(0) { Some(f) => f, None => return };

    let err = send(
        &mut svm,
        g3m_swap_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            user_tokens[0],
            user_tokens[0],
            vaults[0],
            vaults[0],
            0,
            0,                 // same index
            1_000,
            0,
        ),
        &payer,
    )
    .err()
    .expect("same-index swap should reject");
    assert_custom_err(&err, ERR_INVALID_TOKEN_INDEX, "same in/out index");
}

#[test]
fn swap_rejects_slippage_exceeded() {
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens } =
        match init_pool(0) { Some(f) => f, None => return };

    // 1_000 in against 10M / 10M reserves → out ≈ 990 after fees.
    // Setting min_out = 10_000_000 ensures slippage fires.
    let err = send(
        &mut svm,
        g3m_swap_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            user_tokens[0],
            user_tokens[1],
            vaults[0],
            vaults[1],
            0,
            1,
            1_000,
            10_000_000,
        ),
        &payer,
    )
    .err()
    .expect("unsatisfiable min_out should reject");
    assert_custom_err(&err, ERR_SLIPPAGE_EXCEEDED, "slippage");
}

#[test]
fn rebalance_rejects_during_cooldown() {
    require_fixture!(AXIS_G3M_SO);
    // cooldown = 10_000 slots. init's last_rebalance_slot = current_slot.
    // warp only a few slots forward → still well inside the cooldown.
    let Fixture { mut svm, payer, pool, .. } =
        match init_pool(10_000) { Some(f) => f, None => return };
    warp(&mut svm, 50);

    let err = send(
        &mut svm,
        g3m_rebalance_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &[10_000_000, 10_000_000],
        ),
        &payer,
    )
    .err()
    .expect("rebalance during cooldown should reject");
    assert_custom_err(&err, ERR_COOLDOWN_ACTIVE, "cooldown");
}

#[test]
fn rebalance_rejects_wrong_authority() {
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, pool, .. } =
        match init_pool(0) { Some(f) => f, None => return };

    let intruder = Keypair::new();
    svm.airdrop(&intruder.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send(
        &mut svm,
        g3m_rebalance_ix(
            axis_g3m_id(),
            intruder.pubkey(),
            pool,
            &[10_000_000, 10_000_000],
        ),
        &intruder,
    )
    .err()
    .expect("rebalance from non-authority should reject");
    assert_custom_err(&err, ERR_UNAUTHORIZED, "wrong authority");
}

// ─── Init edge cases ────────────────────────────────────────────────────

fn fresh_init_svm() -> Option<(LiteSVM, Keypair)> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_G3M_SO).exists() {
        eprintln!("SKIP: axis_g3m.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_g3m_id(), AXIS_G3M_SO).ok()?;
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
    Some((svm, payer))
}

fn build_init_accounts(svm: &mut LiteSVM, payer: &Keypair, n: usize) -> (Address, Vec<Address>, Vec<Address>) {
    let (pool, _) = Address::find_program_address(
        &[b"g3m_pool", payer.pubkey().as_ref()],
        &axis_g3m_id(),
    );
    let mut user_tokens = Vec::with_capacity(n);
    let mut vaults = Vec::with_capacity(n);
    for _ in 0..n {
        let mint = Address::new_unique();
        create_mint(svm, mint, &payer.pubkey(), 6);
        let u = Address::new_unique();
        let v = Address::new_unique();
        create_token_account(svm, u, &mint, &payer.pubkey(), 50_000_000);
        create_token_account(svm, v, &mint, &pool, 0);
        user_tokens.push(u);
        vaults.push(v);
    }
    (pool, user_tokens, vaults)
}

#[test]
fn init_rejects_invalid_token_count() {
    require_fixture!(AXIS_G3M_SO);
    let (mut svm, payer) = match fresh_init_svm() { Some(p) => p, None => return };
    let (pool, user_tokens, vaults) = build_init_accounts(&mut svm, &payer, 1);

    let err = send(
        &mut svm,
        g3m_init_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &user_tokens,
            &vaults,
            1, // below min (2)
            30,
            500,
            0,
            &[10_000],
            &[10_000_000],
        ),
        &payer,
    )
    .err()
    .expect("token_count=1 should reject");
    assert_custom_err(&err, ERR_INVALID_TOKEN_COUNT, "tc too small");
}

#[test]
fn init_rejects_weights_not_10_000() {
    require_fixture!(AXIS_G3M_SO);
    let (mut svm, payer) = match fresh_init_svm() { Some(p) => p, None => return };
    let (pool, user_tokens, vaults) = build_init_accounts(&mut svm, &payer, 2);

    let err = send(
        &mut svm,
        g3m_init_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &user_tokens,
            &vaults,
            2,
            30,
            500,
            0,
            &[5000, 4999], // sums to 9999
            &[10_000_000, 10_000_000],
        ),
        &payer,
    )
    .err()
    .expect("weights != 10_000 should reject");
    assert_custom_err(&err, ERR_WEIGHTS_MISMATCH, "weights mismatch");
}

#[test]
fn init_rejects_duplicate() {
    require_fixture!(AXIS_G3M_SO);
    let (mut svm, payer) = match fresh_init_svm() { Some(p) => p, None => return };
    let (pool, user_tokens, vaults) = build_init_accounts(&mut svm, &payer, 2);

    // First init — happy path.
    send(
        &mut svm,
        g3m_init_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &user_tokens,
            &vaults,
            2,
            30,
            500,
            0,
            &[5000, 5000],
            &[10_000_000, 10_000_000],
        ),
        &payer,
    )
    .expect("first init should succeed");

    svm.expire_blockhash();

    // Second init — PR #48's explicit re-init guard fires.
    let err = send(
        &mut svm,
        g3m_init_ix(
            axis_g3m_id(),
            payer.pubkey(),
            pool,
            &user_tokens,
            &vaults,
            2,
            30,
            500,
            0,
            &[5000, 5000],
            &[10_000_000, 10_000_000],
        ),
        &payer,
    )
    .err()
    .expect("duplicate init should reject");
    assert_custom_err(&err, ERR_ALREADY_INITIALIZED, "duplicate init");
}

// ─── Rebalance >50% attestation reserve-change cap ─────────────────────

const ERR_RESERVE_CHANGE_EXCEEDED: u32 = 7019;
const ERR_ATTESTATION_REQUIRES_JUPITER: u32 = 7022;

#[test]
fn rebalance_attestation_rejects_over_50pct_reserve_change() {
    require_fixture!(AXIS_G3M_SO);
    // Attestation mode requires Jupiter V6 loaded in the SVM — skip
    // when the fork fixture isn't present.
    if !std::path::Path::new(JUPITER_V6_SO).exists() {
        eprintln!("SKIP: jupiter_v6.so fixture missing");
        return;
    }

    let Fixture { mut svm, payer, pool, .. } =
        match init_pool(0) { Some(f) => f, None => return };

    svm.add_program_from_file(jupiter_id(), JUPITER_V6_SO).unwrap();

    // Force the pool into a drifted state so `needs_rebalance()` fires,
    // then drop the drift threshold to 1 bp so any drift qualifies.
    // Layout offsets (verified via offset_of! probe):
    //   reserves[0]             @ 376  (u64)
    //   drift_threshold_bps     @ 434  (u16)
    //   max_invariant_drift_bps @ 436  (u16)
    {
        let mut acc = svm.get_account(&pool).unwrap();
        acc.data[376..384].copy_from_slice(&9_000_000u64.to_le_bytes()); // reserve_0 → 9M
        acc.data[434..436].copy_from_slice(&1u16.to_le_bytes());         // drift_threshold = 1 bp
        acc.data[436..438].copy_from_slice(&10_000u16.to_le_bytes());    // max invariant drift = 100%
        svm.set_account(pool, acc).unwrap();
    }

    // Attestation-mode Rebalance: authority + pool + jupiter_program
    // (exactly 3 accounts — no vault slots). Reserve 0 drops from 9M
    // to 3M (~66.7% change), well past the 5000-bp (50%) cap.
    let mut data = vec![3u8];
    data.extend_from_slice(&3_000_000u64.to_le_bytes());
    data.extend_from_slice(&10_000_000u64.to_le_bytes());

    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_g3m_id(),
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(pool, false),
                AccountMeta::new_readonly(jupiter_id(), false),
            ],
            data,
        },
        &payer,
    )
    .err()
    .expect("over-50% attestation reserve change should reject");
    assert_custom_err(&err, ERR_RESERVE_CHANGE_EXCEEDED, ">50% change");
}

#[test]
fn rebalance_attestation_rejects_missing_jupiter() {
    // PR #48's #33 hardening: attestation mode now requires the
    // Jupiter V6 program account as an explicit opt-in. Sending
    // attestation-mode accounts without Jupiter should return
    // AttestationRequiresJupiter.
    //
    // The Jupiter check happens after `needs_rebalance()`, so the pool
    // must first be in a drifted state; otherwise DriftBelowThreshold
    // fires first.
    require_fixture!(AXIS_G3M_SO);
    let Fixture { mut svm, payer, pool, .. } =
        match init_pool(0) { Some(f) => f, None => return };

    // Force drift — same byte-level tweak as the >50% test.
    {
        let mut acc = svm.get_account(&pool).unwrap();
        acc.data[376..384].copy_from_slice(&9_000_000u64.to_le_bytes());
        acc.data[434..436].copy_from_slice(&1u16.to_le_bytes());
        svm.set_account(pool, acc).unwrap();
    }

    let mut data = vec![3u8];
    data.extend_from_slice(&10_000_000u64.to_le_bytes());
    data.extend_from_slice(&10_000_000u64.to_le_bytes());

    let err = send(
        &mut svm,
        Instruction {
            program_id: axis_g3m_id(),
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(pool, false),
                // no jupiter_program — attestation mode detected but
                // Jupiter opt-in missing.
            ],
            data,
        },
        &payer,
    )
    .err()
    .expect("missing jupiter should reject");
    assert_custom_err(
        &err,
        ERR_ATTESTATION_REQUIRES_JUPITER,
        "attestation requires jupiter",
    );
}
