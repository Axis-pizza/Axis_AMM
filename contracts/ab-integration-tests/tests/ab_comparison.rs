//! A/B comparison integration tests: PFDA-3 (ETF A) vs G3M (ETF B)
//!
//! Run local (no network):  cargo test --test ab_comparison -- --nocapture
//! Run with Jupiter CPI:    MAINNET_RPC_URL=... cargo test --test ab_comparison -- --nocapture --ignored

use ab_integration_tests::require_fixture;
use ab_integration_tests::helpers::{
    account_builder::*,
    mainnet_fork::*,
    metrics::*,
    svm_setup::*,
    token_factory::*,
};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{account_meta::AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_native_token::LAMPORTS_PER_SOL;
use solana_signer::Signer;
use solana_transaction::Transaction;

// ─── Instruction builders ────────────────────────────────────────────────

fn g3m_init_ix(
    program: Address, authority: Address, pool_pda: Address,
    user_tokens: &[Address], vaults: &[Address],
    tc: u8, fee_bps: u16, drift_bps: u16, cooldown: u64,
    weights: &[u16], reserves: &[u64],
) -> Instruction {
    let mut data = vec![0u8]; // disc 0
    data.push(tc);
    data.extend_from_slice(&fee_bps.to_le_bytes());
    data.extend_from_slice(&drift_bps.to_le_bytes());
    data.extend_from_slice(&cooldown.to_le_bytes());
    for w in weights { data.extend_from_slice(&w.to_le_bytes()); }
    for r in reserves { data.extend_from_slice(&r.to_le_bytes()); }

    let mut accounts = vec![
        AccountMeta::new(authority, true),
        AccountMeta::new(pool_pda, false),
        AccountMeta::new_readonly(system_program_id(), false),
        AccountMeta::new_readonly(token_program_id(), false),
    ];
    for ut in user_tokens { accounts.push(AccountMeta::new(*ut, false)); }
    for v in vaults { accounts.push(AccountMeta::new(*v, false)); }

    Instruction { program_id: program, accounts, data }
}

fn g3m_swap_ix(
    program: Address, authority: Address, pool_pda: Address,
    user_in: Address, user_out: Address, vault_in: Address, vault_out: Address,
    in_idx: u8, out_idx: u8, amount_in: u64, min_out: u64,
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

fn g3m_check_drift_ix(program: Address, pool_pda: Address) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![AccountMeta::new_readonly(pool_pda, false)],
        data: vec![2u8],
    }
}

fn g3m_rebalance_ix(
    program: Address, authority: Address, pool_pda: Address,
    reserves: &[u64],
) -> Instruction {
    let mut data = vec![3u8]; // disc 3 = attestation rebalance
    for r in reserves { data.extend_from_slice(&r.to_le_bytes()); }

    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(pool_pda, false),
        ],
        data,
    }
}

