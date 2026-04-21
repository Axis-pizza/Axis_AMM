//! pfda-amm (2-token legacy) regression coverage (issue #33).
//!
//! Counterpart to `close_delay.rs` and `scenario_coverage.rs`, which cover
//! pfda-amm-3. This file hits the same rejection-path rows from kidney's
//! #33 table, but against the legacy 2-token program.
//!
//! Covered:
//!   - CloseBatchHistory success after 100-batch delay
//!   - CloseBatchHistory rejection before delay
//!   - CloseExpiredTicket success after 200-batch delay
//!   - CloseExpiredTicket rejection before delay
//!   - AddLiquidity on paused pool → PoolPaused
//!   - AddLiquidity with wrong vault → VaultMismatch
//!   - SwapRequest duplicate call same batch is atomic (post PR#50 reorder)
//!
//! All tests fabricate canonical PDAs via set_account so we don't depend
//! on driving a real ClearBatch / 200-batch timeline in the test.

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

// PfmmError codes (mirror of contracts/pfda-amm/src/error.rs)
const ERR_BATCH_WINDOW_NOT_ENDED: u32 = 6002;
const ERR_POOL_PAUSED: u32 = 6018;
const ERR_VAULT_MISMATCH: u32 = 6020;

// ─── Instruction builders ───────────────────────────────────────────────

fn pfda_close_batch_history_ix(
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
        data: vec![7u8], // CloseBatchHistory
    }
}

fn pfda_close_expired_ticket_ix(
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
        data: vec![8u8], // CloseExpiredTicket
    }
}

fn pfda_add_liquidity_ix(
    program: Address,
    user: Address,
    pool: Address,
    vault_a: Address,
    vault_b: Address,
    user_token_a: Address,
    user_token_b: Address,
    amount_a: u64,
    amount_b: u64,
) -> Instruction {
    let mut data = vec![4u8]; // AddLiquidity
    data.extend_from_slice(&amount_a.to_le_bytes());
    data.extend_from_slice(&amount_b.to_le_bytes());
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(user, true),
            AccountMeta::new(pool, false),
            AccountMeta::new(vault_a, false),
            AccountMeta::new(vault_b, false),
            AccountMeta::new(user_token_a, false),
            AccountMeta::new(user_token_b, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data,
    }
}

