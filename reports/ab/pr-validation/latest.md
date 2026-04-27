# Axis A/B PR Validation Report

- Generated At: 1777308608s-since-epoch
- Run ID: ab-pr-validation-1777308493
- Base Seed: 20260408-1777308493
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=43770.00 vs baseline(g3m) 40701.15 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 69.69% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 40.07 / 125.67 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=69.69% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.000000000006128431095930864 ci=Some([2710.597625000002, 4631.420374999997]) | slippage p=0 ci=Some([78.76515774684955, 91.6671147691467]) |

### Scenario: scenario-01

- Description: reserve=10000000 | swap_ratio=75bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1777308493-scenario-01
- Token sample: ["USDT", "JUP", "wSOL", "mSOL", "JTO"]
- Comparison tokens: ["USDT", "JUP", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33270.00 / 46245.00 | 34705.00 / 39194.85 |
| Slippage bps p50/p95 | 50.06 / 50.12 | 123.69 / 125.52 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2417.6600 | [347.5280, 4428.1250] | 0.009585 |
| slippage_bps | 73.5981 | [73.2998, 73.9108] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=75bps | drift_ratio=500bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777308493-scenario-02
- Token sample: ["JTO", "USDC", "USDT", "JUP", "mSOL"]
- Comparison tokens: ["JTO", "USDC", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31770.00 / 42420.00 | 36191.50 / 43694.55 |
| Slippage bps p50/p95 | 30.07 / 30.12 | 104.13 / 105.82 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4816.0600 | [3017.7015, 6705.9255] | 0.000019 |
| slippage_bps | 73.8514 | [73.4994, 74.2221] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=4
- Scenario seed: 20260408-1777308493-scenario-03
- Token sample: ["USDC", "JUP", "bSOL", "mSOL"]
- Comparison tokens: ["USDC", "JUP", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 111
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 29520.00 / 42420.00 | 35446.50 / 40696.10 |
| Slippage bps p50/p95 | 30.05 / 30.09 | 127.49 / 128.57 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 45.05% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3650.3800 | [1460.4925, 5629.5525] | 0.002597 |
| slippage_bps | 97.2761 | [97.0046, 97.5484] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777308493-scenario-04
- Token sample: ["wSOL", "JUP", "mSOL", "USDT", "bSOL"]
- Comparison tokens: ["wSOL", "JUP", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 76
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30270.00 / 43095.00 | 34708.00 / 39193.00 |
| Slippage bps p50/p95 | 100.05 / 100.09 | 196.11 / 197.57 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 65.79% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3800.8200 | [2031.2645, 5600.9260] | 0.000203 |
| slippage_bps | 96.0403 | [95.7330, 96.3433] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

