# API Verification: 2026-04-27 M4

## Scope

M4 requires `API_VERIFICATION.md` sections 3, 5, and 10.

This note records the M4 API dependencies for in-memory state and order books. It does not add live write behavior, strategy execution, paper execution, or live order logic.

Inputs reviewed:

- `API_VERIFICATION.md` sections 3, 5, and 10.
- `verification/2026-04-26-api-verification.md`.
- `verification/2026-04-27-m3-api-verification.md`.
- Official Polymarket docs for CLOB market info, market WebSocket channel, `GET /book`, and `POST /books`, rechecked on 2026-04-27.

## M4 Dependency Matrix

| API section | M4 status | M4 dependency |
| --- | --- | --- |
| Section 3: Token IDs and outcome mapping | PASS | Order book state must key books by token ID / asset ID and keep explicit token/outcome labels from discovery. |
| Section 5: CLOB WebSocket message shapes | PASS | State must apply `book` as a full snapshot and `price_change` as side-specific level updates/removals. |
| Section 10: REST book snapshot recovery | PASS | REST snapshot endpoint shape is verified; M4 now defines and tests deterministic state replacement from `BookSnapshot`. |

## Section 3: Token IDs And Outcome Mapping

M2 verification passed this dependency.

Confirmed behavior:

- CLOB market info accepts `condition_id`.
- CLOB market info returns token objects in `t`, with token ID in `t` and outcome label in `o`.
- Current BTC/ETH/SOL 15-minute markets use explicit `Up` and `Down` outcome labels in the verified sample.
- Implementation should prefer CLOB token/outcome mapping, then fall back to Gamma `clobTokenIds` paired with `outcomes`.

M4 dependency:

- Do not infer outcome from token array position after parsing.
- Keep the market's two explicit `OutcomeToken` records available to decision snapshots.
- Use token ID / asset ID as the order book key for REST and WebSocket book state.

## Section 5: CLOB WebSocket Message Shapes

M3 verification passed this dependency for feed normalization.

Confirmed behavior:

- Market WebSocket subscription uses `assets_ids`.
- `book` includes `event_type`, `asset_id`, `market`, `bids`, `asks`, level `price`, level `size`, `timestamp`, and `hash`.
- `price_change` includes `market`, `price_changes`, nested `asset_id`, `price`, `size`, `side`, `hash`, `best_bid`, `best_ask`, root `timestamp`, and `event_type`.
- Official docs state that `size = "0"` removes a price level from the book.
- M3 parser tests cover the documented message variants. The live M3 sample observed `book`; other documented variants are covered by parser tests until longer live capture observes them.

M4 dependency:

- Treat `book` as full replacement for one token book.
- Treat `price_change` entries as updates to the side named by `side`.
- Remove the level when `size` is zero.
- Preserve the latest observed `hash` and source timestamp for freshness and audit state.
- Keep unknown event types raw-persisted upstream; M4 state should only mutate on recognized normalized book events.

## Section 10: Order Book Snapshot Recovery

M3 verification passed REST endpoint shape and normalization, and M4 closes the local reconciliation requirement with deterministic snapshot replacement tests.

Confirmed behavior:

- Single-book recovery uses `GET /book?token_id=<token_id>`.
- Batch-book recovery uses `POST /books` with request items containing `token_id`.
- REST snapshot responses include `market`, `asset_id`, `timestamp`, `hash`, `bids`, `asks`, `min_order_size`, `tick_size`, `neg_risk`, and `last_trade_price`.
- REST and WebSocket snapshots normalize into the same `BookSnapshot` shape.

M4 dependency:

- Startup can seed each token book from REST `BookSnapshot`.
- Recovery after WebSocket degradation can replace the in-memory token book with a fresh REST `BookSnapshot`.
- Snapshot replacement should be deterministic and should clear levels that are absent from the replacement snapshot.
- Delta application should be deterministic after replacement; if ordering or gap confidence is lost, prefer a fresh REST snapshot over trying to repair from stale deltas.
- Hashes are available for audit/freshness, but the reviewed docs do not provide a sequence number that proves no missed deltas between REST recovery and WebSocket resume.

## Gate Status

M4 API sections 3, 5, and 10 are PASS for the in-memory state and order-book scope.

Implementation evidence:

- `state::order_book` tests cover snapshot replacement, price-level updates/removals, best bid/ask, spread, depth, last trade, freshness, and deterministic replay of the same event sequence.
- `state::snapshot` tests cover stale book/reference state and coherent read-only decision snapshots with explicit position state. Position accumulation remains deferred to paper-execution work; M4 exposes an empty read-only position vector rather than omitting the field.
- Recovery behavior is local state replacement from a fresh `BookSnapshot`; no strategy, paper execution, live order behavior, signing, or private-key path was added.