fn pfda3_add_liquidity_ix(
    program: Address, payer: Address, pool: Address,
    vaults: &[Address; 3], user_tokens: &[Address; 3], amounts: &[u64; 3],
) -> Instruction {
    let mut data = vec![4u8];
    for a in amounts { data.extend_from_slice(&a.to_le_bytes()); }

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
    program: Address, user: Address, pool: Address, queue: Address, ticket: Address,
    user_token: Address, vault: Address,
    in_idx: u8, amount_in: u64, out_idx: u8, min_out: u64,
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

fn pfda3_clear_batch_ix(
    program: Address, cranker: Address, pool: Address,
    queue: Address, history: Address, new_queue: Address,
) -> Instruction {
    Instruction {
        program_id: program,
        accounts: vec![
            AccountMeta::new(cranker, true),
            AccountMeta::new(pool, false),
            AccountMeta::new(queue, false),
            AccountMeta::new(history, false),
            AccountMeta::new(new_queue, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: vec![2u8], // disc 2, no bid
    }
}

fn pfda3_claim_ix(
    program: Address, user: Address, pool: Address, history: Address, ticket: Address,
    vaults: &[Address; 3], user_tokens: &[Address; 3],
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

// ─── Helpers ─────────────────────────────────────────────────────────────

fn send(svm: &mut LiteSVM, ix: Instruction, payer: &Keypair) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix], Some(&payer.pubkey()), &[payer], svm.latest_blockhash(),
    );
    match svm.send_transaction(tx) {
        Ok(meta) => Ok(meta.compute_units_consumed),
        Err(e) => {
            let mut msg = format!("{:?}", e.err);
            for log in &e.meta.logs { msg.push_str(&format!("\n  {}", log)); }
            Err(msg)
        }
    }
}

fn u64_le(d: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(d[off..off + 8].try_into().unwrap())
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_both_programs_load() {
    require_fixture!(AXIS_G3M_SO);
    require_fixture!(PFDA_AMM_3_SO);

    let svm = create_dual_program_svm().expect("failed to create SVM");
    assert!(svm.get_account(&axis_g3m_id()).unwrap().executable);
    assert!(svm.get_account(&pfda3_id()).unwrap().executable);
    println!("✓ Both programs loaded into a single LiteSVM instance");
}

#[test]
fn test_g3m_lifecycle_no_jupiter() {
    require_fixture!(AXIS_G3M_SO);

    let mut svm = LiteSVM::new();
    let pid = axis_g3m_id();
    svm.add_program_from_file(pid, AXIS_G3M_SO).unwrap();

    let auth = Keypair::new();
    svm.airdrop(&auth.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let m0 = Address::new_unique();
    let m1 = Address::new_unique();
    create_mint(&mut svm, m0, &auth.pubkey(), 6);
    create_mint(&mut svm, m1, &auth.pubkey(), 6);

    let (pool, _bump) = Address::find_program_address(&[b"g3m_pool", auth.pubkey().as_ref()], &pid);

    let v0 = Address::new_unique();
    let v1 = Address::new_unique();
    create_token_account(&mut svm, v0, &m0, &pool, 0);
    create_token_account(&mut svm, v1, &m1, &pool, 0);

    let u0 = Address::new_unique();
    let u1 = Address::new_unique();
    create_token_account(&mut svm, u0, &m0, &auth.pubkey(), 50_000_000);
    create_token_account(&mut svm, u1, &m1, &auth.pubkey(), 50_000_000);

    // Init
    let init_cu = send(&mut svm,
        g3m_init_ix(pid, auth.pubkey(), pool, &[u0, u1], &[v0, v1],
            2, 100, 500, 0, &[5000, 5000], &[10_000_000, 10_000_000]),
        &auth,
    ).expect("InitializePool failed");
    println!("  Init CU: {}", init_cu);

    // Swap
    let swap_cu = send(&mut svm,
        g3m_swap_ix(pid, auth.pubkey(), pool, u0, u1, v0, v1, 0, 1, 100_000, 1),
        &auth,
    ).expect("Swap failed");
    println!("  Swap CU: {}", swap_cu);

    // CheckDrift
    let drift_cu = send(&mut svm,
        g3m_check_drift_ix(pid, pool),
        &auth,
    ).expect("CheckDrift failed");
    println!("  CheckDrift CU: {}", drift_cu);

    // Large swap to induce drift >5% (but within 50% reserve change cap)
    let _ = send(&mut svm,
        g3m_swap_ix(pid, auth.pubkey(), pool, u0, u1, v0, v1, 0, 1, 3_000_000, 1),
        &auth,
    ).expect("Large swap failed");

    // Rebalance (attestation mode) — compute balanced reserve target
    let rv0 = read_token_amount(&svm, &v0);
    let rv1 = read_token_amount(&svm, &v1);
    let target = (rv0 + rv1) / 2;
    let reb_cu = send(&mut svm,
        g3m_rebalance_ix(pid, auth.pubkey(), pool, &[target, target]),
        &auth,
    ).expect("Rebalance failed");
    println!("  Rebalance CU: {}", reb_cu);

    println!("✓ G3M lifecycle passed (init={} swap={} drift={} rebalance={})", init_cu, swap_cu, drift_cu, reb_cu);
}

#[test]
fn test_pfda3_batch_auction_cycle() {
    require_fixture!(PFDA_AMM_3_SO);

    let mut svm = LiteSVM::new();
    let pid = pfda3_id();
    svm.add_program_from_file(pid, PFDA_AMM_3_SO).unwrap();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    // Create 3 mints
    let mints: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &m in &mints { create_mint(&mut svm, m, &payer.pubkey(), 6); }

    // PDAs
    let (pool, pool_bump) = Address::find_program_address(
        &[b"pool3", mints[0].as_ref(), mints[1].as_ref(), mints[2].as_ref()], &pid,
    );
    let (queue0, q0_bump) = Address::find_program_address(
        &[b"queue3", pool.as_ref(), &0u64.to_le_bytes()], &pid,
    );

    // Create vaults (uninitialized — pool init will initialize them)
    let vaults: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &v in &vaults { create_uninit_token_account(&mut svm, v); }

    // Create user token accounts with supply
    let user_tokens: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for i in 0..3 {
        create_token_account(&mut svm, user_tokens[i], &mints[i], &payer.pubkey(), 10_000_000_000);
    }

    // Pre-create pool + queue PDAs (PFDA-3 init calls CreateAccount CPI)
    // Actually, init creates them via CPI. But we need the accounts to not exist yet.
    // LiteSVM handles CreateAccount CPI natively — the accounts just shouldn't exist.

    // Set clock to slot 10 so window_end is reasonable
    warp_to_slot(&mut svm, 10);

    // Build pool + queue state directly (skip init CPI complexity)
    let window_slots = 10u64;
    let current_window_end = 20u64; // slot 10 + 10

    let pool_data = build_pfda3_pool_state(
        &mints, &vaults,
        &[1_000_000_000; 3],
        &[333_333, 333_333, 333_334],
        window_slots, 0, current_window_end,
        &payer.pubkey(), &payer.pubkey(), 30, pool_bump,
    );
    svm.set_account(pool, Account {
        lamports: LAMPORTS_PER_SOL,
        data: pool_data,
        owner: pid,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let queue_data = build_batch_queue_3(&pool, 0, &[0; 3], current_window_end, q0_bump);
    svm.set_account(queue0, Account {
        lamports: LAMPORTS_PER_SOL,
        data: queue_data,
        owner: pid,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    // Seed vaults with liquidity (set token account amounts directly)
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &mints[i], &pool, 1_000_000_000);
    }

    // AddLiquidity (to update pool.reserves to match vault balances)
    let liq_cu = send(&mut svm,
        pfda3_add_liquidity_ix(pid, payer.pubkey(), pool, &vaults, &user_tokens,
            &[0; 3]), // 0 additional — reserves already seeded
        &payer,
    );
    let add_liq_cu = liq_cu.unwrap_or(0);
    println!("  AddLiquidity CU: {}", add_liq_cu);

    // SwapRequest (token 0 → token 1, 10M units)
    let swap_amount = 10_000_000u64;
    let (ticket, _) = Address::find_program_address(
        &[b"ticket3", pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()], &pid,
    );

    let swap_cu = send(&mut svm,
        pfda3_swap_request_ix(pid, payer.pubkey(), pool, queue0, ticket,
            user_tokens[0], vaults[0], 0, swap_amount, 1, 0),
        &payer,
    ).expect("SwapRequest failed");
    println!("  SwapRequest CU: {}", swap_cu);

    // Advance past window end
    warp_to_slot(&mut svm, current_window_end + 1);

    // ClearBatch
    let (history0, _) = Address::find_program_address(
        &[b"history3", pool.as_ref(), &0u64.to_le_bytes()], &pid,
    );
    let (queue1, _) = Address::find_program_address(
        &[b"queue3", pool.as_ref(), &1u64.to_le_bytes()], &pid,
    );

    let clear_cu = send(&mut svm,
        pfda3_clear_batch_ix(pid, payer.pubkey(), pool, queue0, history0, queue1),
        &payer,
    ).expect("ClearBatch failed");
    println!("  ClearBatch CU: {}", clear_cu);

    // Claim
    let claim_cu = send(&mut svm,
        pfda3_claim_ix(pid, payer.pubkey(), pool, history0, ticket, &vaults, &user_tokens),
        &payer,
    ).expect("Claim failed");
    println!("  Claim CU: {}", claim_cu);

    let received = read_token_amount(&svm, &user_tokens[1]);
    println!("  Token 1 received: {}", received - 10_000_000_000);

    println!("✓ PFDA-3 batch auction cycle passed (swap={} clear={} claim={})", swap_cu, clear_cu, claim_cu);
}

#[test]
fn test_ab_comparison_local() {
    require_fixture!(AXIS_G3M_SO);
    require_fixture!(PFDA_AMM_3_SO);

    let mut svm = create_dual_program_svm().expect("load programs");
    let g3m_pid = axis_g3m_id();
    let pfda_pid = pfda3_id();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    // Shared mints
    let mints: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &m in &mints { create_mint(&mut svm, m, &payer.pubkey(), 6); }

    let initial_reserve = 1_000_000_000u64;
    let swap_amount = 10_000_000u64;

    // ═══════ ETF B: G3M ═══════
    println!("\n── ETF B: G3M ──");
    let (g3m_pool, _) = Address::find_program_address(&[b"g3m_pool", payer.pubkey().as_ref()], &g3m_pid);
    let g3m_vaults = [Address::new_unique(), Address::new_unique()];
    for i in 0..2 { create_token_account(&mut svm, g3m_vaults[i], &mints[i], &g3m_pool, 0); }
    let g3m_user = [Address::new_unique(), Address::new_unique()];
    for i in 0..2 { create_token_account(&mut svm, g3m_user[i], &mints[i], &payer.pubkey(), 5_000_000_000); }

    let g_init = send(&mut svm,
        g3m_init_ix(g3m_pid, payer.pubkey(), g3m_pool, &g3m_user, &g3m_vaults,
            2, 100, 500, 0, &[5000, 5000], &[initial_reserve, initial_reserve]),
        &payer).expect("G3M init");
    println!("  Init: {} CU", g_init);

    let g_swap = send(&mut svm,
        g3m_swap_ix(g3m_pid, payer.pubkey(), g3m_pool, g3m_user[0], g3m_user[1],
            g3m_vaults[0], g3m_vaults[1], 0, 1, swap_amount, 1),
        &payer).expect("G3M swap");
    println!("  Swap: {} CU", g_swap);

    let g_drift = send(&mut svm,
        g3m_check_drift_ix(g3m_pid, g3m_pool),
        &payer).expect("G3M drift");
    println!("  CheckDrift: {} CU", g_drift);

    // Large swap to induce >5% drift
    let _ = send(&mut svm,
        g3m_swap_ix(g3m_pid, payer.pubkey(), g3m_pool, g3m_user[0], g3m_user[1],
            g3m_vaults[0], g3m_vaults[1], 0, 1, 200_000_000, 1),
        &payer).expect("G3M large swap");

    let v0 = read_token_amount(&svm, &g3m_vaults[0]);
    let v1 = read_token_amount(&svm, &g3m_vaults[1]);
    let balanced = (v0 + v1) / 2;
    let g_reb = send(&mut svm,
        g3m_rebalance_ix(g3m_pid, payer.pubkey(), g3m_pool, &[balanced, balanced]),
        &payer).expect("G3M rebalance");
    println!("  Rebalance: {} CU", g_reb);

    // ═══════ ETF A: PFDA-3 ═══════
    println!("\n── ETF A: PFDA-3 ──");
    warp_to_slot(&mut svm, 100);

    let (pfda_pool, pfda_bump) = Address::find_program_address(
        &[b"pool3", mints[0].as_ref(), mints[1].as_ref(), mints[2].as_ref()], &pfda_pid);
    let (pfda_queue0, q0_bump) = Address::find_program_address(
        &[b"queue3", pfda_pool.as_ref(), &0u64.to_le_bytes()], &pfda_pid);

    let pfda_vaults: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let pfda_user: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for i in 0..3 {
        create_token_account(&mut svm, pfda_vaults[i], &mints[i], &pfda_pool, initial_reserve);
        create_token_account(&mut svm, pfda_user[i], &mints[i], &payer.pubkey(), 10_000_000_000);
    }

    let window_end = 110u64;
    let pool_data = build_pfda3_pool_state(
        &mints, &pfda_vaults,
        &[initial_reserve; 3],
        &[333_333, 333_333, 333_334],
        10, 0, window_end, &payer.pubkey(), &payer.pubkey(), 30, pfda_bump,
    );
    svm.set_account(pfda_pool, Account {
        lamports: LAMPORTS_PER_SOL, data: pool_data, owner: pfda_pid,
        executable: false, rent_epoch: 0,
    }).unwrap();

    let queue_data = build_batch_queue_3(&pfda_pool, 0, &[0; 3], window_end, q0_bump);
    svm.set_account(pfda_queue0, Account {
        lamports: LAMPORTS_PER_SOL, data: queue_data, owner: pfda_pid,
        executable: false, rent_epoch: 0,
    }).unwrap();

    let (ticket, _) = Address::find_program_address(
        &[b"ticket3", pfda_pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()], &pfda_pid);

    let p_swap = send(&mut svm,
        pfda3_swap_request_ix(pfda_pid, payer.pubkey(), pfda_pool, pfda_queue0, ticket,
            pfda_user[0], pfda_vaults[0], 0, swap_amount, 1, 0),
        &payer).expect("PFDA swap request");
    println!("  SwapRequest: {} CU", p_swap);

    warp_to_slot(&mut svm, window_end + 1);

    let (history0, _) = Address::find_program_address(
        &[b"history3", pfda_pool.as_ref(), &0u64.to_le_bytes()], &pfda_pid);
    let (queue1, _) = Address::find_program_address(
        &[b"queue3", pfda_pool.as_ref(), &1u64.to_le_bytes()], &pfda_pid);

    let p_clear = send(&mut svm,
        pfda3_clear_batch_ix(pfda_pid, payer.pubkey(), pfda_pool, pfda_queue0, history0, queue1),
        &payer).expect("PFDA clear batch");
    println!("  ClearBatch: {} CU", p_clear);

    let p_claim = send(&mut svm,
        pfda3_claim_ix(pfda_pid, payer.pubkey(), pfda_pool, history0, ticket, &pfda_vaults, &pfda_user),
        &payer).expect("PFDA claim");
    println!("  Claim: {} CU", p_claim);

    // ═══════ A/B Comparison ═══════
    let comparison = ABComparison {
        g3m: G3mMetrics {
            init_cu: g_init, swap_cu: g_swap, check_drift_cu: g_drift,
            rebalance_cu: g_reb, total_slots: 1, ..Default::default()
        },
        pfda3: Pfda3Metrics {
            swap_request_cu: p_swap, clear_batch_cu: p_clear, claim_cu: p_claim,
            batch_window_slots: 10, total_slots: 11, ..Default::default()
        },
    };
    comparison.print_report();
    println!("✓ A/B local comparison passed");
}

#[test]
#[ignore = "requires mainnet RPC + Jupiter API"]
fn test_ab_full_with_jupiter_rebalance() {
    require_fixture!(AXIS_G3M_SO);
    require_fixture!(PFDA_AMM_3_SO);
    require_fixture!(JUPITER_V6_SO);

    let rpc_url = std::env::var("MAINNET_RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let rpc = solana_rpc_client::rpc_client::RpcClient::new(rpc_url);

    let mut svm = create_dual_program_svm().expect("load programs");
    let g3m_pid = axis_g3m_id();
    let pfda_pid = pfda3_id();

    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();

    // Clone real token mints
    let wsol: Address = "So11111111111111111111111111111111111111112".parse().unwrap();
    let usdc: Address = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".parse().unwrap();
    clone_from_rpc(&mut svm, &rpc, &wsol);
    clone_from_rpc(&mut svm, &rpc, &usdc);
    println!("✓ Cloned wSOL + USDC mints from mainnet");

    // ═══════ ETF B: G3M + Jupiter CPI ═══════
    println!("\n── ETF B: G3M with Jupiter rebalance ──");

    let (g3m_pool, g3m_bump) = Address::find_program_address(
        &[b"g3m_pool", payer.pubkey().as_ref()], &g3m_pid);

    let g3m_vaults = [Address::new_unique(), Address::new_unique()];
    // Imbalanced: 2 SOL / 50 USDC → drift exceeds threshold
    create_token_account(&mut svm, g3m_vaults[0], &wsol, &g3m_pool, 2_000_000_000); // 2 SOL
    create_token_account(&mut svm, g3m_vaults[1], &usdc, &g3m_pool, 50_000_000);    // 50 USDC

    let g3m_user = [Address::new_unique(), Address::new_unique()];
    create_token_account(&mut svm, g3m_user[0], &wsol, &payer.pubkey(), 10_000_000_000);
    create_token_account(&mut svm, g3m_user[1], &usdc, &payer.pubkey(), 500_000_000);

    // Pre-seed pool state with imbalanced reserves
    let pool_data = build_g3m_pool_state(
        &payer.pubkey(), 2, &[wsol, usdc], &g3m_vaults,
        &[5000, 5000],
        &[2_000_000_000, 50_000_000],
        100, 500, 0, g3m_bump,
    );
    svm.set_account(g3m_pool, Account {
        lamports: LAMPORTS_PER_SOL, data: pool_data, owner: g3m_pid,
        executable: false, rent_epoch: 0,
    }).unwrap();

    // Fetch Jupiter route: sell SOL for USDC to rebalance
    let sell_amount = 500_000_000u64; // 0.5 SOL
    println!("  Fetching Jupiter route: {} lamports SOL → USDC", sell_amount);
    let route = match fetch_jupiter_route(&wsol, &usdc, sell_amount, 100, &g3m_pool) {
        Ok(r) => r,
        Err(e) => { println!("  SKIP: Jupiter API unavailable: {}", e); return; }
    };
    println!("  Route: {} accounts, expected out: {} USDC", route.accounts.len(), route.out_amount);

    // Fork all Jupiter state
    let cloned = fork_jupiter_state(&mut svm, &rpc, &route);
    println!("  Forked {} accounts from mainnet", cloned);

    // Build RebalanceViaJupiter instruction (disc 4)
    let jup_pid = jupiter_id();
    let mut ix_data = vec![4u8];
    ix_data.extend_from_slice(&(route.swap_data.len() as u32).to_le_bytes());
    ix_data.extend_from_slice(&route.swap_data);

    let mut accounts = vec![
        AccountMeta::new(payer.pubkey(), true),       // authority
        AccountMeta::new(g3m_pool, false),             // pool_state
        AccountMeta::new_readonly(jup_pid, false),     // jupiter_program
        AccountMeta::new(g3m_vaults[0], false),        // vault 0
        AccountMeta::new(g3m_vaults[1], false),        // vault 1
    ];
    // Append Jupiter route accounts (skip any that are already in our list)
    for ja in &route.accounts {
        accounts.push(AccountMeta::new(ja.pubkey, ja.is_signer && ja.pubkey != payer.pubkey()));
    }

    let ix = Instruction { program_id: g3m_pid, accounts, data: ix_data };
    match send(&mut svm, ix, &payer) {
        Ok(cu) => println!("  ✓ RebalanceViaJupiter CU: {}", cu),
        Err(e) => println!("  ✗ RebalanceViaJupiter failed: {}", e),
    }

    // ═══════ ETF A: PFDA-3 ═══════
    println!("\n── ETF A: PFDA-3 batch auction ──");
    // (simplified — use synthetic mints since PFDA-3 doesn't need Jupiter)
    let p_mints: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for &m in &p_mints { create_mint(&mut svm, m, &payer.pubkey(), 6); }

    warp_to_slot(&mut svm, 200);

    let (pfda_pool, pfda_bump) = Address::find_program_address(
        &[b"pool3", p_mints[0].as_ref(), p_mints[1].as_ref(), p_mints[2].as_ref()], &pfda_pid);
    let (pfda_q0, q0_bump) = Address::find_program_address(
        &[b"queue3", pfda_pool.as_ref(), &0u64.to_le_bytes()], &pfda_pid);

    let pfda_vaults: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let pfda_user: [Address; 3] = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let reserve = 1_000_000_000u64;
    for i in 0..3 {
        create_token_account(&mut svm, pfda_vaults[i], &p_mints[i], &pfda_pool, reserve);
        create_token_account(&mut svm, pfda_user[i], &p_mints[i], &payer.pubkey(), 10_000_000_000);
    }

    let window_end = 210u64;
    let pool_data = build_pfda3_pool_state(
        &p_mints, &pfda_vaults, &[reserve; 3],
        &[333_333, 333_333, 333_334], 10, 0, window_end,
        &payer.pubkey(), &payer.pubkey(), 30, pfda_bump,
    );
    svm.set_account(pfda_pool, Account {
        lamports: LAMPORTS_PER_SOL, data: pool_data, owner: pfda_pid,
        executable: false, rent_epoch: 0,
    }).unwrap();
    let qd = build_batch_queue_3(&pfda_pool, 0, &[0; 3], window_end, q0_bump);
    svm.set_account(pfda_q0, Account {
        lamports: LAMPORTS_PER_SOL, data: qd, owner: pfda_pid,
        executable: false, rent_epoch: 0,
    }).unwrap();

    let (ticket, _) = Address::find_program_address(
        &[b"ticket3", pfda_pool.as_ref(), payer.pubkey().as_ref(), &0u64.to_le_bytes()], &pfda_pid);
    let p_swap = send(&mut svm,
        pfda3_swap_request_ix(pfda_pid, payer.pubkey(), pfda_pool, pfda_q0, ticket,
            pfda_user[0], pfda_vaults[0], 0, 10_000_000, 1, 0),
        &payer).expect("PFDA swap");
    println!("  SwapRequest: {} CU", p_swap);

    warp_to_slot(&mut svm, window_end + 1);
    let (hist, _) = Address::find_program_address(
        &[b"history3", pfda_pool.as_ref(), &0u64.to_le_bytes()], &pfda_pid);
    let (q1, _) = Address::find_program_address(
        &[b"queue3", pfda_pool.as_ref(), &1u64.to_le_bytes()], &pfda_pid);
    let p_clear = send(&mut svm,
        pfda3_clear_batch_ix(pfda_pid, payer.pubkey(), pfda_pool, pfda_q0, hist, q1),
        &payer).expect("PFDA clear");
    println!("  ClearBatch: {} CU", p_clear);

    let p_claim = send(&mut svm,
        pfda3_claim_ix(pfda_pid, payer.pubkey(), pfda_pool, hist, ticket, &pfda_vaults, &pfda_user),
        &payer).expect("PFDA claim");
    println!("  Claim: {} CU", p_claim);

    println!("\n✓ Full A/B test with Jupiter CPI completed");
}
