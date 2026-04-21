//! Close-delay success paths (issue #33).
//!
//! Kidney flagged that `CloseBatchHistory` and `CloseExpiredTicket` in
//! both pfda-amm and pfda-amm-3 had never had their success path
//! exercised — the 100-batch / 200-batch delays made it impractical to
//! drive through a real flow in unit tests.
//!
//! LiteSVM lets us sidestep that by pre-seeding account state via
//! `set_account`: fabricate a ClearedBatchHistory3 / UserOrderTicket3
//! at the canonical PDA, advance pool.current_batch_id past the delay,
//! and invoke the close instruction. The on-chain program logic is
//! exercised in full — only the intermediate ClearBatch / time-waiting
//! scaffolding is skipped.
//!
//! Covered here (pfda-amm-3):
//!   1. CloseBatchHistory success once 100 batches have elapsed.
//!   2. CloseBatchHistory rejection before the delay elapses.
//!   3. CloseExpiredTicket success once 200 batches have elapsed.
//!   4. CloseExpiredTicket rejection before the delay elapses.
//!
//! pfda-amm has the same instruction shape and delay constants; a
//! follow-up PR can clone this file against that program's account
//! layout.

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

// ─── Account fabrication ────────────────────────────────────────────────
// Sizes confirmed via `cargo test print_sizes -- --nocapture` in
// contracts/pfda-amm-3/src/lib.rs (both structs are 128 bytes).

fn build_cleared_batch_history_3(
    pool: &Address,
    batch_id: u64,
    fee_bps: u16,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 128];
    // Layout (repr(C), total 128 bytes):
    //   0:   discriminator [8]        = b"clrd3h\0\0"
    //   8:   pool          [32]
    //  40:   batch_id      u64
    //  48:   clearing_prices [u64; 3]  (Q32.32)
    //  72:   total_out     [u64; 3]
    //  96:   total_in      [u64; 3]
    // 120:   fee_bps       u16
    // 122:   is_cleared    bool
    // 123:   bump          u8
    // 124:   _padding      [u8; 4]
    d[0..8].copy_from_slice(b"clrd3h\0\0");
    d[8..40].copy_from_slice(pool.as_ref());
    d[40..48].copy_from_slice(&batch_id.to_le_bytes());
    // clearing_prices / total_out / total_in stay zero — CloseBatchHistory
    // doesn't read them.
    d[120..122].copy_from_slice(&fee_bps.to_le_bytes());
    d[122] = 1; // is_cleared
    d[123] = bump;
    d
}

fn build_user_order_ticket_3(
    owner: &Address,
    pool: &Address,
    batch_id: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 128];
    // Layout (repr(C), total 128 bytes):
    //   0:   discriminator [8] = b"usrord3\0"
    //   8:   owner         [32]
    //  40:   pool          [32]
    //  72:   batch_id      u64
    //  80:   amounts_in    [u64; 3]
    // 104:   out_token_idx u8
    // 105:   padding to u64 align (7)  ← verified via print_sizes
    // 112:   min_amount_out u64
    // 120:   is_claimed    bool
    // 121:   bump          u8
    // 122:   _padding      [u8; 5]  + trailing alignment pad = 6 bytes → 128
    d[0..8].copy_from_slice(b"usrord3\0");
    d[8..40].copy_from_slice(owner.as_ref());
    d[40..72].copy_from_slice(pool.as_ref());
    d[72..80].copy_from_slice(&batch_id.to_le_bytes());
    // amounts_in / out_token_idx / min_amount_out stay zero — CloseExpiredTicket
    // only reads owner + batch_id off the ticket.
    d[120] = 0; // is_claimed = false (so it's eligible to close)
    d[121] = bump;
    d
}

// ─── Instruction builders ───────────────────────────────────────────────

fn pfda3_close_batch_history_ix(
    program: Address,
    rent_recipient: Address,
    pool: Address,
    history: Address,
) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(rent_recipient, true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new(history, false),
        ],
        data: vec![7u8], // disc = CloseBatchHistory
    }
}

fn pfda3_close_expired_ticket_ix(
    program: Address,
    caller: Address,
    pool: Address,
    ticket: Address,
    rent_recipient: Address,
) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(caller, true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(rent_recipient, false),
        ],
        data: vec![8u8], // disc = CloseExpiredTicket
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

// ─── Test scaffolding ───────────────────────────────────────────────────

struct Seed {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
}

