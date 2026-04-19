# Axis A/B PR Validation Report

- Generated At: 1776397849s-since-epoch
- Run ID: ab-pr-validation-1776397760
- Base Seed: 20260408-1776397760
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40097.00 vs baseline(pfda3) 36073.05 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 89.69% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 50.34 / 111.33 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7984.869750000001, 9514.425]) | slippage p=0 ci=Some([54.102237755678985, 67.90908383984512]) |

### Scenario: scenario-01

- Description: reserve=10000000 | swap_ratio=25bps | drift_ratio=500bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1776397760-scenario-01
- Token sample: ["bSOL", "USDC", "JUP"]
- Comparison tokens: ["bSOL", "USDC", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27074.00 / 36224.00 | 35585.00 / 40095.65 |
| Slippage bps p50/p95 | 50.20 / 50.38 | 74.78 / 75.40 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8575.4600 | [6954.3685, 10198.0080] | 0.000000 |
| slippage_bps | 24.5566 | [24.4269, 24.6930] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=800bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1776397760-scenario-02
- Token sample: ["USDT", "JTO", "USDC", "JUP"]
- Comparison tokens: ["USDT", "JTO", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 73
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 26324.00 / 35399.00 | 34849.00 / 39422.00 |
| Slippage bps p50/p95 | 100.05 / 100.09 | 195.99 / 197.73 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 68.49% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8616.2400 | [7086.9205, 10086.2195] | 0.000000 |
| slippage_bps | 96.0081 | [95.6814, 96.3266] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=800bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776397760-scenario-03
- Token sample: ["wSOL", "USDT", "mSOL", "USDC"]
- Comparison tokens: ["wSOL", "USDT", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25573.00 / 35398.00 | 35593.00 / 38601.55 |
| Slippage bps p50/p95 | 50.98 / 51.82 | 99.35 / 100.79 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8644.4400 | [6964.7580, 10263.4635] | 0.000000 |
| slippage_bps | 48.2320 | [47.9356, 48.5469] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=500bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776397760-scenario-04
- Token sample: ["USDC", "USDT", "wSOL", "bSOL", "mSOL"]
- Comparison tokens: ["USDC", "USDT", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25575.50 / 34575.55 | 34118.00 / 40113.65 |
| Slippage bps p50/p95 | 50.00 / 50.01 | 124.24 / 125.63 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9164.3000 | [7541.0235, 10666.3515] | 0.000000 |
| slippage_bps | 73.9902 | [73.6581, 74.3420] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

