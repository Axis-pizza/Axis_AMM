# Axis A/B PR Validation Report

- Generated At: 1781321026s-since-epoch
- Run ID: ab-pr-validation-1781320939
- Base Seed: 20260408-1781320939
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=45754.05 vs baseline(g3m) 39196.05 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 100.00% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 50.00 / 125.96 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=100.00% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000000008892753200484549 ci=Some([1752.5543750000024, 3906.9530000000013]) | slippage p=0 ci=Some([56.00596909287663, 67.18541762529821]) |

### Scenario: scenario-01

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=800bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1781320939-scenario-01
- Token sample: ["wSOL", "bSOL", "JTO", "USDT", "mSOL"]
- Comparison tokens: ["wSOL", "bSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 32254.00 / 42754.00 | 33166.50 / 38492.15 |
| Slippage bps p50/p95 | 50.05 / 50.09 | 148.10 / 150.26 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2432.1600 | [421.5375, 4261.5315] | 0.003647 |
| slippage_bps | 98.1096 | [97.7319, 98.5016] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000000 | swap_ratio=75bps | drift_ratio=800bps | fee=30bps | sampled_tokens=4
- Scenario seed: 20260408-1781320939-scenario-02
- Token sample: ["mSOL", "JUP", "wSOL", "bSOL"]
- Comparison tokens: ["mSOL", "JUP", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30754.50 / 44405.00 | 34683.00 / 42336.55 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 104.13 / 106.02 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 4202.1800 | [1919.8115, 6454.1240] | 0.000038 |
| slippage_bps | 74.2039 | [73.8590, 74.5643] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=1000000000 | swap_ratio=25bps | drift_ratio=800bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1781320939-scenario-03
- Token sample: ["USDC", "JUP", "JTO", "USDT", "mSOL"]
- Comparison tokens: ["USDC", "JUP", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33005.00 / 48755.00 | 33200.50 / 39195.10 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 74.74 / 75.40 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 1175.9800 | [-1376.6230, 3606.1855] | 0.097271 |
| slippage_bps | 24.7096 | [24.5771, 24.8409] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1781320939-scenario-04
- Token sample: ["bSOL", "USDT", "USDC", "JUP"]
- Comparison tokens: ["bSOL", "USDT", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 29255.00 / 42755.55 | 33205.00 / 38514.80 |
| Slippage bps p50/p95 | 100.00 / 100.00 | 149.16 / 150.15 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3518.2600 | [1508.7915, 5379.3695] | 0.000026 |
| slippage_bps | 48.9911 | [48.7492, 49.2178] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

