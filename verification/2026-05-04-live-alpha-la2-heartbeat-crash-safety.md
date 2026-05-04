# 2026-05-04 Live Alpha LA2 Heartbeat Crash Safety

## Scope

LA2 adds heartbeat state/evaluation, user-event fixture parsing, startup recovery evaluation, durable halt/recovery event types, runbooks, and status handoff updates.

This phase does not authorize or add live order placement, live canceling, cancel-all, taker/FOK/FAK/marketable-limit live calls, controlled fill canaries, maker autonomy, strategy-selected live trading, LA3 work, or resetting/bypassing the consumed LB6 one-order cap.

## Official Documentation Recheck

Official Polymarket docs were rechecked before LA2 coding:

- User channel: `https://docs.polymarket.com/market-data/websocket/user-channel`
  - authenticated endpoint is `wss://ws-subscriptions-clob.polymarket.com/ws/user`;
  - order events include `PLACEMENT`, `UPDATE`, and `CANCELLATION`;
  - trade lifecycle statuses include `MATCHED`, `MINED`, `CONFIRMED`, `RETRYING`, and `FAILED`.
- Authentication: `https://docs.polymarket.com/api-reference/authentication`
  - authenticated CLOB trading endpoints, including heartbeat, require L2 `POLY_*` headers;
  - L2 credentials cover open-order queries, balance/allowance checks, and posting signed orders, while order creation still requires a signed order payload.
- Geographic restrictions: `https://docs.polymarket.com/api-reference/geoblock`
  - builders should check `GET https://polymarket.com/api/geoblock` before placing orders;
  - blocked regions must remain fail-closed.
- Orders/cancel/readback: `https://docs.polymarket.com/trading/orders/cancel`
  - cancel endpoints require L2 auth;
  - open-order objects include order status, size, matched size, price, order type, owner, and associated trades;
  - trade history uses the same `MATCHED`/`MINED`/`CONFIRMED`/`RETRYING`/`FAILED` lifecycle.
- Heartbeat and order safety: `https://docs.polymarket.com/trading/orders/overview`
  - heartbeat uses `postHeartbeat`;
  - latest `heartbeat_id` must be reused;
  - official timing is 5-second sends with 10-second validity plus a 5-second buffer.
- Fees: `https://docs.polymarket.com/trading/fees`
  - fees are set per market at match time;
  - makers are not charged fees in the documented fee table;
  - LA2 does not create or submit orders.
- Rate limits: `https://docs.polymarket.com/api-reference/rate-limits`
  - CLOB readback and trading endpoints have documented limits and must fail closed if unavailable or throttled.

## Heartbeat Behavior

LA2 adds `src/live_heartbeat.rs`.

Tracked fields:

- `heartbeat_id`
- `last_sent_at`
- `last_acknowledged_at`
- `expected_interval_ms`
- `max_staleness_ms`
- `associated_open_orders`
- `heartbeat_enabled`
- `heartbeat_failure_action`

State actions:

- `HeartbeatNotStarted`
- `HeartbeatHealthy`
- `HeartbeatStale`
- `HeartbeatRejected`
- `HeartbeatUnknown`

Default timing follows the official docs shape:

- `expected_interval_ms=5000`
- `max_staleness_ms=15000`

Heartbeat readiness maps only `HeartbeatHealthy` to passed. Stale or rejected heartbeat maps to failed. Not-started or unknown heartbeat maps to unknown. The Live Alpha gate blocks live-capable modes when `heartbeat_required=true` and heartbeat readiness is failed or unknown.

Network heartbeat POST is intentionally disabled in LA2:

- `HEARTBEAT_NETWORK_POST_ENABLED=false`
- official method label retained as `postHeartbeat`

## User-Event Parser Fixture Result

LA2 adds `src/live_user_events.rs`.

The parser covers official user channel order and trade events:

- order `PLACEMENT`
- order `UPDATE`
- order `CANCELLATION`
- trade `MATCHED`
- trade `MINED`
- trade `CONFIRMED`
- trade `RETRYING`
- trade `FAILED`

The user WebSocket network subscription remains disabled in LA2:

- `USER_CHANNEL_NETWORK_ENABLED=false`

Focused result:

```text
cargo test --offline live_user_events
```

Result: PASS.

## Startup Recovery Behavior

LA2 adds `src/live_startup_recovery.rs`.

For non-disabled Live Alpha modes, recovery evaluation requires:

- geoblock check;
- account preflight;
- balance/allowance readback;
- open-order readback;
- recent-trade readback;
- journal replay;
- position reconstruction;
- reconciliation.

Failed or unknown state enters halt-required status. The halt event plan includes:

