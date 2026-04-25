//! #59 mainnet-blocker regression coverage.
//!
//! Locks down the three code fixes in PR "fix/issue-59-mainnet-blockers":
//!
//!   1. [P0] pfda-amm-3 `SwapRequest` rejects a token account of the
//!      right mint but wrong key (`VaultMismatch` = 8025). Pre-fix the
//!      instruction only checked the mint, so a cranker could pass in a
//!      user-owned token account and user tokens would be Transfer'd
//!      there while the batch queue still incremented.
//!
//!   2. [P0] pfda-amm-3 `Claim` rejects the same spoof. The check has
//!      been present since PR #3 but #59 flagged it as a possible gap;
//!      we cover it explicitly so any regression is caught.
//!
//!   3. [P1] pfda-amm `ClearBatch` rejects a non-authority bid
//!      recipient (`TreasuryMismatch` = 6023) and a missing bid
//!      recipient when `bid_lamports > 0` (`BidWithoutTreasury` = 6024).
//!      Pre-fix, account[8] was Transfer'd to without any check.
//!
//!   4. [P1] pfda-amm-3 `SetPaused` (new disc=6): authority-gated flip
//!      of `pool.paused`. Without this ix the byte was dead after
//!      deploy.
//!
//!   5. [P1] axis-g3m `SetPaused` (new disc=5): same shape.

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

// ─── error codes (mirror program error enums) ───────────────────────────
const PFDA_ERR_TREASURY_MISMATCH: u32 = 6023;
const PFDA_ERR_BID_WITHOUT_TREASURY: u32 = 6024;
const PFDA3_ERR_VAULT_MISMATCH: u32 = 8025;
const PFDA3_ERR_UNAUTHORIZED: u32 = 8035;
const G3M_ERR_UNAUTHORIZED: u32 = 7020;

// Built-in Solana ProgramError::IllegalOwner — surfaces as either
// `IllegalOwner` (literal variant) or numeric InstructionError(0, ...)
// depending on how Pinocchio bubbles it. The check below tolerates both.
fn assert_illegal_owner(err: &str, label: &str) {
    assert!(
        err.contains("IllegalOwner") || err.contains("Custom(4)") || err.contains("0x4"),
        "{label}: expected IllegalOwner, got: {err}"
    );
}

// ─── common tx helpers ──────────────────────────────────────────────────
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

fn send_signed_by(
    svm: &mut LiteSVM,
    ix: Instruction,
    payer: &Keypair,
    signer: &Keypair,
) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer, signer],
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

// ─── pfda-amm-3 SwapRequest vault-key spoofing ─────────────────────────

struct Pfda3Fixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    mints: [Address; 3],
    vaults: [Address; 3],
    user_tokens: [Address; 3],
}

fn seed_pfda3_pool() -> Option<Pfda3Fixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(PFDA_AMM_3_SO).exists() {
        eprintln!("SKIP: pfda_amm_3.so fixture missing");
        return None;
    }
    svm.add_program_from_file(pfda3_id(), PFDA_AMM_3_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mints = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &m in &mints {
        create_mint(&mut svm, m, &payer.pubkey(), 6);
    }

    let (pool, pool_bump) = Address::find_program_address(
        &[b"pool3", mints[0].as_ref(), mints[1].as_ref(), mints[2].as_ref()],
        &pfda3_id(),
    );

    let vaults = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let user_tokens = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 1_000_000);
        create_token_account(&mut svm, user_tokens[i], &mints[i], &payer.pubkey(), 1_000_000_000);
    }

    let treasury = Address::new_unique();
    let pd = build_pfda3_pool_state(
        &mints,
        &vaults,
        &[1_000_000; 3],
        &[333_333, 333_333, 333_334],
        10,
        0,
        100,
        &treasury,
        &payer.pubkey(),
        30,
        pool_bump,
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

    Some(Pfda3Fixture { svm, payer, pool, mints, vaults, user_tokens })
}

