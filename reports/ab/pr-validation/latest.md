# Axis A/B PR Validation Report

- Generated At: 1776773522s-since-epoch
- Run ID: ab-pr-validation-1776773395
- Base Seed: 20260408-1776773395
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=39155.75 vs baseline(pfda3) 46678.05 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 75.19% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 75.05 / 159.61 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.000000002295445167277421 ci=Some([2296.0222499999986, 4335.659874999999]) | slippage p=0 ci=Some([63.6916223833431, 80.77871150977596]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=800bps | fee=30bps | sampled_tokens=3
- Scenario seed: 20260408-1776773395-scenario-01
- Token sample: ["USDT", "wSOL", "bSOL"]
- Comparison tokens: ["USDT", "wSOL", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31680.00 / 44655.00 | 34579.50 / 39910.30 |
| Slippage bps p50/p95 | 30.02 / 30.04 | 54.60 / 55.33 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2772.5600 | [642.5545, 4694.6615] | 0.007681 |
| slippage_bps | 24.6472 | [24.5330, 24.7561] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000 | swap_ratio=100bps | drift_ratio=500bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1776773395-scenario-02
- Token sample: ["USDC", "mSOL", "USDT", "bSOL"]
- Comparison tokens: ["USDC", "mSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 81
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30178.00 / 44503.00 | 34576.50 / 39065.20 |
| Slippage bps p50/p95 | 100.55 / 100.92 | 195.43 / 197.53 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 61.73% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3790.0800 | [1628.6885, 5829.3525] | 0.009007 |
| slippage_bps | 95.2774 | [94.9561, 95.5913] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1776773395-scenario-03
- Token sample: ["mSOL", "JUP", "wSOL", "JTO"]
- Comparison tokens: ["mSOL", "JUP", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 85
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30929.00 / 43679.00 | 36059.00 / 39073.65 |
| Slippage bps p50/p95 | 50.05 / 50.09 | 146.44 / 148.32 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.82% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4059.9200 | [2200.6585, 5919.8410] | 0.000066 |
| slippage_bps | 96.6235 | [96.3695, 96.8990] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=800bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1776773395-scenario-04
- Token sample: ["USDT", "JUP", "JTO", "bSOL"]
- Comparison tokens: ["USDT", "JUP", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31680.50 / 47505.45 | 35308.00 / 40558.10 |
| Slippage bps p50/p95 | 100.00 / 100.00 | 172.53 / 174.44 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2674.6000 | [515.5770, 4923.3505] | 0.007703 |
| slippage_bps | 72.5992 | [72.3114, 72.9187] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

