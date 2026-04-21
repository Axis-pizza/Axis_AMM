# Axis A/B PR Validation Report

- Generated At: 1776792009s-since-epoch
- Run ID: ab-pr-validation-1776791920
- Base Seed: 20260408-1776791920
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40699.15 vs baseline(pfda3) 45290.85 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 100.00% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 30.01 / 103.56 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.0000000000001325606291402437 ci=Some([2739.3775000000032, 4796.259749999999]) | slippage p=0 ci=Some([55.84765901627852, 66.62650432764045]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776791920-scenario-01
- Token sample: ["bSOL", "JUP", "wSOL"]
- Comparison tokens: ["bSOL", "JUP", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31715.00 / 44541.00 | 36200.00 / 40030.60 |
| Slippage bps p50/p95 | 30.01 / 30.01 | 103.59 / 105.84 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3439.1200 | [1490.1180, 5179.2575] | 0.000247 |
| slippage_bps | 73.7463 | [73.4286, 74.0768] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=800bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1776791920-scenario-02
- Token sample: ["JUP", "bSOL", "mSOL", "wSOL", "USDT"]
- Comparison tokens: ["JUP", "bSOL", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 29463.00 / 42213.00 | 36188.50 / 40852.25 |
| Slippage bps p50/p95 | 100.88 / 101.87 | 148.73 / 150.22 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 5050.3800 | [3281.5355, 6851.7100] | 0.000076 |
| slippage_bps | 47.8224 | [47.5100, 48.1406] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776791920-scenario-03
- Token sample: ["USDC", "USDT", "wSOL"]
- Comparison tokens: ["USDC", "USDT", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31715.00 / 47541.00 | 36196.00 / 40029.40 |
| Slippage bps p50/p95 | 30.01 / 30.01 | 103.54 / 106.05 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2417.5400 | [77.9550, 4667.6760] | 0.003342 |
| slippage_bps | 73.7625 | [73.3458, 74.1499] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=800bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776791920-scenario-04
- Token sample: ["wSOL", "USDC", "mSOL"]
- Comparison tokens: ["wSOL", "USDC", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30216.00 / 43713.55 | 34690.00 / 40013.20 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 79.42 / 80.71 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4256.8600 | [2277.4490, 6028.1165] | 0.000045 |
| slippage_bps | 49.3932 | [49.1646, 49.6232] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

