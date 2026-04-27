# API Verification: 2026-04-27 M5

## Scope

M5 signal/risk depends on `API_VERIFICATION.md` sections 7, 8, 11, and 12.

This note records M5 API verification. It does not add strategy execution, paper execution, live orders, signing, wallet, or key handling.

Inputs reviewed:

- `API_VERIFICATION.md` sections 7, 8, 11, and 12.
- Existing verification notes for M2, M3, and M4.
- Official Polymarket docs for fees, geoblock, rate limits, RTDS crypto prices, market channel WebSocket, resolution, server time, and V2 migration.
- Read-only live checks against Gamma, CLOB V2, geoblock, CLOB WebSocket, and RTDS Chainlink WebSocket.
- Read-only recheck at `2026-04-27T12:04:15Z` against Gamma keyset, CLOB V2 market info, CLOB V2 `/time`, CLOB V2 fee-rate, and geoblock.

## M5 Dependency Matrix

| API section | M5 status | M5 dependency |
| --- | --- | --- |
| Section 7: Fee fields and fee calculation | PASS | Load market-specific fee parameters from Gamma `feeSchedule` / CLOB V2 `fd`; include taker fees in EV and taker paper fills; treat maker fee as zero per docs unless official docs/live data change. |
| Section 8: Geoblock endpoint | PASS | Run geoblock at startup for trading-capable modes; fail closed on blocked/unreachable responses; never bypass. |
| Section 11: Resolution source for 15-minute crypto markets | PASS for M5 signal/risk | Current sampled BTC/ETH/SOL markets explicitly point to asset-matched Chainlink Data Streams, and M5 now keeps markets ineligible when `resolutionSource` or rules are missing, non-Chainlink, asset-mismatched, or ambiguous. Final start/end settlement artifact reconciliation remains deferred to paper P&L/reporting work. |
| Section 12: Server time and timestamp handling | PASS | CLOB server time is Unix seconds; CLOB WebSocket timestamps are string Unix milliseconds; RTDS timestamps are number Unix milliseconds; use monotonic local time for latency and wall-clock time for lifecycle/reporting. |

## Section 7: Fee Fields And Fee Calculation

Result: PASS for M5 signal/risk.

Official docs confirm:

- Fees are set by protocol and applied at match time.
- Markets with fees enabled have `feesEnabled = true`.
- Formula: `fee = C * feeRate * p * (1 - p)`.
- Makers are never charged fees; only takers pay.
- Crypto taker fee rate is `0.072`.
- V2 removes user-set `feeRateBps`; `getClobMarketInfo(conditionID)` returns fee details in `fd = { r, e, to }`.

Read-only live sample from `2026-04-27T12:04:15Z`:

```text
markets:
btc-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/btc-usd feesEnabled=true feeSchedule={exponent:1,rate:0.072,takerOnly:true,rebateRate:0.2}
eth-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/eth-usd feesEnabled=true feeSchedule={exponent:1,rate:0.072,takerOnly:true,rebateRate:0.2}
sol-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/sol-usd feesEnabled=true feeSchedule={exponent:1,rate:0.072,takerOnly:true,rebateRate:0.2}

first 5 Gamma keyset pages:
matching_crypto_15m=12
eligible_source_fee_shape=12

clob-v2 getClobMarketInfo fields for all three samples:
mos=5
mts=0.01
mbf=1000
tbf=1000
fd={r:0.072,e:1,to:true}
t=[{t:<token_id>,o:Up},{t:<token_id>,o:Down}]

clob-v2 fee-rate first Up token for each sample:
base_fee=1000
```

M5 decision:

- EV and paper taker fills should use the documented formula with market fee schedule / `fd` fee details, not WebSocket `last_trade_price.fee_rate_bps`.
- Store the observed `mbf`, `tbf`, and `base_fee` fields for audit, but do not reinterpret them against the documented formula without a separate SDK/exchange-specific check.
- Reject or mark ineligible any market where `feesEnabled`, `feeSchedule`, or `fd` cannot be loaded.
- M5 code now treats maker fees as zero, uses raw fee rate when present for taker EV, and only falls back to stored taker bps when raw fee config is unavailable.

## Section 8: Geoblock And Rate Limits

Result: PASS for M5 geoblock dependency.

Official docs confirm:

- Endpoint: `GET https://polymarket.com/api/geoblock`.
- Fields: `blocked`, `ip`, `country`, `region`.
- The endpoint is hosted on `polymarket.com`, not CLOB or Gamma.
- Orders from blocked regions are rejected.

Read-only live sample from `2026-04-27T12:04:15Z`:

```text
{
  "blocked": false,
  "country": "MX",
  "region": "CHP",
  "has_ip": true
}
```

M5 decision:

