# Axis A/B PR Validation Report

- Generated At: 1776178966s-since-epoch
- Run ID: ab-pr-validation-1776178851
- Base Seed: 20260408-1776178851
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40086.10 vs baseline(pfda3) 37572.15 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 61.54% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 30.53 / 127.38 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7340.289374999998, 9028.220375]) | slippage p=0 ci=Some([73.43723939404582, 83.21917600092692]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=800bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1776178851-scenario-01
- Token sample: ["USDT", "wSOL", "bSOL", "USDC", "JTO"]
- Comparison tokens: ["USDT", "wSOL", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 84
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27075.00 / 36899.55 | 34110.50 / 38607.85 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 127.29 / 128.43 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 59.52% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 7807.2400 | [6308.1130, 9307.2325] | 0.000000 |
| slippage_bps | 97.1786 | [96.9129, 97.4209] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=25bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=4
- Scenario seed: 20260408-1776178851-scenario-02
- Token sample: ["JUP", "USDT", "JTO", "wSOL"]
- Comparison tokens: ["JUP", "USDT", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27072.00 / 36897.00 | 35589.50 / 40916.20 |
| Slippage bps p50/p95 | 32.13 / 32.86 | 55.36 / 56.74 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8131.1200 | [6180.0755, 9902.8860] | 0.000000 |
| slippage_bps | 22.9846 | [22.6149, 23.3578] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=800bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776178851-scenario-03
- Token sample: ["JTO", "USDC", "bSOL"]
- Comparison tokens: ["JTO", "USDC", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 105
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27073.50 / 36076.00 | 35594.00 / 38594.60 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 127.45 / 128.28 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 47.62% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8467.0800 | [6938.6700, 9938.1240] | 0.000000 |
| slippage_bps | 97.2037 | [96.9440, 97.4646] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776178851-scenario-04
- Token sample: ["wSOL", "USDC", "JTO", "JUP"]
- Comparison tokens: ["wSOL", "USDC", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 86
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25575.00 / 37724.45 | 34109.50 / 38591.45 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 146.75 / 148.35 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.14% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8346.3200 | [6726.2300, 9936.3630] | 0.000000 |
| slippage_bps | 96.8937 | [96.6194, 97.1723] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

