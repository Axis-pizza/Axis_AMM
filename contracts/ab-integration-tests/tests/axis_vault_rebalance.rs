//! axis-vault v1.2 Rebalance / ProposeWeights / ApplyWeights coverage.
//!
//! Uses the drain-style mock Jupiter (see `mock-jupiter/src/lib.rs`)
//! loaded at the canonical Jupiter V6 program ID. The mock's two
//! transfers map onto a rebalance leg as:
//!
//!   - drain:  `in_amount` out of the sell vault (accounts[0], auto-
//!     prepended by axis-vault) authorized by accounts[1] — here the
//!     REAL etf_state PDA, exercising the full invoke_signed +
//!     meta-elevation path that production Jupiter routes rely on.
//!   - emit:   `out_amount` from a pre-funded filler account into the
//!     buy vault (or, for the theft tests, from a third vault to
//!     somewhere it shouldn't go).
//!
//! Vaults carry the production shape: SPL authority = etf_state PDA.
//!
//! The headline scenario is `rebalance_unblocks_nav_deadlocked_deposit`:
//! once relative prices drift, per-vault mint candidates spread past
//! MAX_NAV_DEVIATION_BPS and Deposit reverts (the P-2 deadlock);
//! Rebalance restores the balance/weight proportion and the same
//! Deposit goes through. That is the liveness property this upgrade
//! exists to provide.

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

const ERR_WEIGHTS_MISMATCH: u32 = 9003;
const ERR_OWNER_MISMATCH: u32 = 9008;
const ERR_POOL_PAUSED: u32 = 9012;
const ERR_SLIPPAGE_EXCEEDED: u32 = 9015;
const ERR_NAV_DEVIATION: u32 = 9016;
const ERR_JUPITER_CPI_NO_OUTPUT: u32 = 9030;
const ERR_EXCESS_VAULT_DRAIN: u32 = 9036;
const ERR_TURNOVER_EXCEEDED: u32 = 9040;
const ERR_WEIGHT_DELTA_EXCEEDED: u32 = 9042;
const ERR_TIMELOCK_NOT_ELAPSED: u32 = 9043;
const ERR_NO_PENDING_PROPOSAL: u32 = 9044;

const REBALANCE_WINDOW_SLOTS: u64 = 9_000;
const WEIGHT_TIMELOCK_SLOTS: u64 = 216_000;
const START_SLOT: u64 = 100;

fn assert_custom_err(err: &str, code: u32, label: &str) {
    let hex = format!("0x{:x}", code);
    let custom = format!("Custom({})", code);
    assert!(
        err.contains(&hex) || err.contains(&custom),
        "{label}: expected {code} ({hex}), got: {err}"
    );
}

/// Build the v3 EtfState blob — same offsets as
/// `axis_vault_withdraw_sol_bound::build_etf_state`.
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
    paused: u8,
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
    d[450] = paused;
    d[451] = bump;
    d[452..452 + name.len()].copy_from_slice(name);
    d[484..486].copy_from_slice(b"RB");
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

struct RebalanceFixture {
    svm: LiteSVM,
    /// ETF authority = manager = depositor in these tests.
    manager: Keypair,
    /// Owns `filler_src`, the pre-funded buy-token account the mock
    /// emits from. Tx-level signer on rebalance calls.
    filler: Keypair,
    etf_state: Address,
    rebalance_state: Address,
    etf_mint: Address,
    basket_mints: [Address; 3],
    vaults: [Address; 3],
    user_basket_atas: [Address; 3],
    user_etf_ata: Address,
    treasury_etf_ata: Address,
    drain_sink: Address,
    filler_src: Address,
    name: Vec<u8>,
}

