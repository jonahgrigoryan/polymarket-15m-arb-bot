# API Verification Checklist: Polymarket CLOB V2

## Purpose

This document defines what must be verified against Polymarket CLOB V2 before the bot relies on any external API behavior.

The goal is to prevent coding against stale assumptions. Polymarket's CLOB V2 migration changes order fields, fee handling, collateral, market info, and SDK support. API behavior must be verified with official docs and read-only live checks before implementation decisions are locked.

Before the April 28, 2026 cutover, the V2 test endpoint was:

```text
https://clob-v2.polymarket.com
```

As of the April 28, 2026 post-cutover recheck, Polymarket docs and live read-only behavior identify the production CLOB endpoint as:

```text
https://clob.polymarket.com
```

## Verification Rules

- Prefer official Polymarket docs and official Polymarket GitHub repositories.
- Capture evidence for every completed check: command, timestamp, endpoint, sanitized response sample, and result.
- Do read-only checks first.
- Do not use private keys for MVP verification unless explicitly required for a signing-only test.
- Do not place real orders for MVP verification.
- If a check fails or is ambiguous, mark the dependent implementation area blocked.
- If docs and live behavior disagree, live behavior wins only after it is reproduced and documented.

## Evidence Format

Each verification item should be recorded using this shape:

```text
Check:
Date/time:
Environment:
Endpoint:
Command or script:
Expected:
Observed:
Result: PASS | FAIL | BLOCKED | NEEDS_RECHECK
Notes:
```

Store larger captured payloads under a future `verification/` directory instead of pasting huge JSON blobs into this file.

## Go/No-Go Summary

The implementation can start M0 and M1 without completing this checklist because those milestones are local scaffolding, config, event models, and storage.

M2 and later require API verification:

- M2 market discovery requires verified Gamma/keyset market fields and geoblock response shape.
- M3 feed ingestion requires verified CLOB WebSocket endpoint, subscription shape, and message shapes.
- M5 signal/risk requires verified fee fields, token/outcome mapping, and resolution-source fields.
- Any future live-beta work requires verified V2 signing, auth, post/cancel behavior, and rate limits.

## Checklist

### 1. V2 Endpoint And Cutover Behavior

Purpose: confirm which CLOB base URL should be used during development and after cutover.

Official docs to check:

- `https://docs.polymarket.com/v2-migration`
- `https://docs.polymarket.com/api-reference/introduction`

What to verify:

- `https://clob-v2.polymarket.com` is reachable before cutover.
- `https://clob.polymarket.com` is the production base URL.
- After April 28, 2026 around 11:00 UTC, production should serve V2 behavior.
- Any downtime, order wipe, or migration notes are captured.
- V2 uses pUSD collateral and new exchange contracts.

Suggested evidence:

```text
curl -i https://clob-v2.polymarket.com/ok
curl -i https://clob.polymarket.com/ok
```

Pass criteria:

- The configured endpoint responds.
- The selected endpoint matches the current date/cutover state.
- The implementation has one config field for CLOB base URL so it can be switched without code edits.

Blocks:

- Market discovery can still use Gamma.
- CLOB REST and signing behavior must wait if endpoint state is unclear.

Post-cutover evidence, 2026-04-28:

- Official docs checked: `https://docs.polymarket.com/api-reference/introduction` and `https://docs.polymarket.com/v2-migration`.
- Docs state the CLOB API base URL is `https://clob.polymarket.com`; the V2 migration page says `https://clob-v2.polymarket.com` was for pre-cutover testing and V2 takes over `https://clob.polymarket.com` after the April 28 go-live.
- `curl -sS -D - https://clob.polymarket.com/ok -o /tmp/polymarket-clob-ok.txt` returned `HTTP/2 200` and body `"OK"`.
- `curl -sS -D - https://clob-v2.polymarket.com/ok -o /tmp/polymarket-clob-v2-ok.txt` returned `HTTP/2 301` with `location: https://clob.polymarket.com/ok`.
- Result: PASS. `config/polymarket-rtds-chainlink.example.toml` now uses `https://clob.polymarket.com`.

