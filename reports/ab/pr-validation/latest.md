# Axis A/B PR Validation Report

- Generated At: 1777604761s-since-epoch
- Run ID: ab-pr-validation-1777604656
- Base Seed: 20260408-1777604656
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

- Baseline: G3M
- Candidate: PFDA-3
- Gate Result: **PASS**

| Gate | Pass | Detail |
|---|---|---|
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=46770.05 vs baseline(g3m) 40705.00 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 75.19% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 75.00 / 135.24 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=75.19% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.0000000007784661804066673 ci=Some([2231.261125000001, 4270.56825]) | slippage p=0 ci=Some([51.90444223997218, 68.58361510757176]) |

### Scenario: scenario-01

- Description: reserve=10000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777604656-scenario-01
- Token sample: ["USDC", "JTO", "bSOL", "wSOL", "USDT"]
- Comparison tokens: ["USDC", "JTO", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33270.00 / 43770.00 | 36183.50 / 41664.40 |
| Slippage bps p50/p95 | 100.17 / 100.37 | 124.33 / 125.10 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3338.8400 | [1449.1790, 5255.9760] | 0.000502 |
| slippage_bps | 24.2142 | [24.0967, 24.3398] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=500bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777604656-scenario-02
- Token sample: ["USDC", "bSOL", "wSOL"]
- Comparison tokens: ["USDC", "bSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 86
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31020.00 / 46770.00 | 35454.50 / 40024.05 |
| Slippage bps p50/p95 | 100.04 / 100.09 | 195.79 / 197.82 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.14% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2962.3600 | [620.8035, 5122.7560] | 0.001037 |
| slippage_bps | 96.0163 | [95.6559, 96.3731] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=500bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777604656-scenario-03
- Token sample: ["wSOL", "JUP", "bSOL", "mSOL", "USDC"]
- Comparison tokens: ["wSOL", "JUP", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 32521.50 / 47597.00 | 36199.00 / 40031.80 |
| Slippage bps p50/p95 | 30.01 / 30.04 | 54.81 / 55.42 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3413.0200 | [1432.6315, 5423.2040] | 0.002988 |
| slippage_bps | 24.7411 | [24.6043, 24.8693] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=800bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1777604656-scenario-04
- Token sample: ["JTO", "bSOL", "wSOL", "USDC"]
- Comparison tokens: ["JTO", "bSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 80
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31772.00 / 46096.45 | 36192.50 / 42352.55 |
| Slippage bps p50/p95 | 50.00 / 50.01 | 146.85 / 148.21 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 62.50% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3380.9200 | [1252.0210, 5390.3450] | 0.009062 |
| slippage_bps | 96.7549 | [96.4970, 97.0087] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

