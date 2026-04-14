# Axis A/B PR Validation Report

- Generated At: 1776135864s-since-epoch
- Run ID: ab-pr-validation-1776135769
- Base Seed: 20260408-1776135769
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
| P95 CU Gate | YES | samples_ok=true (4 scenarios >=30 comparable runs), candidate(g3m) p95_total_cu=38614.05 vs baseline(pfda3) 37574.00 (limit <= +10%) |
| P50 Latency Gate | YES | success baseline/candidate = 88.50% / 100.00%, p50 slots baseline/candidate = 11.00 / 1.00, limit <= +20% |
| Quality Gate | NO | p50 slippage baseline/candidate = 75.01 / 136.50 bps; compensation_via_cu=NO |
| Reliability Gate | YES | candidate success=100.00% (>=99%), candidate critical invariant violations=0 |
| Significance Gate | YES | N=200 comparable, sample_rule=true (4 / 4 scenarios >=30 comparable runs) | total_cu p=0 ci=Some([7777.267750000003, 9442.458625]) | slippage p=0 ci=Some([56.59250178444576, 64.27286385766567]) |

### Scenario: scenario-01

- Description: reserve=1000000 | swap_ratio=25bps | drift_ratio=1000bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1776135769-scenario-01
- Token sample: ["wSOL", "USDC", "JTO"]
- Comparison tokens: ["wSOL", "USDC", "JTO"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27074.00 / 36224.00 | 35587.50 / 38599.30 |
| Slippage bps p50/p95 | 102.06 / 103.45 | 124.70 / 127.32 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8398.3000 | [6928.6780, 9748.7400] | 0.000000 |
| slippage_bps | 22.6852 | [22.1383, 23.2310] | 0.000000 |

### Scenario: scenario-02

- Description: reserve=100000000 | swap_ratio=100bps | drift_ratio=1200bps | fee=50bps | sampled_tokens=5
- Scenario seed: 20260408-1776135769-scenario-02
- Token sample: ["JTO", "mSOL", "wSOL", "USDT", "JUP"]
- Comparison tokens: ["JTO", "mSOL", "wSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 76
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 25576.00 / 36226.00 | 35594.50 / 38597.10 |
| Slippage bps p50/p95 | 50.00 / 50.01 | 146.81 / 148.39 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 65.79% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 8675.7000 | [7145.9540, 10055.8900] | 0.000000 |
| slippage_bps | 96.9829 | [96.6945, 97.2687] | 0.000000 |

### Scenario: scenario-03

- Description: reserve=100000000 | swap_ratio=75bps | drift_ratio=800bps | fee=50bps | sampled_tokens=3
- Scenario seed: 20260408-1776135769-scenario-03
- Token sample: ["wSOL", "mSOL", "USDT"]
- Comparison tokens: ["wSOL", "mSOL", "USDT"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 27076.00 / 35401.45 | 34116.50 / 38761.15 |
| Slippage bps p50/p95 | 50.01 / 50.01 | 123.75 / 125.70 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 7753.5200 | [5744.3095, 9583.9605] | 0.000000 |
| slippage_bps | 73.6768 | [73.3018, 74.0425] | 0.000000 |

### Scenario: scenario-04

- Description: reserve=100000000 | swap_ratio=50bps | drift_ratio=800bps | fee=100bps | sampled_tokens=3
- Scenario seed: 20260408-1776135769-scenario-04
- Token sample: ["mSOL", "JUP", "bSOL"]
- Comparison tokens: ["mSOL", "JUP", "bSOL"]
- Comparable for gate: true
- Target repeats: 50
- Attempts: 50
- Comparable runs: 50

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|---|---:|---:|
| Total CU p50/p95 | 24826.50 / 36901.55 | 35606.00 / 38615.00 |
| Slippage bps p50/p95 | 100.01 / 100.01 | 148.58 / 149.76 |
| Slots-to-finality p50/p95 | 11.00 / 11.00 | 1.00 / 1.00 |
| Success rate | 100.00% | 100.00% |

Significance checks:

| Metric | Δ mean (candidate - baseline) | 95% bootstrap CI | Mann-Whitney p |
|---|---:|---|---:|
| total_cu | 9671.4200 | [7661.6720, 11440.7445] | 0.000000 |
| slippage_bps | 48.5866 | [48.3853, 48.7902] | 0.000000 |

## Environment: local-validator

- Status: not_run
- Note: Run local-validator transaction-behavior benchmark separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM conclusions.

## Environment: devnet/mainnet-fork

- Status: not_run
- Note: Run real routing / fork validation separately and publish as an isolated layer.
- Note: Do not mix this layer with LiteSVM or local-validator conclusions.

