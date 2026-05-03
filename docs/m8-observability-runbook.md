# M8 Observability And Production-Like Runbook

This runbook is for the current replay-first, paper-first MVP. It does not cover live order placement, signing, wallets, private keys, API keys, real CLOB order clients, or live trading paths.

## Scope And Assumptions

- Runtime modes remain `validate`, `paper`, and `replay`.
- `paper` and `replay` are now file-backed runtime paths after M9 work; treat a paper run as valid evidence only when it writes a session under `reports/sessions/<run_id>` and `replay --run-id <run_id>` passes deterministically.
- The source of record is persisted raw/normalized events plus config snapshots, not metrics.
- M6 final start/end settlement artifact verification remains partial unless a separate verification file proves it.
- Geoblock, feed, storage, stale-state, and risk failures must fail closed: no new paper decisions, no live orders, and clear logs/metrics.

## Local Checklist

These commands avoid external writes and match the current repository conventions:

```sh
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- validate --local-only --config config/default.toml
cargo run --offline -- validate --local-only --metrics-smoke --config config/default.toml
cargo run -- --config config/default.toml paper --run-id m9-runtime-smoke-YYYYMMDD --feed-message-limit 1 --cycles 1
cargo run --offline -- --config config/default.toml replay --run-id m9-runtime-smoke-YYYYMMDD
rg -n "live order|private key|api key|wallet|signing|submit order|create order|order client|clob.*order|secret" src Cargo.toml config
```

Expected local-mode markers:

- `validate`: `validation_status=ok`, `mode=validate`, `online_validation_status=skipped`, `live_order_placement_enabled=false`.
- `paper`: either a fail-closed geoblock error or `paper_mode_status=runtime_enabled`, `paper_runtime_status=ok`, a `paper_report_path`, and `live_order_placement_enabled=false`.
- `replay`: `replay_status=deterministic`, matching generated/recorded paper-event counts, a `replay_report_path`, and `live_order_placement_enabled=false`.
- Safety scan: source hits must not reveal live order placement, signing, wallet, key handling, or a real order client path.

Optional read-only network smoke, no external writes:

```sh
cargo run -- validate --local-only --feed-smoke --feed-message-limit 1 --config config/default.toml
```

Avoid running plain `validate` against `config/default.toml` unless local Postgres is expected to receive discovery-market writes. The online M2 validation path writes discovered markets to local Postgres and fails closed on blocked geoblock status.

## Metrics Check

Configured metrics bind address:

```sh
rg -n "bind_addr|metrics" config/default.toml src runbooks docs
```

When the M8 Prometheus endpoint is wired into a running process, check it locally:

```sh
cargo run --offline -- validate --local-only --metrics-smoke --config config/default.toml
curl -fsS http://127.0.0.1:9100/metrics | rg "feed|latency|reconnect|staleness|signal|paper|pnl|risk|storage|replay"
```

The `--metrics-smoke` command uses an ephemeral loopback listener and exits after one scrape. A long-running runtime may bind `metrics.bind_addr` once that mode is wired. Expected metric families should cover feed message rate, feed latency, WebSocket reconnect count, book/reference staleness, signal count, paper order/fill count, paper P&L, risk halt count, storage write failures, and replay determinism failures. Missing metric families are an M8 gap, not proof that the underlying system is healthy.

## Dashboard Notes

A local Prometheus/Grafana dashboard should stay operational and audit-oriented:

- Feed health: `p15m_feed_message_rate_per_second`, `p15m_feed_latency_ms`, `p15m_websocket_reconnects_total`.
- Freshness: `p15m_book_staleness_ms`, `p15m_reference_staleness_ms`.
- Decisions: `p15m_signal_decisions_total`, `p15m_risk_halts_total`.
- Paper execution: `p15m_paper_orders_total`, `p15m_paper_fills_total`, `p15m_paper_pnl`.
- Reliability: `p15m_storage_write_failures_total`, `p15m_replay_determinism_failures_total`.

Recommended panels are current feed latency by source, stale book/reference gauges by market or asset, signal candidate versus skip counts, risk halt counts by reason, paper P&L by market/asset, and replay determinism failures over time. Alert candidates are stale book/reference age above risk thresholds, any storage write failure, any replay determinism failure, sustained feed message rate of zero during an expected session, and any geoblock or storage-related risk halt.

## Live Beta Observability Addendum

LB7 does not expand live trading. It records the observability contract any future approved live-beta milestone must satisfy before another live action is considered. The LB6 canary was an order-lifecycle probe only and is not profitability evidence.

Required live-beta status fields:

