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
