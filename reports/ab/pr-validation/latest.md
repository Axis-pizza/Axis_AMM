# Axis A/B PR Validation Report

- Generated At: 1776094992s-since-epoch
- Run ID: ab-pr-validation-1776094885
- Base Seed: 20260408-1776094885
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40082.40 vs baseline(pfda3) 36077.05 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 85.84% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 40.20 / 100.46 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7396.722125000003, 9008.715625]) | slippage p=0 ci=Some([45.9373135675197, 62.52235104839496]) |

### Scenario: scenario-01

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=500bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776094885-scenario-01
- Token sample: ["bSOL", "JUP", "USDC", "USDT"]
- Comparison tokens: ["bSOL", "JUP", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 83
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25575.00 / 33075.00 | 34109.00 / 38608.30 |
| Slippage bps p50/p95 | 50.06 / 50.10 | 146.69 / 148.27 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 60.24% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8824.9000 | [7415.2555, 10113.5095] | 0.000000 |
| slippage_bps | 96.6843 | [96.4319, 96.9409] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=25bps | drift_ratio=800bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776094885-scenario-02
- Token sample: ["USDT", "JTO", "USDC"]
- Comparison tokens: ["USDT", "JTO", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 28575.00 / 41550.00 | 34095.00 / 40086.40 |
| Slippage bps p50/p95 | 30.16 / 30.38 | 54.79 / 55.43 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 6862.7200 | [4971.9225, 8634.1035] | 0.000000 |
| slippage_bps | 24.5773 | [24.4415, 24.7090] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1776094885-scenario-03
- Token sample: ["bSOL", "wSOL", "mSOL"]
- Comparison tokens: ["bSOL", "wSOL", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27075.00 / 34575.00 | 34107.00 / 39426.70 |
| Slippage bps p50/p95 | 100.07 / 100.13 | 172.84 / 174.39 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8224.0600 | [6873.3705, 9483.5820] | 0.000000 |
| slippage_bps | 72.7737 | [72.4884, 73.0655] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=4
- Scenario seed: 20260408-1776094885-scenario-04
- Token sample: ["JUP", "wSOL", "JTO", "bSOL"]
- Comparison tokens: ["JUP", "wSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25576.00 / 36903.00 | 34850.00 / 38598.10 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 54.81 / 55.43 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8882.4400 | [7173.6255, 10442.5625] | 0.000000 |
| slippage_bps | 24.8237 | [24.7129, 24.9387] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

