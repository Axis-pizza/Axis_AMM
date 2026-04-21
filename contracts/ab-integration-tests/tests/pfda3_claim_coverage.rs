//! pfda-amm-3 Claim coverage (issue #33).
//!
//! Two rejection rows kidney flagged that weren't reached by earlier PRs:
//!   - Claim slippage exceeded → SlippageExceeded (8006)
//!   - Claim zero-volume token → silent success, ticket marked claimed
//!
//! The zero-volume case is "success" at the protocol level — the Claim
//! instruction short-circuits when `total_in[in_i] == 0` and marks the
//! ticket claimed without transferring. That's still a rejection row
//! worth pinning: any future change to that branch (e.g. rejecting
//! instead of refunding) would be caught.
//!
//! Pre-seeds PoolState3 + ClearedBatchHistory3 + UserOrderTicket3 via
//! set_account so we can stage post-clearing state directly, rather
//! than driving a full SwapRequest → ClearBatch → Claim flow (each step
//! has its own rejection branches already covered elsewhere).

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

const ERR_SLIPPAGE_EXCEEDED: u32 = 8006;
const Q32_32_ONE: u64 = 1u64 << 32;

// ─── History + ticket builders (richer than close_delay's stubs) ────────
// Offsets verified via offset_of! probe on the real structs.

#[allow(clippy::too_many_arguments)]
fn build_cleared_batch_history_full(
    pool: &Address,
    batch_id: u64,
    clearing_prices: [u64; 3],
    total_out: [u64; 3],
    total_in: [u64; 3],
    fee_bps: u16,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 128];
    d[0..8].copy_from_slice(b"clrd3h\0\0");
    d[8..40].copy_from_slice(pool.as_ref());
    d[40..48].copy_from_slice(&batch_id.to_le_bytes());
    for i in 0..3 {
        d[48 + i * 8..48 + (i + 1) * 8].copy_from_slice(&clearing_prices[i].to_le_bytes());
        d[72 + i * 8..72 + (i + 1) * 8].copy_from_slice(&total_out[i].to_le_bytes());
        d[96 + i * 8..96 + (i + 1) * 8].copy_from_slice(&total_in[i].to_le_bytes());
    }
    d[120..122].copy_from_slice(&fee_bps.to_le_bytes());
    d[122] = 1; // is_cleared
    d[123] = bump;
    d
}

#[allow(clippy::too_many_arguments)]
fn build_user_order_ticket_full(
    owner: &Address,
    pool: &Address,
    batch_id: u64,
    amounts_in: [u64; 3],
    out_token_idx: u8,
    min_amount_out: u64,
    bump: u8,
) -> Vec<u8> {
    let mut d = vec![0u8; 128];
    d[0..8].copy_from_slice(b"usrord3\0");
    d[8..40].copy_from_slice(owner.as_ref());
    d[40..72].copy_from_slice(pool.as_ref());
    d[72..80].copy_from_slice(&batch_id.to_le_bytes());
    for i in 0..3 {
        d[80 + i * 8..80 + (i + 1) * 8].copy_from_slice(&amounts_in[i].to_le_bytes());
    }
    d[104] = out_token_idx;
    d[112..120].copy_from_slice(&min_amount_out.to_le_bytes());
    d[120] = 0; // is_claimed
    d[121] = bump;
    d
}

// ─── Instruction + tx helpers ───────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn pfda3_claim_ix(
    program: Address,
    user: Address,
    pool: Address,
    history: Address,
    ticket: Address,
    vaults: &[Address; 3],
    user_tokens: &[Address; 3],
) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new_readonly(user, true),
            AccountMeta::new(pool, false),
            AccountMeta::new_readonly(history, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(vaults[0], false),
            AccountMeta::new(vaults[1], false),
            AccountMeta::new(vaults[2], false),
            AccountMeta::new(user_tokens[0], false),
            AccountMeta::new(user_tokens[1], false),
            AccountMeta::new(user_tokens[2], false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: vec![3u8],
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

// ─── Fixture ────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    vaults: [Address; 3],
    user_tokens: [Address; 3],
    mints: [[u8; 32]; 3],
}

fn seed() -> Option<Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(PFDA_AMM_3_SO).exists() {
        eprintln!("SKIP: pfda_amm_3.so fixture missing");
        return None;
    }
    svm.add_program_from_file(pfda3_id(), PFDA_AMM_3_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mints = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &m in &mints { create_mint(&mut svm, m, &payer.pubkey(), 6); }

    let (pool, bump) = Address::find_program_address(
        &[b"pool3", mints[0].as_ref(), mints[1].as_ref(), mints[2].as_ref()],
        &pfda3_id(),
    );

    let vaults = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let user_tokens = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 10_000_000);
        create_token_account(&mut svm, user_tokens[i], &mints[i], &payer.pubkey(), 1_000_000_000);
    }

    let treasury = Address::new_unique();
    let pd = build_pfda3_pool_state(
        &mints,
        &vaults,
        &[10_000_000; 3],
        &[333_333, 333_333, 333_334],
        10, 1, 100, // window_slots, current_batch_id=1 (so history for batch 0 is cleared), current_window_end
        &treasury, &payer.pubkey(), 30, bump,
    );
    svm.set_account(
        pool,
        Account { lamports: LAMPORTS_PER_SOL, data: pd, owner: pfda3_id(), executable: false, rent_epoch: 0 },
    ).unwrap();

    let mint_arrays: [[u8; 32]; 3] = [
        mints[0].as_ref().try_into().unwrap(),
        mints[1].as_ref().try_into().unwrap(),
        mints[2].as_ref().try_into().unwrap(),
    ];

    Some(Fixture { svm, payer, pool, vaults, user_tokens, mints: mint_arrays })
}