### 2. Market Discovery Endpoint

Purpose: confirm how BTC, ETH, and SOL 15-minute markets are discovered and filtered.

Official docs to check:

- `https://docs.polymarket.com/api-reference/markets/list-markets-keyset-pagination`

Primary endpoint:

```text
GET https://gamma-api.polymarket.com/markets/keyset
```

What to verify:

- Keyset pagination request and response shape.
- Pagination cursor fields and termination behavior.
- Field names for:
  - `id`
  - `conditionId`
  - `slug`
  - `question`
  - `resolutionSource`
  - `startDate`
  - `endDate`
  - `active`
  - `closed`
  - `enableOrderBook`
  - `acceptingOrders`
  - `orderPriceMinTickSize`
  - `orderMinSize`
  - `clobTokenIds`
  - `outcomes`
  - `feesEnabled` or equivalent fee indicator if present
  - `fee_schedule` or equivalent fee schedule if present
- Whether `clobTokenIds` and `outcomes` are JSON strings or arrays in live payloads.
- Whether 15-minute crypto markets are identifiable by slug, title, event metadata, tags, series, or another stable field.
- Whether market start/end fields are exact enough to classify 15-minute windows.

Suggested evidence:

```text
curl -s 'https://gamma-api.polymarket.com/markets/keyset?limit=5' | jq .
```

Pass criteria:

- The implementation can identify BTC, ETH, and SOL 15-minute markets without fragile title-only matching.
- Token IDs and outcomes can be paired deterministically.
- Markets missing required metadata can be marked ineligible.

Blocks:

- Market discovery implementation.
- Token/outcome mapping.
- Market lifecycle state.

### 3. Token IDs And Outcome Mapping

Purpose: confirm that token IDs map correctly to Up/Down or Yes/No outcomes for each market.

Official docs to check:

- `https://docs.polymarket.com/api-reference/markets/list-markets-keyset-pagination`
- `https://docs.polymarket.com/api-reference/markets/get-clob-market-info`

Primary endpoints:

```text
GET https://gamma-api.polymarket.com/markets/keyset
GET https://clob.polymarket.com/clob-markets/{condition_id}
```

What to verify:

- Gamma `clobTokenIds` ordering.
- Gamma `outcomes` ordering.
- CLOB market info `t` array fields:
  - `t` token ID
  - `o` outcome label
- Whether Gamma and CLOB token/outcome mapping agree.
- Whether binary crypto markets use `Up`/`Down`, `Yes`/`No`, or another label.
- Whether condition IDs in Gamma match condition IDs accepted by CLOB market info.

Suggested evidence:

```text
curl -s 'https://clob.polymarket.com/clob-markets/{condition_id}' | jq .
```

Pass criteria:

- For a sample BTC, ETH, and SOL 15-minute market, token ID to outcome label is confirmed from CLOB market info.
- The implementation stores explicit token/outcome pairs and does not rely on array position alone.

Blocks:

- Order book subscription asset IDs.
- Signal direction.
- Paper position accounting.

### 4. CLOB WebSocket Endpoint And Subscription Shape

Purpose: confirm the public market data stream used by live paper mode.

Official docs to check:

- `https://docs.polymarket.com/market-data/websocket/market-channel`

Endpoint:

```text
wss://ws-subscriptions-clob.polymarket.com/ws/market
```

Subscription shape from docs:

```json
{
  "assets_ids": ["<token_id_1>", "<token_id_2>"],
  "type": "market",
  "custom_feature_enabled": true
}
```

What to verify:

- Endpoint connects from the intended host.
- Subscription field is exactly `assets_ids` as documented, not `asset_ids`.
- `custom_feature_enabled: true` is accepted.
- Multiple token IDs can be subscribed on one connection.
- Behavior when subscribing to expired, inactive, or invalid token IDs.
- Heartbeat/ping/pong behavior.
- Reconnect behavior after network interruption.