/// Seed a 3-token ETF in the price-drifted state:
///
///   weights        = [5000, 2500, 2500]
///   total_supply   = 1_000_000
///   vault balances = [600_000, 200_000, 250_000]
///
/// Balanced (deposit-friendly) balances for these weights would be
/// proportional to [2:1:1]. The drift makes Deposit's per-vault mint
/// candidates [83_333, 125_000, 100_000] — a 50% spread, far past the
/// 3% NAV gate. One Rebalance leg (sell 100_000 of token0, buy 50_000
/// of token1) restores [500_000, 250_000, 250_000] ∝ weights.
fn seed(paused: u8) -> Option<RebalanceFixture> {
    let mut svm = LiteSVM::new();
    if !std::path::Path::new(AXIS_VAULT_SO).exists()
        || !std::path::Path::new(MOCK_JUPITER_SO).exists()
    {
        eprintln!("SKIP: fixture .so missing (axis_vault / mock_jupiter)");
        return None;
    }
    svm.add_program_from_file(axis_vault_id(), AXIS_VAULT_SO).ok()?;
    svm.add_program_from_file(jupiter_id(), MOCK_JUPITER_SO).ok()?;
    warp_to_slot(&mut svm, START_SLOT);

    let manager = Keypair::new();
    svm.airdrop(&manager.pubkey(), 100 * LAMPORTS_PER_SOL).unwrap();
    let filler = Keypair::new();
    svm.airdrop(&filler.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let name = b"REBAL".to_vec();
    let (etf_state, bump) = Address::find_program_address(
        &[b"etf", manager.pubkey().as_ref(), &name],
        &axis_vault_id(),
    );
    let (rebalance_state, _) = Address::find_program_address(
        &[b"rebal", etf_state.as_ref()],
        &axis_vault_id(),
    );

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

    let basket_mints: [Address; 3] =
        [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    let vaults: [Address; 3] =
        [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for m in &basket_mints {
        create_mint(&mut svm, *m, &manager.pubkey(), 6);
    }
    // Production shape: vault SPL authority = etf_state PDA. The mock's
    // drain transfer is authorized by the PDA whose signer status comes
    // from axis-vault's invoke_signed + meta elevation.
    let drifted = [600_000u64, 200_000, 250_000];
    for i in 0..3 {
        create_token_account(&mut svm, vaults[i], &basket_mints[i], &etf_state, drifted[i]);
    }

    // Depositor-side accounts (manager doubles as depositor).
    let mut user_basket_atas = [Address::new_unique(), Address::new_unique(), Address::new_unique()];
    for i in 0..3 {
        user_basket_atas[i] = Address::new_unique();
        create_token_account(
            &mut svm, user_basket_atas[i], &basket_mints[i], &manager.pubkey(), 1_000_000,
        );
    }
    let user_etf_ata = Address::new_unique();
    create_token_account(&mut svm, user_etf_ata, &etf_mint, &manager.pubkey(), 0);

    let treasury = Address::new_unique();
    let treasury_etf_ata = Address::new_unique();
    create_token_account(&mut svm, treasury_etf_ata, &etf_mint, &treasury, 0);

    // Mock plumbing: a sink for drained sell-side tokens and a
    // pre-funded buy-side source.
    let drain_sink = Address::new_unique();
    create_token_account(&mut svm, drain_sink, &basket_mints[0], &manager.pubkey(), 0);
    let filler_src = Address::new_unique();
    create_token_account(&mut svm, filler_src, &basket_mints[1], &filler.pubkey(), 10_000_000);

    let data = build_etf_state(
        &manager.pubkey(),
        &etf_mint,
        3,
        &basket_mints,
        &vaults,
        &[5_000u16, 2_500, 2_500],
        1_000_000,
        &treasury,
        bump,
        &name,
        paused,
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

    Some(RebalanceFixture {
        svm,
        manager,
        filler,
        etf_state,
        rebalance_state,
        etf_mint,
        basket_mints,
        vaults,
        user_basket_atas,
        user_etf_ata,
        treasury_etf_ata,
        drain_sink,
        filler_src,
        name,
    })
}

/// Rebalance ix data:
/// [disc=9][sell][buy][amount_in][min_out][route_account_count=6]
/// [route_len=16][mock in_amount][mock out_amount]
fn rebalance_data(sell: u8, buy: u8, amount_in: u64, min_out: u64, mock_in: u64, mock_out: u64) -> Vec<u8> {
    let mut d = vec![9u8, sell, buy];
    d.extend_from_slice(&amount_in.to_le_bytes());
    d.extend_from_slice(&min_out.to_le_bytes());
    d.push(6u8); // route_account_count
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&mock_in.to_le_bytes());
    d.extend_from_slice(&mock_out.to_le_bytes());
    d
}

/// Mock route accounts (axis-vault auto-prepends the sell vault as
/// accounts[0] = drain source):
///   [0] source_authority — etf_state PDA (meta-elevated signer)
///   [1] drain sink
///   [2] token_program
///   [3] emit source
///   [4] emit source authority
///   [5] emit destination
#[allow(clippy::too_many_arguments)]
fn rebalance_ix(
    f: &RebalanceFixture,
    authority: Address,
    sell: u8,
    buy: u8,
    amount_in: u64,
    min_out: u64,
    mock_in: u64,
    mock_out: u64,
    sink: Address,
    emit_src: Address,
    emit_auth: Address,
    emit_auth_is_tx_signer: bool,
    emit_dst: Address,
) -> Instruction {
    // Dedup-friendly: the runtime merges duplicate keys (etf_state and
    // vaults may appear both in the fixed prefix and the route tail).
    // `sink` must share the sell vault's mint — SPL transfers cannot
    // cross mints.
    let accounts = vec![
        AccountMeta::new(authority, true),                       // 0
        AccountMeta::new_readonly(f.etf_state, false),           // 1
        AccountMeta::new(f.rebalance_state, false),              // 2
        AccountMeta::new_readonly(system_program_id(), false),   // 3
        AccountMeta::new(f.vaults[0], false),                    // 4
        AccountMeta::new(f.vaults[1], false),                    // 5
        AccountMeta::new(f.vaults[2], false),                    // 6
        AccountMeta::new_readonly(jupiter_id(), false),          // 7
        // route accounts
        AccountMeta::new_readonly(f.etf_state, false),
        AccountMeta::new(sink, false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new(emit_src, false),
        AccountMeta::new_readonly(emit_auth, emit_auth_is_tx_signer),
        AccountMeta::new(emit_dst, false),
    ];
    let data = rebalance_data(sell, buy, amount_in, min_out, mock_in, mock_out);
    Instruction { program_id: axis_vault_id(), accounts, data }
}

fn propose_ix(f: &RebalanceFixture, authority: Address, weights: &[u16]) -> Instruction {
    let mut data = vec![10u8, weights.len() as u8];
    for w in weights {
        data.extend_from_slice(&w.to_le_bytes());
    }
    Instruction {
        program_id: axis_vault_id(),
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new_readonly(f.etf_state, false),
            AccountMeta::new(f.rebalance_state, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    }
}

fn apply_ix(f: &RebalanceFixture, authority: Address) -> Instruction {
    Instruction {
        program_id: axis_vault_id(),
        accounts: vec![
            AccountMeta::new_readonly(authority, true),
            AccountMeta::new(f.etf_state, false),
            AccountMeta::new(f.rebalance_state, false),
        ],
        data: vec![11u8],
    }
}

fn deposit_ix(f: &RebalanceFixture, amount: u64) -> Instruction {
    let mut data = vec![1u8];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_mint_out
    data.push(f.name.len() as u8);
    data.extend_from_slice(&f.name);

    let mut accounts = vec![
        AccountMeta::new(f.manager.pubkey(), true),
        AccountMeta::new(f.etf_state, false),
        AccountMeta::new(f.etf_mint, false),
        AccountMeta::new(f.user_etf_ata, false),
        AccountMeta::new_readonly(token_program_id(), false),
        AccountMeta::new(f.treasury_etf_ata, false),
    ];
    for a in &f.user_basket_atas {
        accounts.push(AccountMeta::new(*a, false));
    }
    for v in &f.vaults {
        accounts.push(AccountMeta::new(*v, false));
    }
    Instruction { program_id: axis_vault_id(), accounts, data }
}

fn send(svm: &mut LiteSVM, ix: Instruction, signers: &[&Keypair]) -> Result<u64, String> {
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&signers[0].pubkey()),
        signers,
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

/// Read RebalanceState fields straight off the account bytes.
/// Layout: disc 0..8, etf_state 8..40, bump 40, pad 41..48,
/// window_start_slot 48..56, window_snapshot 56..96, window_sold
/// 96..136, proposed_weights 136..146, pad 146..152, eta 152..160.
fn read_rebal(svm: &LiteSVM, addr: &Address) -> (u64, [u64; 5], [u64; 5], [u16; 5], u64) {
    let acc = svm.get_account(addr).expect("rebalance_state must exist");
    assert_eq!(&acc.data[0..8], b"rebal__1", "sidecar discriminator");
    let u64_at = |off: usize| u64::from_le_bytes(acc.data[off..off + 8].try_into().unwrap());
    let mut snapshot = [0u64; 5];
    let mut sold = [0u64; 5];
    let mut proposed = [0u16; 5];
    for i in 0..5 {
        snapshot[i] = u64_at(56 + i * 8);
        sold[i] = u64_at(96 + i * 8);
        proposed[i] =
            u16::from_le_bytes(acc.data[136 + i * 2..138 + i * 2].try_into().unwrap());
    }
    (u64_at(48), snapshot, sold, proposed, u64_at(152))
}

fn read_etf_weights(svm: &LiteSVM, etf_state: &Address) -> [u16; 5] {
    let acc = svm.get_account(etf_state).unwrap();
    let mut w = [0u16; 5];
    for i in 0..5 {
        w[i] = u16::from_le_bytes(acc.data[394 + i * 2..396 + i * 2].try_into().unwrap());
    }
    w
}

// ─── Rebalance ─────────────────────────────────────────────────────────

/// Happy path through the REAL PDA-signing chain: axis-vault
/// invoke_signed elevates etf_state to signer in the CPI metas, the
/// mock drains the sell vault under that authority, and the emit
/// transfer credits the buy vault. Sidecar gets created lazily and
/// tracks the window.
#[test]
fn rebalance_happy_path_moves_balance_and_tracks_window() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 45_000, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();
    let cu = send(&mut f.svm, ix, &[&manager, &filler])
        .expect("happy-path rebalance must succeed");
    eprintln!("rebalance CU: {}", cu);

    assert_eq!(read_token_amount(&f.svm, &f.vaults[0]), 500_000, "sell vault drained by amount_in");
    assert_eq!(read_token_amount(&f.svm, &f.vaults[1]), 250_000, "buy vault credited by out");
    assert_eq!(read_token_amount(&f.svm, &f.vaults[2]), 250_000, "third vault untouched");

    let (window_start, snapshot, sold, _, eta) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(window_start, START_SLOT, "window opened at current slot");
    assert_eq!(snapshot[0], 600_000, "snapshot taken before the swap");
    assert_eq!(sold[0], 100_000, "consumption accounted");
    assert_eq!(eta, 0, "no weight proposal yet");
}

/// THE scenario this upgrade exists for. Drifted vaults deadlock
/// Deposit on the NAV gate; one rebalance leg restores proportionality
/// and the identical Deposit succeeds.
#[test]
fn rebalance_unblocks_nav_deadlocked_deposit() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    // Before: candidates [83_333, 125_000, 100_000] → spread ≫ 3%.
    let ix = deposit_ix(&f, 100_000);
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("drifted vaults must deadlock Deposit");
    assert_custom_err(&err, ERR_NAV_DEVIATION, "P-2 deadlock reproduction");

    // Rebalance 100_000 token0 → 50_000 token1: [500k, 250k, 250k] ∝ weights.
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 45_000, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    send(&mut f.svm, ix, &[&manager, &filler]).expect("rebalance leg");

    // After: candidates all equal 100_000 → mint succeeds. (Expire the
    // blockhash — the deposit bytes are identical to the deadlocked
    // attempt and LiteSVM would reject the duplicate signature.)
    f.svm.expire_blockhash();
    let ix = deposit_ix(&f, 100_000);
    send(&mut f.svm, ix, &[&manager])
        .expect("rebalance must unblock the deposit");
    let minted = read_token_amount(&f.svm, &f.user_etf_ata);
    // fee = 100_000 * 30bps = 300 → net 99_700.
    assert_eq!(minted, 99_700, "post-rebalance deposit mints at restored NAV");
}

#[test]
fn rebalance_rejects_non_authority() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let mallory = Keypair::new();
    f.svm.airdrop(&mallory.pubkey(), LAMPORTS_PER_SOL).unwrap();
    let filler = f.filler.insecure_clone();

    let ix = rebalance_ix(
        &f, mallory.pubkey(), 0, 1, 100_000, 45_000, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&mallory, &filler])
        .err()
        .expect("non-authority must be rejected");
    assert_custom_err(&err, ERR_OWNER_MISMATCH, "authority gate");
}

#[test]
fn rebalance_rejects_when_paused() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(1) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 45_000, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("paused ETF must reject rebalance");
    assert_custom_err(&err, ERR_POOL_PAUSED, "paused gate");
}

