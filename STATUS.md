# Project Status Handoff

Last updated: 2026-04-27

## Purpose

`STATUS.md` is the current-state handoff file for coding agents. Use it to resume work without re-deriving the active branch, milestone, gate, verification state, blockers, and next action from scratch.

Authoritative sources remain:

- `AGENTS.md`: permanent project rules and safety boundaries.
- `PRD.md`: product requirements and project scope.
- `IMPLEMENTATION_PLAN.md`: milestone roadmap, build tasks, verification, and exit gates.
- `API_VERIFICATION.md`: required external API verification checklist.
- `verification/*.md`: dated evidence logs.
- `STATUS.md`: current handoff context.

## Current Branch

- Branch: `m9/multi-session-validation`
- Short commit: `610de7f`
- Worktree status: M9 runtime paper/replay replacement, storage-backed fixture validation, and live findings changes are present but uncommitted.

## Milestones

- Last completed milestone: M8 - Observability And Production-Like Runbook.
- Active milestone: M9 - Multi-Session Validation And Live-Readiness Review is PARTIAL because runtime capture/replay now works, but the real session produced no paper orders/fills due missing verified resolution-source reference ticks.
- Next milestone: verify and wire the real resolution/reference source required by Polymarket rules, then rerun BTC/ETH/SOL paper sessions and replay determinism with actual paper decisions.

## M3 Scope Lock

M3 proved read-only feed ingestion and normalization. It did not promote `paper` or `replay` into live runtimes.

M3 includes:

- API verification sections 4, 5, 9, and 10.
- `validate --feed-smoke` checks for Polymarket CLOB, Binance, and Coinbase.
- REST `/book` snapshot normalization into `BookSnapshot`.
- Raw and normalized event persistence.
- Feed health/staleness observability.

M3 explicitly defers:

- Full live paper runtime.
- Strategy execution and simulated order placement from live feeds.
- Replay execution over captured feed sessions.
- Live resolution-source ingestion until API section 11 confirms the actual settlement source/subscription behavior.

Historical M3 runtime markers were intentionally placeholder-only. Current M9 runtime work replaces those placeholders with file-backed paper sessions and deterministic replay.

## M3 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| API sections 4, 5, 9, 10 | PASS | See `verification/2026-04-27-m3-api-verification.md`. |
| Polymarket `validate --feed-smoke` | PASS | Connected read-only; raw and normalized events persisted. |
| Binance `validate --feed-smoke` | PASS | Connected read-only; trade tick normalized. |
| Coinbase `validate --feed-smoke` | PASS | Connected read-only; ticker messages normalized; one non-ticker/control message preserved as unknown. |
| REST `/book` snapshot to `BookSnapshot` | PASS | Live snapshot recovery probe normalized one snapshot. |
| Raw plus normalized persistence | PASS | Final live gate persisted 6 raw messages and 6 normalized events. |
| Feed staleness / health | PASS | `FeedHealthTracker` tested; smoke reports connected health. |
| Paper runtime | NA at M3 | Deferred at M3; replaced by M9 file-backed runtime work. |
| Replay runtime | NA at M3 | Deferred at M3; replaced by M9 deterministic replay CLI work. |
| Resolution-source ingest | PARTIAL | Generic adapter/parser exists; live source verification deferred to section 11 before M5. |

Heartbeat intent for M3:

- Send text `PING` on idle reads.
- Ignore text `PING`/`PONG` as control messages, not feed payloads.
- Respond to WebSocket protocol ping frames with protocol pong frames.
- No heartbeat behavior change is required before M3 commit.

## M4 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| API sections 3, 5, 10 | PASS | See `verification/2026-04-27-m4-api-verification.md`. |
| Deterministic book updates | PASS | `state::order_book` tests cover snapshot replacement, deltas, removals, and identical event sequences. |
| Explicit stale state | PASS | `BookFreshness` and `ReferenceFreshness` expose missing/fresh/stale states. |
| Coherent decision snapshots | PASS | `StateStore::decision_snapshot` returns one read-only view of market, book, reference, predictive, and explicit position state. |
| Runtime scope lock | PASS | At M4, `paper` and `replay` were not promoted; no strategy, paper execution, live order, signing, wallet, or private-key path was added. |

