# Axis Vault — SOL-in / SOL-out helpers

Pump-style UX for ETFs: **pay in SOL, receive SOL on exit.** Closes [issue #36](../../../issues/36).

## Why client-bundled (not on-chain CPI)

Production basket / ETF protocols on Solana (Kamino, Drift, Marginfi, DeFi Land vaults, etc.) all integrate Jupiter the same way: **the client assembles a single versioned transaction** with Jupiter swaps + the protocol's own Deposit / Withdraw IX, using an Address Lookup Table to fit the large account list.

This gives us the same atomicity guarantee as an on-chain CPI (a versioned tx is all-or-nothing) without the three big downsides of CPI'ing Jupiter from inside axis-vault:

1. **Account list blow-up.** A single Jupiter route carries 20–40 accounts; 5 legs × 30 ≈ 150 accounts, well over even the ALT-expanded envelope if you also need the program's own accounts.
2. **CU budget.** A Jupiter swap costs 200–400 k CU. Five legs alone blow through the 1.4 M CU cap before axis-vault's Deposit logic runs.
3. **No Jupiter on devnet.** On-chain CPI forces every test to mainnet-fork. Client-bundled tests work against any RPC that clones Jupiter state.

Same atomicity, no CPI, no ALT plumbing inside the program.

## Files

| File | Purpose |
|------|---------|
| `jupiter.ts` | Thin wrapper over Jupiter V6 Swap API (`/quote`, `/swap-instructions`). |
| `deposit-sol.ts` | Assembles `[ComputeBudget][Jupiter × N][axis-vault Deposit]` versioned tx. |
| `withdraw-sol.ts` | Assembles `[ComputeBudget][axis-vault Withdraw][Jupiter × N][Close wSOL]` versioned tx. |

## Usage

```ts
import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import { buildDepositSolPlan } from "./scripts/axis-vault/deposit-sol";

const conn = new Connection(RPC_URL, "confirmed");
const plan = await buildDepositSolPlan({
  conn,
  user: wallet.publicKey,
  programId: AXIS_VAULT_PROGRAM_ID,
  etfName: "AXBTC",
  etfState,            // PDA: [b"etf", authority, name]
  etfMint,             // stored on EtfState
  treasuryEtfAta,      // owner = etf.treasury, mint = etfMint
  basketMints: [WBTC, WETH, JITOSOL],
  weights: [5000, 3000, 2000],        // bps, sums to 10_000
  vaults: [wbtcVault, wethVault, jitosolVault],
  solIn: 1_000_000_000n,               // 1 SOL
  minEtfOut: 900_000n,                 // user-side slippage guard
  slippageBps: 50,                     // Jupiter-side (per leg)
});

plan.versionedTx.sign([wallet]);
const sig = await conn.sendTransaction(plan.versionedTx);
```

Both helpers return a `VersionedTransaction` ready to sign + send, plus diagnostic data (Jupiter quotes, expected per-leg outputs, ALT accounts, instruction count).

## Slippage layering

Three independent guards:

| Layer | Where | What it catches |
|-------|-------|-----------------|
| `slippageBps` per leg | Jupiter `otherAmountThreshold` | Adverse price move between quote and execution. |
| `minEtfOut` / `minSolOut` | axis-vault Deposit (`min_mint_out`) / client check | Rounding drift, composition drift, Jupiter partial fill. |
| `MAX_NAV_DEVIATION_BPS = 300` | axis-vault Deposit (on-chain) | Vault ratio drift between quote and execution (issue #18). |

All three have to pass — conservative by design.

## Testing

### Against real Jupiter (mainnet-fork)

Jupiter V6 is mainnet-only, so the integration test runs against a local validator cloning mainnet state:

```bash
# Terminal 1 — start a Jupiter-populated local validator
solana-test-validator \
  --reset \
  --url https://api.mainnet-beta.solana.com \
  --clone JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4 \
  --clone <each DEX program Jupiter routes through> \
  --bpf-program DeeUnCHcnPG8arbjGTLhTKeDhpPUBper3TDrpFPHnCwy \
    contracts/axis-vault/target/deploy/axis_vault.so

# Terminal 2 — run the plan helper against real Jupiter
RPC_URL=http://localhost:8899 bun scripts/axis-vault/demo.ts
```

`axis-g3m` has the same pattern wired up at `test/e2e/axis-g3m/axis-g3m.jupiter-fork.e2e.ts` — copy its `solana-test-validator --clone` invocation for the full DEX account list.

### Without Jupiter (typecheck only)

```bash
bash ci/ts-typecheck.sh   # covered by scripts/tsconfig.json
```

## Follow-up work tracked under #36

Shipped here:
- [x] Client-side Jupiter route construction
- [x] Versioned transaction assembly with ALT support
- [x] Deposit SOL-in path
- [x] Withdraw SOL-out path
- [x] Slippage layering (user + Jupiter + NAV)

Deferred (separate PRs when product ships):
- [ ] Dust sweep utility — small basket-token leftovers in user ATAs after `Deposit` (inherent to the client-bundled flow, see `deposit-sol.ts` head comment).
- [ ] Mainnet-fork CI integration (requires a scripted `--clone` list that stays in sync with Jupiter's canonical DEX surface).
- [ ] Native `DepositSol` / `WithdrawSol` on-chain instructions for callers that need strict on-chain `minSolOut` enforcement (today it's client-checked).