/// Single call above 20% of the window snapshot must reject.
/// Snapshot(vault0) = 600_000 → cap = 120_000.
#[test]
fn rebalance_rejects_turnover_above_cap() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 120_001, 45_000, 120_001, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("amount above the turnover cap must reject");
    assert_custom_err(&err, ERR_TURNOVER_EXCEEDED, "single-call cap");
}

/// Cumulative legs share the window budget; a fresh window re-snapshots
/// and resets it.
#[test]
fn rebalance_cumulative_turnover_and_window_reset() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    // Leg 1: 70_000 of the 120_000 budget.
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 70_000, 30_000, 70_000, 35_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    send(&mut f.svm, ix, &[&manager, &filler]).expect("leg 1 inside budget");

    // Leg 2: +60_000 → 130_000 > 120_000 → reject.
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 60_000, 25_000, 60_000, 30_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("cumulative turnover above cap must reject");
    assert_custom_err(&err, ERR_TURNOVER_EXCEEDED, "cumulative cap");

    // Warp past the window: snapshot(vault0) = 530_000 → cap 106_000;
    // the same 60_000 leg now fits.
    warp_to_slot(&mut f.svm, START_SLOT + REBALANCE_WINDOW_SLOTS + 1);
    f.svm.expire_blockhash();
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 60_000, 25_000, 60_000, 30_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    send(&mut f.svm, ix, &[&manager, &filler]).expect("fresh window resets the budget");

    let (window_start, snapshot, sold, _, _) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(window_start, START_SLOT + REBALANCE_WINDOW_SLOTS + 1);
    assert_eq!(snapshot[0], 530_000, "re-snapshot at window open");
    assert_eq!(sold[0], 60_000, "sold counter reset then accumulated");
}