## M7 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| Captured/synthetic runs replay deterministically | PASS | `ReplayEngine` consumes ordered `EventEnvelope`s from fixtures or `StorageBackend::read_run_events`; storage-backed config snapshot loading and in-memory storage replay tests pass. |
| Report generation works offline | PASS | `reporting::ReplayReport` is built from replay-local records only and includes latency, feed-staleness, opportunity, paper audit, and per-market/per-asset P&L summaries. |
| Determinism checks fail on intentional event drift | PASS | Replay tests remove an input event, mutate an ordering key, and compare generated paper events against recorded paper events. |
| M4/M5/M6 logic is reused | PASS | Replay updates `StateStore`, evaluates `SignalEngine`/`RiskEngine`, opens/fills only through `PaperExecutor`, and updates `PaperPositionBook`. |
| Runtime scope lock | PASS | At M7, runtime CLI promotion was deferred; M7 was library/offline wiring only. |

## M8 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| Metrics endpoint works against test config | PASS | `validate --local-only --metrics-smoke` renders the M8 metric families through an ephemeral loopback `/metrics` endpoint and verifies the response body. |
| Structured logs include operational fields | PASS | Runtime logs include `run_id`, `mode`, `source`, `event_type`, `reason`, and shutdown fields; the metrics field contract includes market, asset, risk reason, and replay fingerprint fields. |
| Graceful shutdown works | PASS | `GracefulShutdownState` transitions running -> draining -> complete and runtime commands emit a final shutdown log with `accepting_new_work=false` on successful and failed command paths. |
| Runbook commands work against test config | PASS | M8 runbook commands cover local validation, metrics smoke, offline replay/reporting tests, safety scan, and service-template expectations. |
| Runtime scope lock | PASS | At M8, runtime CLI promotion was deferred; metrics smoke was local/test-only and no live trading path was added. |

## M9 Acceptance Matrix

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| At least one full captured paper session per BTC/ETH/SOL can be replayed | PASS for capture/replay mechanics | Live bounded run `m9-runtime-smoke-20260427b` captured BTC/ETH/SOL markets, raw feed messages, normalized events, config snapshot, balance/P&L artifacts, reports, and metrics under `reports/sessions/m9-runtime-smoke-20260427b`; `replay --run-id m9-runtime-smoke-20260427b` replayed deterministically. |
| Replay determinism passes for selected sessions | PASS | Runtime replay fingerprint matched paper report fingerprint: `sha256:f1446dc2b3a6bb4862df7cfd9c9cd6b5629655ff5869dc1ee227153d4b5b7d60`. Storage-backed fixture drift tests still cover recorded-paper divergence. |
| Reports identify whether strategy performance survives fees and conservative fills | PARTIAL | Real runtime report had 6 signal evaluations, all skipped for `missing_reference_price`, so no paper orders/fills/fees were produced. Fixture tests still exercise fee/P&L math, but real strategy performance needs verified resolution-source reference ticks. |
| Live-readiness blockers are listed before real orders | PASS | See `verification/2026-04-27-m9-live-readiness-findings.md`. |
| Live trading remains disabled | PASS | `LIVE_ORDER_PLACEMENT_ENABLED=false`; safety scan found no live order, signing, wallet/key, API-key, real CLOB order-client, live-trading, external-write, or new live-feed path introduced by M9. |

## M6 Current State

- M6 paper execution is now reachable from the `paper` runtime through the same replay/state/signal/risk/paper path used by deterministic replay. The latest live bounded session produced no orders because signal evaluation failed closed on missing resolution-source reference price.
- `src/paper_executor.rs` has been split into `src/paper_executor/mod.rs`, `src/paper_executor/lifecycle.rs`, and `src/paper_executor/pnl.rs`.
- `PaperExecutor` consumes only risk-approved `PaperOrderIntent`s. A denied risk decision emits a paper audit rejection and creates no paper order.
- Paper lifecycle support covers open, partial, filled, canceled, expired, and rejected states with explicit audit events.
- Maker fills use conservative visible-queue assumptions from later trade ticks; taker fills consume visible executable book depth and per-market fee parameters.
- `PaperPositionBook` tracks positions by market/token/asset, average price, realized P&L, unrealized marks, settlement marks, fees, and deterministic exposure snapshots.
- Storage now has paper position and balance write APIs matching the existing Postgres M6 tables.
- Position and risk context exposure remain in `state::snapshot` and `risk_engine` from M5 context.
- Final start/end settlement artifact verification remains deferred; M7 reports can carry this evidence when available, but no live final settlement artifact verification has been completed.

