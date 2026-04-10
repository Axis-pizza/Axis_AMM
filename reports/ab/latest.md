# Axis A/B Test Report

- Generated: 1775625417s-since-epoch
- Environment: LiteSVM (local, multi-scenario)

## Scenario 1: Small pool, tiny swap

Reserve: 1000000, Swap: 10000, Drift trigger: 200000

- Swap amount: 10000
- Initial reserves: [1000000, 1000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 13083 |
| Swap/Request CU | 10736 | 11939 |
| Clear/Rebalance CU | 31536 | 9494 |
| Claim CU | 3223 | N/A |
| **Total CU** | **45495** | **35775** |
| Tokens received | 9970 | 0 |
| Execution slots | 11 | 1 |

## Scenario 2: Medium pool, 1% swap

Reserve: 100000000, Swap: 1000000, Drift trigger: 20000000

- Swap amount: 1000000
- Initial reserves: [100000000, 100000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11615 |
| Swap/Request CU | 10736 | 11901 |
| Clear/Rebalance CU | 13536 | 9491 |
| Claim CU | 3224 | N/A |
| **Total CU** | **27496** | **34266** |
| Tokens received | 997000 | 0 |
| Execution slots | 11 | 1 |

## Scenario 3: Large pool, 0.5% swap

Reserve: 1000000000, Swap: 5000000, Drift trigger: 200000000

- Swap amount: 5000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11607 |
| Swap/Request CU | 9236 | 11911 |
| Clear/Rebalance CU | 12035 | 9505 |
| Claim CU | 3224 | N/A |
| **Total CU** | **24495** | **34282** |
| Tokens received | 4985000 | 0 |
| Execution slots | 11 | 1 |

## Scenario 4: Large pool, 1% swap

Reserve: 1000000000, Swap: 10000000, Drift trigger: 200000000

- Swap amount: 10000000
- Initial reserves: [1000000000, 1000000000]

| Metric | ETF A (PFDA-3) | ETF B (G3M) |
|--------|---------------:|------------:|
| Init CU | 0 | 11607 |
| Swap/Request CU | 9236 | 11934 |
| Clear/Rebalance CU | 10537 | 9500 |
| Claim CU | 3224 | N/A |
| **Total CU** | **22997** | **34300** |
| Tokens received | 9970000 | 0 |
| Execution slots | 11 | 1 |

## Summary

- Average total CU: ETF A = 30120, ETF B = 34655
- CU efficiency: ETF B uses 115% of ETF A's compute