/// Build a LiteSVM with pfda-amm-3 loaded, pre-seed a pool and its three
/// vault token accounts, and return handles.
///
/// `current_batch_id` is written straight onto the pool so individual
/// tests can pick where in the batch timeline they want to land (e.g.
/// 150 for a history-eligible pool, 50 for a rejection case).
fn seed_pool(current_batch_id: u64) -> Option<Seed> {
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

    let (pool, bump) = Address::find_program_address(
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
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 1_000_000);
    }

    // Treasury distinct from authority so we can also catch TreasuryMismatch
    // regressions if close_batch_history ever gains that gate.
    let treasury = Address::new_unique();

    let pd = build_pfda3_pool_state(
        &mints,
        &vaults,
        &[1_000_000; 3],
        &[333_333, 333_333, 333_334],
        1,                       // window_slots
        current_batch_id,        // current_batch_id
        10,                      // current_window_end (doesn't matter for close)
        &treasury,
        &payer.pubkey(),         // authority — required for PR #50 rent_recipient gate
        30,                      // base_fee_bps
        bump,
    );
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

    Some(Seed { svm, payer, pool })
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn close_batch_history_success_after_delay() {
    require_fixture!(PFDA_AMM_3_SO);
    let Seed { mut svm, payer, pool } = match seed_pool(/*current_batch_id=*/ 150) {
        Some(s) => s,
        None => return,
    };

    let old_batch_id: u64 = 0;
    let (history, hbump) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &old_batch_id.to_le_bytes()],
        &pfda3_id(),
    );
    let rent = 1_000_000u64;
    svm.set_account(
        history,
        Account {
            lamports: rent,
            data: build_cleared_batch_history_3(&pool, old_batch_id, 30, hbump),
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let before = svm.get_balance(&payer.pubkey()).unwrap_or_else(|| {
        panic!("payer has no balance entry; airdrop may have failed")
    });
    send(
        &mut svm,
        pfda3_close_batch_history_ix(pfda3_id(), payer.pubkey(), pool, history),
        &payer,
    )
    .expect("CloseBatchHistory should succeed after delay");

    // History account drained, payer credited (net of tx fee).
    // After close the runtime may GC the zero-lamport account entirely
    // instead of leaving a 0-balance stub — accept either.
    assert_eq!(
        svm.get_balance(&history).unwrap_or(0),
        0,
        "history lamports should be zero after close"
    );
    let after = svm.get_balance(&payer.pubkey()).unwrap_or(0);
    assert!(
        after > before,
        "rent_recipient should gain lamports ({} -> {})",
        before,
        after
    );
}

#[test]
fn close_batch_history_rejects_before_delay() {
    require_fixture!(PFDA_AMM_3_SO);
    // current_batch_id = 50 → only 50 batches elapsed vs the 100-batch
    // CLOSE_DELAY. Should reject.
    let Seed { mut svm, payer, pool } = match seed_pool(/*current_batch_id=*/ 50) {
        Some(s) => s,
        None => return,
    };

    let old_batch_id: u64 = 0;
    let (history, hbump) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &old_batch_id.to_le_bytes()],
        &pfda3_id(),
    );
    svm.set_account(
        history,
        Account {
            lamports: 1_000_000,
            data: build_cleared_batch_history_3(&pool, old_batch_id, 30, hbump),
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let err = send(
        &mut svm,
        pfda3_close_batch_history_ix(pfda3_id(), payer.pubkey(), pool, history),
        &payer,
    )
    .err()
    .expect("CloseBatchHistory should reject pre-delay");
    // Pfda3Error::BatchWindowNotEnded = 8002 (0x1f42)
    assert!(
        err.contains("0x1f42") || err.contains("Custom(8002)") || err.contains("BatchWindowNotEnded"),
        "expected BatchWindowNotEnded, got: {err}"
    );
}

#[test]
fn close_expired_ticket_success_after_delay() {
    require_fixture!(PFDA_AMM_3_SO);
    // 250 batches elapsed > 200-batch expiry.
    let Seed { mut svm, payer, pool } = match seed_pool(/*current_batch_id=*/ 250) {
        Some(s) => s,
        None => return,
    };

    let ticket_batch_id: u64 = 0;
    let ticket_owner = payer.pubkey();
    let (ticket, tbump) = Address::find_program_address(
        &[
            b"ticket3",
            pool.as_ref(),
            ticket_owner.as_ref(),
            &ticket_batch_id.to_le_bytes(),
        ],
        &pfda3_id(),
    );
    let rent = 1_500_000u64;
    svm.set_account(
        ticket,
        Account {
            lamports: rent,
            data: build_user_order_ticket_3(&ticket_owner, &pool, ticket_batch_id, tbump),
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let before = svm.get_balance(&ticket_owner).unwrap_or_else(|| {
        panic!("ticket owner has no balance entry; airdrop may have failed")
    });
    send(
        &mut svm,
        pfda3_close_expired_ticket_ix(pfda3_id(), payer.pubkey(), pool, ticket, ticket_owner),
        &payer,
    )
    .expect("CloseExpiredTicket should succeed after delay");

    assert_eq!(
        svm.get_balance(&ticket).unwrap_or(0),
        0,
        "ticket lamports should be zero after close"
    );
    let after = svm.get_balance(&ticket_owner).unwrap_or(0);
    assert!(
        after > before,
        "ticket owner should gain lamports ({} -> {})",
        before,
        after
    );
}

#[test]
fn close_expired_ticket_rejects_before_delay() {
    require_fixture!(PFDA_AMM_3_SO);
    // 100 batches elapsed < 200-batch expiry.
    let Seed { mut svm, payer, pool } = match seed_pool(/*current_batch_id=*/ 100) {
        Some(s) => s,
        None => return,
    };

    let ticket_batch_id: u64 = 0;
    let ticket_owner = payer.pubkey();
    let (ticket, tbump) = Address::find_program_address(
        &[
            b"ticket3",
            pool.as_ref(),
            ticket_owner.as_ref(),
            &ticket_batch_id.to_le_bytes(),
        ],
        &pfda3_id(),
    );
    svm.set_account(
        ticket,
        Account {
            lamports: 1_500_000,
            data: build_user_order_ticket_3(&ticket_owner, &pool, ticket_batch_id, tbump),
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let err = send(
        &mut svm,
        pfda3_close_expired_ticket_ix(pfda3_id(), payer.pubkey(), pool, ticket, ticket_owner),
        &payer,
    )
    .err()
    .expect("CloseExpiredTicket should reject pre-delay");
    assert!(
        err.contains("0x1f42") || err.contains("Custom(8002)") || err.contains("BatchWindowNotEnded"),
        "expected BatchWindowNotEnded, got: {err}"
    );
}