/// A route that funds the buy vault by draining a THIRD vault (the
/// PDA's elevated signer status would happily authorize it) must be
/// caught by the non-decreasing check on uninvolved vaults.
#[test]
fn rebalance_rejects_route_stealing_from_third_vault() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();

    // Sell leg 1 → buy 2; the emit leg steals from UNINVOLVED vaults[0]
    // into an attacker-chosen account (drain_sink, same mint), authorized
    // by the etf_state PDA itself — no tx-level signer needed, the meta
    // elevation makes it a signer inside the route, which is exactly the
    // abuse vector. The drain sink for the sell leg reuses filler_src
    // (mint1). vaults[0] is checked (and must be non-decreasing) before
    // the buy-vault delta is evaluated.
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 1, 2, 10_000, 1, 10_000, 50_000,
        f.filler_src, f.vaults[0], f.etf_state, false, f.drain_sink,
    );
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("third-vault drain must reject");
    assert_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "uninvolved vault non-decreasing");
}

/// Output below min_out → SlippageExceeded; zero output → JupiterCpiNoOutput.
#[test]
fn rebalance_rejects_low_or_zero_output() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 45_000, 100_000, 44_999,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("output below min_out must reject");
    assert_custom_err(&err, ERR_SLIPPAGE_EXCEEDED, "min_out gate");

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 45_000, 100_000, 0,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("zero output must reject");
    assert_custom_err(&err, ERR_JUPITER_CPI_NO_OUTPUT, "no-output gate");
}