Suggested evidence:

Use a small local WebSocket script to subscribe to one active market's two token IDs and capture the first 20 messages.

Pass criteria:

- A read-only WebSocket client can receive market data for active token IDs.
- The subscription payload is confirmed against live behavior.
- Messages include enough timestamps and market/token identifiers for normalization.

Blocks:

- Feed ingestion.
- Order book state.
- Live paper mode.

### 5. CLOB WebSocket Message Shapes

Purpose: confirm every message type needed by the event normalizer and order book state.

Official docs to check:

- `https://docs.polymarket.com/market-data/websocket/market-channel`

Message types to verify:

- `book`
- `price_change`
- `tick_size_change`
- `last_trade_price`
- `best_bid_ask`
- `new_market`
- `market_resolved`

For `book`, verify:

- `event_type`
- `asset_id`
- `market`
- `bids`
- `asks`
- level `price`
- level `size`
- `timestamp`
- `hash`

For `price_change`, verify:

- `market`
- `price_changes`
- nested `asset_id`
- nested `price`
- nested `size`
- nested `side`
- nested `hash`
- nested `best_bid`
- nested `best_ask`
- `timestamp`
- `event_type`
- whether `size = "0"` removes a price level.

For `best_bid_ask`, verify:

- `asset_id`
- `best_bid`
- `best_ask`
- `spread`
- `timestamp`
- requirement for `custom_feature_enabled: true`.

For `last_trade_price`, verify:

- `asset_id`
- `price`
- `side`
- `size`
- `fee_rate_bps`
- `timestamp`
- whether trade side is from taker perspective.

Pass criteria:

- All observed message variants can be parsed into normalized events.
- Unknown fields are preserved in raw storage.
- Unknown event types are logged and stored, not silently dropped.
- Order book update semantics are confirmed with snapshots and deltas.

Blocks:

- Normalization.
- Order book state.
- Paper maker queue simulation.

### 6. V2 Client And Signing Support

Purpose: determine whether the official Rust client can be used directly or whether a minimal custom V2 client is required.

Official docs to check:

- `https://docs.polymarket.com/api-reference/clients-sdks`
- `https://docs.polymarket.com/v2-migration`
- `https://github.com/Polymarket/rs-clob-client-v2`

What to verify:

- Official Rust package name and repository.
- Repository has recent V2 commits and examples.
- Package builds in this project.
- Examples cover:
  - market data reads
  - authentication
  - order creation/signing
  - order posting
  - order canceling
- V2 order fields are supported:
  - `timestamp`
  - `metadata`
  - `builder`
- V1-only fields are not required from users:
  - `nonce`
  - `feeRateBps`
  - `taker`
- EIP-712 exchange domain version is V2.
- API auth headers are unchanged from V1 where docs say they are.
- SDK can point to `https://clob-v2.polymarket.com`.

Allowed MVP verification:

- Build the SDK.
- Run read-only examples.
- Generate/sign an order locally with a test wallet if explicitly approved.
- Do not submit real orders.

Pass criteria:

- Rust SDK can support planned read-only and future signing needs, or a clear gap list justifies a custom minimal client.
- Signing behavior is verified before any future live-beta PRD relies on it.

Blocks:

- Future live order placement.
- Any code path that assumes SDK signing behavior.

### 7. Fee Fields And Fee Calculation

Purpose: confirm fee sources and fee math before EV calculations depend on them.

Official docs to check:

- `https://docs.polymarket.com/trading/fees`
- `https://docs.polymarket.com/api-reference/markets/get-clob-market-info`
- `https://docs.polymarket.com/api-reference/market-data/get-fee-rate`
- `https://docs.polymarket.com/v2-migration`

What docs currently state:

- Makers are not charged fees.
- Takers pay fees on fee-enabled markets.
- Fees are determined at match time.
- V2 removes `feeRateBps` from user-set order fields.
- `getClobMarketInfo(conditionID)` returns fee details under `fd`.
- Crypto taker fee rate is listed in the trading fees doc.

