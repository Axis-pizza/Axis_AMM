# Axis A/B PR Validation Report

- Generated At: 1776180525s-since-epoch
- Run ID: ab-pr-validation-1776180417
- Base Seed: 20260408-1776180417
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40105.20 vs baseline(pfda3) 36148.95 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 78.12% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 65.03 / 150.10 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7986.206124999997, 9545.01575]) | slippage p=0 ci=Some([62.33833508278145, 81.49518593709104]) |

### Scenario: scenario-01

- Description: reserve=1000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1776180417-scenario-01
- Token sample: ["USDC", "mSOL", "USDT"]
- Comparison tokens: ["USDC", "mSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 70
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25573.00 / 35398.00 | 35593.00 / 40102.65 |
| Slippage bps p50/p95 | 100.57 / 100.97 | 196.07 / 197.76 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 71.43% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8947.6200 | [7444.8170, 10507.4465] | 0.000000 |
| slippage_bps | 95.4491 | [95.1271, 95.7693] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1776180417-scenario-02
- Token sample: ["wSOL", "JUP", "mSOL", "USDC", "USDT"]
- Comparison tokens: ["wSOL", "JUP", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 86
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 26323.00 / 36073.00 | 35592.50 / 41756.25 |
| Slippage bps p50/p95 | 100.43 / 100.89 | 195.96 / 197.77 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.14% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8766.3600 | [7267.1295, 10177.5240] | 0.000000 |
| slippage_bps | 95.5288 | [95.2275, 95.8149] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=800bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776180417-scenario-03
- Token sample: ["bSOL", "USDC", "USDT"]
- Comparison tokens: ["bSOL", "USDC", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25575.50 / 32401.10 | 35591.00 / 38597.65 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 103.68 / 105.63 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9781.1000 | [8370.8525, 11163.6850] | 0.000000 |
| slippage_bps | 73.8321 | [73.5180, 74.1667] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=500bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776180417-scenario-04
- Token sample: ["USDC", "JUP", "JTO"]
- Comparison tokens: ["USDC", "JUP", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27075.00 / 39900.00 | 34851.50 / 39441.80 |
| Slippage bps p50/p95 | 30.02 / 30.04 | 54.80 / 55.46 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 7628.3000 | [5857.8960, 9337.3345] | 0.000000 |
| slippage_bps | 24.7782 | [24.6560, 24.8946] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

