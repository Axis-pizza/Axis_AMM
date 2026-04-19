# Axis A/B Test Report

- Generated: 1776397758s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 13992 |
| Swap/Request CU | 7739 | 10977 |
| Clear/Rebalance CU | 10301 | 8639 |
| Claim CU | 3033 | N/A |
| **Total CU** | **21073** | **34774** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11000 |
| Swap/Request CU | 13739 | 10969 |
| Clear/Rebalance CU | 20803 | 8630 |
| Claim CU | 3033 | N/A |
| **Total CU** | **37575** | **31765** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 13998 |
| Swap/Request CU | 12239 | 10964 |
| Clear/Rebalance CU | 14802 | 8632 |
| Claim CU | 3033 | N/A |
| **Total CU** | **30074** | **34760** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 10998 |
| Swap/Request CU | 15239 | 10975 |
| Clear/Rebalance CU | 14804 | 8633 |
| Claim CU | 3033 | N/A |
| **Total CU** | **33076** | **31772** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 30449, ETF B = 33267
- CU efficiency: ETF B uses 109% of ETF A's compute
