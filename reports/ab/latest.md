# Axis A/B Test Report

- Generated: 1776776597s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11168 |
| Swap/Request CU | 9241 | 11148 |
| Clear/Rebalance CU | 10302 | 8838 |
| Claim CU | 9135 | N/A |
| **Total CU** | **28678** | **32320** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11176 |
| Swap/Request CU | 16741 | 11140 |
| Clear/Rebalance CU | 20804 | 8829 |
| Claim CU | 9135 | N/A |
| **Total CU** | **46680** | **32311** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 14174 |
| Swap/Request CU | 10741 | 11135 |
| Clear/Rebalance CU | 14803 | 8831 |
| Claim CU | 7635 | N/A |
| **Total CU** | **33179** | **35306** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 18674 |
| Swap/Request CU | 12241 | 11146 |
| Clear/Rebalance CU | 14805 | 8832 |
| Claim CU | 9135 | N/A |
| **Total CU** | **36181** | **39818** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 36179, ETF B = 34938
- CU efficiency: ETF B uses 97% of ETF A's compute
