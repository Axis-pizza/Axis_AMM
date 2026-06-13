# Axis A/B Test Report

- Generated: 1781320937s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 15442 |
| Swap/Request CU | 6262 | 10570 |
| Clear/Rebalance CU | 10117 | 8292 |
| Claim CU | 5375 | N/A |
| **Total CU** | **21754** | **35400** |
| Tokens received | 9970 | 9803 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 10942 |
| Swap/Request CU | 13762 | 10590 |
| Clear/Rebalance CU | 20619 | 8287 |
| Claim CU | 5375 | N/A |
| **Total CU** | **39756** | **30915** |
| Tokens received | 997000 | 980296 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 10944 |
| Swap/Request CU | 9262 | 10575 |
| Clear/Rebalance CU | 14617 | 8281 |
| Claim CU | 5375 | N/A |
| **Total CU** | **29254** | **30896** |
| Tokens received | 4985000 | 4925619 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 13944 |
| Swap/Request CU | 9262 | 10578 |
| Clear/Rebalance CU | 14619 | 8286 |
| Claim CU | 5375 | N/A |
| **Total CU** | **29256** | **33904** |
| Tokens received | 9970000 | 9802951 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 30005, ETF B = 32778
- CU efficiency: ETF B uses 109% of ETF A's compute
