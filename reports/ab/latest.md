# Axis A/B Test Report

- Generated: 1776773393s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11148 |
| Swap/Request CU | 7741 | 11133 |
| Clear/Rebalance CU | 10302 | 8795 |
| Claim CU | 7635 | N/A |
| **Total CU** | **25678** | **32242** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12656 |
| Swap/Request CU | 16741 | 11125 |
| Clear/Rebalance CU | 20804 | 8786 |
| Claim CU | 9135 | N/A |
| **Total CU** | **46680** | **33733** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12654 |
| Swap/Request CU | 16741 | 11120 |
| Clear/Rebalance CU | 14803 | 8788 |
| Claim CU | 13635 | N/A |
| **Total CU** | **45179** | **33728** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11154 |
| Swap/Request CU | 15241 | 11131 |
| Clear/Rebalance CU | 14805 | 8789 |
| Claim CU | 12135 | N/A |
| **Total CU** | **42181** | **32240** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 39929, ETF B = 32985
- CU efficiency: ETF B uses 83% of ETF A's compute
