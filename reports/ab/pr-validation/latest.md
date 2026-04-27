# Axis A/B PR Validation Report

- Generated At: 1777305481s-since-epoch
- Run ID: ab-pr-validation-1777305366
- Base Seed: 20260408-1777305366
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=48346.85 vs baseline(g3m) 39275.50 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 74.91% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 75.00 / 146.87 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=74.91% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.000000005045290407679204 ci=Some([1203.5106250000024, 3439.195625]) | slippage p=0 ci=Some([62.5184163833403, 69.82136660519845]) |

### Scenario: scenario-01

- Description: reserve=1000000 | swap_ratio=25bps | drift_ratio=500bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1777305366-scenario-01
- Token sample: ["USDT", "bSOL", "mSOL", "JUP"]
- Comparison tokens: ["USDT", "bSOL", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31769.00 / 50594.00 | 36188.00 / 40693.75 |
| Slippage bps p50/p95 | 102.25 / 103.57 | 124.31 / 127.47 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 1992.2200 | [-648.2855, 4272.4045] | 0.000244 |
| slippage_bps | 22.2265 | [21.6225, 22.8595] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=100000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1777305366-scenario-02
- Token sample: ["JUP", "USDC", "bSOL", "wSOL"]
- Comparison tokens: ["JUP", "USDC", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30271.50 / 44597.00 | 34710.50 / 37717.65 |
| Slippage bps p50/p95 | 100.00 / 100.01 | 148.82 / 150.06 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3803.3600 | [1853.5325, 5754.3430] | 0.000108 |
| slippage_bps | 48.8467 | [48.6434, 49.0632] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1777305366-scenario-03
- Token sample: ["wSOL", "mSOL", "JTO", "USDT"]
- Comparison tokens: ["wSOL", "mSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 85
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33271.50 / 49546.00 | 34709.50 / 39199.20 |
| Slippage bps p50/p95 | 50.01 / 50.01 | 146.76 / 148.28 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.82% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 1789.7200 | [-553.7260, 3859.2210] | 0.040238 |
| slippage_bps | 96.8070 | [96.5684, 97.0493] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1777305366-scenario-04
- Token sample: ["USDC", "mSOL", "wSOL"]
- Comparison tokens: ["USDC", "mSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 82
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33271.00 / 47597.00 | 34711.50 / 37715.75 |
| Slippage bps p50/p95 | 50.00 / 50.01 | 147.12 / 148.30 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 60.98% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 1731.4000 | [-518.9885, 3741.7030] | 0.044421 |
| slippage_bps | 96.8437 | [96.5498, 97.1375] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

