# Live Alpha Runbook

Live Alpha is approval-gated by phase. LA2 adds heartbeat state, user-event parsing, and startup recovery checks only. It does not authorize live order placement, live cancels, cancel-all, controlled fill canaries, maker autonomy, strategy-selected live trading, or LA3 work.

## Scope Boundary

- `LIVE_ORDER_PLACEMENT_ENABLED` remains false.
- The `live-alpha-orders` feature remains off by default.
- Heartbeat POST remains disabled unless explicitly approved for the heartbeat endpoint only.
- User WebSocket support is parser-only in LA2; no live subscription is started by default.
- Unknown venue state halts. LA2 does not submit cancels or run a cancel loop.

## Local Validation

Use local validation before any approved-host check:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline live_heartbeat
cargo test --offline live_user_events
cargo test --offline startup_recovery
```

Expected local posture:

- `live_order_placement_enabled=false`
- `live_alpha_enabled=false`
- `live_alpha_mode=disabled`
- `live_alpha_heartbeat_required=true`
- `live_alpha_gate_status=blocked`

## Startup Recovery

For any non-disabled Live Alpha mode, startup recovery must prove all of the following before live-capable work can continue:

- geoblock passed;
- account preflight passed;
- balance and allowance readback passed;
- open-order readback passed;
- recent-trade readback passed;
- journal replay passed;
- position reconstruction passed;
- reconciliation passed.

Any failed or unknown check produces a halt-required recovery report and durable event plan:

```text
LiveStartupRecoveryStarted
LiveStartupRecoveryFailed
LiveRiskHalt
```

A fully proven startup emits:

```text
LiveStartupRecoveryStarted
LiveStartupRecoveryPassed
```

## Heartbeat

LA2 heartbeat state tracks:

- `heartbeat_id`
- `last_sent_at`
- `last_acknowledged_at`
- `expected_interval_ms`
- `max_staleness_ms`
- `associated_open_orders`
- `heartbeat_enabled`
- `heartbeat_failure_action`

Operational interpretation:

- `healthy`: gate may proceed only if every other gate also passes.
- `not_started`, `unknown`, `rejected`, or `stale`: live-capable modes stay blocked.
- stale or rejected heartbeat must trigger halt-and-reconcile handling before any resume.

Inspect heartbeat-related code and evidence:

```text
rg -n -i "postHeartbeat|heartbeat|LiveHeartbeatStale" src runbooks verification
cargo test --offline live_heartbeat
```

## User Events

LA2 parses official user-channel order and trade events from fixtures:

- order `PLACEMENT`
- order `UPDATE`
- order `CANCELLATION`
- trade `MATCHED`
- trade `MINED`
- trade `CONFIRMED`
- trade `RETRYING`
- trade `FAILED`

Inspect parser coverage:

```text
cargo test --offline live_user_events
rg -n -i "wss://ws-subscriptions-clob|MATCHED|MINED|CONFIRMED|RETRYING|FAILED" src/live_user_events.rs
```

## Approved-Host Read-Only Check

Run only with explicit approval and approved host/session context. Do not print secret values or auth headers.

```text
cargo run -- --config <approved-live-alpha-config> validate --live-readback-preflight
```

Record exact output in a verification note using counts and statuses only:

- geoblock country/region and blocked state;
- open-order count;
- recent-trade count;
- balance and allowance pass/fail;
- heartbeat state;
- recovery/reconciliation result.

## Resume Requirements

Resume from halt only after:

- the halt reason is documented;
- journal replay and reconciliation pass;
- open orders and recent trades are read back;
- heartbeat is healthy if required;
- geoblock/account/balance/allowance checks pass;
- the next phase has explicit human/operator approval.

Do not resume by cancel-all, live order submission, autonomous cancel loops, or starting LA3.