/// The route may not pull more out of the sell vault than `amount_in`,
/// no matter what the route bytes say.
#[test]
fn rebalance_rejects_overdrain_of_sell_vault() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    // Declared amount_in = 50_000 (inside the turnover cap), but the
    // route actually drains 100_000.
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 50_000, 45_000, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("overdrain past amount_in must reject");
    assert_custom_err(&err, ERR_EXCESS_VAULT_DRAIN, "amount_in bound");
}

/// min_out = 0 is rejected at parse time — it would make the
/// output-reaches-custody proof vacuous.
#[test]
fn rebalance_rejects_zero_min_out() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 100_000, 0, 100_000, 50_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    let err = send(&mut f.svm, ix, &[&manager, &filler])
        .err()
        .expect("zero min_out must reject");
    assert_custom_err(&err, ERR_SLIPPAGE_EXCEEDED, "min_out floor");
}

// ─── ProposeWeights / ApplyWeights ─────────────────────────────────────

#[test]
fn propose_then_apply_after_timelock() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let new_weights = [4_500u16, 2_750, 2_750];

    let ix = propose_ix(&f, f.manager.pubkey(), &new_weights);
    send(&mut f.svm, ix, &[&manager])
        .expect("propose within delta cap");

    let (_, _, _, proposed, eta) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(&proposed[..3], &new_weights, "proposal staged");
    assert_eq!(eta, START_SLOT + WEIGHT_TIMELOCK_SLOTS, "eta = now + timelock");

    // Early apply must reject.
    let ix = apply_ix(&f, f.manager.pubkey());
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("apply before eta must reject");
    assert_custom_err(&err, ERR_TIMELOCK_NOT_ELAPSED, "timelock gate");

    // Warp past eta → apply succeeds, weights written, proposal cleared.
    warp_to_slot(&mut f.svm, START_SLOT + WEIGHT_TIMELOCK_SLOTS + 1);
    f.svm.expire_blockhash();
    let ix = apply_ix(&f, f.manager.pubkey());
    send(&mut f.svm, ix, &[&manager])
        .expect("apply after timelock");

    let w = read_etf_weights(&f.svm, &f.etf_state);
    assert_eq!(&w[..3], &new_weights, "weights_bps updated in place");
    let (_, _, _, proposed, eta) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(eta, 0, "proposal consumed");
    assert_eq!(proposed, [0u16; 5], "staged vector cleared");

    // Second apply with nothing pending must reject.
    f.svm.expire_blockhash();
    let ix = apply_ix(&f, f.manager.pubkey());
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("apply without pending proposal must reject");
    assert_custom_err(&err, ERR_NO_PENDING_PROPOSAL, "consumed proposal");
}

