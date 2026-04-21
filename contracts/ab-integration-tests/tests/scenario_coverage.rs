//! Scenario-gap coverage (issue #33).
//!
//! Kidney's scenario table in #33 lists per-instruction rejection paths
//! that had no regression tests. This file covers the LiteSVM-suitable
//! rows for pfda-amm-3 — the ones where PRs #46 / #47 added a new
//! rejection and a later regression could silently remove it.
//!
//! Covered:
//!   - AddLiquidity wrong vault  → VaultMismatch (8025)      [PR #47]
//!   - AddLiquidity paused pool  → PoolPaused    (8018)      [PR #47]
//!   - SwapRequest paused pool   → PoolPaused    (8018)      [PR #47]
//!
//! The pattern mirrors close_delay.rs: pre-seed a pool + vaults + user
//! accounts via set_account, flip the relevant pool field, submit the
//! instruction, expect the named custom error code.

use ab_integration_tests::helpers::{account_builder::*, svm_setup::*, token_factory::*};
use ab_integration_tests::require_fixture;
use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

// Pfda3Error codes (mirror of contracts/pfda-amm-3/src/error.rs)
const ERR_POOL_PAUSED: u32 = 8018;
const ERR_VAULT_MISMATCH: u32 = 8025;

// ─── Instruction builders ───────────────────────────────────────────────

fn pfda3_add_liquidity_ix(
    program: Address,
    payer: Address,
    pool: Address,
    vaults: &[Address; 3],
    user_tokens: &[Address; 3],
    amounts: &[u64; 3],
) -> Instruction {
    let mut data = vec![4u8];
    for a in amounts {
        data.extend_from_slice(&a.to_le_bytes());
    }
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(pool, false),
            AccountMeta::new(vaults[0], false),
            AccountMeta::new(vaults[1], false),
            AccountMeta::new(vaults[2], false),
            AccountMeta::new(user_tokens[0], false),
            AccountMeta::new(user_tokens[1], false),
            AccountMeta::new(user_tokens[2], false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data,
    }
}