fn pfda_swap_request_ix(
    program: Address,
    user: Address,
    pool: Address,
    queue: Address,
    ticket: Address,
    user_token_a: Address,
    user_token_b: Address,
    vault_a: Address,
    vault_b: Address,
    amount_in_a: u64,
    amount_in_b: u64,
) -> Instruction {
    let mut data = vec![1u8]; // SwapRequest
    data.extend_from_slice(&amount_in_a.to_le_bytes());
    data.extend_from_slice(&amount_in_b.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_amount_out
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(user, true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new(queue, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(user_token_a, false),
            AccountMeta::new(user_token_b, false),
            AccountMeta::new(vault_a, false),
            AccountMeta::new(vault_b, false),
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

// ─── Fixture ────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    mint_a: Address,
    mint_b: Address,
    vault_a: Address,
    vault_b: Address,
    user_tok_a: Address,
    user_tok_b: Address,
}

fn seed(current_batch_id: u64, paused: bool) -> Option<Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(PFDA_AMM_SO).exists() {
        eprintln!("SKIP: pfda_amm.so fixture missing");
        return None;
    }
    svm.add_program_from_file(pfda_amm_id(), PFDA_AMM_SO).ok()?;

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

    let user_tok_a = Address::new_unique();
    let user_tok_b = Address::new_unique();
    create_token_account(&mut svm, user_tok_a, &mint_a, &payer.pubkey(), 1_000_000_000);
    create_token_account(&mut svm, user_tok_b, &mint_b, &payer.pubkey(), 1_000_000_000);

    let mut pd = build_pfda_pool_state(
        &mint_a,
        &mint_b,
        &vault_a,
        &vault_b,
        1_000_000,
        1_000_000,
        500_000,           // weight_a = 50%
        10,                // window_slots
        current_batch_id,
        100,               // current_window_end
        30,                // base_fee_bps
        &payer.pubkey(),   // authority
        bump,
    );
    if paused {
        pd[238] = 1; // paused flag offset
    }
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

    Some(Fixture {
        svm,
        payer,
        pool,
        mint_a,
        mint_b,
        vault_a,
        vault_b,
        user_tok_a,
        user_tok_b,
    })
}

fn seed_queue(svm: &mut LiteSVM, pool: Address, batch_id: u64, window_end: u64) -> Address {
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

fn seed_history(svm: &mut LiteSVM, pool: Address, batch_id: u64) -> (Address, u64) {
    let (hist, bump) = Address::find_program_address(
        &[b"history", pool.as_ref(), &batch_id.to_le_bytes()],
        &pfda_amm_id(),
    );
    let rent = 1_000_000u64;
    svm.set_account(
        hist,
        Account {
            lamports: rent,
            data: build_cleared_batch_history(&pool, batch_id, bump),
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    (hist, rent)
}

fn seed_ticket(svm: &mut LiteSVM, pool: Address, owner: Address, batch_id: u64) -> (Address, u64) {
    let (ticket, bump) = Address::find_program_address(
        &[b"ticket", pool.as_ref(), owner.as_ref(), &batch_id.to_le_bytes()],
        &pfda_amm_id(),
    );
    let rent = 1_500_000u64;
    svm.set_account(
        ticket,
        Account {
            lamports: rent,
            data: build_user_order_ticket(&owner, &pool, batch_id, bump),
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
    (ticket, rent)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[test]
fn pfda_close_batch_history_success_after_delay() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, .. } = match seed(150, false) {
        Some(f) => f,
        None => return,
    };
    let (hist, _) = seed_history(&mut svm, pool, 0);

    let before = svm.get_balance(&payer.pubkey()).unwrap();
    send(
        &mut svm,
        pfda_close_batch_history_ix(pfda_amm_id(), payer.pubkey(), pool, hist),
        &payer,
    )
    .expect("CloseBatchHistory should succeed after delay");

    assert_eq!(svm.get_balance(&hist).unwrap_or(0), 0);
    let after = svm.get_balance(&payer.pubkey()).unwrap_or(0);
    assert!(after > before, "rent credited: {before} -> {after}");
}

#[test]
fn pfda_close_batch_history_rejects_before_delay() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, .. } = match seed(50, false) {
        Some(f) => f,
        None => return,
    };
    let (hist, _) = seed_history(&mut svm, pool, 0);

    let err = send(
        &mut svm,
        pfda_close_batch_history_ix(pfda_amm_id(), payer.pubkey(), pool, hist),
        &payer,
    )
    .err()
    .expect("should reject");
    assert_custom_err(&err, ERR_BATCH_WINDOW_NOT_ENDED, "pre-delay");
}

#[test]
fn pfda_close_expired_ticket_success_after_delay() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, .. } = match seed(250, false) {
        Some(f) => f,
        None => return,
    };
    let owner = payer.pubkey();
    let (ticket, _) = seed_ticket(&mut svm, pool, owner, 0);

    let before = svm.get_balance(&owner).unwrap();
    send(
        &mut svm,
        pfda_close_expired_ticket_ix(pfda_amm_id(), payer.pubkey(), pool, ticket, owner),
        &payer,
    )
    .expect("CloseExpiredTicket should succeed after delay");

    assert_eq!(svm.get_balance(&ticket).unwrap_or(0), 0);
    let after = svm.get_balance(&owner).unwrap_or(0);
    assert!(after > before, "rent credited: {before} -> {after}");
}

#[test]
fn pfda_close_expired_ticket_rejects_before_delay() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, .. } = match seed(100, false) {
        Some(f) => f,
        None => return,
    };
    let owner = payer.pubkey();
    let (ticket, _) = seed_ticket(&mut svm, pool, owner, 0);

    let err = send(
        &mut svm,
        pfda_close_expired_ticket_ix(pfda_amm_id(), payer.pubkey(), pool, ticket, owner),
        &payer,
    )
    .err()
    .expect("should reject");
    assert_custom_err(&err, ERR_BATCH_WINDOW_NOT_ENDED, "pre-delay");
}

#[test]
fn pfda_add_liquidity_rejects_when_paused() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture {
        mut svm, payer, pool, mint_a: _, mint_b: _, vault_a, vault_b, user_tok_a, user_tok_b,
    } = match seed(0, /*paused=*/ true) {
        Some(f) => f,
        None => return,
    };

    let err = send(
        &mut svm,
        pfda_add_liquidity_ix(
            pfda_amm_id(),
            payer.pubkey(),
            pool,
            vault_a,
            vault_b,
            user_tok_a,
            user_tok_b,
            100,
            100,
        ),
        &payer,
    )
    .err()
    .expect("paused add_liquidity should reject");
    assert_custom_err(&err, ERR_POOL_PAUSED, "paused rejection");
}

