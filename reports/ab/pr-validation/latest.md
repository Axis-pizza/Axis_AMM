# Axis A/B PR Validation Report

- Generated At: 1776768878s-since-epoch
- Run ID: ab-pr-validation-1776768775
- Base Seed: 20260408-1776768775
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40569.05 vs baseline(pfda3) 36077.00 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 85.84% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 50.00 / 86.73 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([8139.564749999998, 9661.306874999998]) | slippage p=0 ci=Some([45.80253760865993, 52.147144456931755]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1776768775-scenario-01
- Token sample: ["JTO", "USDT", "wSOL"]
- Comparison tokens: ["JTO", "USDT", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27076.50 / 36902.00 | 34569.00 / 42065.30 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 99.50 / 100.62 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8891.8200 | [7303.0160, 10423.1435] | 0.000000 |
| slippage_bps | 49.5146 | [49.2952, 49.7211] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1776768775-scenario-02
- Token sample: ["USDC", "USDT", "bSOL"]
- Comparison tokens: ["USDC", "USDT", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25575.00 / 34725.00 | 34561.50 / 39056.70 |
| Slippage bps p50/p95 | 50.18 / 50.39 | 74.58 / 75.36 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9161.2600 | [7841.4295, 10332.0080] | 0.000000 |
| slippage_bps | 24.4621 | [24.3263, 24.5973] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000000 | swap_ratio=25bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776768775-scenario-03
- Token sample: ["JTO", "USDT", "JUP", "USDC", "bSOL"]
- Comparison tokens: ["JTO", "USDT", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27075.50 / 36076.10 | 34573.50 / 41394.60 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 74.72 / 75.33 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8538.7000 | [6707.8775, 10278.4620] | 0.000000 |
| slippage_bps | 24.7344 | [24.6240, 24.8353] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1776768775-scenario-04
- Token sample: ["JTO", "wSOL", "bSOL", "mSOL", "JUP"]
- Comparison tokens: ["JTO", "wSOL", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 83
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27074.00 / 36226.45 | 36065.00 / 39081.00 |
| Slippage bps p50/p95 | 30.01 / 30.01 | 127.39 / 128.64 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 60.24% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9082.9800 | [7491.6625, 10523.5500] | 0.000000 |
| slippage_bps | 97.2403 | [96.9751, 97.5009] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

