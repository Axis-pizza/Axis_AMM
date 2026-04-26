# Axis AMM — Devnet Demo

Tiny Vite + React + TypeScript dashboard that connects to Solana devnet, reads the deployed `axis-vault` and `pfda-amm-3` program state, and exposes a wallet-connect + airdrop sanity action.

## Why it exists

Pure smoke-testing harness for the kidney-owned mainnet-scope deploy. Lets a non-engineer verify the programs are live on-chain through a real Phantom/Solflare wallet, without cloning the repo or running `bun run e2e:*`.

## Stack

- **Vite 7** + **React 19** + **TypeScript** — minimal scaffolding, no Next.js (no SSR / API routes needed for a read-mostly dashboard).
- **Tailwind CSS v4** via `@tailwindcss/vite` — dark theme, no custom design system.
- **`@solana/wallet-adapter-react`** + **Phantom**/**Solflare** adapters — picks up whatever's installed in the browser.
- **`@solana/web3.js`** for RPC reads. Matches the e2e test stack so the bytecode path is identical.

## Run it

```bash
cd frontend
bun install   # or npm install
bun run dev
```

Opens at <http://localhost:5173>. The default RPC is `https://api.devnet.solana.com` — change in `src/lib/programs.ts` if you want to point at the canonical Muse-owned IDs instead.

## What the demo does

1. **Header** — shows the current cluster + RPC URL.
2. **Scope note** — reminds visitors this is un-audited research, links to `MAINNET_SCOPE.md`.
3. **Wallet panel** — connect Phantom/Solflare, shows pubkey + SOL balance, exposes a one-click "Airdrop 1 devnet SOL" action so the user can prove the wallet→RPC path works without touching our programs.
4. **Program cards** — for each deployed program: live status, upgrade authority, code-data size, link to Solana Explorer. Reads ProgramData via the BPFLoaderUpgradeable layout to surface the real upgrade authority (the most useful "is this the right deploy?" signal).

## What it does NOT do (intentionally)

- No CreateEtf / Deposit / Withdraw flow. The on-chain ix builders are hand-rolled in `test/e2e/**/*.ts`; pulling them into the frontend would 5× the LoC for a smoke demo. Those flows live in the e2e suite.
- No history / metrics. The A/B research data is in `reports/ab/`; no point re-rendering it client-side.
- No mainnet support. This stack defaults to devnet and that's the only environment the program IDs in `src/lib/programs.ts` exist on.

## Updating program IDs

Re-run `scripts/ops/deploy-devnet.sh --fresh --mainnet-scope` from the repo root → it writes `.env.devnet.kidney.mainnet-scope`. Copy the IDs into `src/lib/programs.ts` and restart `bun run dev`.

## Layout

```
src/
├── App.tsx                    # page composition + header/footer
├── main.tsx                   # entry, Buffer/global polyfill for wallet-adapter
├── index.css                  # Tailwind + wallet-adapter overrides
├── components/
│   ├── WalletProvider.tsx     # @solana/wallet-adapter wiring (Phantom + Solflare)
│   ├── WalletPanel.tsx        # connect button, balance, airdrop action
│   ├── ProgramCard.tsx        # one card per deployed program
│   └── ScopeNote.tsx          # static "research, not audited" disclaimer
└── lib/
    ├── programs.ts            # constant program IDs + RPC URL
    └── format.ts              # truncatePubkey / lamportsToSol / formatBytes
```
