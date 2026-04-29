# Solscan / OtterSec verification runbook for `axis-vault`

Goal: flip the `is_verified` flag at
`https://verify.osec.io/status/Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX`
from `false` to `true`. Once flipped, Solscan picks the badge up
automatically (typically within ~10 minutes).

The on-chain upgrade authority for both `axis-vault` and the
verification PDA write is the **Squads V4 multisig vault**
`BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6` (2-of-2 between
@muse0509 and @kidneyweakx — see `docs/ops/SQUADS_RUNBOOK.md`). Every
on-chain step below requires the multisig to sign.

---

## Current state (snapshot 2026-04-29, after running `verify-build.sh`)

| | value |
|---|---|
| Program ID | `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX` |
| ProgramData PDA | `6szAV5iFQKzJ7BuYZipSeWc3thauWCVi9q26k1WQEjrt` |
| Upgrade authority | `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6` (Squads vault) |
| On-chain raw SHA-256 (`solana program dump` + `sha256sum`) | `bdad2e1ed2248618b624625318c653b4b29dc78442b8d2ca042e087b147439a0` |
| On-chain `solana-verify` hash (normalized) | `cd6db59a5491b43ab00bf416ac3c052425250537a3f6151d0e597d9ff0598b5f` |
| Local non-docker `target/deploy/axis_vault.so` (raw) | `bdad2e1ed2248618b624625318c653b4b29dc78442b8d2ca042e087b147439a0` ← same as on-chain raw |
| Docker reproducible build (`solana-verifiable-build:3.0.14`) `solana-verify` hash | `0253e0316161f40608fdcd5ba325db4b600956f83dd6547c02ef7ae827a5af53` |
| OtterSec verifier | `is_verified: false`, `repo_url: ""` |
| Public source | `https://github.com/Axis-pizza/Axis_AMM` (default branch `main`) |

