# Axis A/B PR Validation Report

- Generated At: 1777487951s-since-epoch
- Run ID: ab-pr-validation-1777487864
- Base Seed: 20260408-1777487864
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=49771.00 vs baseline(g3m) 40704.20 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 100.00% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 100.00 / 124.79 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=100.00% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000000008537592854906961 ci=Some([2034.174499999995, 4194.327499999998]) | slippage p=0 ci=Some([44.79284089637065, 53.24054604097394]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=500bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777487864-scenario-01
- Token sample: ["USDT", "JTO", "USDC"]
- Comparison tokens: ["USDT", "JTO", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31771.00 / 46096.00 | 36210.00 / 40858.05 |
| Slippage bps p50/p95 | 100.01 / 100.01 | 173.26 / 174.96 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2938.4800 | [749.3835, 4978.3195] | 0.000990 |
| slippage_bps | 73.0731 | [72.7230, 73.4083] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1777487864-scenario-02
- Token sample: ["bSOL", "USDT", "wSOL", "mSOL"]
- Comparison tokens: ["bSOL", "USDT", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30271.00 / 46096.00 | 34721.00 / 39207.40 |
| Slippage bps p50/p95 | 50.01 / 50.01 | 123.76 / 125.70 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3535.9000 | [1346.9990, 5635.0825] | 0.002927 |
| slippage_bps | 73.7528 | [73.3991, 74.1079] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=800bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777487864-scenario-03
- Token sample: ["USDC", "wSOL", "bSOL", "JTO", "USDT"]
- Comparison tokens: ["USDC", "wSOL", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31770.50 / 48421.00 | 36202.50 / 40706.75 |
| Slippage bps p50/p95 | 100.01 / 100.03 | 124.57 / 125.10 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3081.1400 | [859.1440, 5182.3850] | 0.001175 |
| slippage_bps | 24.5412 | [24.4265, 24.6566] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1777487864-scenario-04
- Token sample: ["mSOL", "JTO", "bSOL", "USDT"]
- Comparison tokens: ["mSOL", "JTO", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31021.00 / 49771.00 | 34710.50 / 40696.60 |
| Slippage bps p50/p95 | 100.01 / 100.03 | 124.60 / 125.13 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2962.0400 | [740.7665, 5032.7605] | 0.000349 |
| slippage_bps | 24.5229 | [24.4066, 24.6345] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