What to verify live:

- Market object fee indicator: `feesEnabled` or equivalent.
- CLOB market info fields:
  - `mos`
  - `mts`
  - `mbf`
  - `tbf`
  - `fd.r`
  - `fd.e`
  - `fd.to`
- Fee-rate endpoint response for a token ID:
  - `base_fee`
- Whether 15-minute BTC, ETH, and SOL markets always have fees enabled.
- Whether WebSocket `last_trade_price.fee_rate_bps` is useful or stale under V2.
- Exact formula to use in paper taker simulation.

Suggested evidence:

```text
curl -s 'https://clob.polymarket.com/clob-markets/{condition_id}' | jq '.fd, .mbf, .tbf, .mos, .mts, .t'
curl -s 'https://clob.polymarket.com/fee-rate?token_id={token_id}' | jq .
```

Pass criteria:

- EV calculation can load market-specific fee parameters.
- Maker fee is treated as zero unless docs/live data change.
- Taker fee is included in all taker paper fills.

Blocks:

- Signal EV calculation.
- Paper taker fill simulation.
- Strategy reporting.

### 8. Geoblock Endpoint

Purpose: confirm compliance gate behavior before any trading-capable mode starts.

Official docs to check:

- `https://docs.polymarket.com/api-reference/geoblock`

Endpoint:

```text
GET https://polymarket.com/api/geoblock
```

Expected response fields:

- `blocked`
- `ip`
- `country`
- `region`

What to verify:

- Endpoint is reachable from the intended deployment host.
- Response shape matches docs.
- `blocked = true` prevents `paper` mode startup.
- `validate` mode can report geoblock status without running strategy logic.
- The code does not contain bypass behavior.

Suggested evidence:

```text
curl -s https://polymarket.com/api/geoblock | jq .
```

Pass criteria:

- Compliance check works and is persisted with the run.
- Blocked response fails closed.

Blocks:

- Live paper mode on deployment host.
- Any future live-trading release gate.

### 9. Rate Limits

Purpose: set conservative client-side throttles before polling or recovery logic is implemented.

Official docs to check:

- `https://docs.polymarket.com/api-reference/rate-limits`

What to verify:

- Gamma API limits:
  - general limit
  - `/markets`
  - listing limits
- CLOB API limits:
  - general limit
  - `/book`
  - `/books`
  - `/price`
  - `/prices`
  - `/midpoint`
  - `/midpoints`
  - `/clob-markets/{condition_id}`
  - order endpoints for future live-beta planning only
- Behavior when limits are exceeded:
  - throttled/delayed
  - rejected
  - Cloudflare response shape
- Whether WebSocket subscription limits exist outside the rate-limit page.

Pass criteria:

- Config includes conservative polling intervals and request budgets.
- REST is used only for startup, recovery, and metadata.
- WebSocket is the primary market data source.
- Rate-limit responses are parsed and surfaced as degraded state.

Blocks:

- Market discovery polling cadence.
- REST book recovery behavior.
- Future order placement throughput assumptions.

### 10. Order Book Snapshot Recovery

Purpose: confirm how to rebuild book state after startup or WebSocket degradation.

Official docs to check:

- `https://docs.polymarket.com/api-reference/market-data/get-order-book`
- `https://docs.polymarket.com/api-reference/market-data/get-order-books-request-body`
- `https://docs.polymarket.com/market-data/websocket/market-channel`

What to verify:

- Single-book endpoint shape.
- Batch-books endpoint shape.
- Whether book snapshots include hashes compatible with WebSocket `hash`.
- Whether REST snapshots use token ID or condition ID.
- How to safely reconcile REST snapshot with incoming WebSocket deltas.

Pass criteria:

- Startup can load initial books.
- Recovery can replace in-memory book state after a gap.
- Snapshot/delta reconciliation rules are documented.

Blocks:

- Robust order book state.
- Replay parity with live paper sessions after reconnects.

### 11. Resolution Source For 15-Minute Crypto Markets

