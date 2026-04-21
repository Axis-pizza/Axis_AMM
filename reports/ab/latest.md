# Axis A/B Test Report

- Generated: 1776771168s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 14148 |
| Swap/Request CU | 6241 | 11133 |
| Clear/Rebalance CU | 10302 | 8795 |
| Claim CU | 6135 | N/A |
| **Total CU** | **22678** | **35242** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11156 |
| Swap/Request CU | 16741 | 11125 |
| Clear/Rebalance CU | 20804 | 8786 |
| Claim CU | 9135 | N/A |
| **Total CU** | **46680** | **32233** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11154 |
| Swap/Request CU | 9241 | 11120 |
| Clear/Rebalance CU | 14803 | 8788 |
| Claim CU | 6135 | N/A |
| **Total CU** | **30179** | **32228** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 20154 |
| Swap/Request CU | 10741 | 11131 |
| Clear/Rebalance CU | 14805 | 8789 |
| Claim CU | 7635 | N/A |
| **Total CU** | **33181** | **41240** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 33179, ETF B = 35235
- CU efficiency: ETF B uses 106% of ETF A's compute