fn pfda3_swap_request_ix(
    program: Address,
    user: Address,
    pool: Address,
    queue: Address,
    ticket: Address,
    user_token: Address,
    vault: Address,
    in_idx: u8,
    amount_in: u64,
    out_idx: u8,
    min_out: u64,
) -> Instruction {
    let mut data = vec![1u8];
    data.push(in_idx);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.push(out_idx);
    data.extend_from_slice(&min_out.to_le_bytes());
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(user, true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new(queue, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(user_token, false),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(token_program_id(), false),
            AccountMeta::new_readonly(system_program_id(), false),
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

// ─── Seed ───────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    mints: [Address; 3],
    vaults: [Address; 3],
    user_tokens: [Address; 3],
}

fn seed_pool(paused: bool) -> Option<Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(PFDA_AMM_3_SO).exists() {
        eprintln!("SKIP: pfda_amm_3.so fixture missing");
        return None;
    }
    svm.add_program_from_file(pfda3_id(), PFDA_AMM_3_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mints = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for &m in &mints {
        create_mint(&mut svm, m, &payer.pubkey(), 6);
    }

    let (pool, pool_bump) = Address::find_program_address(
        &[
            b"pool3",
            mints[0].as_ref(),
            mints[1].as_ref(),
            mints[2].as_ref(),
        ],
        &pfda3_id(),
    );

    let vaults = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let user_tokens = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 1_000_000);
        create_token_account(&mut svm, user_tokens[i], &mints[i], &payer.pubkey(), 1_000_000_000);
    }

    let treasury = Address::new_unique();
    let mut pd = build_pfda3_pool_state(
        &mints,
        &vaults,
        &[1_000_000; 3],
        &[333_333, 333_333, 333_334],
        10,        // window_slots
        0,         // current_batch_id
        100,       // current_window_end
        &treasury,
        &payer.pubkey(),
        30,        // base_fee_bps
        pool_bump,
    );
    if paused {
        // PoolState3.paused offset = 332 (per account_builder.rs layout comment).
        pd[332] = 1;
    }

    svm.set_account(
        pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pd,
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    Some(Fixture {
        svm,
        payer,
        pool,
        mints,
        vaults,
        user_tokens,
    })
}

fn seed_batch_queue(svm: &mut LiteSVM, pool: Address, batch_id: u64, window_end: u64) -> Address {
    let (queue, qbump) = Address::find_program_address(
        &[b"queue3", pool.as_ref(), &batch_id.to_le_bytes()],
        &pfda3_id(),
    );
    let qd = build_batch_queue_3(&pool, batch_id, &[0; 3], window_end, qbump);
    svm.set_account(
        queue,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: qd,
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    queue
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn add_liquidity_rejects_wrong_vault() {
    require_fixture!(PFDA_AMM_3_SO);
    let Fixture {
        mut svm,
        payer,
        pool,
        mints,
        vaults,
        user_tokens,
    } = match seed_pool(/*paused=*/ false) {
        Some(f) => f,
        None => return,
    };

    // Swap in a vault that matches the correct mint but isn't the
    // program-registered vault for this pool. Pre-PR#47 this would have
    // silently transferred to the attacker account.
    let rogue_vault = Address::new_unique();
    create_token_account(&mut svm, rogue_vault, &mints[0], &pool, 0);

    let mut bad_vaults = vaults;
    bad_vaults[0] = rogue_vault;

    let err = send(
        &mut svm,
        pfda3_add_liquidity_ix(
            pfda3_id(),
            payer.pubkey(),
            pool,
            &bad_vaults,
            &user_tokens,
            &[100, 100, 100],
        ),
        &payer,
    )
    .err()
    .expect("AddLiquidity with wrong vault should reject");
    assert_custom_err(&err, ERR_VAULT_MISMATCH, "wrong-vault rejection");
}

#[test]
fn add_liquidity_rejects_when_paused() {
    require_fixture!(PFDA_AMM_3_SO);
    let Fixture {
        mut svm,
        payer,
        pool,
        mints: _,
        vaults,
        user_tokens,
    } = match seed_pool(/*paused=*/ true) {
        Some(f) => f,
        None => return,
    };

    let err = send(
        &mut svm,
        pfda3_add_liquidity_ix(
            pfda3_id(),
            payer.pubkey(),
            pool,
            &vaults,
            &user_tokens,
            &[100, 100, 100],
        ),
        &payer,
    )
    .err()
    .expect("AddLiquidity on paused pool should reject");
    assert_custom_err(&err, ERR_POOL_PAUSED, "paused-pool rejection");
}

#[test]
fn swap_request_rejects_when_paused() {
    require_fixture!(PFDA_AMM_3_SO);
    let Fixture {
        mut svm,
        payer,
        pool,
        mints: _,
        vaults,
        user_tokens,
    } = match seed_pool(/*paused=*/ true) {
        Some(f) => f,
        None => return,
    };

    // Seed the batch queue so the instruction can get past queue
    // validation and actually exercise the pool.paused branch.
    let queue = seed_batch_queue(&mut svm, pool, 0, 100);

    let (ticket, _) = Address::find_program_address(
        &[
            b"ticket3",
            pool.as_ref(),
            payer.pubkey().as_ref(),
            &0u64.to_le_bytes(),
        ],
        &pfda3_id(),
    );

    let err = send(
        &mut svm,
        pfda3_swap_request_ix(
            pfda3_id(),
            payer.pubkey(),
            pool,
            queue,
            ticket,
            user_tokens[0],
            vaults[0],
            0,
            100,
            1,
            0,
        ),
        &payer,
    )
    .err()
    .expect("SwapRequest on paused pool should reject");
    // PR #47 specifically changed this from InvalidDiscriminator to PoolPaused.
    assert_custom_err(&err, ERR_POOL_PAUSED, "paused-pool rejection");
}