## Recent Verification

- Evidence file: `verification/2026-04-26-api-verification.md`.
- M3 evidence file: `verification/2026-04-27-m3-api-verification.md`.
- M2 required sections 1, 2, 3, 8, and 9 passed for M2 scope.
- `validate` reached geoblock and reported blocked `US/CA`.
- Gamma keyset discovery listed active BTC/ETH/SOL 15-minute up/down markets.
- Final M2 gate discovered 30 matching markets across 5 pages, persisted 30 records to Postgres, read back 30 records, and emitted 30 lifecycle events.
- `paper` mode failed closed from the blocked geoblock response.
- No live order placement or signing path was added.
- M3 local checks passed: `cargo fmt --check`, `cargo test --offline`, `cargo clippy --offline -- -D warnings`, `cargo run --offline -- validate --local-only --config config/default.toml`, and `cargo run -- validate --local-only --feed-smoke --feed-message-limit 1 --config config/default.toml`.
- M3 feed smoke connected read-only to Polymarket CLOB, Binance, and Coinbase.
- M3 feed smoke persisted 6 raw messages and emitted 6 normalized events.
- M3 REST book snapshot recovery probe normalized 1 book snapshot.
- M3 parser/recorder tests cover documented CLOB market WebSocket message types, REST book snapshots, Binance ticks, Coinbase ticks, generic resolution-source ticks, raw+normalized persistence, staleness, heartbeat filtering, and reconnect backoff.
- M4 evidence file: `verification/2026-04-27-m4-api-verification.md`.
- M4 local checks passed: `cargo fmt --check`, `cargo test --offline`, `cargo clippy --offline -- -D warnings`, and `cargo run --offline -- validate --local-only --config config/default.toml`.
- M4 safety scan found no source path for live order placement, signing, wallet, API key, or private-key handling.
- M5 evidence file: `verification/2026-04-27-m5-api-verification.md`.
- M5 local checks passed: `cargo fmt --check`, `cargo test --offline` (69 tests), `cargo clippy --offline -- -D warnings`, and `cargo run --offline -- validate --local-only --config config/default.toml`.
- M5 signal tests cover controlled fair probability, EV with maker/taker costs, raw fee formula handling, phase classification, candidates, missing/mismatched resolution source skips, ineligible-market skips, and explicit skip reasons.
- M5 risk tests cover stale reference, stale book, geoblock, market loss, market/asset/total/correlated notional, order rate, daily drawdown, ineligible/asset-mismatched resolution source rejection, approval, and multi-reason rejection.
- M5 discovery tests cover asset-matched Chainlink resolution rule eligibility and ineligible handling for mismatched or incomplete metadata.

M6 executor/P&L implementation status: PASS. Final settlement artifact verification status: PARTIAL.
- M6 local checks passed: `cargo fmt --check`, `cargo test --offline` (84 tests), and `cargo clippy --offline -- -D warnings`.
- M6 lifecycle tests cover maker fills, taker fills with per-market fees, partial fill math, cancel, expire, reject, and the no-risk-approval/no-order invariant.
- M6 P&L tests cover taker fee math, maker fee zero, position average price updates, realized P&L, unrealized P&L, deterministic replay of identical fills, settlement marking to winning/losing/split market outcomes, and storage-ready position snapshots.
- M6 storage tests cover paper order, fill, position, balance, and risk-event write paths.
- M6 safety scan found no source path for live order placement, signing, wallet, API key, or private-key handling. Matches were documentation/status text plus the live-order-disabled runtime flag.
- No runtime CLI promotion was done during M6 itself; M9 now wires the runtime path.

M7 verification status: PASS.
- M7 local checks passed: `cargo fmt --check`, `cargo test --offline` (96 tests), and `cargo clippy --offline -- -D warnings`.
- M7 replay tests cover synthetic deterministic replay, storage-backed event loading in deterministic order, captured config snapshot loading, identical-input determinism, ordering-key drift failure, removed-event drift failure, and generated-vs-recorded paper-event comparison.
- M7 reporting tests cover deterministic report fingerprints, fingerprint drift, event/signal/risk/paper/P&L counts, audit details, latency summary, feed-staleness windows, opportunity diagnostics, fee totals, and per-market/per-asset P&L grouping.
- M7 safety scan found no new source path for live order placement, signing, wallet, API key, private-key handling, live feed, or network behavior in the replay/report diff. Full-tree network/feed hits are existing M3 read-only validation/feed-ingestion paths and documented endpoint config.
- No runtime CLI promotion was done during M7 itself; M9 now wires deterministic replay from file-backed sessions.

