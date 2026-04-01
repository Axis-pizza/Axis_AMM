# Axis Protocol — A/B Test: Devnet Testing Guide

## What This Is

Four on-chain programs deployed to Solana devnet for the Axis Protocol A/B test. The goal is to compare a **PFDA batch auction** (ETF A) against a **G3M continuous AMM** (ETF B) under real conditions.

- **ETF A** batches swaps into windows and clears them at a single price (MEV protection)
- **ETF B** executes swaps continuously against a geometric mean invariant (standard AMM)
- **Vault** manages ETF token lifecycle: deposit basket tokens, receive ETF tokens; burn ETF tokens, receive basket back

All programs accept **any SPL tokens** — the token choice is a deployment-time decision, not hardcoded.

---

## Deployed Programs (Devnet)

| Program | ID | Purpose |
|---|---|---|
| **pfda-amm** | `CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk` | ETF A: 2-token PFDA batch auction (Muse's original + Switchboard + Jito) |
| **pfda-amm-3** | `DbAPmgkrpCCZrpBMv5x1ye6nJUreqY313SuQjZsMyjEf` | ETF A: 3-token PFDA batch auction (for SOL/BONK/WIF or any 3 tokens) |
| **axis-g3m** | `65aE9QdVz5bapV19BGt5cyTgVitYpekGwusRoQEovNUi` | ETF B: 5-token G3M AMM with drift-based rebalancing |
| **axis-vault** | `DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy` | ETF token lifecycle: create, deposit/mint, withdraw/burn |

Upgrade authority: `6t4B1TVgSjnAM9h5MpahLhGc9MtWFTGmcaPsy9JGskoV`

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

## Test ETF B: G3M (5-token pool)

This creates a pool with 5 test tokens, executes swaps, checks drift, and rebalances.

```bash
cd axis-g3m/client
npm install
npx ts-node e2e-devnet.ts
```

**What it does:**
1. Creates 5 test token mints (any arbitrary SPL tokens)
2. Creates user token accounts and mints 100,000 tokens each
3. Initializes a G3M pool (5 tokens, 20% weight each, 1% fee, 5% drift threshold)
4. Executes a small swap (10 tokens: token 0 → token 1)
5. Checks drift levels
6. Executes a large swap (200 tokens) to push drift above 5%
7. Rebalances the pool back to target weights
8. Prints CU consumption for every instruction

**Expected output:**
```
InitializePool  :  ~22,000 CU
Swap            :  ~18,000 CU
CheckDrift      :   ~3,100 CU
Rebalance       :  ~13,300 CU
```

**Note:** Each wallet gets its own pool (PDA derived from your pubkey). If you run it twice, the second run will fail with "account already in use" — that just means your pool already exists from the first run.

---

## Test ETF A: 3-Token PFDA (batch auction)

This creates a 3-token pool, submits a swap intent, waits for the batch window to close, clears the batch, and claims output tokens.

```bash
cd pfda-amm/programs/pfda-amm-3/client
npm install
npx ts-node e2e.ts
```

**What it does:**
1. Creates 3 test token mints
2. Initializes a PFDA pool (33.3% weight each, 100-slot batch window, 30bps fee)
3. Adds liquidity (1,000 tokens per vault)
4. Submits a swap request: 10 tokens of token 0, wanting token 1
5. Waits for the batch window to end (~40 seconds on devnet)
6. Clears the batch (O(1) settlement — all intents settled at one price)
7. Claims output tokens (should receive ~9.97 tokens after 30bps fee)

**Expected output:**
```
InitPool      :  ~12,000 CU
SwapRequest   :  ~10,700 CU
ClearBatch    :  ~16,000 CU  ← the key O(1) metric
Claim         :   ~1,900 CU
Token 1 received: 9,970,000  (= 9.97 tokens after 30bps fee)
```

---

## Test Switchboard Oracle Integration

This tests that ClearBatch can read real price data from a Switchboard oracle feed on devnet.

```bash
cd pfda-amm/client
npm install
npx ts-node test-oracle.ts
```

**What it does:**
1. Creates a 2-token PFDA pool
2. Adds liquidity and submits a swap
3. Calls ClearBatch with a **real Switchboard price feed** (`BV9mGAy5MJLYWJT5HF74izYKjF9CmL4BqkswfTu9gW2w`) passed as an oracle account
4. Verifies the instruction succeeds with the oracle data

**Expected output:**
```
ClearBatch with oracle: CU = ~26,400
Oracle integration test PASSED
```

---

## Run the A/B Benchmark (local validator)

Compares both ETFs side by side on a local test validator.

```bash
# Terminal 1: Start validator with all programs
cd SolanaAMM
solana-test-validator \
  --bpf-program CSBgQGeBTiAu4a9Kgoas2GyR8wbHg5jxctQjq3AenKk pfda-amm/target/deploy/pfda_amm.so \
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
solana-keygen new --outfile my-pfda.json --no-bip39-passphrase
solana-keygen new --outfile my-g3m.json --no-bip39-passphrase

# Deploy (requires ~1 SOL per program)
solana program deploy pfda-amm/target/deploy/pfda_amm.so --program-id my-pfda.json
solana program deploy axis-g3m/target/deploy/axis_g3m.so --program-id my-g3m.json

# Update PROGRAM_ID in the client scripts to match your new IDs
```

---

## Repository Structure

```
SolanaAMM/
├── pfda-amm/                         # ETF A programs
│   ├── programs/
│   │   ├── pfda-amm/                 # 2-token PFDA (Muse's original)
│   │   │   └── src/
│   │   │       ├── lib.rs            # Entrypoint (6 instructions)
│   │   │       ├── instructions/     # InitPool, SwapRequest, ClearBatch, Claim, AddLiquidity, UpdateWeight
│   │   │       ├── state/            # PoolState, BatchQueue, UserOrderTicket, ClearedBatchHistory
│   │   │       ├── math/fp64.rs      # Q32.32 fixed-point: log2, exp2, pow, G3M clearing price
│   │   │       ├── oracle.rs         # Switchboard price feed reader (zero-dependency)
│   │   │       └── jito.rs           # Jito auction bid enforcement + revenue split
│   │   │
│   │   └── pfda-amm-3/              # 3-token PFDA (new)
│   │       └── src/
│   │           ├── lib.rs            # Entrypoint (4 instructions)
│   │           ├── instructions/     # InitPool, SwapRequest, ClearBatch, Claim
│   │           └── state/            # PoolState3, BatchQueue3, UserOrderTicket3, ClearedBatchHistory3
│   │
│   └── client/                       # Muse's e2e + oracle test
│
├── axis-g3m/                         # ETF B program
│   ├── programs/axis-g3m/
│   │   └── src/
│   │       ├── lib.rs                # Entrypoint (4 instructions)
│   │       ├── instructions/         # InitializePool, Swap, CheckDrift, Rebalance
│   │       ├── state/pool_state.rs   # G3mPoolState (5-token, 464 bytes)
│   │       ├── math/fp64.rs          # G3M invariant + swap math
│   │       ├── jupiter.rs            # Vault balance reader for Jupiter rebalance pattern
│   │       └── error.rs              # Error codes 7000-7017
│   └── client/                       # e2e tests (local + devnet)
│
├── axis-vault/                        # ETF token lifecycle
│   ├── programs/axis-vault/
│   │   └── src/
│   │       ├── lib.rs                # Entrypoint (3 instructions)
│   │       ├── instructions/         # CreateEtf, Deposit, Withdraw
│   │       ├── state/etf.rs          # EtfState (basket config + ETF mint)
│   │       └── error.rs              # Error codes 9000-9010
│   └── (no client yet — coming soon)
│
├── solana-tfmm-rs/                   # Economic simulation (Muse's, unchanged)
├── benchmark/                        # A/B CU comparison script
├── scripts/                          # Switchboard feed setup
└── DEVNET_TESTING.md                 # This file
```

---

## How the Integrations Work

### Switchboard (ETF A oracle)
- `oracle.rs` reads raw bytes from Switchboard PullFeedAccountData accounts
- Price is at byte offset 1272 (i128 scaled by 10^18), verified against live devnet feed
- Pass feed accounts as accounts[6] and [7] in ClearBatch
- If feeds are passed, clearing price is bounded within ±5% of oracle price
- If no feeds passed, falls back to invariant-based pricing (backwards compatible)

### Jito (ETF A auction)
- Searchers submit Jito bundles containing: SOL bid + ClearBatch + Jito tip
- ClearBatch accepts `bid_lamports` in instruction data
- If `bid_lamports > 0` and accounts[8] is a treasury, SOL transfers from cranker to treasury
- Revenue split: 50% protocol / 50% LP (configurable via `alpha_bps`)
- The Jito Block Engine selects the highest-tipping bundle off-chain

### Jupiter (ETF B rebalancing)
- Two-step pattern used by production protocols (Drift, Mango, etc.)
- Step 1: Keeper bot calls Jupiter API off-chain, executes swap to move tokens between vaults
- Step 2: Keeper calls Rebalance instruction, passing vault accounts
- On-chain: reads actual SPL token balances from vaults (trustless — no claimed amounts)
- Verifies G3M invariant maintained within 1% tolerance

---

## Key Concepts for Reviewers

### O(1) Batch Clearing (ETF A)
- Swaps are batched into windows (default 10 slots ≈ 4 seconds)
- During window: `SwapRequest` only increments a u64 counter (`total_in += amount`). No loops.
- At window end: `ClearBatch` reads two numbers, computes one clearing price. No iteration over users.
- Users call `Claim` to withdraw their proportional share. Single multiply. No loops.
- **Proven O(1)**: same CU for 1 user or 1000 users

### G3M Invariant (ETF B)
- Maintains `∏ x_i^{w_i} = k` where x_i are reserves and w_i are target weights
- Swaps are priced to preserve the invariant. Fee accrual makes k monotonically increasing.
- Drift = how far actual weights deviate from targets. Rebalance fires at >5%.

### Fee Handling
- ETF A: 30bps fee applied at claim time (not at clearing — avoids cancellation bug)
- ETF B: 1% fee applied at swap time (deducted from input before pricing)

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
