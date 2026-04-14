# Axis A/B PR Validation Report

- Generated At: 1776163987s-since-epoch
- Run ID: ab-pr-validation-1776163896
- Base Seed: 20260408-1776163896
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
| P95 CU Gate | NO | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=41587.55 vs baseline(pfda3) 37652.00 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 100.00% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 40.10 / 113.90 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7967.346874999999, 9633.200625]) | slippage p=0 ci=Some([56.11324923533822, 66.42787546890489]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=500bps | fee=30bps | sampled_tokens=4
- Scenario seed: 20260408-1776163896-scenario-01
- Token sample: ["USDC", "USDT", "wSOL", "JUP"]
- Comparison tokens: ["USDC", "USDT", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 26328.50 / 37878.45 | 34121.00 / 40938.90 |
| Slippage bps p50/p95 | 30.01 / 30.01 | 103.71 / 105.94 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8770.3600 | [7058.6015, 10448.1470] | 0.000000 |
| slippage_bps | 73.8912 | [73.5473, 74.2568] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=50bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776163896-scenario-02
- Token sample: ["wSOL", "mSOL", "USDC"]
- Comparison tokens: ["wSOL", "mSOL", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25577.00 / 37577.00 | 35600.00 / 40249.15 |
| Slippage bps p50/p95 | 30.09 / 30.19 | 79.16 / 80.81 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9362.1400 | [7832.6755, 10802.8480] | 0.000000 |
| slippage_bps | 49.2344 | [48.9718, 49.4863] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=50bps | drift_ratio=800bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1776163896-scenario-03
- Token sample: ["bSOL", "USDC", "wSOL", "JTO"]
- Comparison tokens: ["bSOL", "USDC", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27077.00 / 38402.00 | 35596.00 / 40094.75 |
| Slippage bps p50/p95 | 100.08 / 100.18 | 148.77 / 150.03 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8251.4600 | [6272.4075, 9992.8605] | 0.000000 |
| slippage_bps | 48.6445 | [48.3960, 48.8898] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776163896-scenario-04
- Token sample: ["JUP", "mSOL", "wSOL", "bSOL"]
- Comparison tokens: ["JUP", "mSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 26328.00 / 37728.45 | 34101.00 / 40929.60 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 123.64 / 125.39 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8908.8000 | [7409.4655, 10380.5810] | 0.000000 |
| slippage_bps | 73.6258 | [73.3173, 73.9270] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

