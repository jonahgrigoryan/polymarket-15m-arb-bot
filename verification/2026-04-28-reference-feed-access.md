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

M9 remains PARTIAL for final live-readiness, but the first implementation path changes after the Polymarket RTDS recheck below.

The earlier direct Chainlink API blocker was external access to Data Streams credentials/subscription, not an in-repo runtime stub. The current fail-closed `missing_reference_price` behavior is correct when no verified settlement/reference feed is present because using Binance, Coinbase, or the delayed public Chainlink webpage as the settlement/reference price would not match the market's stated resolution source.

Do not synthesize `ReferenceTick` events from predictive CEX feeds or delayed informational pages for M9 gate evidence.

## Polymarket RTDS Addendum

Polymarket's official RTDS docs identify an unauthenticated websocket endpoint and crypto Chainlink price stream:

- Endpoint: `wss://ws-live-data.polymarket.com`
- Topic: `crypto_prices_chainlink`
- Symbols: `btc/usd`, `eth/usd`, `sol/usd`
- Documented payload fields include `symbol`, `timestamp`, and `value`.

This changes the first implementation path:

1. Use Polymarket RTDS Chainlink as the first read-only reference provider for current M9 paper/replay validation.
2. Persist RTDS `crypto_prices_chainlink` messages as `ReferenceTick`s tagged with `provider = "polymarket_rtds_chainlink"`.
3. Keep `ReferencePrice.source` equal to each market's asset-matched Chainlink Data Streams URL so existing resolution-source gates remain intact.
4. Treat direct authenticated Chainlink Data Streams access as a fallback only if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.

The direct Chainlink API authentication finding above remains useful for the fallback path, but it is no longer the first path to try for the current workspace bot.

## Next Action

To unblock final live-readiness evidence:

1. Use the new `polymarket_rtds_chainlink` provider for read-only BTC/ETH/SOL paper sessions.
2. Replay the stored sessions and verify deterministic paper/report output.
3. Verify final start/end settlement artifacts for resolved 15-minute markets.
4. Only pursue sponsored/direct Chainlink API credentials if RTDS proves unavailable, delayed, insufficiently precise, or not accepted for settlement-source evidence.

Live trading remains disabled.
