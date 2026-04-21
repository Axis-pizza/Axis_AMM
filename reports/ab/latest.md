# Axis A/B Test Report

- Generated: 1776768773s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11148 |
| Swap/Request CU | 7739 | 11133 |
| Clear/Rebalance CU | 10302 | 8795 |
| Claim CU | 3033 | N/A |
| **Total CU** | **21074** | **32242** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 17156 |
| Swap/Request CU | 13739 | 11125 |
| Clear/Rebalance CU | 20804 | 8786 |
| Claim CU | 3033 | N/A |
| **Total CU** | **37576** | **38233** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 15654 |
| Swap/Request CU | 22739 | 11120 |
| Clear/Rebalance CU | 14803 | 8788 |
| Claim CU | 3033 | N/A |
| **Total CU** | **40575** | **36728** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12654 |
| Swap/Request CU | 10739 | 11131 |
| Clear/Rebalance CU | 14805 | 8789 |
| Claim CU | 3033 | N/A |
| **Total CU** | **28577** | **33740** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 31950, ETF B = 35235
- CU efficiency: ETF B uses 110% of ETF A's compute