**Verdict: hash mismatch.** The on-chain program was built with a
non-docker toolchain (probably plain `cargo build-sbf` from
@kidneyweakx's machine — the local `target/deploy/.so` has the same
raw hash as the on-chain dump, confirming the deploy artifact
matches what's in tree, just not produced inside a verifiable image).
OtterSec compares the *normalized* hash, so we cannot verify the
existing binary; we must redeploy a docker-built `.so` first. Step 1B
below.

---

## Step 0 — bring tooling online

```bash
# Once per workstation
cargo install solana-verify        # 0.4.15+ confirmed to work for this repo

# Per-session (Docker Desktop must be running)
open -a Docker
```

`solana-verify build` runs `cargo build-sbf` inside
`solanafoundation/solana-verifiable-build` so the toolchain version,
filesystem layout, and build flags are pinned. That's the only thing
that produces a hash OtterSec will accept.

---

## Step 1 — try a reproducible build and compare hashes

```bash
bash scripts/ops/verify-build.sh
```

This wraps `solana-verify build`, `get-executable-hash`, and
`get-program-hash` and prints PASS / FAIL. The PASS path skips most of
the work; the FAIL path is documented below.

Two outcomes:

### 1A. Hashes match → "easy" path

Skip to **Step 2**. The deployed binary is reproducible from this repo
and a verify attestation just needs the metadata PDA written.

### 1B. Hashes differ → redeploy first

The deployed binary was built with a non-reproducible toolchain (e.g.
local `cargo build-sbf` from kidneyweakx's machine, not the docker
image). OtterSec's remote builder will produce the docker hash, which
won't match the chain. We must redeploy the docker-built `.so` before
we can verify.

```bash
# 1B-1. Reproduce the build (script step 1 already did this, but to
# be explicit:)
solana-verify build \
  --library-name axis_vault \
  contracts/axis-vault

# .so now lives at contracts/axis-vault/target/deploy/axis_vault.so
# with the docker hash. Save the hash for the audit log:
solana-verify get-executable-hash \
  contracts/axis-vault/target/deploy/axis_vault.so
```

To redeploy via Squads:

1. Write the program buffer to chain (single-sig with the deploy
   wallet, ~0.7297 SOL of rent). Always pass an explicit `--buffer
   <keypair>` so a crash mid-upload leaves a recoverable buffer
   keypair on disk; otherwise the rent is locked forever.
   ```bash
   BUF_KP="$HOME/.config/solana/axis_vault_redeploy_buffer_$(date +%Y%m%d).json"
   solana-keygen new --no-bip39-passphrase --silent --outfile "$BUF_KP"
   solana program write-buffer \
     contracts/axis-vault/target/deploy/axis_vault.so \
     --buffer "$BUF_KP" \
     -u mainnet-beta --output json
   ```
   On crash, recover via `solana program close --buffers
   --buffer-authority $HOME/.config/solana/<wallet>.json -u
   mainnet-beta`.

2. Set the buffer's authority to the Squads vault so the multisig can
   consume it during the upgrade tx:
   ```bash
   solana program set-buffer-authority \
     "$(solana-keygen pubkey "$BUF_KP")" \
     --new-buffer-authority BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6 \
     -u mainnet-beta
   ```

3. **Done as of 2026-04-30** — buffer
   `GLapRTYhvTs4gFdnaXVadsZi23pSL49Sa1vBeBcgyyWD`, authority flipped
   to the Squads vault, hash `0253e0316161f40608fdcd5ba325db4b6
   00956f83dd6547c02ef7ae827a5af53` (matches local docker build).
   Backup keypair at `~/.config/solana/axis_vault_redeploy_buffer_
   20260430.json`. Next step is in Squads UI:
   - Network: Mainnet Beta
   - Vault: `BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6`
   - New transaction → "Upgrade program" (or Custom TX equivalent)
     - Program: `Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX`
     - Buffer: `GLapRTYhvTs4gFdnaXVadsZi23pSL49Sa1vBeBcgyyWD`
     - Spill: any signer wallet (the ~0.7297 SOL buffer rent goes here)
   - Both signers approve, Muse executes.

4. After the upgrade lands, re-run `bash scripts/ops/verify-build.sh`
   and confirm PASS — on-chain normalized hash now equals
   `0253e0316161…147439a0`. If it doesn't, the upgrade tx didn't go
   through; check Squads.

Then fall through to the easy path (Step 2 below).

---

## Step 2 — export the verify-PDA tx for Squads

```bash
solana-verify export-pda-tx \
  --library-name axis_vault \
  --mount-path contracts/axis-vault \
  --program-id Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX \
  --uploader BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6 \
  --commit-hash $(git rev-parse HEAD) \
  --encoding base58 \
  https://github.com/Axis-pizza/Axis_AMM
```

This emits a base58-encoded transaction that writes the otter-verify
PDA on-chain (linking the program ID to the GitHub repo + commit). It
must be signed by the `--uploader` key, which is the Squads vault.

Paste the base58 string into a Squads "Custom Transaction" proposal.
Both signers approve, Muse executes.

Sanity-check the PDA after execution:

```bash
solana-verify get-program-pda \
  --program-id Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX \
  --uploader BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6 \
  -u mainnet-beta
```

Expected: a record with the GitHub URL, commit hash, and library name.

---

## Step 3 — submit the remote build job to OtterSec

```bash
solana-verify remote submit-job \
  --program-id Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX \
  --uploader BtjuCMkLC9MuzagvGSS9E26XjMNTBR6isj8e1xVyeak6 \
  -u mainnet-beta
```

This call is a plain HTTP request to OtterSec's verifier; no on-chain
signature required. OtterSec clones the repo at the recorded commit,
runs `solana-verify build` in their docker, hashes the output, and
posts an attestation if the hashes match.

Poll status:

```bash
solana-verify remote get-status \
  --program-id Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX
# Or visit:
#   https://verify.osec.io/status/Agae3WetHx7J9CE7nP927ekzAeegSKE1KfkZDMYLDGHX
```

`is_verified: true` is the success state. Solscan typically reflects
the badge within ~10 minutes once the OtterSec attestation lands.

---

## Step 4 — record in the deploy log

Update `docs/architecture/MAINNET_SCOPE.md` "Deployment artifacts"
section with:

- Verify PDA tx signature
- OtterSec job ID
- Confirmed `is_verified: true` timestamp
- The repo URL + commit hash that backed the verification

This makes the verified state auditable from the docs alongside the
deploy itself.

---

## Failure modes

| symptom | likely cause | fix |
|---|---|---|
| `docker info` errors | Docker Desktop not running | start Docker, retry |
| `solana-verify build` hangs on first run | image pull (~1 GB) | wait it out |
| Local hash != on-chain hash | non-docker original deploy | redeploy via Squads (Step 1B) |
| `submit-job` returns 4xx | PDA missing or stale | re-export and re-execute Step 2 |
| OtterSec status stays `pending` for >30 min | their queue or repo ACL | check repo is public, retry submit-job |
| `is_verified: false` after build completes | hash mismatch on their end | redeploy via Squads (Step 1B) |

The runbook is idempotent at every step — re-executing a step that
already succeeded is a no-op (write-PDA upserts; submit-job is
keyed on the latest PDA contents).
