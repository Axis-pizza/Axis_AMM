# Axis A/B PR Validation Report

- Generated At: 1776776716s-since-epoch
- Run ID: ab-pr-validation-1776776599
- Base Seed: 20260408-1776776599
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=39130.25 vs baseline(pfda3) 45181.05 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 76.63% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 50.00 / 113.11 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0.00000010635746194864737 ci=Some([2082.8681250000027, 4137.292750000003]) | slippage p=0 ci=Some([66.49348215652022, 79.2085599134031]) |

### Scenario: scenario-01

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=100bps | sampled_tokens=5
- Scenario seed: 20260408-1776776599-scenario-01
- Token sample: ["USDC", "wSOL", "JUP", "mSOL", "JTO"]
- Comparison tokens: ["USDC", "wSOL", "JUP"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 76
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 33180.50 / 44504.45 | 34629.00 / 39948.45 |
| Slippage bps p50/p95 | 100.00 / 100.00 | 196.56 / 197.95 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 65.79% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2908.3600 | [1139.5830, 4621.5340] | 0.023949 |
| slippage_bps | 96.3029 | [95.9825, 96.6326] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=1000000000 | swap_ratio=50bps | drift_ratio=800bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776776599-scenario-02
- Token sample: ["USDT", "mSOL", "JTO", "USDC", "bSOL"]
- Comparison tokens: ["USDT", "mSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 30930.00 / 47655.35 | 34616.00 / 39934.80 |
| Slippage bps p50/p95 | 50.00 / 50.00 | 99.44 / 100.46 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3557.0200 | [1487.0960, 5597.2250] | 0.002078 |
| slippage_bps | 49.1835 | [48.9421, 49.4279] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=50bps | drift_ratio=1000bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776776599-scenario-03
- Token sample: ["bSOL", "mSOL", "USDT", "wSOL", "JTO"]
- Comparison tokens: ["bSOL", "mSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31680.00 / 43680.00 | 34637.50 / 39125.10 |
| Slippage bps p50/p95 | 50.01 / 50.02 | 99.20 / 100.51 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 2556.3400 | [68.0120, 4895.6630] | 0.024778 |
| slippage_bps | 49.1717 | [48.9465, 49.3950] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=1000000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=30bps | sampled_tokens=5
- Scenario seed: 20260408-1776776599-scenario-04
- Token sample: ["JUP", "wSOL", "JTO", "USDC", "USDT"]
- Comparison tokens: ["JUP", "wSOL", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 85
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 31679.00 / 44503.90 | 34626.00 / 39125.10 |
| Slippage bps p50/p95 | 30.00 / 30.00 | 127.65 / 128.60 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 58.82% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 3536.7600 | [1764.7640, 5306.8245] | 0.002800 |
| slippage_bps | 97.3937 | [97.1371, 97.6597] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

