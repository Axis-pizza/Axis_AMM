# Axis Protocol — A/B Test: Devnet Testing Guide

## What This Is

Four on-chain programs deployed to Solana devnet for the Axis Protocol A/B test. The goal is to compare a **PFDA batch auction** (ETF A) against a **G3M continuous AMM** (ETF B) under real conditions.

- **ETF A** batches swaps into windows and clears them at a single price (MEV protection)
- **ETF B** executes swaps continuously against a geometric mean invariant (standard AMM)
- **Vault** manages ETF token lifecycle: deposit basket tokens, receive ETF tokens; burn ETF tokens, receive basket back

All programs accept **any SPL tokens** — the token choice is a deployment-time decision, not hardcoded.

---

## Deployed Programs (Devnet)

| Program | ID | Role |
|---|---|---|
| **pfda-amm** | `CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk` | Legacy: 2-token PFDA (Muse's original + Switchboard + Jito). Kept as regression test. |
| **pfda-amm-3** | `DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf` | **Canonical ETF A**: 3-token PFDA batch auction with Switchboard oracle bounding + Jito bid/treasury |
| **axis-g3m** | `65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi` | **Current ETF B rehearsal**: 5-token G3M AMM with drift detection plus manual/state-sync rebalance |
| **axis-vault** | `DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy` | ETF token lifecycle: create, deposit/mint, withdraw/burn |

Upgrade authority: `6t4B1TVgSjnAM9h5MpahLhGc9MtWFTGmcaPsy9JGskoV`

---

## Canonical A/B Test Paths

### ETF A: 3-Token PFDA (pfda-amm-3)

The canonical test path for ETF A uses the **3-token PFDA** program with oracle bounding and bid/treasury payment.

```bash
cd pfda-amm/programs/pfda-amm-3/client
npm install
npx ts-node oracle-bid-e2e.ts   # Full canonical path: oracle + bid + treasury
npx ts-node e2e.ts               # Basic path without oracle/bid (faster, for quick checks)
RPC_URL=http://localhost:8899 WINDOW_SLOTS=10 npx ts-node e2e.ts   # Local validator equivalent used in CI
```

### ETF B: 5-Token G3M Rehearsal (axis-g3m)

```bash
cd axis-g3m/client
npm install
npx ts-node e2e-devnet.ts
```

The ETF B devnet script now uses a fresh funded run authority each time, so it is safely rerunnable without colliding with an existing `g3m_pool` PDA from a previous successful run.

### One-Command Rehearsal

```bash
cd scripts
npm install
npx ts-node run-ab-rehearsal.ts   # Runs ETF A canonical flow + current ETF B rehearsal
```

---

## Quick Start (5 minutes)

### 1. Install prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Solana CLI
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

# Node.js 22+ (via nvm or nodejs.org)
nvm install 22 # or download from https://nodejs.org

# Verify
solana --version   # 3.x
node --version     # 22+
```

### 2. Set up your wallet

```bash
# Create a new devnet wallet (or skip if you already have one)
solana-keygen new --outfile ~/.config/solana/id.json

# Point to devnet
solana config set --url devnet

# Get test SOL
solana airdrop 2
solana airdrop 2
# If rate-limited, use https://faucet.solana.com

# Check
solana balance   # Should be 2-4 SOL
```

### 3. Clone and install

```bash
git clone https://github.com/tobySolutions/SolanaAMM.git
cd SolanaAMM
```

---

## ETF A Details: 3-Token PFDA Batch Auction

The canonical ETF A test (`oracle-bid-e2e.ts`) exercises:

1. Creates 3 test token mints
2. Initializes a PFDA pool (33.3% weight each, 100-slot batch window, 30bps fee)
3. Adds liquidity (1B tokens per vault)
4. Submits a swap request: 10M tokens of token 0, wanting token 1
5. Waits for the batch window to end (~40 seconds on devnet)
6. **Clears the batch with 3 Switchboard oracle accounts + bid payment to treasury**
7. Claims output tokens
8. **Verifies treasury balance increased** from bid payment

**Expected output:**
```
InitPool      :  ~12,000 CU
SwapRequest   :  ~10,700 CU
ClearBatch    :  ~16,000 CU  (O(1) — same CU regardless of order count)
Claim         :   ~1,900 CU
Treasury delta: +1,000,000 lamports (0.001 SOL bid)
```

### Switchboard Oracle Integration

- `oracle.rs` reads raw bytes from Switchboard PullFeedAccountData accounts
- Price is at byte offset 1272 (i128 scaled by 10^18), verified against live devnet feed
- Pass 3 oracle feed accounts as accounts[6..8] in ClearBatch
- Clearing price is bounded within ±5% of oracle price
- If oracle feeds fail to read, falls back to invariant-based pricing (graceful degradation)

### Jito Bid / Treasury Integration

- ClearBatch accepts `bid_lamports` in instruction data (first 8 bytes after discriminant)
- If `bid_lamports > 0` and accounts[9] is a valid treasury, SOL transfers from cranker to treasury
- Minimum bid: 0.001 SOL (anti-spam)
- Revenue split: 50% protocol / 50% LP (configurable via `alpha_bps`)

---

## ETF B Details: 5-Token G3M AMM

**Status:** this is the current ETF B rehearsal path, not the full March 2026 spec implementation. The original spec calls for same-transaction automatic Jupiter-backed rebalancing when drift breaches 5%. The deployed `axis-g3m` program currently exercises drift detection plus a rebalance state update, but it does not perform a live Jupiter CPI on devnet.

The test (`e2e-devnet.ts`) exercises:

1. Creates 5 test token mints (simulating CEX-unlisted memecoins)
2. Initializes a G3M pool (5 tokens, 20% weight each, 1% fee, 5% drift threshold)
3. Executes a small swap (10 tokens: token 0 → token 1)
4. **CheckDrift — returns structured data: max_drift_bps, token index, threshold, needs_rebalance**
5. Executes a large swap (200 tokens) to push drift above 5%
6. **CheckDrift again — shows threshold exceeded**
7. Rehearses the rebalance state transition back toward target weights

**Expected output:**
```
InitializePool  :  ~22,000 CU
Swap            :  ~18,000 CU
CheckDrift      :   ~3,100 CU
  Max drift    : 42 bps (token 0)     ← below threshold
  Needs rebal  : false
LargeSwap       :  ~18,000 CU
CheckDrift (post-big-swap):
  Max drift    : 812 bps (token 2)    ← above 500 bps threshold
  Needs rebal  : true ** THRESHOLD EXCEEDED **
Rebalance       :  ~13,300 CU
```

### G3M Invariant

Maintains `∏ x_i^{w_i} = k` where x_i are reserves and w_i are target weights. Swaps are priced to preserve the invariant. Fee accrual makes k monotonically increasing.

### Drift-Based Rebalancing

- Drift = |actual_weight - target_weight| / target_weight (in basis points)
- When any token's drift exceeds the threshold (default 5%), the pool is eligible for rebalance
- `CheckDrift` returns the structured trigger signal used by the rehearsal scripts
- `Rebalance` currently updates pool state and recomputes invariant `k`
- A true Jupiter-routed same-transaction CPI rebalance remains a follow-up implementation

---

## Legacy Test Scripts (Regression)

These use the 2-token PFDA program (`pfda-amm`). They are retained as regression tests but are **not** the canonical ETF A path.

```bash
cd pfda-amm/client
npx ts-node e2e.ts              # 2-token basic flow
npx ts-node test-oracle.ts      # 2-token oracle integration
npx ts-node test-jito-bid.ts    # 2-token Jito bid verification
```

---

## Run the A/B Benchmark (local validator)

Compares both ETFs side by side on a local test validator.

```bash
# Terminal 1: Start validator with all programs
cd SolanaAMM
solana-test-validator \
  --bpf-program DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf pfda-amm/target/deploy/pfda_amm_3.so \
  --bpf-program 65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi axis-g3m/target/deploy/axis_g3m.so \
  --reset

# Terminal 2: Run benchmark
solana config set --url localhost
cd benchmark
npm install
npm run bench
```

**Output: side-by-side CU comparison table.**

---

## Build from Source

```bash
# Build all programs
cd pfda-amm && cargo build-sbf
cd ../axis-g3m && cargo build-sbf
cd ../axis-vault && cargo build-sbf

# Run all unit tests
cd ../pfda-amm && cargo test
cd ../axis-g3m && cargo test

# Binaries are at:
#   pfda-amm/target/deploy/pfda_amm.so
#   pfda-amm/target/deploy/pfda_amm_3.so
#   axis-g3m/target/deploy/axis_g3m.so
```

### Deploy your own copy

```bash
# Generate fresh program keypairs
solana-keygen new --outfile my-pfda3.json --no-bip39-passphrase
solana-keygen new --outfile my-g3m.json --no-bip39-passphrase

# Deploy (requires ~1 SOL per program)
solana program deploy pfda-amm/target/deploy/pfda_amm_3.so --program-id my-pfda3.json
solana program deploy axis-g3m/target/deploy/axis_g3m.so --program-id my-g3m.json

# Update PROGRAM_ID in the client scripts to match your new IDs
```

---

## Repository Structure

```
SolanaAMM/
├── pfda-amm/                         # ETF A programs
│   ├── programs/
│   │   ├── pfda-amm/                 # Legacy: 2-token PFDA (regression tests only)
│   │   │   └── src/
│   │   │       ├── oracle.rs         # Switchboard price feed reader
│   │   │       └── jito.rs           # Jito auction bid enforcement
│   │   │
│   │   └── pfda-amm-3/              # ★ Canonical ETF A: 3-token PFDA
│   │       └── src/
│   │           ├── lib.rs            # Entrypoint (6 instructions)
│   │           ├── instructions/     # InitPool, SwapRequest, ClearBatch, Claim, AddLiquidity, WithdrawFees
│   │           ├── state/            # PoolState3, BatchQueue3, UserOrderTicket3, ClearedBatchHistory3
│   │           ├── oracle.rs         # Switchboard oracle (ported from 2-token)
│   │           └── jito.rs           # Jito bid/treasury (ported from 2-token)
│   │
│   └── client/                       # Legacy 2-token e2e + oracle test
│
├── axis-g3m/                         # ★ Current ETF B rehearsal: 5-token G3M
│   ├── programs/axis-g3m/
│   │   └── src/
│   │       ├── instructions/         # InitializePool, Swap, CheckDrift, Rebalance
│   │       ├── state/pool_state.rs   # G3mPoolState (drift computation, invariant k)
│   │       ├── math/fp64.rs          # G3M invariant + swap math
│   │       ├── jupiter.rs            # Vault balance reader for keeper rebalance
│   │       └── error.rs
│   └── client/                       # e2e tests (local + devnet)
│
├── axis-vault/                        # ETF token lifecycle
│   └── programs/axis-vault/src/
│
├── scripts/                          # Metrics collector, rehearsal, Switchboard setup
├── benchmark/                        # A/B CU comparison
└── DEVNET_TESTING.md                 # This file
```

---

## Key Concepts

### O(1) Batch Clearing (ETF A)
- Swaps are batched into windows (default 10 slots ≈ 4 seconds)
- During window: `SwapRequest` only increments a u64 counter (`total_in += amount`). No loops.
- At window end: `ClearBatch` reads two numbers, computes one clearing price. No iteration over users.
- Users call `Claim` to withdraw their proportional share. Single multiply. No loops.
- **Proven O(1)**: same CU for 1 user or 1000 users

### G3M Invariant (ETF B)
- Maintains `∏ x_i^{w_i} = k` where x_i are reserves and w_i are target weights
- Swaps are priced to preserve the invariant. Fee accrual makes k monotonically increasing.
- Drift = how far actual weights deviate from targets. Keeper rebalances at >5%.

### Fee Handling
- ETF A: 30bps fee applied at claim time (not at clearing — avoids cancellation bug)
- ETF B: 1% fee applied at swap time (deducted from input before pricing)

---

## Metrics Collection (48-hour A/B test)

```bash
cd scripts
npx ts-node collect-ab-metrics.ts
# Runs until Ctrl+C. Writes to:
#   metrics/etf-a-metrics.jsonl
#   metrics/etf-b-metrics.jsonl
```

---

## Troubleshooting

| Problem | Fix |
|---|---|
| `insufficient funds` | `solana airdrop 2` or use https://faucet.solana.com |
| `account already in use` | Pool already created. Each wallet gets one pool per program. |
| Airdrop rate limited | Wait 30 seconds or use the web faucet |
| Build fails `edition2024` | Run `agave-install update` to get latest Solana CLI |
| ClearBatch `window not ended` | Increase `WINDOW_SLOTS` in the test or wait longer |
| `custom program error: 0x1` | Usually means a vault doesn't have enough tokens for the transfer |
| Oracle clear fails | Oracle feed may be stale. Falls back to invariant pricing automatically. |
