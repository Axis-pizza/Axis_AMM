# Axis A/B PR Validation Report

- Generated At: 1776793039s-since-epoch
- Run ID: ab-pr-validation-1776792949
- Base Seed: 20260408-1776792949
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=40688.25 vs baseline(pfda3) 46713.00 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 100.00% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 50.02 / 111.42 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.0000000015063423841610302 ci=Some([2569.6935000000003, 4624.709124999998]) | slippage p=0 ci=Some([48.24545553793637, 61.65657483158314]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=25bps | drift_ratio=500bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1776792949-scenario-01
- Token sample: ["JUP", "JTO", "mSOL", "wSOL", "USDC"]
- Comparison tokens: ["JUP", "JTO", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31715.00 / 45216.10 | 36187.00 / 40690.75 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 54.92 / 55.43 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3394.3200 | [1354.8100, 5403.9630] | 0.004751 |
| slippage_bps | 24.8153 | [24.6924, 24.9295] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=800bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776792949-scenario-02
- Token sample: ["JUP", "bSOL", "USDT", "mSOL", "USDC"]
- Comparison tokens: ["JUP", "bSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 29463.00 / 46713.00 | 34698.00 / 39191.55 |
| Slippage bps p50/p95 | 50.84 / 51.83 | 99.19 / 101.17 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3728.4200 | [1660.1200, 5769.4075] | 0.001086 |
| slippage_bps | 48.3881 | [48.0810, 48.7017] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776792949-scenario-03
- Token sample: ["bSOL", "USDC", "JUP", "wSOL"]
- Comparison tokens: ["bSOL", "USDC", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 32465.50 / 47540.55 | 36186.50 / 42187.40 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 123.88 / 125.64 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2911.9000 | [542.5980, 5132.5375] | 0.013186 |
| slippage_bps | 73.8441 | [73.4744, 74.2371] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=500bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1776792949-scenario-04
- Token sample: ["wSOL", "USDT", "bSOL"]
- Comparison tokens: ["wSOL", "USDT", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31715.00 / 41540.45 | 34702.50 / 40009.50 |
| Slippage bps p50/p95 | 100.00 / 100.00 | 173.30 / 174.91 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4414.9400 | [2553.6125, 6216.0720] | 0.001122 |
| slippage_bps | 73.1354 | [72.7812, 73.5030] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