#[test]
fn propose_rejects_delta_above_cap_and_bad_vectors() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();

    // 5000 → 2000 is a 3000-bps move > MAX_WEIGHT_DELTA_BPS (2000).
    let ix = propose_ix(&f, f.manager.pubkey(), &[2_000, 5_500, 2_500]);
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("delta above cap must reject");
    assert_custom_err(&err, ERR_WEIGHT_DELTA_EXCEEDED, "delta cap");

    // Zero weight bricks Deposit's NAV candidates — rejected.
    let ix = propose_ix(&f, f.manager.pubkey(), &[7_500, 2_500, 0]);
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("zero weight must reject");
    assert_custom_err(&err, ERR_WEIGHTS_MISMATCH, "zero-weight guard");

    // Sum ≠ 10_000.
    let ix = propose_ix(&f, f.manager.pubkey(), &[5_000, 2_500, 2_400]);
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("bad sum must reject");
    assert_custom_err(&err, ERR_WEIGHTS_MISMATCH, "sum guard");
}

#[test]
fn propose_rejects_non_authority() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let mallory = Keypair::new();
    f.svm.airdrop(&mallory.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let ix = propose_ix(&f, mallory.pubkey(), &[4_500, 2_750, 2_750]);
    let err = send(&mut f.svm, ix, &[&mallory])
        .err()
        .expect("non-authority propose must reject");
    assert_custom_err(&err, ERR_OWNER_MISMATCH, "authority gate");
}

// ─── Security hardening regressions ────────────────────────────────────

/// Overwrite an SPL token account's amount (offset 64..72) in place.
fn set_token_amount(svm: &mut LiteSVM, addr: &Address, amount: u64) {
    let mut acc = svm.get_account(addr).expect("token account must exist");
    acc.data[64..72].copy_from_slice(&amount.to_le_bytes());
    svm.set_account(*addr, acc).unwrap();
}

/// Flip EtfState.paused (offset 450) in place.
fn set_paused(svm: &mut LiteSVM, etf_state: &Address, paused: u8) {
    let mut acc = svm.get_account(etf_state).expect("etf_state must exist");
    acc.data[450] = paused;
    svm.set_account(*etf_state, acc).unwrap();
}

/// A griefer can compute the sidecar PDA from the public etf_state and
/// pre-fund it with a lamport, which makes a plain CreateAccount abort.
/// The adopt path must take ownership anyway so rebalance/governance
/// can't be permanently bricked for a targeted ETF.
#[test]
fn rebalance_state_adopts_pre_funded_sidecar() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();

    // Grief: send lamports to the not-yet-created sidecar PDA. It is now
    // a system-owned, zero-data account that CreateAccount would reject.
    f.svm.airdrop(&f.rebalance_state, LAMPORTS_PER_SOL / 100).unwrap();

    let ix = propose_ix(&f, f.manager.pubkey(), &[4_500, 2_750, 2_750]);
    send(&mut f.svm, ix, &[&manager])
        .expect("propose must adopt the pre-funded sidecar, not abort");

    let acc = f.svm.get_account(&f.rebalance_state).unwrap();
    assert_eq!(acc.owner, axis_vault_id(), "sidecar now owned by the program");
    let (_, _, _, proposed, eta) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(&proposed[..3], &[4_500u16, 2_750, 2_750], "proposal staged");
    assert!(eta > 0, "timelock armed");
}