fn seed_history(
    svm: &mut LiteSVM,
    pool: Address,
    batch_id: u64,
    clearing_prices: [u64; 3],
    total_out: [u64; 3],
    total_in: [u64; 3],
) -> Address {
    let (hist, bump) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &batch_id.to_le_bytes()],
        &pfda3_id(),
    );
    let data = build_cleared_batch_history_full(
        &pool, batch_id, clearing_prices, total_out, total_in, 30, bump,
    );
    svm.set_account(
        hist,
        Account { lamports: 1_500_000, data, owner: pfda3_id(), executable: false, rent_epoch: 0 },
    ).unwrap();
    hist
}

#[allow(clippy::too_many_arguments)]
fn seed_ticket(
    svm: &mut LiteSVM,
    pool: Address,
    owner: Address,
    batch_id: u64,
    amounts_in: [u64; 3],
    out_token_idx: u8,
    min_amount_out: u64,
) -> Address {
    let (ticket, bump) = Address::find_program_address(
        &[b"ticket3", pool.as_ref(), owner.as_ref(), &batch_id.to_le_bytes()],
        &pfda3_id(),
    );
    let data = build_user_order_ticket_full(
        &owner, &pool, batch_id, amounts_in, out_token_idx, min_amount_out, bump,
    );
    svm.set_account(
        ticket,
        Account { lamports: 2_000_000, data, owner: pfda3_id(), executable: false, rent_epoch: 0 },
    ).unwrap();
    ticket
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn claim_rejects_slippage_exceeded() {
    require_fixture!(PFDA_AMM_3_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens, .. } =
        match seed() { Some(f) => f, None => return };

    // Equal clearing prices (Q32.32 = 1.0) and nonzero volume on both legs
    // so Claim actually runs the payout math. amount_out will be roughly
    // amount_in * (1 - fee) = 100_000 * 0.997 = 99_700. min_amount_out = 1M
    // forces SlippageExceeded.
    let hist = seed_history(
        &mut svm, pool, 0,
        [Q32_32_ONE, Q32_32_ONE, Q32_32_ONE],
        [0, 100_000, 0],
        [100_000, 100_000, 0],
    );
    let ticket = seed_ticket(
        &mut svm, pool, payer.pubkey(), 0,
        [100_000, 0, 0], // user deposited token 0
        1,               // wants token 1 out
        1_000_000_000,   // absurd min_out → slippage
    );

    let err = send(
        &mut svm,
        pfda3_claim_ix(pfda3_id(), payer.pubkey(), pool, hist, ticket, &vaults, &user_tokens),
        &payer,
    )
    .err()
    .expect("claim with unsatisfiable min_out should reject");
    assert_custom_err(&err, ERR_SLIPPAGE_EXCEEDED, "claim slippage");
}

#[test]
fn claim_zero_volume_token_marks_claimed_without_payout() {
    require_fixture!(PFDA_AMM_3_SO);
    let Fixture { mut svm, payer, pool, vaults, user_tokens, mints } =
        match seed() { Some(f) => f, None => return };

    // Batch cleared but token 2 had zero volume in: total_in[2] == 0.
    // Claim should short-circuit on that branch, mark claimed, pay
    // nothing out.
    let hist = seed_history(
        &mut svm, pool, 0,
        [Q32_32_ONE, Q32_32_ONE, Q32_32_ONE],
        [0, 0, 0],
        [0, 0, 0], // total_in[2] = 0 → short-circuit
    );
    let ticket_addr = seed_ticket(
        &mut svm, pool, payer.pubkey(), 0,
        [0, 0, 100_000], // user says they deposited token 2 (in_idx=2)
        0,               // wants token 0 out
        0,
    );

    let user_vault_before = [
        token_balance(&svm, &user_tokens[0]),
        token_balance(&svm, &user_tokens[1]),
        token_balance(&svm, &user_tokens[2]),
    ];

    send(
        &mut svm,
        pfda3_claim_ix(pfda3_id(), payer.pubkey(), pool, hist, ticket_addr, &vaults, &user_tokens),
        &payer,
    )
    .expect("zero-volume claim should succeed");

    // No token movements — the short-circuit path doesn't touch vaults.
    for i in 0..3 {
        assert_eq!(
            token_balance(&svm, &user_tokens[i]),
            user_vault_before[i],
            "user_token[{i}] must not move on zero-volume claim"
        );
    }

    // Ticket must be marked claimed so a second Claim would be
    // TicketAlreadyClaimed (not worth asserting here — behavior is
    // already covered by ab_comparison; we only need to confirm the
    // zero-volume branch *doesn't* revert).
    let ticket_after = svm.get_account(&ticket_addr).unwrap();
    assert_eq!(
        ticket_after.data[120], 1,
        "ticket is_claimed must flip to 1 on zero-volume Claim"
    );

    // Silence unused warning from the `mints` field (only needed by the
    // seed fixture for layout consistency).
    let _ = mints;
}

fn token_balance(svm: &LiteSVM, addr: &Address) -> u64 {
    let acc = svm.get_account(addr).expect("token account");
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}
