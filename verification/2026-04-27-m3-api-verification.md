# API Verification: 2026-04-27 M3

## Scope

M3 requires `API_VERIFICATION.md` sections 4, 5, 9, and 10.

This note records official-doc evidence and the current local live-check status for feed ingestion and normalization.

## M3 Scope Lock

M3 exit is for read-only feed ingestion and normalization only.

In scope:

- API verification for sections 4, 5, 9, and 10.
- Read-only `validate --feed-smoke` connection checks for Polymarket CLOB, Binance, and Coinbase.
- REST `/book` snapshot recovery probe into `BookSnapshot`.
- Raw message persistence and normalized event persistence.
- Feed health/staleness observability.

Out of scope for M3:

- Full live paper runtime.
- Strategy execution.
- Paper order generation or fills from live feeds.
- Replay runtime for captured feed sessions.
- Live resolution-source ingestion, because current 15-minute crypto settlement-source details belong to API verification section 11 and are needed before M5 signal correctness.

The CLI confirms runtime stubs remain intentional:

```text
paper_mode_status=stubbed_until_later_milestones
replay_status=stubbed_until_later_milestones
```

## M3 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| API section 4: CLOB WebSocket endpoint/subscription | PASS | Docs confirmed; live `polymarket_clob` feed smoke connected with `assets_ids` subscription. |
| API section 5: CLOB message shapes | PASS | Docs confirmed; parser tests cover documented variants; live sample observed `book`. |
| API section 9: rate limits | PASS | Docs confirmed; REST remains startup/recovery/metadata only; WebSocket is primary live data path. |
| API section 10: REST book snapshot recovery | PASS | Live `/book` probe normalized into `BookSnapshot`; reconciliation deferred to M4 state. |
| `validate --feed-smoke` Polymarket | PASS | `connected=true`, 1 raw message, 2 normalized events. |
| `validate --feed-smoke` Binance | PASS | `connected=true`, 1 raw message, 1 normalized event. |
| `validate --feed-smoke` Coinbase | PASS | `connected=true`, 3 raw messages, 2 normalized events, 1 unknown non-ticker/control message preserved. |
| REST `/book` snapshot to `BookSnapshot` | PASS | `book_snapshot_recovery_status=ok,normalized_events=1`. |
| Raw plus normalized persistence | PASS | Final live gate persisted 6 raw messages and 6 normalized events. |
| Feed staleness / health | PASS | `FeedHealthTracker` and stale threshold tests pass; live smoke prints `health=Connected`. |
| Paper runtime | NA | Explicitly stubbed until later milestones; paper mode still prints `paper_mode_status=stubbed_until_later_milestones`. |
| Replay runtime | NA | Explicitly stubbed until later milestones; replay mode still prints `replay_status=stubbed_until_later_milestones`. |
| Resolution-source ingest | PARTIAL | Generic adapter and parser tests exist; live resolution-source smoke is deferred until section 11 verifies actual settlement source/subscription behavior. |

## Heartbeat Intent

At this checkpoint heartbeat behavior should remain as implemented:

- Send text `PING` on idle reads, matching the documented market-channel heartbeat convention.
- Ignore text `PING` and text `PONG` as control messages, not feed payloads.
- Respond to protocol-level WebSocket ping frames with protocol pong frames.

No change is needed before M3 commit. Treating text `PING`/`PONG` as feed JSON would create false parser failures and pollute raw+normalized feed counts.

## Environment

- Date: 2026-04-27
- Branch: `m3/feed-ingestion-normalization`
- Shell network status: live read-only Polymarket, Binance, and Coinbase feed checks succeeded from this Codex session.
- Official docs access: reachable through browser-backed documentation lookup.

## Section 4: CLOB WebSocket Endpoint And Subscription Shape

Result: PASS.

Official docs confirm:

- Market channel endpoint: `wss://ws-subscriptions-clob.polymarket.com/ws/market`.
- Market channel does not require auth.
- Subscription uses `assets_ids`, not `asset_ids`.
- `custom_feature_enabled: true` enables `best_bid_ask`, `new_market`, and `market_resolved`.
- Market/user channels require heartbeat handling: send `PING` every 10 seconds and expect `PONG`.

Implementation decision:

- `PolymarketMarketSubscription` emits exactly `assets_ids`, `type = "market"`, and `custom_feature_enabled = true`.
- The read-only WebSocket probe performs a standard client handshake, sends the subscription payload, reads text frames, sends `PING` on idle reads, and answers ping frames with pong frames.
- `validate --feed-smoke` is the M3 live check entrypoint.

Live shell evidence:

```text
cargo run -- validate --local-only --feed-smoke --feed-message-limit 1 --config config/default.toml
...
feed_smoke_source=polymarket_clob,connected=true,raw_messages=1,normalized_events=2,unknown_messages=0,health=Connected
```

- Live read-only WebSocket connection and subscription are confirmed.
- The documented heartbeat behavior is implemented by sending text `PING` and ignoring text `PONG` control replies.