/// ApplyWeights must refuse while paused: Withdraw/WithdrawSol are also
/// paused, so activating a composition change would move the target out
/// from under holders who have no exit, defeating the timelock guarantee.
#[test]
fn apply_weights_rejects_when_paused() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();

    let ix = propose_ix(&f, f.manager.pubkey(), &[4_500, 2_750, 2_750]);
    send(&mut f.svm, ix, &[&manager]).expect("propose");

    warp_to_slot(&mut f.svm, START_SLOT + WEIGHT_TIMELOCK_SLOTS + 1);
    f.svm.expire_blockhash();
    set_paused(&mut f.svm, &f.etf_state, 1);

    let ix = apply_ix(&f, f.manager.pubkey());
    let err = send(&mut f.svm, ix, &[&manager])
        .err()
        .expect("apply while paused must reject");
    assert_custom_err(&err, ERR_POOL_PAUSED, "apply paused gate");

    // Unpausing lets the matured proposal apply normally.
    set_paused(&mut f.svm, &f.etf_state, 0);
    f.svm.expire_blockhash();
    let ix = apply_ix(&f, f.manager.pubkey());
    send(&mut f.svm, ix, &[&manager]).expect("apply after unpause");
    assert_eq!(&read_etf_weights(&f.svm, &f.etf_state)[..3], &[4_500u16, 2_750, 2_750]);
}

/// A vault empty when the window opened gets a zero turnover budget. Once
/// it is funded, the next rebalance selling it must re-baseline the
/// snapshot to the current balance instead of staying locked at cap=0
/// until the window rolls.
#[test]
fn rebalance_rebaselines_zero_snapshot_after_funding() {
    require_fixture!(AXIS_VAULT_SO);
    require_fixture!(MOCK_JUPITER_SO);
    let mut f = match seed(0) {
        Some(x) => x,
        None => return,
    };
    let manager = f.manager.insecure_clone();
    let filler = f.filler.insecure_clone();

    // token2 (mint2) is empty when the window opens.
    set_token_amount(&mut f.svm, &f.vaults[2], 0);

    // First rebalance sells token0 → buy token1, opening the window with
    // snapshot[2] = 0. (buy must be token1: the filler source is mint1.)
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 0, 1, 50_000, 20_000, 50_000, 25_000,
        f.drain_sink, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    send(&mut f.svm, ix, &[&manager, &filler]).expect("open window with token2 empty");

    // token2 is funded after the window opened.
    set_token_amount(&mut f.svm, &f.vaults[2], 100_000);

    // A mint2 sink for the sell leg's drain.
    let sink2 = Address::new_unique();
    create_token_account(&mut f.svm, sink2, &f.basket_mints[2], &f.manager.pubkey(), 0);

    // Selling token2 (snapshot was 0) must now succeed: cap re-baselines
    // to 100_000 → 20_000 budget, and 10_000 fits.
    f.svm.expire_blockhash();
    let ix = rebalance_ix(
        &f, f.manager.pubkey(), 2, 1, 10_000, 4_000, 10_000, 5_000,
        sink2, f.filler_src, f.filler.pubkey(), true, f.vaults[1],
    );
    send(&mut f.svm, ix, &[&manager, &filler])
        .expect("funded zero-snapshot vault must be sellable, not locked at cap=0");

    let (_, snapshot, sold, _, _) = read_rebal(&f.svm, &f.rebalance_state);
    assert_eq!(snapshot[2], 100_000, "snapshot re-baselined from 0 to current balance");
    assert_eq!(sold[2], 10_000, "consumption charged against the re-baselined budget");
}

// Field-use suppressor for fixture members kept for future tests.
#[allow(dead_code)]
fn _fixture_field_uses(f: &RebalanceFixture) {
    let _ = (&f.basket_mints, &f.etf_mint, &f.treasury_etf_ata);
}