- `LiveStartupRecoveryStarted`
- `LiveStartupRecoveryFailed`
- `LiveRiskHalt`

Passed recovery emits:

- `LiveStartupRecoveryStarted`
- `LiveStartupRecoveryPassed`

Startup recovery detects unknown open orders through reconciliation and halts. LA2 does not submit cancels, add cancel-all, or add an autonomous cancel loop.

## Readback And Reconstruction

No optional approved-host live read-only or heartbeat check has been run for this LA2 branch as of this note.

Local readback integration added:

- `TradeReadback` now carries an optional related order ID from documented `taker_order_id` or `maker_orders[].order_id`.
- startup recovery can convert read-only open-order/trade readback into `VenueLiveState`.
- trade status `RETRYING` is preserved and treated as nonterminal.

Journal/reducer updates:

- `LiveHeartbeatStale` marks risk halted.
- `LiveStartupRecoveryFailed` marks risk halted.
- `LiveRiskHalt` remains risk halted.
- `LiveTradeRetrying` is a durable event type.

## Halt Behavior For Stale Or Unknown State

LA2 fail-closed behavior:

- stale heartbeat maps to failed readiness and blocks live-capable modes;
- not-started/unknown heartbeat maps to unknown readiness and blocks live-capable modes when heartbeat is required;
- rejected heartbeat maps to failed readiness;
- unknown startup checks halt;
- unknown open orders halt;
- nonterminal trade statuses `MATCHED`, `MINED`, and `RETRYING` halt reconciliation until proven terminal.

## Verification Commands

Focused checks:

```text
cargo test --offline live_heartbeat
cargo test --offline live_user_events
cargo test --offline live_reconciliation
cargo test --offline startup_recovery
cargo test --offline risk_halt
cargo test --offline live_alpha_gate
cargo test --offline live_beta_readback
```

Results:

- `live_heartbeat`: PASS
- `live_user_events`: PASS
- `live_reconciliation`: PASS
- `startup_recovery`: PASS
- `risk_halt`: PASS
- `live_alpha_gate`: PASS
- `live_beta_readback`: PASS

These focused checks passed before final full verification and after the LA2 code changes.

## Final Verification

Passed:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
git status --short --branch
rg -n -i "(postHeartbeat|heartbeat|wss://ws-subscriptions-clob|user.*channel|MATCHED|MINED|CONFIRMED|RETRYING|FAILED)" src runbooks verification
rg -n -i "(createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|submit.*order|place.*order|FOK|FAK)" src Cargo.toml config
rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
```

Exact results:

- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS, run ID `18ac38592fea3be0-1664c-0`; output confirmed `live_order_placement_enabled=false`, `live_alpha_enabled=false`, `live_alpha_mode=disabled`, `live_alpha_heartbeat_required=true`, `live_alpha_compile_time_orders_enabled=false`, and `live_alpha_gate_status=blocked`.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 283 lib tests, 8 main tests, 0 doc tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- `git status --short --branch`: branch `live-alpha/la2-heartbeat-crash-safety` with only LA2 source/docs/status/verification changes.
- Heartbeat/user-event scan: PASS with expected hits for LA2 heartbeat constants/state, user-channel parser fixtures, durable event names, runbooks, and historical verification text.
- Order/FOK/FAK scan: PASS with expected hits only for LA2 fail-closed config flags/tests, existing LB6 gated canary code, paper-order simulation, and disabled placement gates.
- Live-alpha/gate scan: PASS with expected hits for disabled defaults, heartbeat/reconciliation/risk halt gates, config, and tests.
- Sensitive/no-secret scan: PASS. Broad scan hits were expected historical docs, non-secret env handle names, public fixture IDs, public feed IDs, and existing gated LB6 code. Targeted scan over new LA2 files found only warning text and the scan command itself; no secret values, API-key values, private-key material, raw L2 credentials, auth headers, signed payloads, mnemonic, seed phrase, or wallet/private-key material were added.

## Safety Result

PASS.

- No live order was placed.
- No live cancel was sent.
- No cancel-all path was added.
- No autonomous cancel loop was added.
- No strategy-selected live trading was added.
- No controlled fill canary or LA3 work was started.
- `LIVE_ORDER_PLACEMENT_ENABLED` remains false.
- The `live-alpha-orders` feature remains off by default.
- Heartbeat POST remains disabled in LA2 unless separately approved for only the heartbeat endpoint.
- User WebSocket support remains parser-only in LA2.
- No optional approved-host live read-only or heartbeat check was run for this LA2 branch.

## Result

LA2 PASS for heartbeat, user events, startup recovery, durable halt/recovery events, runbooks, and evidence only. LA2 remains branch-local until PR review/merge and does not authorize LA3.
