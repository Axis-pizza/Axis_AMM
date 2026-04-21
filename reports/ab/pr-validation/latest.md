# Axis A/B PR Validation Report

- Generated At: 1776771284s-since-epoch
- Run ID: ab-pr-validation-1776771170
- Base Seed: 20260408-1776771170
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40561.15 vs baseline(pfda3) 49679.05 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 84.03% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 50.00 / 122.89 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000000004821876231630995 ci=Some([2014.9037499999993, 4174.801125000003]) | slippage p=0 ci=Some([54.59858116538965, 67.14164462888591]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=500bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776771170-scenario-01
- Token sample: ["bSOL", "JTO", "mSOL", "wSOL", "JUP"]
- Comparison tokens: ["bSOL", "JTO", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33181.00 / 49680.55 | 35320.00 / 39078.05 |
| Slippage bps p50/p95 | 50.02 / 50.04 | 74.85 / 75.26 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2201.5400 | [-109.5720, 4360.7720] | 0.042300 |
| slippage_bps | 24.6429 | [24.5200, 24.7664] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=800bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1776771170-scenario-02
- Token sample: ["bSOL", "USDT", "JUP"]
- Comparison tokens: ["bSOL", "USDT", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30181.50 / 51330.10 | 36056.00 / 40558.65 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 99.25 / 100.49 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4227.5200 | [1736.1160, 6327.8025] | 0.000019 |
| slippage_bps | 49.1861 | [48.9574, 49.4339] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1776771170-scenario-03
- Token sample: ["JTO", "USDC", "wSOL", "JUP", "bSOL"]
- Comparison tokens: ["JTO", "USDC", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31679.00 / 47504.00 | 35313.50 / 40571.45 |
| Slippage bps p50/p95 | 100.07 / 100.12 | 173.09 / 174.88 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2795.2400 | [214.9890, 5015.9225] | 0.000297 |
| slippage_bps | 72.9218 | [72.5752, 73.2774] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776771170-scenario-04
- Token sample: ["JTO", "USDC", "bSOL", "USDT", "JUP"]
- Comparison tokens: ["JTO", "USDC", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 88
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31679.00 / 45180.55 | 36059.50 / 39886.30 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 146.82 / 148.44 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 56.82% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3218.6200 | [1419.4245, 4930.3550] | 0.002444 |
| slippage_bps | 96.8598 | [96.5940, 97.1306] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

