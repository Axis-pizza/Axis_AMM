# Axis A/B Test Report

- Generated: 1776792947s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11242 |
| Swap/Request CU | 6241 | 11148 |
| Clear/Rebalance CU | 10328 | 8838 |
| Claim CU | 6144 | N/A |
| **Total CU** | **22713** | **32394** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 14250 |
| Swap/Request CU | 15241 | 11140 |
| Clear/Rebalance CU | 20830 | 8829 |
| Claim CU | 7644 | N/A |
| **Total CU** | **43715** | **35385** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 12748 |
| Swap/Request CU | 10741 | 11135 |
| Clear/Rebalance CU | 14829 | 8831 |
| Claim CU | 7644 | N/A |
| **Total CU** | **33214** | **33880** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11248 |
| Swap/Request CU | 12241 | 11146 |
| Clear/Rebalance CU | 14831 | 8832 |
| Claim CU | 9144 | N/A |
| **Total CU** | **36216** | **32392** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 33964, ETF B = 33512
- CU efficiency: ETF B uses 99% of ETF A's compute
