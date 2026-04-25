# Axis A/B PR Validation Report

- Generated At: 1777138147s-since-epoch
- Run ID: ab-pr-validation-1777138039
- Base Seed: 20260408-1777138039
- Repeats/Scenario: 50

## Fairness Rules

- Token universe candidates: ["wSOL", "USDC", "USDT", "JUP", "JTO", "mSOL", "bSOL"]
- Initial liquidity: ETF A/B use equal initial reserve value per active token under each scenario.
- Fee rule: ETF A/B use the same fee_bps sampled per scenario.
- Swap rule: ETF A/B use the same swap ratio and swap amount per run.
- Note: Cold-start CU is separated from steady-state CU.
- Note: Gate evaluation is environment-local and never mixed across layers.
- Note: Sampler auto-runs additional attempts (up to AB_MAX_ATTEMPT_MULT=4x) to hit target comparable N per scenario.

## Environment: LiteSVM

- Status: completed
- Note: Fast iteration environment; conclusions stay within LiteSVM layer.
- Note: A/B gate uses grouped comparison: PFDA-3 executes 3-token batch path while G3M executes 2-token path on the same active swap pair.

### Multi-Metric Gate

- Baseline: PFDA-3
- Candidate: G3M
- Gate Result: **FAIL**

| Gate | Pass | Detail |
|---|---|---|
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=39206.05 vs baseline(pfda3) 46741.00 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 79.68% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 75.01 / 137.51 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.000000013433361534254118 ci=Some([1687.4656249999975, 3802.717499999999]) | slippage p=0 ci=Some([68.22701347912684, 78.26778533489951]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777138039-scenario-01
- Token sample: ["USDC", "mSOL", "USDT", "JUP", "wSOL"]
- Comparison tokens: ["USDC", "mSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 101
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31741.00 / 47565.45 | 34708.50 / 40025.00 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 127.22 / 128.56 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 49.50% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2297.0600 | [-72.8205, 4428.2750] | 0.053123 |
| slippage_bps | 97.2019 | [96.9407, 97.4562] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1777138039-scenario-02
- Token sample: ["mSOL", "bSOL", "wSOL", "JUP"]
- Comparison tokens: ["mSOL", "bSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31741.00 / 45390.45 | 34710.00 / 37706.40 |
| Slippage bps p50/p95 | 50.01 / 50.01 | 124.13 / 125.78 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 1376.2600 | [-933.2420, 3538.4630] | 0.215808 |
| slippage_bps | 74.1021 | [73.7411, 74.4200] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=500bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777138039-scenario-03
- Token sample: ["USDC", "JUP", "JTO"]
- Comparison tokens: ["USDC", "JUP", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31740.00 / 42390.55 | 34716.50 / 39211.35 |
| Slippage bps p50/p95 | 100.00 / 100.01 | 173.21 / 174.92 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4137.1200 | [2156.3195, 5966.5195] | 0.000007 |
| slippage_bps | 73.1099 | [72.7583, 73.4471] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=800bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777138039-scenario-04
- Token sample: ["USDC", "JUP", "USDT"]
- Comparison tokens: ["USDC", "JUP", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31738.00 / 46063.00 | 36184.50 / 39208.65 |
| Slippage bps p50/p95 | 100.91 / 101.90 | 148.28 / 150.41 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3290.2000 | [1370.4300, 5270.8830] | 0.000397 |
| slippage_bps | 47.5955 | [47.2466, 47.9615] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

