# Reference Feed Access Verification

Date: 2026-04-28
Branch: `m9/multi-session-validation`
Head commit at start: `df28e3e`

## Scope

This note verifies the current blocker behind live M9 paper strategy evidence: real resolution-source reference ticks for BTC, ETH, and SOL 15-minute Polymarket markets.

No source code was changed for this check. Under the current paper-only/live-disabled boundary, credential handling, API-key storage, signing, wallet/key handling, and live order placement remain out of scope.

## Findings

Current captured Polymarket markets point to Chainlink Data Streams pages as their resolution source:

| Asset | Market evidence | Resolution source |
| --- | --- | --- |
| BTC | `reports/sessions/m9-runtime-smoke-20260427b/markets.jsonl` | `https://data.chain.link/streams/btc-usd` |
| ETH | `reports/sessions/m9-runtime-smoke-20260427b/markets.jsonl` | `https://data.chain.link/streams/eth-usd` |
| SOL | `reports/sessions/m9-runtime-smoke-20260427b/markets.jsonl` | `https://data.chain.link/streams/sol-usd` |

The public Chainlink pages identify the streams as reference-price Data Streams:

| Asset | Product name | Feed ID | Schema | Service level |
| --- | --- | --- | --- | --- |
| BTC | `BTC/USD-RefPrice-DS-Premium-Global-003` | `0x00039d9e45394f473ab1f050a1b963e6b05351e52d71e507509ada0c95ed75b8` | `v3` | `Streams` |
| ETH | `ETH/USD-RefPrice-DS-Premium-Global-003` | `0x000362205e10b3a147d02792eccee483dca6c7b44ecce7012cb8c6e0b68b3ae9` | `v3` | `Streams` |
| SOL | `SOL/USD-RefPrice-DS-Premium-Global-003` | `0x0003b778d3f6b2ac4991302b89cb313f99a42467d6c9c5f96f57c29c0d2bc24f` | `v3` | `Streams` |

However, the public Chainlink stream pages are delayed informational pages, not the real-time report feed. Chainlink's public page states that real-time access to Data Streams reports requires contacting Chainlink, and the official REST/WebSocket docs state that all Data Streams API requests require authentication headers.

Sources checked:

- `https://data.chain.link/streams/btc-usd`
- `https://data.chain.link/streams/eth-usd`
- `https://data.chain.link/streams/sol-usd`
- `https://docs.chain.link/data-streams`
- `https://docs.chain.link/data-streams/reference/data-streams-api/authentication`
- `https://docs.chain.link/data-streams/reference/data-streams-api/interface-api`
- `https://docs.chain.link/data-streams/reference/data-streams-api/interface-ws`

Live unauthenticated probe against the real BTC stream report endpoint:

```text
curl -sS -D /tmp/chainlink_real_feed_probe_headers.txt \
  -o /tmp/chainlink_real_feed_probe_body.json \
  'https://api.dataengine.chain.link/api/v1/reports/latest?feedID=0x00039d9e45394f473ab1f050a1b963e6b05351e52d71e507509ada0c95ed75b8'
```

Result:

```text
HTTP/2 400
{"error":"Key: 'Headers.UserId' Error:Field validation for 'UserId' failed on the 'required' tag\nKey: 'Headers.Timestamp' Error:Field validation for 'Timestamp' failed on the 'required' tag\nKey: 'Headers.HmacSignature' Error:Field validation for 'HmacSignature' failed on the 'required' tag"}
```

This confirms that the real Data Streams API is not anonymously accessible from the current environment.

## Decision

M9 remains PARTIAL.

The blocker is external access to Chainlink Data Streams credentials/subscription, not an in-repo runtime stub. The current fail-closed `missing_reference_price` behavior is correct because using Binance, Coinbase, or the delayed public Chainlink webpage as the settlement/reference price would not match the market's stated resolution source.

Do not synthesize `ReferenceTick` events from predictive CEX feeds or delayed informational pages for M9 gate evidence.

## Next Action

To unblock real reference-backed paper sessions:

1. Obtain authorized Chainlink Data Streams access for the BTC/USD, ETH/USD, and SOL/USD reference streams, or document that access is unavailable for this project.
2. Add a separate, explicitly approved credential-handling scope before implementing authenticated Data Streams REST/WebSocket ingestion.
3. Decode v3 Data Streams reports into `ReferenceTick` events with source values matching each market's `resolution_source`.
4. Rerun BTC/ETH/SOL paper sessions, replay them, and verify deterministic paper events and P&L.

Live trading remains disabled.
