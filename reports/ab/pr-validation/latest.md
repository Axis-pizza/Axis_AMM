# Axis A/B PR Validation Report

- Generated At: 1777254718s-since-epoch
- Run ID: ab-pr-validation-1777254602
- Base Seed: 20260408-1777254602
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=46770.05 vs baseline(g3m) 39286.35 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 74.63% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 50.11 / 146.74 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=74.63% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.000000011429448942834597 ci=Some([2053.408625000002, 4064.317875000001]) | slippage p=0 ci=Some([80.81588067846793, 88.38851807384853]) |

### Scenario: scenario-01

- Description: reserve=1000000 | swap_ratio=100bps | drift_ratio=500bps | fee=50bps | sampled_tokens=4
- Scenario seed: 20260408-1777254602-scenario-01
- Token sample: ["mSOL", "bSOL", "USDT", "USDC"]
- Comparison tokens: ["mSOL", "bSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 79
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31769.00 / 45269.00 | 36194.00 / 39194.00 |
| Slippage bps p50/p95 | 50.44 / 50.96 | 146.74 / 148.49 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 63.29% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2808.7400 | [649.1525, 4877.1675] | 0.002855 |
| slippage_bps | 96.1440 | [95.8297, 96.4445] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1777254602-scenario-02
- Token sample: ["mSOL", "USDT", "JTO", "bSOL", "wSOL"]
- Comparison tokens: ["mSOL", "USDT", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 89
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30272.00 / 49247.00 | 34710.50 / 40686.60 |
| Slippage bps p50/p95 | 50.00 / 50.01 | 146.75 / 148.27 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 56.18% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3289.5400 | [920.6965, 5361.6095] | 0.000738 |
| slippage_bps | 96.8021 | [96.5110, 97.0888] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=75bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1777254602-scenario-03
- Token sample: ["bSOL", "wSOL", "JTO", "USDC", "JUP"]
- Comparison tokens: ["bSOL", "wSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31770.00 / 42270.00 | 36186.00 / 39207.65 |
| Slippage bps p50/p95 | 50.06 / 50.12 | 123.78 / 125.48 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3406.5200 | [1668.4880, 5145.6955] | 0.007998 |
| slippage_bps | 73.4672 | [73.1235, 73.8205] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=800bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1777254602-scenario-04
- Token sample: ["USDC", "bSOL", "mSOL"]
- Comparison tokens: ["USDC", "bSOL", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31771.50 / 47596.55 | 34719.50 / 40710.60 |
| Slippage bps p50/p95 | 100.00 / 100.01 | 172.67 / 174.75 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2760.1400 | [600.3305, 4829.8835] | 0.023073 |
| slippage_bps | 72.8684 | [72.5269, 73.2151] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

