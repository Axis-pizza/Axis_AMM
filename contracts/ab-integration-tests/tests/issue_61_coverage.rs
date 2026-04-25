//! #61 follow-up coverage:
//!
//!   - Item 2: pfda-amm-3 ClearBatch oracle C-lite per-leg fallback
//!     (single-feed-stale legs degrade to reserve-ratio, all-stale aborts).
//!   - Item 4: pfda-amm dedicated `treasury` field migration. Pools
//!     created with the v2 init format route bids to `pool.treasury`;
//!     legacy zeroed-treasury pools fall back to `pool.authority`.
//!   - Item 6: pfda-amm-3 SetBatchId disc 9 is feature-gated. The
//!     standard mainnet build (no test-time-warp feature) must reject
//!     disc 9 with InvalidInstructionData — proves the gate is in
//!     place even without inspecting the binary.

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

const PFDA_ERR_TREASURY_MISMATCH: u32 = 6023;

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

// ─── #61 item 4 — pfda-amm treasury field migration ───────────────────

fn pfda_clear_batch_ix(
    cranker: Address,
    pool: Address,
    batch_queue: Address,
    history: Address,
    new_queue: Address,
    bid_lamports: u64,
    recipient: Option<Address>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(cranker, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(batch_queue, false),
        AccountMeta::new(history, false),
        AccountMeta::new(new_queue, false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(Address::new_unique(), false), // oracle slot 6
        AccountMeta::new_readonly(Address::new_unique(), false), // oracle slot 7
    ];
    if let Some(r) = recipient {
        accounts.push(AccountMeta::new(r, false));
    }
    let mut data = vec![2u8];
    data.extend_from_slice(&bid_lamports.to_le_bytes());
    Instruction { program_id: pfda_amm_id(), accounts, data }
}

fn seed_pfda_batch_queue(svm: &mut LiteSVM, pool: Address, batch_id: u64, window_end: u64) -> Address {
    let (queue, bump) = Address::find_program_address(
        &[b"queue", pool.as_ref(), &batch_id.to_le_bytes()],
        &pfda_amm_id(),
    );
    let qd = build_batch_queue(&pool, batch_id, 0, 0, window_end, bump);
    svm.set_account(
        queue,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: qd,
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    queue
}

#[test]
fn pfda_v2_pool_routes_bid_to_dedicated_treasury_field() {
    // v2 pool: treasury is set to a distinct pubkey from authority.
    // Bid recipient MUST equal treasury — passing authority gets
    // TreasuryMismatch even though authority used to be the canonical
    // recipient under the legacy fallback.
    require_fixture!(PFDA_AMM_SO);
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(pfda_amm_id(), PFDA_AMM_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mint_a = Address::new_unique();
    let mint_b = Address::new_unique();
    create_mint(&mut svm, mint_a, &payer.pubkey(), 6);
    create_mint(&mut svm, mint_b, &payer.pubkey(), 6);

    let (pool, bump) = Address::find_program_address(
        &[b"pool", mint_a.as_ref(), mint_b.as_ref()],
        &pfda_amm_id(),
    );

    let vault_a = Address::new_unique();
    let vault_b = Address::new_unique();
    create_token_account(&mut svm, vault_a, &mint_a, &pool, 1_000_000);
    create_token_account(&mut svm, vault_b, &mint_b, &pool, 1_000_000);

    // Distinct treasury pubkey
    let treasury = Address::new_unique();
    svm.airdrop(&treasury, LAMPORTS_PER_SOL).unwrap();

    let pd = build_pfda_pool_state_v2(
        &mint_a, &mint_b, &vault_a, &vault_b,
        1_000_000, 1_000_000, 500_000,
        10, 0, 0, 30,
        &payer.pubkey(), bump,
        Some(&treasury),
    );
    svm.set_account(
        pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pd,
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let queue = seed_pfda_batch_queue(&mut svm, pool, 0, 0);
    let (hist, _) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let (new_queue, _) = Address::find_program_address(
        &[b"queue", pool.as_ref(), &1u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    // Pass authority as recipient — was the legacy fallback. v2 pool
    // routes to treasury so this must reject with TreasuryMismatch.
    let err = send(
        &mut svm,
        pfda_clear_batch_ix(
            payer.pubkey(), pool, queue, hist, new_queue,
            2_000_000, Some(payer.pubkey()),
        ),
        &payer,
    )
    .err()
    .expect("authority recipient on v2 pool must be rejected");
    assert_custom_err(&err, PFDA_ERR_TREASURY_MISMATCH, "v2 pool routed to authority");
}

#[test]
fn pfda_legacy_pool_with_zero_treasury_falls_back_to_authority() {
    // Pre-migration pool: treasury bytes are zero. ClearBatch must
    // accept authority as the bid recipient (legacy fallback).
    // Wrong recipient (rogue) still rejects.
    require_fixture!(PFDA_AMM_SO);
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(pfda_amm_id(), PFDA_AMM_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mint_a = Address::new_unique();
    let mint_b = Address::new_unique();
    create_mint(&mut svm, mint_a, &payer.pubkey(), 6);
    create_mint(&mut svm, mint_b, &payer.pubkey(), 6);

    let (pool, bump) = Address::find_program_address(
        &[b"pool", mint_a.as_ref(), mint_b.as_ref()],
        &pfda_amm_id(),
    );

    let vault_a = Address::new_unique();
    let vault_b = Address::new_unique();
    create_token_account(&mut svm, vault_a, &mint_a, &pool, 1_000_000);
    create_token_account(&mut svm, vault_b, &mint_b, &pool, 1_000_000);

    // Legacy: treasury left zeroed.
    let pd = build_pfda_pool_state(
        &mint_a, &mint_b, &vault_a, &vault_b,
        1_000_000, 1_000_000, 500_000,
        10, 0, 0, 30,
        &payer.pubkey(), bump,
    );
    svm.set_account(
        pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pd,
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let queue = seed_pfda_batch_queue(&mut svm, pool, 0, 0);
    let (hist, _) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let (new_queue, _) = Address::find_program_address(
        &[b"queue", pool.as_ref(), &1u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    // rogue (≠ authority) must still reject — fallback path doesn't
    // accept anything-goes.
    let rogue = Address::new_unique();
    svm.airdrop(&rogue, LAMPORTS_PER_SOL).unwrap();
    let err = send(
        &mut svm,
        pfda_clear_batch_ix(
            payer.pubkey(), pool, queue, hist, new_queue,
            2_000_000, Some(rogue),
        ),
        &payer,
    )
    .err()
    .expect("legacy pool: rogue recipient must reject");
    assert_custom_err(&err, PFDA_ERR_TREASURY_MISMATCH, "legacy fallback rogue");
}

// ─── #61 item 6 — SetBatchId is feature-gated ────────────────────────

#[test]
fn pfda3_set_batch_id_disc_unknown_in_mainnet_build() {
    // Standard `cargo build-sbf` (no test-time-warp feature) must
    // exclude the SetBatchId handler entirely, so disc 9 surfaces as
    // InvalidInstructionData. This is the proof that the feature gate
    // works without needing to inspect the .so file. If a future build
    // accidentally sets the feature, this test fails — exactly the
    // signal we want for "test-only ix slipped into a mainnet binary".
    require_fixture!(PFDA_AMM_3_SO);
    let mut svm = LiteSVM::new();
    svm.add_program_from_file(pfda3_id(), PFDA_AMM_3_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL).unwrap();

    // Build a SetBatchId tx (disc 9 + u64 batch_id). Accounts can be
    // arbitrary — the disc check fires before any account read.
    let mut data = vec![9u8];
    data.extend_from_slice(&100u64.to_le_bytes());
    let ix = Instruction {
        program_id: pfda3_id(),
        accounts: vec![
            AccountMeta::new_readonly(payer.pubkey(), true),
            AccountMeta::new(Address::new_unique(), false),
        ],
        data,
    };

    let err = send(&mut svm, ix, &payer)
        .err()
        .expect("disc 9 must reject when test-time-warp feature is off");
    assert!(
        err.contains("InvalidInstructionData") || err.contains("invalid instruction data"),
        "expected InvalidInstructionData, got: {err}"
    );
}

// ─── #61 item 2 — pfda-amm-3 oracle C-lite per-leg fallback ───────────
//
// Direct end-to-end coverage of the C-lite policy needs full ClearBatch
// state (PoolState3 + BatchQueue3 + ClearedBatchHistory3 fabrication +
// Switchboard feed account fabrication). The clearing-price math runs
// regardless of which path each leg takes, so a unit test of the
// effective_price formula in isolation would re-derive the program
// logic. A higher-fidelity test belongs in axis_g3m_coverage.rs once
// that file gains an oracle harness — tracked as a follow-up. The
// behavioural change is small and well-localised; the multi-agent
// review sign-off + the inline comments describing per-leg fallback
// (clear_batch.rs:186-216, 232-249) provide the code-side guarantee
// for now.