M8 verification status: PASS.
- M8 local checks passed: `cargo fmt --check`, `cargo test --offline` (102 tests), and `cargo clippy --offline -- -D warnings`.
- M8 baseline validate passed: `cargo run --offline -- validate --local-only --config config/default.toml`.
- M8 metrics smoke passed: `cargo run --offline -- validate --local-only --metrics-smoke --config config/default.toml`; the smoke now checks every required M8 metric family.
- M8 replay/runbook smoke passed against the then-current replay CLI behavior.
- M8 paper/runbook smoke passed against the then-current paper CLI behavior.
- M8 handoff checks passed: `cargo test --offline replay::` and `cargo test --offline reporting::`.
- M8 metrics tests cover stable Prometheus metric names/labels, rendering/counting, every required M8 metric family, one-shot local `/metrics` behavior, and structured log field contract.
- M8 shutdown tests cover fail-closed shutdown state transitions and CLI runtime mode names.
- M8 runbook artifacts were added under `docs/m8-observability-runbook.md` and `runbooks/polymarket-15m-arb-bot.service.template`.
- M8 safety scan found no source path for live order placement, signing, wallet, API key, private-key handling, live trading, external write behavior, or live feed subscription in the M8 diff. New network hits are limited to local loopback metrics smoke (`TcpListener` on `127.0.0.1:0` and a single local `GET /metrics`).
- Full source/config safety scan over `src`, `Cargo.toml`, and `config` found only the expected `live_order_placement_enabled=false` output fields.
- Direct scrape of `http://127.0.0.1:9100/metrics` is not applicable yet because no long-running metrics process is bound; M8 currently verifies metrics through the one-shot loopback smoke endpoint.

M9 verification status: PARTIAL.
- M9 evidence file: `verification/2026-04-27-m9-live-readiness-findings.md`.
- M9 added storage-backed fixture-session tests: `replay::tests::m9_storage_backed_fixture_sessions_replay_for_default_assets` and `replay::tests::m9_storage_backed_fixture_paper_event_determinism_fails_when_recorded_event_is_missing`.
- M9 local checks passed after runtime wiring: `cargo fmt --check`, `cargo test --offline` (105 tests), and `cargo clippy --offline -- -D warnings`.
- M9 targeted replay/report/session checks passed: `cargo test --offline replay::` (10 tests), `cargo test --offline reporting::` (4 tests), `cargo test --offline m9_storage_backed_fixture_sessions_replay_for_default_assets --lib -- --nocapture`, `cargo test --offline m9_storage_backed_fixture_paper_event_determinism_fails_when_recorded_event_is_missing --lib`, `cargo test --offline events::tests::replay_ordering_key_uses_required_fields`, `cargo test --offline events::tests::every_normalized_event_variant_round_trips`, and `cargo test --offline storage::tests::in_memory_storage_round_trips_sample_records`.
- M9 runtime checks passed: `cargo run --offline -- validate --local-only --config config/default.toml`, `cargo run -- --config config/default.toml paper --run-id m9-runtime-smoke-20260427b --feed-message-limit 1 --cycles 1`, and `cargo run --offline -- --config config/default.toml replay --run-id m9-runtime-smoke-20260427b`.
- Runtime captured session evidence:
  - Run ID: `m9-runtime-smoke-20260427b`.
  - Session dir: `reports/sessions/m9-runtime-smoke-20260427b`.
  - Captured files: `config_snapshot.json`, `raw_messages.jsonl`, `normalized_events.jsonl`, `markets.jsonl`, paper order/fill/position/balance/risk JSONL files, `paper_report.json`, `replay_report.json`, and Prometheus metrics files.
  - Counts: 3 selected markets, 11 raw messages, 18 normalized events, 3 market records, 1 paper balance, 0 paper orders, 0 fills, 0 positions, 0 risk events.
  - Replay determinism fingerprint: `sha256:f1446dc2b3a6bb4862df7cfd9c9cd6b5629655ff5869dc1ee227153d4b5b7d60`.
  - Signal review: 6 evaluations, 0 emitted order intents, 6 skips, all `missing_reference_price`; counts by asset BTC=2, ETH=2, SOL=2.
  - Paper P&L: 0.000000 total P&L because no paper positions were opened.
