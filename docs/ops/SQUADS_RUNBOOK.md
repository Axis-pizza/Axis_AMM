# PROTOCOL_TREASURY Squads V4 deploy

Closes the ops-side of #61 item 3 once executed. Code side is done in
the bundle PR that introduced this doc — `axis-vault/src/constants.rs`
ships a sentinel zero key, the gate in `protocol_treasury_is_active()`
stays inert until this runbook flips the constant.

## Who

2-of-2 between **@muse0509** and **@kidneyweakx** per Muse's 2026-04-20
decision. Both signers must own the wallet they bring to Squads —
neither can delegate.

## Order of operations

Strict order, no skipping. Steps 2–4 are committed code; step 1 is
pure ops. The mainnet Squads vault has already been provisioned, so
this runbook starts at vault-address verification.

### 1. Mainnet vault

Created via Squads UI (https://app.squads.so) with **Network: Mainnet
Beta**, **Threshold: 2 of 2**, members = muse0509's wallet +
kidneyweakx's wallet.

- `MAINNET_VAULT` = `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6`
  (Squads V4 vault PDA, `vault_index = 0`, shown in Receive Assets →
  Account 1)
- Both signers must verify this address out-of-band (Slack DM, etc.)
  before anyone signs a deploy against it

### 2. Code: flip the constant

Single commit on a branch off `main`:

```bash
git checkout -b ops/issue-38-protocol-treasury-flip
bun scripts/ops/flip-protocol-treasury.ts <MAINNET_VAULT>
cargo build-sbf \
  --manifest-path contracts/axis-vault/Cargo.toml \
  --sbf-out-dir contracts/axis-vault/target/deploy
git add contracts/axis-vault/src/constants.rs
git commit -m "ops(#38): flip PROTOCOL_TREASURY to mainnet Squads vault"
git push
```

PR review must include both signers. The diff is small; the
verification is making sure the 32 bytes match the Squads vault
address character-for-character.

Note: `axis-vault` is a Pinocchio program and does not use
Anchor-style `declare_id!()`. The mainnet program ID is controlled by
the deploy keypair passed to `solana program deploy --program-id`.

### 3. Deploy

Muse executes the deploy as a Squads tx (axis-vault program upgrade).
This requires the multisig to sign — kidneyweakx approves the tx in
the Squads UI.

### 4. Verify on-chain

```bash
# 4a: program upgrade succeeded
solana program show <AXIS_VAULT_PROGRAM_ID> -u mainnet-beta

# 4b: gate is now live — try a CreateEtf with the wrong treasury,
# expect TreasuryNotApproved
bun scripts/axis-vault/demo.ts --treasury <SOME_OTHER_KEY>
# expect: failed with custom error 9023

# 4c: try with the right treasury, expect success
bun scripts/axis-vault/demo.ts --treasury <MAINNET_VAULT>
# expect: ETF created, deposit/withdraw round-trip clean
```

### 5. Close #59 + related #61 items

Once 4c is green, mark issue #59 closed and item #3 in #61 done. The
treasury gate is now permanently active for every CreateEtf.

## Rollback

If step 4c fails:
- Do NOT flip the constant back to zero — that re-opens the silent
  bypass. Instead deploy a hotfix that uses the OLD vault address (if
  this is a rotation) or pause the program via `SetPaused` while the
  team investigates.
- Open a P0 issue with the on-chain log output from the failing
  CreateEtf for diagnosis.

## Threat model notes

- Single-signer compromise: 2-of-2 means losing one wallet doesn't
  drain the treasury. Losing both is catastrophic — both signers
  should use hardware wallets (Ledger / Squads-supported).
- Squads program upgrade: Squads V4 itself is upgrade-controlled by
  the Squads team. Their public security policy is at
  https://squads.so/security; if that becomes a concern, treasury
  rotation to a different multisig program is a code-side change to
  the constant.
- The fallback path (`pool.authority` for legacy pfda-amm pools, see
  PR #64 #61 item 4) does NOT route through this treasury. Once
  every active pool is migrated to the v2 schema with a non-zero
  treasury field, that fallback is dead code and can be removed.