- Treat the geoblock result as host/session specific. Prior M2 evidence observed a blocked `US/CA` response; this Codex session currently egresses as `MX/CHP`.
- `paper` and any future trading-capable mode must fail closed on blocked, malformed, or unreachable geoblock checks.
- `validate` may report geoblock status without running strategy logic.
- No bypass behavior is allowed.

Rate-limit dependency inherited from existing Section 9 notes and rechecked against official docs:

- Cloudflare throttles on sliding windows.
- Gamma `/markets`: 300 requests / 10 seconds.
- CLOB `/book`: 1,500 requests / 10 seconds.
- CLOB `/books`: 500 requests / 10 seconds.
- M5 should keep REST to startup, metadata, and recovery; WebSocket remains the primary market-data path.

## Section 11: Resolution Source For 15-Minute Crypto Markets

Result: PASS for M5 signal/risk.

The current source identity is not ambiguous for the sampled markets:

```text
btc-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/btc-usd
eth-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/eth-usd
sol-updown-15m-1777376700 resolutionSource=https://data.chain.link/streams/sol-usd
```

The sampled BTC, ETH, and SOL descriptions state that the market resolves using the corresponding Chainlink data stream URL, not other sources or spot markets.

Official docs confirm:

- Polymarket market rules define the resolution source, end date, and edge cases.
- Polymarket resolution uses UMA Optimistic Oracle flow after outcome knowledge.
- RTDS exposes a `crypto_prices_chainlink` topic with `btc/usd`, `eth/usd`, and `sol/usd` symbols.

Read-only live RTDS check:

```text
wss://ws-live-data.polymarket.com
subscription topic=crypto_prices_chainlink filter={"symbol":"btc/usd"}
topic=crypto_prices_chainlink type=update timestamp_type=number timestamp_digits=13 payload_symbol=btc/usd payload_timestamp_type=number payload_timestamp_digits=13 payload_has_value=true
```

What remains outside M5:

- Current sampled `resolutionSource` fields and descriptions identify Chainlink Data Streams, so the source itself is not guessed.
- Sampled Gamma `metadata` was `null`; start/end price values were not published in the market metadata at verification time.
- The generic resolution docs cover UMA finalization/challenge timing, but no sampled metadata field gave a final start price, end price, or final settlement tick artifact for post-market P&L reconciliation.

M5 decision:

- Treat Chainlink RTDS as the settlement-reference feed only when the market `resolutionSource` points to the matching Chainlink stream and the description/rules match.
- Keep Binance/Coinbase as predictive/reference feeds, not settlement truth.
- Mark any market ineligible if `resolutionSource` is missing, non-Chainlink, asset-mismatched, or contradicts rules text.
- M5 code now enforces this at discovery, signal, and risk gates.
- Defer final start/end settlement reconciliation until paper P&L/reporting can observe resolved 15-minute market metadata or another official final-price artifact.

## Section 12: Server Time And Timestamp Handling

Result: PASS for M5 signal/risk timing assumptions.

Official docs confirm:

- `GET /time` returns CLOB server Unix timestamp in seconds.
- V2 order `timestamp` is order creation time in milliseconds and replaces nonce for uniqueness.
- Market WebSocket messages carry timestamp fields as strings in Unix milliseconds.
- RTDS top-level and payload timestamps are numbers in Unix milliseconds.

Read-only live checks:

```text
https://clob-v2.polymarket.com/time server_seconds=1777291372
https://clob.polymarket.com/time    server_seconds=1777290494 approx_skew_ms=-535 rtt_ms=413

wss://ws-subscriptions-clob.polymarket.com/ws/market
event_type=book timestamp_type=string timestamp_digits=13

wss://ws-live-data.polymarket.com
topic=crypto_prices_chainlink type=update timestamp_type=number timestamp_digits=13 payload_timestamp_type=number payload_timestamp_digits=13
```

M5 decision:

- Normalize external milliseconds timestamps to one internal representation immediately at ingest.
- Parse CLOB market WebSocket timestamps from strings; parse RTDS timestamps from numbers.
- Do not use CLOB `/time` seconds values as event timestamps without conversion.
- Use local monotonic time for latency measurement and ordering diagnostics.
- Use wall-clock UTC for market lifecycle alignment, logs, and reporting.

## Gate Status

M5 can proceed for signal/risk modeling. Section 11 is passable for M5 because source ambiguity is now enforced as a market eligibility guard.

Blocked for M5:

- None remaining.

Not blocked for M5:

- Fee-aware EV calculation using `feeSchedule` / `fd`.
- Geoblock fail-closed gate.
- Conservative REST rate budgets with WebSocket-primary market data.
- Timestamp normalization and latency measurement.
- Markets without explicit, asset-matched Chainlink `resolutionSource` and matching rules text, because they are now ineligible before signal/risk approval.

Deferred beyond M5:

- Final realized P&L reconciliation based on official start/end settlement prices, until those artifacts are separately verified.