- Storage-backed fixture session run IDs and report fingerprints:
  - BTC: `m9-btc-captured-paper-fixture`, report `sha256:5d902f0a82481f8f7482247c71ccb2fbd482945c0255054ab1c0741338f9ffb5`, paper events `sha256:b96ea689336f413c0c9e21aae4cdf31c2b3908ede82064b335f2f6849170f3d8`, input `sha256:c801cbcc9afb71314b05170b9dc41c959c9b5518da0dd71c461f67505064220c`.
  - ETH: `m9-eth-captured-paper-fixture`, report `sha256:e3544a62b85c3619a455d8ebb18b48a3c68ea18d33c82467e3550d317a3325dc`, paper events `sha256:b24c0089378088ba98b23ae508eab794c2a9b8723f87640d442dce80b69a8f96`, input `sha256:19b0539cc475a9ae48ee07bf132c0037d3811e332671b4446ed98548b192577c`.
  - SOL: `m9-sol-captured-paper-fixture`, report `sha256:2f36b64fa6a854af2f61e37dcb63fa5f9e38745b26db7052eb6307bb71005c37`, paper events `sha256:1bd0a4533fc30d8c4f5c2c15526bd4d5638814cd0078297ae7d4cba0959e762e`, input `sha256:ab682519635083bcfade4fb75cca1d11b60e5455631ebc645005ac081fc02cb6`.
- Shared fixture config snapshot fingerprint: `sha256:d6e612ea490f722e60c09f7069ca397f300953fb8268ca188f57bc38d9eb9037`.
- Each storage-backed fixture session produced 1 risk-approved taker fill, 0.200000 fees paid, and total P&L -0.250000, so the report evidence shows the fixture strategy result does not survive fees/conservative fill marking.
- Runtime captured paper-session evidence now exists under `reports/sessions/m9-runtime-smoke-20260427b`; include it in the M9 commit if preserving live evidence locally is desired.
- Dependency/live-readiness audit found no Polymarket SDK, no signing/wallet/key/API-key dependency path, no live order path, and no `.post`/`.put`/`.delete` order endpoint path in source.
- M9 safety scans found no source path for live order placement, signing, wallet/key handling, API-key handling, real CLOB order clients, or live trading. New runtime network behavior is read-only geoblock, market discovery, CLOB book snapshots/WebSocket capture, Binance/Coinbase WebSocket capture, and local file writes under `reports/sessions/<run_id>`.
- M9 remains PARTIAL for trading evidence because the real runtime session did not receive verified resolution-source reference ticks and therefore correctly produced no paper orders/fills.

## Blockers And Risks

- M4 API verification sections 3, 5, and 10 are complete for M4 scope.
- M5 API verification sections 7, 8, 11, and 12 are complete for M5 signal/risk scope.
- Final start/end settlement artifact verification remains deferred for paper P&L/reporting; this no longer blocks M5 because ambiguous or asset-mismatched resolution rules are ineligible at discovery, signal, and risk gates.
- Polymarket geoblock is host/session-specific; prior M2 evidence observed blocked `US/CA`, while the current read-only M5 recheck observed unblocked `MX/CHP`. Trading-capable modes must remain fail-closed on blocked, malformed, or unreachable geoblock checks.
- CLOB V2 cutover timing is time-sensitive; recheck endpoint assumptions if work continues after the April 28, 2026 cutover window.
- Verified resolution-source reference ingestion and final start/end settlement artifact verification are still required before M6/M7/M8/M9 reporting can claim live post-market reconciliation or real strategy performance.

## Next Concrete Action

Verify and wire the real Polymarket/Chainlink resolution-source reference feed so signal evaluation can produce or reject paper candidates using the same source Polymarket settlement rules cite; then rerun captured BTC/ETH/SOL paper sessions and replay determinism.

## Update Checklist

When updating this file, include:

- Current branch and short commit.
- Clean/dirty worktree status and any unrelated user changes to preserve.
- Last completed milestone and active milestone.
- Next required exit gate.
- Latest verification evidence paths and outcomes.
- Current blockers, risks, and API assumptions.
- One concrete next action.
