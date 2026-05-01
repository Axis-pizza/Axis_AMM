# axis-vault v1.1 — Metaplex Token Metadata CPI in `CreateEtf`

**Status:** Draft, pending user sign-off before code lands.
**Target program:** `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX` (mainnet-beta, OtterSec-verified at v1.0).
**Upgrade authority:** Squads V4 vault `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6` (2-of-2 @muse0509 / @kidneyweakx).
**Author:** axis-protocol team, 2026-05-01.

---

## 1. Why

v1.0 mints an SPL token with no on-chain metadata. Wallets and explorers (Phantom, Solflare, Solscan, Jupiter token list) fall back to "Unknown Token" with a generic icon, which:

- Breaks the "branded ETF" UX the closed-beta promised.
- Forces token-list submissions (Jupiter, CoinGecko) to be redone every time a new ETF is created.
- Leaves no on-chain provenance link from the SPL mint to the human-readable name/ticker we already store in `EtfState`.

Three integration paths were considered:

| Path | Sketch | Verdict |
|---|---|---|
| **(A) CPI to Metaplex Token Metadata in `CreateEtf`** | Append `CreateMetadataAccountV3` at the end of `process_create_etf`, with `etfState` PDA signing as `mint_authority`. | **Chosen.** Single ix, atomic with mint creation, can never desync. |
| (B) Separate `CreateEtfMetadata` ix (disc 9) | Caller must invoke after `CreateEtf`. Same PDA seeds re-derived. | Two-step flow, potential half-init state if call 2 never lands. |
| (C) Off-chain metadata only (token list PRs) | No program change. Submit to Jupiter / Solflare lists. | Doesn't scale — every new ETF needs human-in-the-loop. |

Path A also matches how the rest of the protocol works (Jupiter CPI in `DepositSol`, see `axis-vault/src/jupiter.rs`), so the tooling is in place.

## 2. What changes

### 2.1 New CPI target