## Section 5: CLOB WebSocket Message Shapes

Result: PASS.

Official docs confirm these market channel message types:

- `book`
- `price_change`
- `tick_size_change`
- `last_trade_price`
- `best_bid_ask`
- `new_market`
- `market_resolved`

Implementation decision:

- `book` normalizes to `BookSnapshot`.
- `price_change` normalizes to `BookDelta` and `BestBidAsk` when best prices are included.
- `tick_size_change` normalizes to `TickSizeChange`.
- `last_trade_price` normalizes to `LastTrade`.
- `best_bid_ask` normalizes to `BestBidAsk`.
- `new_market` normalizes to `MarketCreated` with raw metadata preserved.
- `market_resolved` normalizes to `MarketResolved`.
- Unknown event types are raw-persisted and reported as unknown instead of being silently dropped.

Local evidence:

```text
cargo test --offline
...
running 32 tests
...
test result: ok. 32 passed; 0 failed
```

Live sample evidence:

```text
sample_market=sol-updown-15m-1777365000
sample_condition_id=0x42d2bcf28873343fd32ce00ff63d7a5969f8aecebe6859b7c106a216ceb299b8
sample_token_count=2
observed_event_type=book
observed_keys=asks,asset_id,bids,event_type,hash,last_trade_price,market,tick_size,timestamp
observed_bids_len=45
observed_asks_len=45
observed_hash_present=true
```

The live observed `book` shape matched the documented parser shape. Other documented event variants remain covered by parser tests using official examples until they occur during longer live capture.

## Section 9: Rate Limits

Result: PASS for M3 config assumptions.

Official docs confirm:

- Rate limits are Cloudflare-throttled/delayed on sliding windows.
- Gamma `/markets` limit is 300 requests per 10 seconds.
- CLOB market data limits include `/book` at 1,500 requests per 10 seconds and `/books` at 500 requests per 10 seconds.

Implementation decision:

- REST remains startup/recovery/metadata only.
- WebSocket is the primary live market data path.
- Feed config includes timeouts, staleness threshold, and bounded reconnect settings.

## Section 10: Order Book Snapshot Recovery

Result: PASS for M3 API and ingestion scope.

Official docs confirm:

- Single REST snapshot endpoint: `GET /book`.
- Batch REST snapshot endpoint: `POST /books`.
- Snapshot response includes `market`, `asset_id`, `timestamp`, `hash`, `bids`, `asks`, `min_order_size`, `tick_size`, `neg_risk`, and `last_trade_price`.
- Requests use token ID / asset ID.

Implementation decision:

- M3 parser uses the same `BookSnapshot` shape as REST and WebSocket snapshots, including REST snapshots without a WebSocket `event_type`.
- `validate --feed-smoke` fetches a live REST book snapshot, raw-persists it, and normalizes it before WebSocket probes.
- Actual snapshot/delta reconciliation still belongs in M4 order book state so it can be tested with state replacement and gap handling.

Live shell evidence:

```text
cargo run -- validate --local-only --feed-smoke --feed-message-limit 1 --config config/default.toml
...
book_snapshot_recovery_status=ok,normalized_events=1
```

Direct REST evidence:

```text
GET https://clob-v2.polymarket.com/book?token_id=<active_token_id>
single_status=200
fields=market,asset_id,timestamp,hash,bids,asks,tick_size,min_order_size,last_trade_price

POST https://clob-v2.polymarket.com/books
batch_status=200
fields=market,asset_id,timestamp,hash,bids,asks,tick_size,min_order_size,last_trade_price
```

## Gate Status

M3 implementation and live read-only verification are complete.

Passed:

- API sections 4, 5, 9, and 10 are confirmed against official docs.
- Live read-only WebSocket smoke connected to Polymarket CLOB, Binance, and Coinbase.
- Live feed smoke raw-persisted 6 messages and emitted 6 normalized events.
- Live REST book snapshot recovery probe normalized 1 book snapshot.
- Parser tests cover documented CLOB message types.
- Binance, Coinbase, and generic resolution-source ticks normalize into predictive/reference events.
- Raw and normalized feed messages are persisted in the in-memory storage backend.
- Feed staleness is observable through `FeedHealthTracker`.
- Bounded reconnect backoff is implemented and tested.
- No live order placement or signing path was added.

Final live gate evidence:

```text
cargo run -- validate --local-only --feed-smoke --feed-message-limit 1 --config config/default.toml
...
book_snapshot_recovery_status=ok,normalized_events=1
feed_smoke_source=polymarket_clob,connected=true,raw_messages=1,normalized_events=2,unknown_messages=0,health=Connected
feed_smoke_source=binance,connected=true,raw_messages=1,normalized_events=1,unknown_messages=0,health=Connected
feed_smoke_source=coinbase,connected=true,raw_messages=3,normalized_events=2,unknown_messages=1,health=Connected
feed_smoke_persisted_raw_count=6
feed_smoke_persisted_normalized_count=6
```