#[test]
fn pfda_add_liquidity_rejects_wrong_vault() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture {
        mut svm, payer, pool, mint_a, mint_b: _, vault_a: _, vault_b, user_tok_a, user_tok_b,
    } = match seed(0, false) {
        Some(f) => f,
        None => return,
    };

    // A rogue vault with the right mint but not the program-registered
    // vault. Pre-PR#46 this would have transferred the deposit away.
    let rogue = Address::new_unique();
    create_token_account(&mut svm, rogue, &mint_a, &pool, 0);

    let err = send(
        &mut svm,
        pfda_add_liquidity_ix(
            pfda_amm_id(),
            payer.pubkey(),
            pool,
            rogue,      // bogus vault_a
            vault_b,
            user_tok_a,
            user_tok_b,
            100,
            100,
        ),
        &payer,
    )
    .err()
    .expect("wrong vault should reject");
    assert_custom_err(&err, ERR_VAULT_MISMATCH, "wrong-vault rejection");
}

#[test]
fn pfda_swap_request_duplicate_same_batch_is_atomic() {
    // PR #50 reordered SwapRequest so CreateAccount runs before the
    // Transfer. A second call in the same batch must now fail on
    // CreateAccount (account in use) without moving any tokens.
    //
    // Pre-fix: the first call would Transfer into the vault, then the
    // CreateAccount would fail → tokens stranded. Post-fix: the second
    // call fails atomically before the Transfer.
    require_fixture!(PFDA_AMM_SO);
    let Fixture {
        mut svm, payer, pool, mint_a: _, mint_b: _, vault_a, vault_b, user_tok_a, user_tok_b,
    } = match seed(0, false) {
        Some(f) => f,
        None => return,
    };

    let queue = seed_queue(&mut svm, pool, 0, 100);

    // First call — happy path. Should create ticket PDA + transfer.
    send(
        &mut svm,
        {
            let (ticket, _) = Address::find_program_address(
                &[b"ticket", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
                &pfda_amm_id(),
            );
            pfda_swap_request_ix(
                pfda_amm_id(),
                payer.pubkey(),
                pool,
                queue,
                ticket,
                user_tok_a,
                user_tok_b,
                vault_a,
                vault_b,
                1000,
                0,
            )
        },
        &payer,
    )
    .expect("first SwapRequest should succeed");

    let vault_a_mid = read_token_amount(&svm, &vault_a);
    let user_a_mid = read_token_amount(&svm, &user_tok_a);

    // Second call — same (user, batch_id). Must fail atomically.
    let (ticket2, _) = Address::find_program_address(
        &[b"ticket", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let err = send(
        &mut svm,
        pfda_swap_request_ix(
            pfda_amm_id(),
            payer.pubkey(),
            pool,
            queue,
            ticket2,
            user_tok_a,
            user_tok_b,
            vault_a,
            vault_b,
            1000,
            0,
        ),
        &payer,
    )
    .err()
    .expect("second SwapRequest same batch should fail");
    // The account-in-use error comes from the system program; message
    // varies across LiteSVM versions. What we *care* about is that no
    // tokens moved between the failed call and the pre-fail snapshot.
    assert!(!err.is_empty(), "expected a rejection, got: {err}");

    let vault_a_after = read_token_amount(&svm, &vault_a);
    let user_a_after = read_token_amount(&svm, &user_tok_a);
    assert_eq!(
        vault_a_after, vault_a_mid,
        "vault_a must not change after failed duplicate SwapRequest"
    );
    assert_eq!(
        user_a_after, user_a_mid,
        "user_tok_a must not change after failed duplicate SwapRequest"
    );
}

fn read_token_amount(svm: &LiteSVM, addr: &Address) -> u64 {
    let acc = svm.get_account(addr).expect("token account");
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}

// ─── Additional scenario rows from kidney's #33 table ──────────────────

const ERR_INVALID_WEIGHT: u32 = 6009;
const ERR_INVALID_WINDOW_SLOTS: u32 = 6014;
const ERR_ALREADY_INITIALIZED: u32 = 6015;
const ERR_UNAUTHORIZED: u32 = 6016;
const ERR_INVALID_FEE_BPS: u32 = 6019;
const ERR_SLIPPAGE_EXCEEDED: u32 = 6006;
const ERR_TICKET_ALREADY_CLAIMED: u32 = 6004;

// pfda_amm SwapRequest wrong vault — companion to the AddLiquidity
// wrong-vault test. Kidney called out both instructions in the same row.
#[test]
fn pfda_swap_request_rejects_wrong_vault() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture {
        mut svm, payer, pool, mint_a, mint_b: _, vault_a: _, vault_b,
        user_tok_a, user_tok_b,
    } = match seed(0, false) { Some(f) => f, None => return };

    let rogue = Address::new_unique();
    create_token_account(&mut svm, rogue, &mint_a, &pool, 0);

    let queue = seed_queue(&mut svm, pool, 0, 100);
    let (ticket, _) = Address::find_program_address(
        &[b"ticket", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    let err = send(
        &mut svm,
        pfda_swap_request_ix(
            pfda_amm_id(), payer.pubkey(), pool, queue, ticket,
            user_tok_a, user_tok_b, rogue, vault_b,
            1000, 0,
        ),
        &payer,
    )
    .err()
    .expect("wrong-vault SwapRequest should reject");
    assert_custom_err(&err, 6020 /* VaultMismatch */, "swap_request wrong vault");
}

// ─── UpdateWeight ─────────────────────────────────────────────────────

fn pfda_update_weight_ix(
    program: Address,
    authority: Address,
    pool: Address,
    target_weight_a: u32,
    weight_end_slot: u64,
) -> Instruction {
    let mut data = vec![5u8]; // UpdateWeight
    data.extend_from_slice(&target_weight_a.to_le_bytes());
    data.extend_from_slice(&weight_end_slot.to_le_bytes());
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(pool, false),
        ],
        data,
    }
}

#[test]
fn pfda_update_weight_rejects_out_of_range() {
    require_fixture!(PFDA_AMM_SO);
    // target_weight_a > 1_000_000 is the #5-fixed upper bound.
    let Fixture { mut svm, payer, pool, .. } =
        match seed(0, false) { Some(f) => f, None => return };

    let err = send(
        &mut svm,
        pfda_update_weight_ix(pfda_amm_id(), payer.pubkey(), pool, 1_000_001, 1_000_000),
        &payer,
    )
    .err()
    .expect("out-of-range target_weight should reject");
    assert_custom_err(&err, ERR_INVALID_WEIGHT, "weight out of range");
}

#[test]
fn pfda_update_weight_rejects_past_end_slot() {
    require_fixture!(PFDA_AMM_SO);
    // weight_end_slot <= current_slot → InvalidWindowSlots.
    // LiteSVM's initial clock slot is 0 unless warped, so end_slot = 0
    // triggers the `<=` branch.
    let Fixture { mut svm, payer, pool, .. } =
        match seed(0, false) { Some(f) => f, None => return };

    let err = send(
        &mut svm,
        pfda_update_weight_ix(pfda_amm_id(), payer.pubkey(), pool, 500_000, 0),
        &payer,
    )
    .err()
    .expect("past end_slot should reject");
    assert_custom_err(&err, ERR_INVALID_WINDOW_SLOTS, "past end slot");
}

#[test]
fn pfda_update_weight_rejects_wrong_authority() {
    require_fixture!(PFDA_AMM_SO);
    // pool.authority = payer; intruder is a different signer.
    let Fixture { mut svm, pool, .. } =
        match seed(0, false) { Some(f) => f, None => return };

    let intruder = Keypair::new();
    svm.airdrop(&intruder.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send(
        &mut svm,
        pfda_update_weight_ix(pfda_amm_id(), intruder.pubkey(), pool, 500_000, 1_000_000),
        &intruder,
    )
    .err()
    .expect("wrong-authority UpdateWeight should reject");
    assert_custom_err(&err, ERR_UNAUTHORIZED, "wrong authority");
}

// ─── InitializePool validation-only rejections ─────────────────────────
// We only exercise the pre-account-setup branches — base_fee_bps >= 10_000
// fires at ix-data validation, before any CreateAccount, so an empty
// accounts fixture is enough.

fn pfda_init_ix_empty_accounts(
    payer: &Keypair,
    program: Address,
    base_fee_bps: u16,
    window_slots: u64,
    weight_a: u32,
) -> Instruction {
    let mut data = vec![0u8]; // InitializePool
    data.extend_from_slice(&base_fee_bps.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes()); // fee_discount_bps
    data.extend_from_slice(&window_slots.to_le_bytes());
    data.extend_from_slice(&weight_a.to_le_bytes());

    // Nine accounts, mostly fresh System-owned — the rejection fires
    // before anything is read.
    let accts = [
        AccountMeta::new(payer.pubkey(), true),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new_readonly(Address::new_unique(), false),
        AccountMeta::new_readonly(Address::new_unique(), false),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new(Address::new_unique(), false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
    ];
    Instruction { program_id: program, accounts: accts.to_vec(), data }
}

#[test]
fn pfda_init_rejects_fee_100_percent() {
    require_fixture!(PFDA_AMM_SO);
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(PFDA_AMM_SO).exists() { return; }
    svm.add_program_from_file(pfda_amm_id(), PFDA_AMM_SO).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send(
        &mut svm,
        pfda_init_ix_empty_accounts(&payer, pfda_amm_id(), 10_000, 10, 500_000),
        &payer,
    )
    .err()
    .expect("base_fee_bps=10_000 should reject");
    assert_custom_err(&err, ERR_INVALID_FEE_BPS, "fee=100%");
}

#[test]
fn pfda_init_rejects_duplicate() {
    require_fixture!(PFDA_AMM_SO);
    // Pre-seed the pool PDA with a valid discriminator so the
    // "already initialized" branch fires. We don't need real accounts
    // elsewhere — the check happens right after PDA derivation.
    let Fixture { mut svm, payer, pool, mint_a, mint_b, .. } =
        match seed(0, false) { Some(f) => f, None => return };

    // Drive init against the already-seeded pool.
    let mut data = vec![0u8];
    data.extend_from_slice(&30u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&10u64.to_le_bytes());
    data.extend_from_slice(&500_000u32.to_le_bytes());

    let batch_queue = Address::new_unique();
    let vault_a = Address::new_unique();
    let vault_b = Address::new_unique();

    let err = send(
        &mut svm,
        Instruction {
            program_id: pfda_amm_id(),
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),
                AccountMeta::new(pool, false),
                AccountMeta::new(batch_queue, false),
                AccountMeta::new_readonly(mint_a, false),
                AccountMeta::new_readonly(mint_b, false),
                AccountMeta::new(vault_a, false),
                AccountMeta::new(vault_b, false),
                AccountMeta::new_readonly(system_program_id(), false),
                AccountMeta::new_readonly(token_program_id(), false),
            ],
            data,
        },
        &payer,
    )
    .err()
    .expect("duplicate init should reject");
    assert_custom_err(&err, ERR_ALREADY_INITIALIZED, "duplicate init");
}

// ─── Claim double-claim + slippage ─────────────────────────────────────
// Fabricate a cleared history + ticket, call Claim once (succeeds with
// slippage or pays out), then call again (TicketAlreadyClaimed).

const Q32_32_ONE: u64 = 1u64 << 32;

fn build_pfda_cleared_batch_history_full(
    pool: &Address,
    batch_id: u64,
    clearing_price: u64,
    out_b_per_in_a: u64,
    out_a_per_in_b: u64,
    bump: u8,
) -> Vec<u8> {
    // ClearedBatchHistory (80 bytes):
    //  0: disc "clrdhist"
    //  8: pool (32)
    // 40: batch_id u64
    // 48: clearing_price u64
    // 56: out_b_per_in_a u64
    // 64: out_a_per_in_b u64
    // 72: is_cleared u8
    // 73: bump u8
    // 74: padding [u8;6]
    let mut d = vec![0u8; 80];
    d[0..8].copy_from_slice(b"clrdhist");
    d[8..40].copy_from_slice(pool.as_ref());
    d[40..48].copy_from_slice(&batch_id.to_le_bytes());
    d[48..56].copy_from_slice(&clearing_price.to_le_bytes());
    d[56..64].copy_from_slice(&out_b_per_in_a.to_le_bytes());
    d[64..72].copy_from_slice(&out_a_per_in_b.to_le_bytes());
    d[72] = 1;
    d[73] = bump;
    d
}

#[allow(clippy::too_many_arguments)]
fn build_pfda_user_order_ticket_full(
    owner: &Address,
    pool: &Address,
    batch_id: u64,
    amount_in_a: u64,
    amount_in_b: u64,
    min_amount_out: u64,
    bump: u8,
) -> Vec<u8> {
    // UserOrderTicket (112 bytes):
    //  0: disc "usrorder"
    //  8: owner (32)
    // 40: pool (32)
    // 72: batch_id u64
    // 80: amount_in_a u64
    // 88: amount_in_b u64
    // 96: min_amount_out u64
    // 104: is_claimed u8
    // 105: bump u8
    // 106: padding [u8;6]
    let mut d = vec![0u8; 112];
    d[0..8].copy_from_slice(b"usrorder");
    d[8..40].copy_from_slice(owner.as_ref());
    d[40..72].copy_from_slice(pool.as_ref());
    d[72..80].copy_from_slice(&batch_id.to_le_bytes());
    d[80..88].copy_from_slice(&amount_in_a.to_le_bytes());
    d[88..96].copy_from_slice(&amount_in_b.to_le_bytes());
    d[96..104].copy_from_slice(&min_amount_out.to_le_bytes());
    d[104] = 0; // is_claimed false
    d[105] = bump;
    d
}

#[allow(clippy::too_many_arguments)]
fn pfda_claim_ix(
    program: Address,
    user: Address,
    pool: Address,
    history: Address,
    ticket: Address,
    vault_a: Address,
    vault_b: Address,
    user_tok_a: Address,
    user_tok_b: Address,
) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new_readonly(user, true),
            AccountMeta::new_readonly(pool, false),
            AccountMeta::new_readonly(history, false),
            AccountMeta::new(ticket, false),
            AccountMeta::new(vault_a, false),
            AccountMeta::new(vault_b, false),
            AccountMeta::new(user_tok_a, false),
            AccountMeta::new(user_tok_b, false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data: vec![3u8], // Claim
    }
}

#[test]
fn pfda_claim_rejects_slippage_exceeded() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, vault_a, vault_b, user_tok_a, user_tok_b, .. } =
        match seed(1, false) { Some(f) => f, None => return };

    // Seed history at batch_id=0 with 1:1 rate (Q32.32 1.0). Ticket
    // deposited 100 token A, wants ≥ 1_000_000 token B out — impossible.
    let (hist, hbump) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    svm.set_account(
        hist,
        Account {
            lamports: 1_500_000,
            data: build_pfda_cleared_batch_history_full(&pool, 0, Q32_32_ONE, Q32_32_ONE, Q32_32_ONE, hbump),
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    ).unwrap();

    let (ticket, tbump) = Address::find_program_address(
        &[b"ticket", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    svm.set_account(
        ticket,
        Account {
            lamports: 2_000_000,
            data: build_pfda_user_order_ticket_full(&payer.pubkey(), &pool, 0, 100, 0, 1_000_000, tbump),
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    ).unwrap();

    // Under the slippage branch, Claim refunds the input token and
    // returns SlippageExceeded — but the input is returned from the
    // vault via CPI, which requires enough vault balance. Vault is
    // seeded with 1_000_000 so the 100-token refund fits.
    let err = send(
        &mut svm,
        pfda_claim_ix(pfda_amm_id(), payer.pubkey(), pool, hist, ticket,
            vault_a, vault_b, user_tok_a, user_tok_b),
        &payer,
    )
    .err()
    .expect("slippage-violating Claim should reject");
    assert_custom_err(&err, ERR_SLIPPAGE_EXCEEDED, "claim slippage");
}

#[test]
fn pfda_claim_rejects_double_claim() {
    require_fixture!(PFDA_AMM_SO);
    let Fixture { mut svm, payer, pool, vault_a, vault_b, user_tok_a, user_tok_b, .. } =
        match seed(1, false) { Some(f) => f, None => return };

    let (hist, hbump) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    svm.set_account(
        hist,
        Account {
            lamports: 1_500_000,
            data: build_pfda_cleared_batch_history_full(&pool, 0, Q32_32_ONE, Q32_32_ONE, Q32_32_ONE, hbump),
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    ).unwrap();

    let (ticket, tbump) = Address::find_program_address(
        &[b"ticket", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    // Pre-mark ticket as claimed so Claim immediately returns
    // TicketAlreadyClaimed. This is a shortcut — the real scenario
    // would Claim once then Claim again, but both hit the same check
    // at the top of the handler.
    let mut data = build_pfda_user_order_ticket_full(
        &payer.pubkey(), &pool, 0, 100, 0, 0, tbump,
    );
    data[104] = 1; // is_claimed = true

    svm.set_account(
        ticket,
        Account {
            lamports: 2_000_000,
            data,
            owner: pfda_amm_id(),
            executable: false,
            rent_epoch: 0,
        },
    ).unwrap();

    let err = send(
        &mut svm,
        pfda_claim_ix(pfda_amm_id(), payer.pubkey(), pool, hist, ticket,
            vault_a, vault_b, user_tok_a, user_tok_b),
        &payer,
    )
    .err()
    .expect("already-claimed ticket should reject");
    assert_custom_err(&err, ERR_TICKET_ALREADY_CLAIMED, "double claim");
}