[Metaplex Token Metadata Program](https://developers.metaplex.com/token-metadata) at `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`. We invoke `CreateMetadataAccountV3` (instruction discriminator `33`).

A new `metaplex.rs` module sits alongside `jupiter.rs` and:

- Pins the program ID as a 32-byte constant (`METAPLEX_TOKEN_METADATA_PROGRAM_ID`).
- Exposes `invoke_create_metadata(ctx, name, symbol, uri, signer_seeds)` — hand-rolled borsh serializer for `CreateMetadataAccountV3InstructionArgs`. No `mpl-token-metadata` or `borsh` crate dep — Pinocchio stays clean.

The borsh layout we hand-roll (matches `mpl-token-metadata` v1.13.x):

```
[disc: u8 = 33]
[name_len: u32 LE][name: bytes]            ; max 32
[symbol_len: u32 LE][symbol: bytes]        ; max 10  ⚠ see §5
[uri_len: u32 LE][uri: bytes]              ; max 200
[seller_fee_basis_points: u16 LE = 0]
[creators: u8 = 0]                         ; None
[collection: u8 = 0]                       ; None
[uses: u8 = 0]                             ; None
[is_mutable: u8 = 1]                       ; see §3
[collection_details: u8 = 0]               ; None
```

Worst-case payload: `1 + 4+32 + 4+10 + 4+200 + 2 + 1 + 1 + 1 + 1 + 1 = 262 bytes`. Stack-only, no heap.

### 2.2 New accounts in `CreateEtf`

Appended after the existing `6 + 2N` accounts:

| Index | Name | Signer | Writable | Notes |
|---|---|---|---|---|
| `6+2N`   | `metadata_pda`   | no | **yes** | Created by Metaplex CPI. PDA = `[b"metadata", METAPLEX_PROGRAM_ID, etf_mint]`. Validated via `find_program_address` before CPI. |
| `6+2N+1` | `metaplex_program` | no | no | Must equal `METAPLEX_TOKEN_METADATA_PROGRAM_ID`; rejected with `VaultError::InvalidMetaplexProgram` otherwise. |

`mint_authority`, `update_authority`, `payer`, `system_program`, and `mint` are all already in the existing account list — Metaplex re-uses `accounts[1]` (etfState PDA) for both authorities, `accounts[0]` for payer, `accounts[4]` for system_program, `accounts[2]` for mint. No `rent` sysvar — `CreateMetadataAccountV3` accepts it as optional in v1.13+ and we omit.

### 2.3 New instruction-data field

Appended after the existing `name` blob:

```
[uri_len: u8][uri: bytes]   ; uri_len ∈ [0, 200]
```

`uri_len = 0` is allowed and emits an empty-URI metadata account (legitimate use case: ETF that wants on-chain name+ticker but no off-chain JSON yet).

### 2.4 No state-struct change

`EtfState` layout is unchanged. Discriminator stays `etfstat3`. No re-init required for any future ETFs. The `_padding: [u8; 4]` tail still has room for v1.2 additions.

## 3. Authority model

| Field | Value | Rationale |
|---|---|---|
| `mint_authority` | etfState PDA (signer via seeds) | Matches v1.0 — etfState PDA already mints/burns. |
| `update_authority` | etfState PDA | Program-controlled. Direct Metaplex `UpdateMetadataAccountV2` calls fail (PDA can't sign without seeds, and only axis-vault knows the seeds). Future v1.2 can add an authority-gated `UpdateEtfMetadata` ix that re-derives seeds. |
| `is_mutable` | `true` | Cheap insurance: lets v1.2 ship metadata updates without a discriminator bump or new mint. If we set `false` now, the only way to fix a typo'd URI is to re-deploy the ETF. |
| `seller_fee_basis_points` | `0` | Fungible token, not NFT royalty. Fees flow through `treasury` per existing model. |
| `creators` / `collection` / `uses` | `None` | Out of scope for v1.1. Fee splits already handled by `treasury` + `fee_bps`. |

## 4. Updated wire format

### `CreateEtf` (discriminator 0)

**Accounts** (delta vs v1.0):

```
0   [signer, writable] authority
1   [writable]          etfState PDA
2   [writable]          etfMint
3   []                  treasury
4   []                  systemProgram
5   []                  tokenProgram
6..6+N        []         basketMints
6+N..6+2N     [writable] basketVaults
6+2N          [writable] metadataPda          ← NEW
6+2N+1        []         metaplexProgram      ← NEW
```

**Instruction data** (delta vs v1.0):

```
[disc: u8 = 0]
[token_count: u8]
[weights_bps: u16 LE × N]
[ticker_len: u8][ticker: bytes]
[name_len: u8][name: bytes]
[uri_len: u8][uri: bytes]                     ← NEW
```

Existing v1.0 clients hitting v1.1 will fail with `InvalidInstructionData` (missing `uri_len`) before any state writes — fail-loud, not fail-silent. Tracked as the deliberate breaking change in `MAINNET_SCOPE.md` change-log.

## 5. Resolved decisions (2026-05-01, user sign-off)

### 5.1 Symbol-length cap → **10**

`process_create_etf` rejects `ticker.len() > 10` with `VaultError::InvalidSymbolLength` (9023). `MAX_ETF_TICKER_LEN = 16` constant stays for state-layout backwards compat with any v1.0 ETF, but new CreateEtf calls are bounded by Metaplex's `MAX_SYMBOL_LENGTH`. Current creator ticker patterns are ~6 chars, so this is non-disruptive.

### 5.2 Existing v1.0 ETFs → **leave as-is**

No `BackfillMetadata` ix. Existing v1.0 ETFs (if any exist) keep their metadata-less state. v1.1 only affects new CreateEtf calls. No off-chain backfill, no per-ETF migration tx.

## 6. Errors

New `VaultError` variants (offset 9000+ as per existing scheme):

```rust
InvalidMetaplexProgram = 9020,
InvalidMetadataPda    = 9021,
InvalidUri            = 9022,    // uri_len > 200
InvalidSymbolLength   = 9023,    // ticker > 10 (if §5.1 accepted)
```

## 7. Test plan

### 7.1 Unit (cargo test) — ✅ landed

- `metaplex.rs::tests::borsh_layout_minimal` — exact byte-layout assertion against the mpl-token-metadata v1.13.x reference encoding.
- `metaplex.rs::tests::empty_uri_packs_with_zero_len_prefix` — borsh empty-string contract.
- `metaplex.rs::tests::worst_case_payload_fits_in_buffer` — bound check on the stack buffer.
- Existing `axis_vault::tests::print_sizes` and `constants::tests::*` still pass — `EtfState` layout unchanged.

### 7.2 LiteSVM / Surfpool integration — ⏳ not yet shipped

To add as a follow-up `test/e2e/axis-vault/create-etf-metadata.e2e.ts`:

- Happy path: CreateEtf → fetch metadata PDA → assert `name`/`symbol`/`uri` match.
- URI=empty: same assertions, empty URI string.
- Ticker > 10: rejected with `VaultError::InvalidTicker` (9019).
- Mis-derived `metadata_pda` rejected with `InvalidMetadataPda` (9038).
- Wrong `metaplex_program` rejected with `InvalidMetaplexProgram` (9037).
- Replay: second CreateEtf with same name reverts on `AlreadyInitialized` (existing path) before reaching CPI.

### 7.3 Forked-mainnet test — required before mainnet upgrade

Surfpool `--fork mainnet-beta` so the real Metaplex program is loaded. Confirms the CPI works against the deployed v1.13.x token-metadata program (not just a local fake). **Hard prerequisite for §8 step 4.**

### 7.4 Updated e2e files

- ✅ `test/e2e/axis-vault/axis-vault.local.e2e.ts` (happy path + the negative tests later in the same file).
- ⏳ `test/e2e/axis-vault/axis-vault.set-paused.test.ts` — still uses v1.0 wire format, will fail with `InvalidInstructionData` after upgrade.
- ⏳ `test/e2e/axis-g3m/axis-g3m.local.e2e.ts`, `axis-g3m.devnet.e2e.ts`, `axis-g3m.jupiter-fork-disc4.e2e.ts` — only if they call CreateEtf directly.
- ⏳ `test/e2e/integration/full-link.e2e.ts`, `test/e2e/pfda-amm-legacy/pfda-amm-legacy.local.e2e.ts` — same.

These are mechanical wire-format updates: append `[uri_len][uri]` to the data buffer and add `metadataPda` + `metaplexProgram` keys after the basket vaults. None block the program build.

## 8. Deploy sequence

1. PR review (both Squads signers must approve).
2. `solana-verify build --library-name axis_vault`. Capture new `sbfSha256`.
3. `solana program write-buffer target/deploy/axis_vault.so --buffer-authority <Squads V4>`. Capture buffer pubkey.
4. Construct Squads V4 tx: `Upgrade` ix targeting program ID `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX` with `buffer = <step 3>`, `spill = <muse0509>`, `upgrade_authority = <Squads V4 vault>`.
5. 2-of-2 sign + execute.
6. Verify: `solana program show Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX -u mainnet-beta` — assert `Last Deployed Slot` matches the upgrade tx and `Data Length` increased by ~the size of the new CPI logic.
7. Smoke test: CreateEtf on mainnet with a tiny throwaway basket, fetch metadata PDA from Solscan, assert it renders in Phantom.
8. Update `idl/axis_vault.json` deploy metadata (slot, signature, sha256). PR + tag `v1.1.0`.
9. Announce in #axis-protocol with the new ETF token's metadata URL.

## 9. Out of scope (v1.2+)

- `UpdateEtfMetadata` ix (authority-gated, re-derives etfState PDA seeds, calls `UpdateMetadataAccountV2`).
- Master Edition for ETF tokens (treats ETF as semi-fungible; not a current need).
- Verified collection grouping (`collection: Some(...)`) — needed if we want a Phantom "Axis ETFs" category but adds an extra Collection NFT setup step per ETF.
- Codama codegen against the updated IDL.
