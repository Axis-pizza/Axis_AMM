# Axis A/B PR Validation Report

- Generated At: 1777606716s-since-epoch
- Run ID: ab-pr-validation-1777606613
- Base Seed: 20260408-1777606613
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=46770.15 vs baseline(g3m) 39200.00 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 83.33% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 65.00 / 137.49 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=83.33% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000011698771684720555 ci=Some([1560.2242500000007, 3810.4087499999973]) | slippage p=0 ci=Some([54.61055584990221, 66.89376442403531]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777606613-scenario-01
- Token sample: ["mSOL", "bSOL", "JUP", "wSOL", "JTO"]
- Comparison tokens: ["mSOL", "bSOL", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31771.50 / 45423.00 | 36177.00 / 40008.40 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 79.38 / 80.81 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3186.1000 | [1205.3145, 5256.5820] | 0.000923 |
| slippage_bps | 49.4266 | [49.1881, 49.6691] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777606613-scenario-02
- Token sample: ["JTO", "wSOL", "USDC", "USDT", "bSOL"]
- Comparison tokens: ["JTO", "wSOL", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31770.00 / 46095.00 | 34705.50 / 39199.50 |
| Slippage bps p50/p95 | 100.08 / 100.18 | 148.72 / 150.11 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3286.7000 | [1247.2765, 5355.5875] | 0.009591 |
| slippage_bps | 48.6287 | [48.3802, 48.8751] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000 | swap_ratio=50bps | drift_ratio=1200bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777606613-scenario-03
- Token sample: ["mSOL", "JUP", "wSOL"]
- Comparison tokens: ["mSOL", "JUP", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33269.00 / 57419.00 | 34701.00 / 38528.15 |
| Slippage bps p50/p95 | 101.13 / 101.89 | 148.76 / 150.37 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 768.7600 | [-2051.8100, 3409.0265] | 0.168820 |
| slippage_bps | 47.6996 | [47.3942, 48.0193] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777606613-scenario-04
- Token sample: ["wSOL", "JTO", "bSOL", "USDC", "JUP"]
- Comparison tokens: ["wSOL", "JTO", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 90
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31770.50 / 44596.90 | 36189.00 / 39198.55 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 127.43 / 128.66 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 55.56% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3557.7200 | [1607.6785, 5447.6750] | 0.001609 |
| slippage_bps | 97.3448 | [97.1021, 97.5865] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