fn pfda3_swap_request_ix(
    user: Address,
    pool: Address,
    queue: Address,
    ticket: Address,
    user_token: Address,
    vault: Address,
) -> Instruction {
    let mut data = vec![1u8];
    data.push(0u8); // in_idx
    data.extend_from_slice(&100u64.to_le_bytes()); // amount_in
    data.push(1u8); // out_idx
    data.extend_from_slice(&0u64.to_le_bytes()); // min_out
    Instruction {
        program_id: pfda3_id(),
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

fn seed_batch_queue_pfda3(svm: &mut LiteSVM, pool: Address, batch_id: u64, window_end: u64) -> Address {
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

#[test]
fn pfda3_swap_request_rejects_spoofed_vault_key() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer, pool, mints, vaults: _, user_tokens } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    let queue = seed_batch_queue_pfda3(&mut svm, pool, 0, 100);
    let (ticket, _) = Address::find_program_address(
        &[b"ticket3", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda3_id(),
    );

    // Rogue token account: same mint as vaults[0], pool-owned, but NOT
    // the vault registered on PoolState3. Pre-fix, the mint-only check
    // in SwapRequest would have accepted this and Transfer'd tokens to
    // the attacker. Post-fix we reject with VaultMismatch.
    let rogue = Address::new_unique();
    create_token_account(&mut svm, rogue, &mints[0], &pool, 0);

    let err = send(
        &mut svm,
        pfda3_swap_request_ix(
            payer.pubkey(), pool, queue, ticket,
            user_tokens[0], rogue,
        ),
        &payer,
    )
    .err()
    .expect("spoofed vault key must be rejected");
    assert_custom_err(&err, PFDA3_ERR_VAULT_MISMATCH, "swap_request spoofed vault");
}

// ─── pfda-amm-3 Claim vault-key regression ─────────────────────────────
//
// PR #47 already covers this; #59 asked us to verify the guard is
// genuinely exercised.

fn pfda3_claim_ix(
    user: Address,
    pool: Address,
    history: Address,
    ticket: Address,
    vaults: &[Address; 3],
    user_tokens: &[Address; 3],
) -> Instruction {
    Instruction {
        program_id: pfda3_id(),
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

#[test]
fn pfda3_claim_rejects_spoofed_vault_key() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer, pool, mints, vaults, user_tokens } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    // Seed a history + ticket so Claim reaches the vault-validation loop.
    let (hist, hbump) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda3_id(),
    );
    let mut hd = vec![0u8; 176];
    hd[0..8].copy_from_slice(b"chist3\0\0");
    hd[8..40].copy_from_slice(pool.as_ref());
    // clearing_prices / total_in / total_out zeroed → Claim's total_in[in_idx]==0
    // short-circuits to "mark claimed" before any Transfer, but the vault
    // validation still runs first. Perfect for a negative test.
    hd[160] = 30; // fee_bps low byte — irrelevant but keeps layout realistic
    hd[168] = 1;  // is_cleared
    hd[169] = hbump;
    svm.set_account(
        hist,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: hd,
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let (ticket, tbump) = Address::find_program_address(
        &[b"ticket3", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()],
        &pfda3_id(),
    );
    let mut td = vec![0u8; 80];
    td[0..8].copy_from_slice(b"tick3\0\0\0");
    td[8..40].copy_from_slice(payer.pubkey().as_ref());
    td[40..72].copy_from_slice(pool.as_ref());
    // batch_id = 0 (already zero)
    // amounts_in left zero
    td[72] = 1; // out_token_idx = 1
    // min_amount_out zero
    td[78] = tbump;
    svm.set_account(
        ticket,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: td,
            owner: pfda3_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    // Replace vaults[0] with a pool-owned, same-mint token account the
    // program does NOT have on record. Claim must reject before touching
    // any token.
    let rogue = Address::new_unique();
    create_token_account(&mut svm, rogue, &mints[0], &pool, 0);
    let bad_vaults = [rogue, vaults[1], vaults[2]];

    let err = send(
        &mut svm,
        pfda3_claim_ix(payer.pubkey(), pool, hist, ticket, &bad_vaults, &user_tokens),
        &payer,
    )
    .err()
    .expect("Claim must reject spoofed vault key");
    assert_custom_err(&err, PFDA3_ERR_VAULT_MISMATCH, "claim spoofed vault");
}

// ─── pfda-amm ClearBatch treasury validation ───────────────────────────

struct PfdaFixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
}

fn seed_pfda_pool() -> Option<PfdaFixture> {
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

    let pd = build_pfda_pool_state(
        &mint_a, &mint_b, &vault_a, &vault_b,
        1_000_000, 1_000_000, 500_000,
        10, 0, 0, // current_window_end = 0 so BatchWindowNotEnded doesn't block
        30, &payer.pubkey(), bump,
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

    Some(PfdaFixture { svm, payer, pool })
}

fn pfda_clear_batch_ix(
    cranker: Address,
    pool: Address,
    batch_queue: Address,
    history: Address,
    new_queue: Address,
    bid_lamports: u64,
    treasury: Option<Address>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(cranker, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(batch_queue, false),
        AccountMeta::new(history, false),
        AccountMeta::new(new_queue, false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(Address::new_unique(), false), // oracle placeholder 6
        AccountMeta::new_readonly(Address::new_unique(), false), // oracle placeholder 7
    ];
    if let Some(t) = treasury {
        accounts.push(AccountMeta::new(t, false));
    }
    let mut data = vec![2u8];
    data.extend_from_slice(&bid_lamports.to_le_bytes());
    Instruction {
        program_id: pfda_amm_id(),
        accounts,
        data,
    }
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
fn pfda_clear_batch_rejects_bid_to_wrong_treasury() {
    require_fixture!(PFDA_AMM_SO);
    let PfdaFixture { mut svm, payer, pool } =
        match seed_pfda_pool() { Some(f) => f, None => return };

    let queue = seed_pfda_batch_queue(&mut svm, pool, 0, 0);
    let (hist, _) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let (new_queue, _) = Address::find_program_address(
        &[b"queue", pool.as_ref(), &1u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    // Rogue treasury — not the pool.authority. Pre-fix this would have
    // happily received the Transfer; post-fix we reject.
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
    .expect("bid to wrong treasury must be rejected");
    assert_custom_err(&err, PFDA_ERR_TREASURY_MISMATCH, "wrong treasury");
}

#[test]
fn pfda_clear_batch_rejects_bid_without_treasury_account() {
    require_fixture!(PFDA_AMM_SO);
    let PfdaFixture { mut svm, payer, pool, .. } =
        match seed_pfda_pool() { Some(f) => f, None => return };

    let queue = seed_pfda_batch_queue(&mut svm, pool, 0, 0);
    let (hist, _) = Address::find_program_address(
        &[b"history", pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let (new_queue, _) = Address::find_program_address(
        &[b"queue", pool.as_ref(), &1u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    let err = send(
        &mut svm,
        pfda_clear_batch_ix(
            payer.pubkey(), pool, queue, hist, new_queue,
            2_000_000, None, // no treasury supplied
        ),
        &payer,
    )
    .err()
    .expect("bid without treasury account must be rejected");
    assert_custom_err(&err, PFDA_ERR_BID_WITHOUT_TREASURY, "missing treasury");
}

// ─── pfda-amm-3 SetPaused ──────────────────────────────────────────────

fn pfda3_set_paused_ix(authority: Address, pool: Address, paused: u8) -> Instruction {
    Instruction {
        program_id: pfda3_id(),
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(pool, false),
        ],
        data: vec![6u8, paused],
    }
}

#[test]
fn pfda3_set_paused_authority_toggles_flag() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer, pool, .. } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    // pre: paused byte is 0
    let before = svm.get_account(&pool).unwrap().data[332];
    assert_eq!(before, 0, "pool starts unpaused");

    send(
        &mut svm,
        pfda3_set_paused_ix(payer.pubkey(), pool, 1),
        &payer,
    )
    .expect("authority-signed SetPaused should succeed");

    let after = svm.get_account(&pool).unwrap().data[332];
    assert_eq!(after, 1, "pool.paused set to 1");

    // and back
    send(
        &mut svm,
        pfda3_set_paused_ix(payer.pubkey(), pool, 0),
        &payer,
    )
    .expect("authority can clear the pause");
    assert_eq!(svm.get_account(&pool).unwrap().data[332], 0);
}

#[test]
fn pfda3_set_paused_rejects_non_authority() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer, pool, .. } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    let intruder = Keypair::new();
    svm.airdrop(&intruder.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send_signed_by(
        &mut svm,
        pfda3_set_paused_ix(intruder.pubkey(), pool, 1),
        &payer,
        &intruder,
    )
    .err()
    .expect("non-authority SetPaused must reject");
    assert_custom_err(&err, PFDA3_ERR_UNAUTHORIZED, "non-authority");
    assert_eq!(svm.get_account(&pool).unwrap().data[332], 0, "pool not paused");
}

// ─── axis-g3m SetPaused ────────────────────────────────────────────────

struct G3mFixture {
    svm: LiteSVM,
    payer: Keypair,
    pool: Address,
    paused_offset: usize,
}

fn seed_g3m_pool() -> Option<G3mFixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_G3M_SO).exists() {
        eprintln!("SKIP: axis_g3m.so fixture missing");
        return None;
    }
    svm.add_program_from_file(axis_g3m_id(), AXIS_G3M_SO).ok()?;

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let mint_a = Address::new_unique();
    let mint_b = Address::new_unique();
    create_mint(&mut svm, mint_a, &payer.pubkey(), 6);
    create_mint(&mut svm, mint_b, &payer.pubkey(), 6);

    let pool = Address::new_unique();
    let vault_a = Address::new_unique();
    let vault_b = Address::new_unique();
    create_token_account(&mut svm, vault_a, &mint_a, &pool, 1_000_000);
    create_token_account(&mut svm, vault_b, &mint_b, &pool, 1_000_000);

    let pd = build_g3m_pool_state(
        &payer.pubkey(),
        2,
        &[mint_a, mint_b],
        &[vault_a, vault_b],
        &[5000, 5000],
        &[10_000_000, 10_000_000],
        100,
        500,
        0,
        255,
    );
    // G3m paused offset: per axis_g3m_coverage.rs set_paused helper,
    // the total size is 464 and paused sits at offset len-8 (= 456).
    // After build_g3m_pool_state (455 bytes), LiteSVM writes the account
    // with the given data length — we pad to the full 464 to match the
    // program's on-disk size.
    let mut padded = pd;
    padded.resize(464, 0);
    let paused_offset = padded.len() - 8;

    svm.set_account(
        pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: padded,
            owner: axis_g3m_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    Some(G3mFixture { svm, payer, pool, paused_offset })
}

fn g3m_set_paused_ix(authority: Address, pool: Address, paused: u8) -> Instruction {
    Instruction {
        program_id: axis_g3m_id(),
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(pool, false),
        ],
        data: vec![5u8, paused],
    }
}

#[test]
fn g3m_set_paused_authority_toggles_flag() {
    require_fixture!(AXIS_G3M_SO);
    let G3mFixture { mut svm, payer, pool, paused_offset } =
        match seed_g3m_pool() { Some(f) => f, None => return };

    send(
        &mut svm,
        g3m_set_paused_ix(payer.pubkey(), pool, 1),
        &payer,
    )
    .expect("authority SetPaused should succeed");

    assert_eq!(svm.get_account(&pool).unwrap().data[paused_offset], 1);

    send(
        &mut svm,
        g3m_set_paused_ix(payer.pubkey(), pool, 0),
        &payer,
    )
    .expect("authority can clear pause");
    assert_eq!(svm.get_account(&pool).unwrap().data[paused_offset], 0);
}

// ─── pfda-amm ClearBatch unowned-pool substitution (B1 round 2) ────────
//
// PR #60 round 1 added `treasury == pool.authority`, but the pool data
// was read before any owner check on pool_state_ai, so a fake account
// (right discriminator + attacker pubkey in the authority field) made
// both checks pass and the bid drained to the attacker. Round 2 hoists
// `pool_state_ai.owner() == program_id` ahead of the bid block.

#[test]
fn pfda_clear_batch_rejects_unowned_pool_state() {
    require_fixture!(PFDA_AMM_SO);
    let PfdaFixture { mut svm, payer, pool: _real_pool } =
        match seed_pfda_pool() { Some(f) => f, None => return };

    // Forge a pool owned by the system program (not pfda-amm). Data
    // carries the right discriminator + attacker's pubkey in the
    // authority slot so the round-1 TreasuryMismatch check WOULD have
    // passed if execution reached it. Round-2 owner check fires first.
    let attacker = Keypair::new();
    let mint_a = Address::new_unique();
    let mint_b = Address::new_unique();
    let vault_a = Address::new_unique();
    let vault_b = Address::new_unique();
    let fake_pool = Address::new_unique();
    let pd = build_pfda_pool_state(
        &mint_a, &mint_b, &vault_a, &vault_b,
        1_000_000, 1_000_000, 500_000,
        10, 0, 0, 30,
        &attacker.pubkey(), 255,
    );
    svm.set_account(
        fake_pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pd,
            // Wrong owner — system program, not pfda-amm.
            owner: system_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let queue = seed_pfda_batch_queue(&mut svm, fake_pool, 0, 0);
    let (hist, _) = Address::find_program_address(
        &[b"history", fake_pool.as_ref(), &0u64.to_le_bytes()],
        &pfda_amm_id(),
    );
    let (new_queue, _) = Address::find_program_address(
        &[b"queue", fake_pool.as_ref(), &1u64.to_le_bytes()],
        &pfda_amm_id(),
    );

    // attacker.pubkey() as the bid recipient — would have matched the
    // forged pool.authority under round 1 logic. Owner check rejects.
    svm.airdrop(&attacker.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send(
        &mut svm,
        pfda_clear_batch_ix(
            payer.pubkey(), fake_pool, queue, hist, new_queue,
            2_000_000, Some(attacker.pubkey()),
        ),
        &payer,
    )
    .err()
    .expect("forged pool_state_ai must be rejected");
    assert_illegal_owner(&err, "clear_batch unowned pool");
}

// ─── pfda-amm-3 WithdrawFees coverage (B3) ─────────────────────────────
//
// PR #60 fixed SwapRequest's vault-key spoofing but left the same
// vulnerability class in WithdrawFees, which uses invoke_signed with
// the pool PDA as authority — so without an owner check + per-vault
// key check, a fake pool (right discriminator + real mints + real
// bump copied from a legit pool) plus an attacker-owned destination
// could siphon any token account the pool PDA happens to authorise.
// PR fix adds both checks; the two tests below pin them down.

fn pfda3_withdraw_fees_ix(
    authority: Address,
    pool: Address,
    vaults: &[Address; 3],
    treasury_tokens: &[Address; 3],
    amounts: [u64; 3],
) -> Instruction {
    let mut data = vec![5u8];
    for a in &amounts {
        data.extend_from_slice(&a.to_le_bytes());
    }
    Instruction {
        program_id: pfda3_id(),
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(pool, false),
            AccountMeta::new(vaults[0], false),
            AccountMeta::new(vaults[1], false),
            AccountMeta::new(vaults[2], false),
            AccountMeta::new(treasury_tokens[0], false),
            AccountMeta::new(treasury_tokens[1], false),
            AccountMeta::new(treasury_tokens[2], false),
            AccountMeta::new_readonly(token_program_id(), false),
        ],
        data,
    }
}

#[test]
fn pfda3_withdraw_fees_rejects_spoofed_vault_key() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer, pool, mints, vaults, user_tokens: _ } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    // Treasury ATAs (same mints, owned by attacker so a successful
    // pre-fix exploit would land tokens in their account).
    let treasury_tokens = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    let attacker = Keypair::new();
    for i in 0..3 {
        create_token_account(&mut svm, treasury_tokens[i], &mints[i], &attacker.pubkey(), 0);
    }

    // Replace vaults[1] with a rogue token account: same mint, same
    // pool-PDA authority (so invoke_signed would have happily signed
    // a Transfer from it), but NOT in pool.vaults. Pre-fix this drained
    // any pool-owned token account; post-fix VaultMismatch fires.
    let rogue = Address::new_unique();
    create_token_account(&mut svm, rogue, &mints[1], &pool, 500_000);
    let bad_vaults = [vaults[0], rogue, vaults[2]];

    let err = send(
        &mut svm,
        pfda3_withdraw_fees_ix(
            payer.pubkey(),
            pool,
            &bad_vaults,
            &treasury_tokens,
            [0, 100_000, 0], // only the spoofed slot has a non-zero amount
        ),
        &payer,
    )
    .err()
    .expect("withdraw_fees must reject spoofed vault key");
    assert_custom_err(&err, PFDA3_ERR_VAULT_MISMATCH, "withdraw_fees spoofed vault");
}

#[test]
fn pfda3_withdraw_fees_rejects_unowned_pool_state() {
    require_fixture!(PFDA_AMM_3_SO);
    let Pfda3Fixture { mut svm, payer: _, pool: _real_pool, mints: real_mints, vaults: real_vaults, user_tokens: _ } =
        match seed_pfda3_pool() { Some(f) => f, None => return };

    // Forge a fake pool owned by system program. Carry the real mints
    // and real bump so seeds derive the legit pool PDA (the precondition
    // for an invoke_signed exploit). Attacker is the "authority" so the
    // OwnerMismatch check (later in the function) would NOT fire — only
    // the round-2 owner check stands between attacker and a Transfer.
    let attacker = Keypair::new();
    let mut svm2 = svm; // local alias just for readability below
    svm2.airdrop(&attacker.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    let (_legit_pool, legit_bump) = Address::find_program_address(
        &[b"pool3", real_mints[0].as_ref(), real_mints[1].as_ref(), real_mints[2].as_ref()],
        &pfda3_id(),
    );
    let treasury_placeholder = Address::new_unique();
    let pd = build_pfda3_pool_state(
        &real_mints,
        &real_vaults,
        &[1_000_000; 3],
        &[333_333, 333_333, 333_334],
        10, 0, 100,
        &treasury_placeholder,
        &attacker.pubkey(),
        30,
        legit_bump,
    );
    let fake_pool = Address::new_unique();
    svm2.set_account(
        fake_pool,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: pd,
            owner: system_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();

    let treasury_tokens = [
        Address::new_unique(),
        Address::new_unique(),
        Address::new_unique(),
    ];
    for i in 0..3 {
        create_token_account(&mut svm2, treasury_tokens[i], &real_mints[i], &attacker.pubkey(), 0);
    }

    let err = send_signed_by(
        &mut svm2,
        pfda3_withdraw_fees_ix(
            attacker.pubkey(),
            fake_pool,
            &real_vaults,
            &treasury_tokens,
            [100_000, 0, 0],
        ),
        &attacker,
        &attacker,
    )
    .err()
    .expect("forged pool_state must be rejected by owner check");
    assert_illegal_owner(&err, "withdraw_fees unowned pool");
}

#[test]
fn g3m_set_paused_rejects_non_authority() {
    require_fixture!(AXIS_G3M_SO);
    let G3mFixture { mut svm, payer, pool, paused_offset } =
        match seed_g3m_pool() { Some(f) => f, None => return };

    let intruder = Keypair::new();
    svm.airdrop(&intruder.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let err = send_signed_by(
        &mut svm,
        g3m_set_paused_ix(intruder.pubkey(), pool, 1),
        &payer,
        &intruder,
    )
    .err()
    .expect("non-authority g3m SetPaused must reject");
    assert_custom_err(&err, G3M_ERR_UNAUTHORIZED, "non-authority g3m");
    assert_eq!(svm.get_account(&pool).unwrap().data[paused_offset], 0);
}
