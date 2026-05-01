# Axis A/B Test Report

- Generated: 1777604654s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11247 |
| Swap/Request CU | 6276 | 11148 |
| Clear/Rebalance CU | 10332 | 8837 |
| Claim CU | 6161 | N/A |
| **Total CU** | **22769** | **32400** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12755 |
| Swap/Request CU | 15276 | 11140 |
| Clear/Rebalance CU | 20834 | 8828 |
| Claim CU | 7661 | N/A |
| **Total CU** | **43771** | **33891** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12753 |
| Swap/Request CU | 9276 | 11135 |
| Clear/Rebalance CU | 14833 | 8830 |
| Claim CU | 6161 | N/A |
| **Total CU** | **30270** | **33886** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11253 |
| Swap/Request CU | 9276 | 11146 |
| Clear/Rebalance CU | 14835 | 8831 |
| Claim CU | 6161 | N/A |
| **Total CU** | **30272** | **32398** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 31770, ETF B = 33143
- CU efficiency: ETF B uses 104% of ETF A's compute
