# Axis A/B PR Validation Report

- Generated At: 1775625434s-since-epoch
- Run ID: ab-pr-validation-1775625417
- Base Seed: 20260408-1775625417
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=42669.80 vs baseline(pfda3) 38071.90 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 86.21% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 40.24 / 87.17 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([9040.096625, 10848.156749999998]) | slippage p=0 ci=Some([44.185588595942676, 52.16495508127018]) |

### Scenario: scenario-01

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1775625417-scenario-01
- Token sample: ["USDT", "USDC", "JTO", "bSOL"]
- Comparison tokens: ["USDT", "USDC", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 26745.00 / 40320.00 | 38179.00 / 43506.15 |
| Slippage bps p50/p95 | 50.86 / 51.81 | 98.98 / 101.06 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 10028.8800 | [8081.4995, 11857.1980] | 0.000000 |
| slippage_bps | 48.1759 | [47.8033, 48.5152] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=25bps | drift_ratio=800bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1775625417-scenario-02
- Token sample: ["bSOL", "JTO", "USDC"]
- Comparison tokens: ["bSOL", "JTO", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27495.00 / 37995.00 | 38139.50 / 41170.70 |
| Slippage bps p50/p95 | 51.94 / 53.45 | 75.07 / 76.75 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 10062.1400 | [8202.4160, 11739.3485] | 0.000000 |
| slippage_bps | 23.3124 | [22.9473, 23.6748] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=25bps | drift_ratio=500bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1775625417-scenario-03
- Token sample: ["USDC", "mSOL", "JTO", "bSOL", "JUP"]
- Comparison tokens: ["USDC", "mSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27497.00 / 37997.00 | 37476.00 / 40534.10 |
| Slippage bps p50/p95 | 30.14 / 30.39 | 54.83 / 55.43 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 10154.3400 | [8534.3125, 11656.2850] | 0.000000 |
| slippage_bps | 24.6493 | [24.5187, 24.7785] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1775625417-scenario-04
- Token sample: ["USDT", "mSOL", "JTO"]
- Comparison tokens: ["USDT", "mSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 82
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27496.00 / 39646.00 | 38228.00 / 43571.65 |
| Slippage bps p50/p95 | 30.06 / 30.09 | 127.10 / 128.49 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 60.98% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9622.9600 | [7756.3060, 11484.7580] | 0.000000 |
| slippage_bps | 97.0059 | [96.7463, 97.2772] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

