# M9 RTDS Settlement Reconciliation

Date: 2026-04-29
Branch: `m9/rtds-natural-paper-validation`
Session: `reports/sessions/m9-rtds-current-window-startuplog-20260429T035356Z`

## Scope

This note verifies post-market settlement artifacts for the startup-log-confirmed RTDS Chainlink paper run:

- Run ID: `m9-rtds-current-window-startuplog-20260429T035356Z`
- Market window: 2026-04-29 03:45:00-04:00:00 UTC
- Selected markets: BTC/ETH/SOL `1777434300`
- Paper behavior: 6 risk-approved taker paper orders, 6 fills, unchanged signal/risk gates
- Live trading remained disabled.

## Read-Only Settlement Artifact Check

At `2026-04-29T04:09:19Z`, read-only Gamma market checks showed all three selected markets closed with final outcome prices `["0","1"]`.

| Asset | Market ID | Slug | Closed | Outcome prices | Winning outcome | Gamma `updatedAt` |
| --- | --- | --- | --- | --- | --- | --- |
| BTC | `2101765` | `btc-updown-15m-1777434300` | true | `["0","1"]` | Down | `2026-04-29T04:08:37.934757Z` |
| ETH | `2101771` | `eth-updown-15m-1777434300` | true | `["0","1"]` | Down | `2026-04-29T04:04:46.435678Z` |
| SOL | `2101772` | `sol-updown-15m-1777434300` | true | `["0","1"]` | Down | `2026-04-29T04:08:37.974824Z` |

The captured paper session held only `Up` outcome tokens:

| Asset | Held outcome | Filled size | Filled notional | Fees paid | Settlement value | Post-settlement P&L |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| BTC | Up | 20.000000 | 1.200000 | 0.081216 | 0.000000 | -1.281216 |
| ETH | Up | 19.200000 | 0.768000 | 0.053084 | 0.000000 | -0.821084 |
| SOL | Up | 10.000000 | 1.500000 | 0.091800 | 0.000000 | -1.591800 |
| Total |  | 49.200000 | 3.468000 | 0.226100 | 0.000000 | -3.694100 |

Full artifact:

```text
reports/sessions/m9-rtds-current-window-startuplog-20260429T035356Z/settlement_reconciliation.json
```

## Reconciliation Result

The pre-settlement report marked the open positions with total P&L `-0.472100`. Final Gamma outcome prices resolved all held `Up` positions to zero, so post-settlement P&L is:

```text
settlement_value - filled_notional - fees_paid
= 0.000000 - 3.468000 - 0.226100
= -3.694100
```

This means the paper strategy result for this single current-window sample did not survive final settlement, despite producing natural risk-approved paper fills.

## Gate Result

- Polymarket RTDS Chainlink reference ingestion: PASS.
- Current-window market selection: PASS.
- Natural RTDS-backed paper trades: PASS.
- Replay determinism and generated-vs-recorded paper event comparison: PASS.
- Final start/end settlement artifact collection: PASS for this BTC/ETH/SOL window.
- Post-market reconciliation: PASS mechanically; strategy result was negative for this run.
- M9 live-readiness review: PASS for paper/replay validation evidence, but live trading remains BLOCKED pending a separate live-beta PRD, legal/access review, geoblock deployment check, key management, order signing/auth verification, and live risk release gate.