- Live mode: `LIVE_ORDER_PLACEMENT_ENABLED`, live beta gate status, canary submission gate status, and whether the one-order cap is available or consumed.
- Compliance: geoblock status, country/region, approval scope, and host/session identifier.
- Safety controls: kill-switch state, service stop result, human approval hash, post-only/GTD/maker-only checks, one-open-order cap, and cancel-all disabled state.
- Heartbeat: latest heartbeat state, age when available, failure count, and whether heartbeat ambiguity blocked a maker order.
- Orders: attempted order count, accepted order count, rejected order count, venue order ID, market/condition ID, token ID, side, price, size, notional, order type, expiry, and venue status.
- Cancels: exact cancel attempt count, exact canceled order ID, `DELETE /order` response summary, `not_canceled` count, and proof that cancel-all was not used.
- Fills and trades: fill count, matched size, trade statuses, transaction hashes for matched/confirmed trades, fees, and whether settlement follow-up is required.
- Readback integrity: exact single-order readback path `/data/order/{orderID}`, open-order list, Rust readback status, official SDK readback status when used, and any readback mismatch.
- Account state: available pUSD, reserved pUSD, balance/reserved mismatch, allowances, open notional, and post-action balance delta.
- P&L: realized P&L for fills and settlement P&L after market resolution. A single canary with no fill has no strategy-performance inference.

Alert candidates for any future live-beta dashboard:

- live order placement enabled outside an approved milestone window.
- geoblock blocked, unknown, malformed, stale, or unreachable.
- kill switch inactive when a rollback is in progress.
- heartbeat unhealthy, stale, missing, or ambiguous while an order is open.
- any order attempt without the expected human approval hash or one-order cap.
- any accepted order count above the approved cap.
- any cancel-all path or multi-order cancel path becoming reachable.
- any readback mismatch between Rust and the official SDK.
- reserved pUSD nonzero after cancel/readback closeout.
- open notional above the approved cap.
- any fill without terminal trade status, transaction hash handling, fee capture, and settlement follow-up.

Live-beta artifacts should include the validate/preflight output, dry-run approval output, canary submission output if approved, exact single-order readback output, exact cancel output if approved, post-cancel readback output, trade/balance/allowance readback, heartbeat state, runbook incident note if any blocker occurred, and safety/no-secret scan output.

## Structured Logs

Logs should be JSON and include enough fields to reconstruct a run:

- Always: `run_id`, `mode`, config path, assets, level, timestamp.
- Market events: `market_id`, `asset`, `source`, `event_type`.
- Decisions and halts: `reason`, risk decision, skip reason, paper order/fill identifiers.
- Shutdown: signal received, stop phase, flush/persist result, exit status.

Quick local log check:

```sh
cargo run --offline -- validate --local-only --config config/default.toml 2>&1 | rg '"run_id"|"mode"|validation_status|live_order_placement_enabled'
```

## Graceful Shutdown Expectations

`validate` is short-lived. If interrupted, it should stop discovery/feed smoke work and exit without starting strategy or paper execution.

`paper` must shut down in this order for bounded sessions and for operator-managed long-running sessions:

1. Stop accepting new market data for decisions and stop creating new paper intents.
2. Cancel or expire open paper maker orders according to paper-executor rules.
3. Flush raw messages, normalized events, paper orders/fills, positions, balances, and risk events.
4. Emit final metrics/log fields with `run_id`, mode, stop reason, and persistence outcome.
5. Exit nonzero if storage flush fails, geoblock becomes blocked/unreachable, or state cannot be audited.

`replay` must be deterministic during shutdown. If interrupted, it should write no partial report as final, log the interrupted replay run ID, and require a fresh replay to claim determinism.

## Fail-Closed Operations

- Geoblock blocked, malformed, or unreachable: `paper` must not start; operators may run `validate` only to report status.
- Storage unavailable or write/readback mismatch: halt new paper decisions because decisions would not be auditable.
- Feed disconnects, excessive reconnects, stale books, stale reference feed, or missing resolution metadata: mark affected markets ineligible or reject intents through risk; do not synthesize confidence.
- Unknown feed messages: persist raw input and report unknown counts; do not silently discard.
- Replay determinism mismatch: fail the replay check and keep the prior report as suspect until the divergence is explained.

## Replay And Report Verification

Current replay verification includes runtime file-backed sessions plus library/offline tests:

```sh
cargo test --offline replay::
cargo test --offline reporting::
```

When runtime replay is connected to storage, a run is acceptable only if:

- It loads the stored config snapshot for the target `run_id`.
- Event ordering is deterministic by the persisted ordering fields.
- Report fingerprint is stable for identical inputs.
- Generated paper events match recorded paper events when that comparison is requested.
- Report text clearly states that M6 final settlement artifact verification is partial unless separately verified.

## Systemd Notes

Use the template in `runbooks/polymarket-15m-arb-bot.service.template` as an operator starting point. Keep it paper-only and local-metrics-only:

- Start with `validate --local-only` during install checks.
- Use bounded `paper --cycles 1` for smoke sessions; use `paper --cycles 0` only after geoblock and storage checks pass on the deployment host.
- Use `KillSignal=SIGTERM` and allow enough `TimeoutStopSec` for final event flushes.
- Keep `Restart=on-failure`; repeated fail-closed exits should page an operator, not silently continue.
- Do not add environment variables for secrets or live trading credentials in this MVP.
