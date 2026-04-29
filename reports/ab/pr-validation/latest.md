# Axis A/B PR Validation Report

- Generated At: 1777476312s-since-epoch
- Run ID: ab-pr-validation-1777476207
- Base Seed: 20260408-1777476207
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(pfda3) p95_total_cu=46770.00 vs baseline(g3m) 40703.45 (limit <= +30%) |
| P95 Latency Gate | YES | candidate(pfda3) p95 slots=11.00 (limit <= 30; ≈ batch window + buffer); reference success rates pfda/g3m = 82.64% / 100.00% |
| Quality Gate | YES | p50 slippage candidate(pfda3)/baseline(g3m) = 100.00 / 159.69 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate(pfda3) critical invariant violations=0 (must be 0); reference: g3m violations=0, candidate tx-success=82.64% (informational — strict-mode oracle rejections by design) |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000000002452971159527806 ci=Some([2262.2041249999947, 4482.306]) | slippage p=0 ci=Some([50.925937385112746, 69.94758295700349]) |

### Scenario: scenario-01

- Description: reserve=100000000 | swap_ratio=25bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1777476207-scenario-01
- Token sample: ["wSOL", "mSOL", "bSOL", "JUP", "JTO"]
- Comparison tokens: ["wSOL", "mSOL", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31771.00 / 43771.00 | 36195.00 / 41527.45 |
| Slippage bps p50/p95 | 30.02 / 30.04 | 54.85 / 55.47 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2481.1800 | [531.7730, 4370.3795] | 0.002987 |
| slippage_bps | 24.7839 | [24.6660, 24.8980] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=800bps | fee=100bps | sampled_tokens=4
- Scenario seed: 20260408-1777476207-scenario-02
- Token sample: ["bSOL", "JTO", "USDC", "USDT"]
- Comparison tokens: ["bSOL", "JTO", "USDC"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 75
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 28770.00 / 43245.00 | 34714.00 / 39202.10 |
| Slippage bps p50/p95 | 100.05 / 100.09 | 196.23 / 197.92 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 66.67% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 5030.4800 | [3109.8705, 6801.7280] | 0.000024 |
| slippage_bps | 96.2744 | [95.9788, 96.5895] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=10000000 | swap_ratio=100bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777476207-scenario-03
- Token sample: ["USDC", "JUP", "JTO", "mSOL", "bSOL"]
- Comparison tokens: ["USDC", "JUP", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 67
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 32520.00 / 47595.00 | 36198.50 / 41522.15 |
| Slippage bps p50/p95 | 100.05 / 100.09 | 195.84 / 197.77 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 74.63% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2270.3600 | [-158.7605, 4491.7660] | 0.006778 |
| slippage_bps | 95.9935 | [95.7047, 96.2819] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1777476207-scenario-04
- Token sample: ["USDC", "wSOL", "mSOL", "JTO", "USDT"]
- Comparison tokens: ["USDC", "wSOL", "mSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30271.50 / 46096.35 | 36191.50 / 40696.00 |
| Slippage bps p50/p95 | 100.00 / 100.00 | 124.45 / 125.04 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3766.2200 | [1455.9535, 5985.3900] | 0.000668 |
| slippage_bps | 24.4252 | [24.3078, 24.5410] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