Purpose: confirm the actual settlement/reference source for BTC, ETH, and SOL 15-minute up/down markets.

Official docs to check:

- Market `resolutionSource` from Gamma.
- Market description/rules from Gamma/event metadata.
- Any Polymarket docs for real-time data sockets or crypto market resolution.

What to verify:

- For each asset, identify the resolution source used by current 15-minute markets.
- Confirm whether the source is Chainlink RTDS, another oracle, exchange close, or market-specific text rules.
- Confirm whether start price and end price are published in metadata.
- Confirm how delayed/finalized resolution works.
- Confirm whether BTC, ETH, and SOL use the same source/rule pattern.

Pass criteria:

- Signal engine can distinguish settlement source from predictive CEX feeds.
- Markets with ambiguous resolution rules are ineligible for trading decisions.

Blocks:

- Fair-probability model.
- Signal correctness.
- Post-market P&L verification.

M9 access recheck, 2026-04-28:

- Current captured BTC/ETH/SOL markets point to the asset-matched Chainlink Data Streams pages:
  - BTC: `https://data.chain.link/streams/btc-usd`
  - ETH: `https://data.chain.link/streams/eth-usd`
  - SOL: `https://data.chain.link/streams/sol-usd`
- The Chainlink pages identify these as reference-price Data Streams, but the public pages are delayed informational pages.
- Chainlink's real-time Data Streams REST and WebSocket APIs require authenticated access headers.
- Unauthenticated REST probe against the BTC feed ID returned missing `Headers.UserId`, `Headers.Timestamp`, and `Headers.HmacSignature`.
- Decision: API section 11 remains PASS for identifying the market resolution source and distinguishing it from predictive CEX feeds, but live reference-backed M9 paper strategy validation remains PARTIAL until authorized Data Streams access exists and authenticated ingestion is explicitly scoped.

Temporary Pyth proxy note, 2026-04-28:

- Pyth Hermes BTC/USD, ETH/USD, and SOL/USD latest prices are available through unauthenticated read-only HTTPS for testing.
- Pyth proxy reference ticks may be used only in explicitly enabled paper/replay sessions.
- Pyth proxy ticks are not settlement-source evidence for current sampled Polymarket markets because those markets cite Chainlink Data Streams.
- Pyth proxy reports must carry `live_readiness_evidence=false` and `settlement_reference_evidence=false`.
- See `verification/2026-04-28-m9-pyth-proxy-reference.md`.

Polymarket RTDS Chainlink addendum, 2026-04-28:

- Polymarket's official RTDS docs identify `wss://ws-live-data.polymarket.com` and an unauthenticated `crypto_prices_chainlink` stream for `btc/usd`, `eth/usd`, and `sol/usd`.
- The bot now treats `polymarket_rtds_chainlink` as the first read-only reference provider for M9 paper/replay testing. Direct authenticated Chainlink Data Streams remains a fallback only if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.
- RTDS reference ticks are persisted with provider/source metadata `polymarket_rtds_chainlink`, while `ReferencePrice.source` remains the asset-matched Chainlink Data Streams URL required by existing market-resolution gates.
- Bounded run `m9-rtds-chainlink-smoke-20260428b` persisted 12 BTC/ETH/SOL RTDS Chainlink `ReferenceTick`s, proceeded beyond the all-`missing_reference_price` blocker, and replayed deterministically. Current replay fingerprint after the runtime ordering compatibility fix is `sha256:8a4dce14a349b92dcf10dfb7dbce1f079f667b2fe91689fb6e93d0fa91f3e0df`.
- RTDS-backed reference ingestion is settlement-source plumbing evidence, but final M9 live-readiness remains PARTIAL until Chainlink-source paper sessions produce/validate natural risk-reviewed paper behavior and final start/end settlement artifacts are verified.
- See `verification/2026-04-28-m9-polymarket-rtds-chainlink-reference.md`.

Natural RTDS paper validation addendum, 2026-04-28:

- Longer bounded runs `m9-rtds-natural-20260428d` and `m9-rtds-natural-20260428e` used unchanged signal/risk gates with `config/polymarket-rtds-chainlink.example.toml`.
- Runtime ordering was fixed so post-RTDS read-only CLOB book snapshots are evaluated against the discovered Gamma market even when CLOB messages use condition IDs.
- `m9-rtds-natural-20260428d`: 168 raw messages, 205 normalized rows, 48 RTDS ticks, 0 orders, 0 fills, 0.0 P&L, replay fingerprint `sha256:20bba0230ba09694c567f1503c5e044b4ef9a361be563d403e4b20fd8b25b228`.
- `m9-rtds-natural-20260428e`: 126 raw messages, 156 normalized rows, 36 RTDS ticks, 0 orders, 0 fills, 0.0 P&L, replay fingerprint `sha256:746d6a18a0d6607d3738fd9a38e8efc919d0d1ab588635ddb03fe52ecf5c0dd4`.
- Natural RTDS-backed paper trades remain NOT EXERCISED because no order intent reached risk approval. Post-fix skip reasons include unchanged EV gate failures (`edge_below_minimum`) plus natural freshness skips.
- See `verification/2026-04-28-m9-rtds-natural-paper-validation.md`.

### 12. Server Time And Timestamp Handling

Purpose: ensure latency and V2 timestamp assumptions are correct.

Official docs to check:

- `https://docs.polymarket.com/api-reference/markets/get-server-time`
- `https://docs.polymarket.com/v2-migration`

What to verify:

- CLOB server time endpoint shape.
- Clock skew between local host and CLOB server.
- V2 order `timestamp` unit is milliseconds.
- WebSocket message timestamp unit is milliseconds.
- Whether timestamps are strings or numbers in live payloads.

Pass criteria:

- Code treats external timestamps consistently.
- Local monotonic timestamps are used for latency and ordering.
- Wall-clock timestamps are used for reporting and lifecycle alignment.

Blocks:

- Replay ordering.
- Latency metrics.
- Future V2 signing.

## Verification Output

Before M2/M3 implementation is considered complete, produce a dated verification note with:

- Current endpoint set.
- Sample market discovery payload.
- Sample CLOB market info payload.
- Token/outcome mapping sample for BTC, ETH, and SOL.
- Sample WebSocket messages for each observed event type.
- Geoblock response from deployment host.
- Rate-limit assumptions.
- Fee model assumptions.
- Any blocked or ambiguous API behavior.

Suggested path:

```text
verification/YYYY-MM-DD-api-verification.md
```

## Implementation Gates

```text
M0 local scaffold:
  API verification not required.

M1 event model/storage:
  API verification not required, but event shapes should be updated after API samples.

M2 market discovery:
  Requires sections 1, 2, 3, 8, and 9.

M3 feed ingestion:
  Requires sections 4, 5, 9, and 10.

M4 state/order books:
  Requires sections 3, 5, and 10.

M5 signal/risk:
  Requires sections 7, 8, 11, and 12.

Future live beta:
  Requires every section, plus a separate live-trading PRD, legal/access review, key management plan, and signing audit.
```

## Source Links

- Polymarket V2 migration: https://docs.polymarket.com/v2-migration
- Polymarket clients and SDKs: https://docs.polymarket.com/api-reference/clients-sdks
- Polymarket market WebSocket channel: https://docs.polymarket.com/market-data/websocket/market-channel
- Polymarket market discovery keyset endpoint: https://docs.polymarket.com/api-reference/markets/list-markets-keyset-pagination
- Polymarket CLOB market info endpoint: https://docs.polymarket.com/api-reference/markets/get-clob-market-info
- Polymarket fee rate endpoint: https://docs.polymarket.com/api-reference/market-data/get-fee-rate
- Polymarket fees: https://docs.polymarket.com/trading/fees
- Polymarket geoblock endpoint: https://docs.polymarket.com/api-reference/geoblock
- Polymarket rate limits: https://docs.polymarket.com/api-reference/rate-limits
