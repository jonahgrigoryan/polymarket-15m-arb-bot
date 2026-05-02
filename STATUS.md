# Project Status Handoff

Last updated: 2026-05-01

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

- Branch: `live-beta/lb4-approved-host-readback`
- Base commit: `a4a54d2d1a3876435be73cd7935f80d1d0928549` (merged PR #19 on `main`).
- Worktree status: scoped LB4 approved-host read-only CLOB readback code, status, and verification updates are present. No live order placement, order post, cancel, cancel-all, wallet key material, API-key value, secret value, geoblock bypass, strategy-to-live routing, or strategy/risk/freshness change was added.

## Milestones

- Last completed milestone: LB4 - Authenticated Readback And Account Preflight is PASS for the approved Mexico host/session. M9 remains the last completed replay/paper milestone.
- Active milestone: LB4 - Authenticated Readback And Account Preflight. The LB3 hold release was explicitly approved by the operator on 2026-04-30 for branch `live-beta/lb4-readback-account-preflight`; the approved-host readback attempt was explicitly approved on 2026-04-30 for this Mexico host/session only.
- M9 - Multi-Session Validation And Live-Readiness Review is PASS for paper/replay validation evidence only. M9 still does not authorize live trading, and the settled sample was negative after final reconciliation.
- Next exit gate: LB4 is PASS for approved-host authenticated readback/account preflight from the approved Mexico host/session. LB5/LB6 must not start until the human explicitly authorizes the next phase.

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
| Reports identify whether strategy performance survives fees and conservative fills | PASS for M9 evidence | Startup-log-confirmed current-window RTDS run `m9-rtds-current-window-startuplog-20260429T035356Z` produced 6 natural risk-approved paper fills under unchanged gates and was reconciled after market close. The strategy result for this sample was negative after settlement: filled notional `3.468000`, fees `0.226100`, settlement value `0.000000`, final P&L `-3.694100`. |
| Live-readiness blockers are listed before real orders | PASS | See `verification/2026-04-27-m9-live-readiness-findings.md`. |
| Live trading remains disabled | PASS | `LIVE_ORDER_PLACEMENT_ENABLED=false`; safety scan found no live order, signing, wallet/key, API-key, real CLOB order-client, live-trading, external-write, or new live-feed path introduced by M9. |
| Temporary Pyth proxy paper runtime mechanics | PROXY-PASS | Explicit opt-in runs `m9-pyth-proxy-smoke-20260428c` and `m9-pyth-proxy-self-verify-20260428a` persisted BTC/ETH/SOL proxy `ReferenceTick`s, proceeded beyond the all-`missing_reference_price` blocker, and replayed deterministically with `live_readiness_evidence=false` and `settlement_reference_evidence=false`. |
| Deterministic paper lifecycle fixture | PASS | Offline run `m9-deterministic-paper-lifecycle-20260428a` used the real state/signal/risk/paper/replay path to produce 1 risk-approved taker order, 1 fill, position/balance/P&L artifacts, matching generated-vs-recorded paper events, and deterministic replay fingerprint `sha256:29412f5cae3d50b892f420ad3b3a2a9a27cd878e343ac5fe16d8dc2635aa6a6a`; labels remain `evidence_type=deterministic_fixture`, `live_market_evidence=false`, `live_readiness_evidence=false`, and `settlement_reference_evidence=false`. |
| Natural live/proxy paper trades | NOT EXERCISED | Natural Pyth proxy run `m9-pyth-proxy-natural-20260428a` captured 220 raw messages, 352 normalized-event rows, and 30 proxy `reference_tick`s, then replayed deterministically with fingerprint `sha256:e87608380e016b801462d5b915abcb8950094d38a0a04a7998ccd1d50f6641da`; it produced 0 orders/fills because all 123 signal evaluations skipped (`missing_reference_price=12`, `stale_book=30`, `stale_reference_price=81`). |
| Polymarket RTDS Chainlink reference ingestion | PASS | RTDS sessions persisted BTC/ETH/SOL Chainlink `ReferenceTick`s without credentials. Latest completed natural runs `m9-rtds-natural-20260428d` and `m9-rtds-natural-20260428e` persisted 48 and 36 RTDS ticks respectively, with `reference_provider=polymarket_rtds_chainlink`, `matches_market_resolution_source=true`, `settlement_reference_evidence=true`, `live_readiness_evidence=false`. |
| Natural RTDS-backed paper trades | PASS | Root-cause-fixed current-window runs selected in-window BTC/ETH/SOL markets and replayed deterministically. Latest startup-log-confirmed run `m9-rtds-current-window-startuplog-20260429T035356Z` selected BTC/ETH/SOL markets for 2026-04-29 03:45-04:00 UTC at selection now 2026-04-29 03:54:19.245 UTC, captured 52 normalized rows including 6 RTDS ticks and 5 predictive ticks, produced 40 signal evaluations, 6 signal intents, 6 risk approvals, 6 filled paper orders, filled notional 3.468000, fees 0.226100, total P&L -0.472100, and replayed byte-identically with fingerprint `sha256:2f07ba8506838e846f0f6b3ab29629c70346ee002da24a22c29ae895956bacf3`. |
| Final M9 live-readiness / settlement-source validation | PASS for paper/replay M9 scope | Polymarket RTDS Chainlink provides read-only reference ticks for current Chainlink-resolved markets, natural RTDS-backed paper order/fill behavior is verified under unchanged gates, and final Gamma settlement artifacts were reconciled for the selected BTC/ETH/SOL window. Live trading remains BLOCKED pending a separate live-beta PRD, legal/access review, deployment geoblock check, key management, signing/auth verification, and live risk release gate. |

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
- M9 paper/replay live-readiness evidence is PASS for M9 scope after natural RTDS-backed paper trades and final settlement reconciliation; real live trading remains blocked by separate live-beta requirements.
- M9 reference-feed access recheck changed the implementation path: current BTC/ETH/SOL markets point to Chainlink Data Streams, direct Chainlink APIs require authenticated headers, and Polymarket RTDS exposes an unauthenticated `crypto_prices_chainlink` stream for BTC/ETH/SOL. See `verification/2026-04-28-reference-feed-access.md`.
- M9 Pyth proxy reference validation file: `verification/2026-04-28-m9-pyth-proxy-reference.md`.
- M9 deterministic paper lifecycle fixture evidence file: `verification/2026-04-28-m9-paper-lifecycle-fixture.md`.
- Deterministic paper lifecycle fixture run: `m9-deterministic-paper-lifecycle-20260428a`.
  - Command: `cargo run --offline -- paper --run-id m9-deterministic-paper-lifecycle-20260428a --deterministic-fixture`.
  - Replay command: `cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a`.
  - Artifact shape: `config_snapshot.json`, `raw_messages.jsonl`, `normalized_events.jsonl`, `markets.jsonl`, `paper_orders.jsonl`, `paper_fills.jsonl`, `paper_positions.jsonl`, `paper_balances.jsonl`, `risk_events.jsonl`, `paper_report.json`, `replay_report.json`, and metrics files under `reports/sessions/m9-deterministic-paper-lifecycle-20260428a`.
  - Counts: 6 fixture input events, 2 recorded paper events, 1 paper order, 1 paper fill, 1 paper position, 1 paper balance, 0 risk events.
  - Filled notional / fees / total P&L: `5.100000` / `0.200000` / `-0.250000`.
  - Paper event match fingerprint: `sha256:5100fdb817c179770ca91b5691cb36813c0333c7e712dc41b023ac7143a0cbfb`.
  - Replay determinism fingerprint: `sha256:29412f5cae3d50b892f420ad3b3a2a9a27cd878e343ac5fe16d8dc2635aa6a6a`.
  - Labels: `evidence_type=deterministic_fixture`, `live_market_evidence=false`, `live_readiness_evidence=false`, and `settlement_reference_evidence=false`.
- Temporary Pyth proxy config: `config/pyth-proxy.example.toml`; default config remains `reference_feed.provider = "none"` and `pyth_enabled = false`.
- Pyth proxy bounded run: `cargo run -- --config config/pyth-proxy.example.toml paper --run-id m9-pyth-proxy-smoke-20260428c --feed-message-limit 1 --cycles 1`.
- Pyth proxy replay: `cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-smoke-20260428c`.
- Pyth proxy evidence: 12 raw messages, 21 total normalized-event rows (18 feed/reference rows plus 3 market-discovery lifecycle rows), 3 `reference_tick`s, 9 signal evaluations, 0 paper orders/fills, deterministic fingerprint `sha256:45b10220dcad3cdecf428f53a8d57cdc1b078583a30aade83df3484e471f4ba3`.
- Pyth proxy self-verification run: `m9-pyth-proxy-self-verify-20260428a` captured 12 raw messages, 21 total normalized-event rows, 3 Pyth proxy `reference_tick`s, 0 paper orders, 0 fills, and replayed deterministically with fingerprint `sha256:f05385206b87f7a30986b34002060d2169a75e168d83cad8f8e005ee7a830b6a`.
- Pyth proxy natural run: `m9-pyth-proxy-natural-20260428a` ran `--feed-message-limit 5 --cycles 10`, captured 220 raw messages, 352 normalized-event rows, 30 Pyth proxy `reference_tick`s, 0 paper orders, 0 fills, 0.000000 total P&L, and replayed deterministically with fingerprint `sha256:e87608380e016b801462d5b915abcb8950094d38a0a04a7998ccd1d50f6641da`.
- Natural proxy trade interpretation: natural live/proxy paper trades are NOT EXERCISED for `m9-pyth-proxy-natural-20260428a`; signal skips were `missing_reference_price=12`, `stale_book=30`, and `stale_reference_price=81`, with no risk approvals/rejections because no paper intent reached the risk gate.
- Proxy run interpretation: evaluation proceeded beyond missing reference after proxy ticks and then failed closed on `stale_book` under the current 1-second book freshness threshold. This validates reference plumbing/replay determinism, not strategy profitability.
- Pyth proxy final checks passed: `cargo fmt --check`, `cargo test --offline` (113 tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- validate --local-only --config config/default.toml`, and `cargo run --offline -- validate --local-only --config config/pyth-proxy.example.toml`.
- Latest M9 lifecycle/proxy verification passed: `cargo fmt --check`, `cargo test --offline` (114 tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- validate --local-only --config config/default.toml`, `cargo run --offline -- validate --local-only --config config/pyth-proxy.example.toml`, replay of `m9-deterministic-paper-lifecycle-20260428a`, replay of `m9-pyth-proxy-natural-20260428a`, `git diff --check`, and safety scans over `src`, `Cargo.toml`, and `config`.
- Polymarket RTDS Chainlink evidence file: `verification/2026-04-28-m9-polymarket-rtds-chainlink-reference.md`.
- Polymarket RTDS Chainlink config: `config/polymarket-rtds-chainlink.example.toml`; default config remains `reference_feed.provider = "none"` and does not enable RTDS.
- Polymarket RTDS Chainlink bounded run: `m9-rtds-chainlink-smoke-20260428b` ran `--feed-message-limit 5 --cycles 1`, captured 36 raw messages, 40 normalized-event rows, 12 RTDS Chainlink `reference_tick`s across BTC/ETH/SOL, 0 paper orders, 0 fills, 0.000000 total P&L, and replayed deterministically. Original smoke replay fingerprint was `sha256:2523c96dfd1f80901e2c402a6b454f66201c6c8232f3377f09e15b334b0ed575`; current replay after runtime ordering compatibility fix is `sha256:8a4dce14a349b92dcf10dfb7dbce1f079f667b2fe91689fb6e93d0fa91f3e0df`.
- RTDS report labels: `evidence_type=polymarket_rtds_chainlink_live_ingestion`, `live_market_evidence=true`, `reference_feed_mode=polymarket_rtds_chainlink`, `reference_provider=polymarket_rtds_chainlink`, `matches_market_resolution_source=true`, `settlement_reference_evidence=true`, `live_readiness_evidence=false`.
- Natural RTDS trade interpretation: natural RTDS-backed paper trades are NOT EXERCISED for `m9-rtds-chainlink-smoke-20260428b`; signal skips were `missing_reference_price=12` and `stale_book=12`, with no risk approvals/rejections because no paper intent reached the risk gate.
- Latest M9 RTDS verification passed: `cargo fmt --check`, `cargo test --offline` (120 tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only`, `cargo run --offline -- --config config/pyth-proxy.example.toml validate --local-only`, replay of `m9-rtds-chainlink-smoke-20260428b`, `git diff --check`, and safety scan over `src`, `Cargo.toml`, and `config`.
- Natural RTDS paper validation file: `verification/2026-04-28-m9-rtds-natural-paper-validation.md`.
- Natural RTDS paper continuation file: `verification/2026-04-28-m9-rtds-natural-paper-continuation.md`.
- Fresh corrected-code RTDS paper session file: `verification/2026-04-29-m9-rtds-natural-paper-fresh-session.md`.
- CLOB endpoint post-cutover check: official docs now identify `https://clob.polymarket.com` as the production CLOB endpoint; live `/ok` returned `HTTP/2 200` from `https://clob.polymarket.com/ok`, while `https://clob-v2.polymarket.com/ok` returned `HTTP/2 301` to `https://clob.polymarket.com/ok`. `config/polymarket-rtds-chainlink.example.toml` now uses `https://clob.polymarket.com`.
- Runtime ordering fixes for natural RTDS paper validation: after each asset's RTDS reference batch, paper capture refreshes that asset's read-only CLOB `/book` snapshots; replay maps condition-ID CLOB book updates back to discovered Gamma market IDs before evaluating. These changes keep signal/risk thresholds unchanged and do not force orders.
- Runtime capture continuation fixes: bounded WebSocket capture now exits after sustained heartbeat-only traffic, quiet CLOB WebSocket probes are non-fatal when read-only CLOB REST book snapshots were already recorded, and stale RTDS Chainlink updates are skipped inside a capture batch while still requiring fresh BTC/ETH/SOL ticks before cycle completion. Tests added for all three behaviors.
- Longer completed natural RTDS sessions:
  - `m9-rtds-natural-20260428d`: `--feed-message-limit 5 --cycles 4`, 168 raw messages, 205 normalized rows, 48 RTDS ticks, 0 orders, 0 fills, 0.0 P&L, replay fingerprint `sha256:20bba0230ba09694c567f1503c5e044b4ef9a361be563d403e4b20fd8b25b228`.
  - `m9-rtds-natural-20260428e`: `--feed-message-limit 5 --cycles 3`, 126 raw messages, 156 normalized rows, 36 RTDS ticks, 0 orders, 0 fills, 0.0 P&L, replay fingerprint `sha256:746d6a18a0d6607d3738fd9a38e8efc919d0d1ab588635ddb03fe52ecf5c0dd4`.
- Continuation completed natural RTDS sessions:
  - `m9-rtds-natural-20260428T160726Z-d`: `--feed-message-limit 8 --cycles 30`, 1635 raw messages, 1467 normalized rows, 363 RTDS ticks, 628 CLOB book-related events, 450 predictive ticks, 1444 signal evaluations, 0 signal intents, 0 risk approvals/rejections, 0 orders, 0 fills, 0.0 P&L, replay fingerprint `sha256:1c7f7c8b0e81e8dfbe0783272603a8a0e4478ae0ec92cf3e4762bc657b1d4906`.
  - `m9-rtds-natural-20260428T163455Z-e`: `--feed-message-limit 10 --cycles 30`, 1939 raw messages, 1612 normalized rows, 366 RTDS ticks, 656 CLOB book-related events, 570 predictive ticks, and 1595 signal evaluations. Pre-fix recorded paper artifacts had 8 signal intents, 8 stale-book risk rejections, 0 orders, and 0 fills.
- Continuation partial attempts:
  - `m9-rtds-natural-20260428T155349Z-a`: `--feed-message-limit 20 --cycles 30`, failed before cycle completion on stale RTDS BTC update (`age_ms=19270`, `max_staleness_ms=5000`), replay fingerprint `sha256:71d744bb0d5263874b5043b47938076de0b7784dd6c681e1dc21b80b3008f45c`.
  - `m9-rtds-natural-20260428T155849Z-b`: `--feed-message-limit 20 --cycles 30`, interrupted before cycle completion due heartbeat-only CLOB capture behavior, replay fingerprint `sha256:b559e3030a82a61db331a7b7908f659c1ef86609d9e0971fcbe976bb7781522a`.
  - `m9-rtds-natural-20260428T160309Z-c`: `--feed-message-limit 8 --cycles 30`, interrupted before cycle completion due the same pre-fix capture behavior, replay fingerprint `sha256:0c3ef217324b3c8e2141b2365bd66d4ca77ef41596055edcf03b79fbb1959f2a`.
- Root cause of no natural paper orders was a condition-ID/Gamma-market-ID mismatch in the paper path. Gamma discovery creates market IDs such as `2107306`, while CLOB book snapshots are keyed by condition IDs. Signal evaluation could use condition-ID books, but risk and paper execution compared those books directly against Gamma-market intents.
- Root-cause fix: risk now accepts condition-ID book freshness for the current Gamma market, replay normalizes matching condition-ID `TokenBookSnapshot`s to the Gamma market ID before paper execution, and open maker fill simulation maps condition-ID book events back to open Gamma-market paper orders.
- Post-fix replay of stored run `m9-rtds-natural-20260428T163455Z-e` generates 8 signal intents, 8 risk approvals, and 8 open maker paper orders. It intentionally diverges from the pre-fix recorded paper events (`generated_count=8`, `recorded_count=0`), so a new paper session is required for a clean generated-vs-recorded match under the corrected code.
- Natural RTDS-backed paper order opening is FIXED in replay for stored inputs. Natural RTDS-backed paper fills remain NOT EXERCISED because the generated orders are passive maker orders and the pre-fix stored session has no later fill evidence.
- Fresh corrected-code run `m9-rtds-natural-20260429T021025Z-fresh` completed `--feed-message-limit 10 --cycles 30`, captured 1947 raw messages, 1655 normalized rows, 365 RTDS ticks, 700 CLOB book-related events, and 570 predictive ticks. It produced 1638 signal evaluations, 0 signal intents, 0 risk approvals/rejections, 0 orders, 0 fills, and 0.0 P&L. Paper and replay reports were byte-identical, with replay/recorded paper events `0 / 0` and fingerprint `sha256:347e12676a7c0e36c01b2d5493c468366edd8a27f147354e324223f7f5ee25a3`.
- Root cause of the fresh zero-order run was future-window market selection. Gamma discovery returned markets that were `active` and accepting orders but whose slug intervals had not started (`btc/eth/sol-updown-15m-1777514400`, 2026-04-30 02:00-02:15 UTC), while the run executed on 2026-04-29 02:10-02:40 UTC. The signal engine also classified pre-start markets as `opening` because of saturating elapsed-time math.
- Current-window root-cause fix: Gamma discovery now bounds by near-term `endDate` and orders ascending by `endDate`; paper market selection requires `market.start_ts <= now_wall_ts < market.end_ts`; signal evaluation hard-skips pre-start markets as `market_not_started`; paper-event determinism canonicalizes JSON float round trips before comparing generated vs recorded paper events.
- Clean current-window paper run `m9-rtds-current-window-rootcause-20260429T0312Z` completed `--feed-message-limit 3 --cycles 1`, selected BTC/ETH/SOL markets for 2026-04-29 03:00-03:15 UTC, produced 40 signal evaluations, 1 signal intent, 1 risk approval, 1 filled ETH taker paper order, filled notional 0.200000, fees 0.014112, total P&L -0.064112, and byte-identical paper/replay reports with fingerprint `sha256:4b77dc55d8aef8ac96b704186308f21ff531c220a47437614cf6441bb450bffd`.
- Startup-log confirmation added to paper startup: each selected market now prints asset, market ID, slug, start/end UTC, and selection `now` UTC before the first paper cycle. Fresh run `m9-rtds-current-window-startuplog-20260429T035356Z` selected exactly one BTC, ETH, and SOL current-window market (`btc/eth/sol-updown-15m-1777434300`) with start `2026-04-29T03:45:00Z`, end `2026-04-29T04:00:00Z`, and selection now `2026-04-29T03:54:19.245Z`.
- Startup-log-confirmed current-window paper run `m9-rtds-current-window-startuplog-20260429T035356Z` completed `--feed-message-limit 3 --cycles 1`, captured 30 raw messages, 52 normalized rows, 6 RTDS ticks, 5 predictive ticks, 40 signal evaluations, 6 signal intents, 6 risk approvals, 6 orders, 6 fills, filled notional 3.468000, fees 0.226100, total P&L -0.472100, and byte-identical paper/replay reports with fingerprint `sha256:2f07ba8506838e846f0f6b3ab29629c70346ee002da24a22c29ae895956bacf3`.
- Settlement reconciliation for `m9-rtds-current-window-startuplog-20260429T035356Z`: read-only Gamma checks after the 2026-04-29 03:45-04:00 UTC window showed BTC/ETH/SOL markets closed with outcome prices `["0","1"]`, so Down won for all three markets. The paper run held Up tokens, producing settlement value `0.000000` and final post-settlement P&L `-3.694100`. Artifact: `reports/sessions/m9-rtds-current-window-startuplog-20260429T035356Z/settlement_reconciliation.json`; note: `verification/2026-04-29-m9-rtds-settlement-reconciliation.md`.
- Latest startup-log verification passed: `cargo fmt --check`, `cargo test --offline` (129 lib tests, 5 main tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only`, clean current-window startup-log paper run and replay, `git diff --check`, and focused safety scan.
- Latest root-cause verification passed: `cargo fmt --check`, `cargo test --offline` (129 lib tests, 4 main tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only`, clean current-window paper run and replay, `git diff --check`, and targeted post-fix replay of `m9-rtds-current-window-rootcause-20260429T0308Z` showing 5 generated/recorded paper events now compare deterministically after JSON-float canonicalization.
- Fresh corrected-code session verification passed: `cargo fmt --check`, `cargo test --offline` (126 lib tests, 2 main tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only`, replay of `m9-rtds-natural-20260429T021025Z-fresh`, `git diff --check`, and focused safety scan.
- Latest natural RTDS verification passed: `cargo test --offline` (122 tests), `cargo clippy --offline -- -D warnings`, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only`, `git diff --check`, replay-all-stored-sessions twice, and focused safety scans.

## LB2 Verification Status

PASS.
- Evidence file: `verification/2026-04-29-live-beta-lb2-auth-secret-handling.md`.
- Approved backend is environment-variable handles managed outside the repo. Config stores only non-secret handle names: `P15M_LIVE_BETA_CLOB_L2_ACCESS`, `P15M_LIVE_BETA_CLOB_L2_CREDENTIAL`, and `P15M_LIVE_BETA_CLOB_L2_PASSPHRASE`.
- LB2 local validation prints `live_beta_secret_backend=env`, `live_beta_secret_handle_count=3`, and `live_beta_secret_values_loaded=false`.
- Missing-handle validation with all three handles unset fails closed and prints only handle names plus `present=false`.
- Secretless deterministic paper/replay smoke run `lb2-secretless-fixture-20260429a` passed with all three handles unset; replay fingerprint `sha256:317adb0ffa1fd61270e7e4b4eb22ed18c7718903360d34337af3fb478f1fe918`.
- LB2 checks passed: `cargo test --offline secret`, `cargo test --offline redaction`, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo fmt --check`, `cargo test --offline` (142 lib tests, 5 main tests), `cargo clippy --offline -- -D warnings`, and required safety/no-secret scans.
- LB2 safety result: no secret values, live order placement, signing, wallet key material, API-key values, authenticated CLOB client, order post, cancel, readback, or live-trading path was added.

## LB3 Verification Status

PASS for dry-run payload construction.
- Evidence file: `verification/2026-04-30-live-beta-lb3-signing-dry-run.md`.
- Design note: `docs/live-beta-lb3-signing-dry-run.md`.
- SDK decision: do not import `polymarket_client_sdk_v2` or `rs-clob-client-v2` in LB3; keep those official Rust paths as audit targets before any real signing/authenticated-client work. LB3 uses only a minimal custom V2 payload draft builder with no HTTP client.
- Dry-run command: `cargo run --offline -- --config config/default.toml validate --local-only --live-beta-signing-dry-run`.
- Dry-run fingerprint: `sha256:649e44a4913f5e58ad60147932c253eab0cf35e93f12c44631d2ec9ec2744d3c`.
- Dry-run output confirmed `live_order_placement_enabled=false`, `live_beta_gate_status=blocked`, `not_submitted=true`, and `network_post_enabled=false`.
- The artifact is sanitized and includes the required funder/proxy fixture field: owner redacted, signature redacted, no cryptographic signature produced, no credential values loaded, and no network post path.
- Non-secret config cleanup: `config/default.toml`, `config/example.local.toml`, and `config/pyth-proxy.example.toml` now use `https://clob.polymarket.com`, matching the already-recorded post-cutover endpoint evidence.
- LB3 checks passed: `cargo test --offline safety`, `cargo test --offline compliance`, `cargo test --offline secret`, `cargo test --offline redaction`, `cargo test --offline signing`, `cargo test --offline dry_run`, `cargo run --offline -- --config config/default.toml validate --local-only`, LB3 dry-run validate, `cargo fmt --check`, `cargo test --offline` (147 lib tests, 5 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, required safety/no-secret scans, and `.env` guard.
- LB3 safety result: no live order placement, network order submission, live cancel, authenticated readback, authenticated CLOB client, production signing, wallet key material, API-key value, secret value, geoblock bypass, live-trading path, or strategy/risk/freshness weakening was added.
- Historical hold: LB3 stopped before LB4 until explicit human/operator approval was recorded; that hold was released for LB4 on 2026-04-30.

## LB4 Verification Status

PASS for approved-host authenticated readback/account preflight.
- Evidence files: `verification/2026-04-30-live-beta-lb4-readback-account-preflight.md` and `verification/2026-04-30-live-beta-lb4-approved-host-readback.md`.
- Operator approval to release the LB3 hold and start LB4 was recorded on 2026-04-30 for branch `live-beta/lb4-readback-account-preflight`.
- Operator approval for LB4 approved-host authenticated readback/account preflight from the current Mexico host/session was recorded on 2026-04-30. The same approval explicitly did not authorize order posting, canceling, cancel-all, live trading, LB5, or LB6.
- LB4 added local readback/account-preflight parsing and fail-closed evaluation for runtime-derived LB4 prerequisites, pUSD balance/allowance, reserved balance from open orders using fixed-unit open-order sizes, open-order status, trade lifecycle status and transaction hash presence, venue state, heartbeat readiness, CLOB host, chain ID, nonzero case-insensitive wallet/funder consistency, EOA wallet/funder equality, and redacted endpoint-error classification.
- LB4 approved-host continuation adds a read-only authenticated CLOB preflight path for `GET /balance-allowance`, `GET /data/orders`, and `GET /trades`, with L2 HMAC headers loaded only from approved env handles and never printed.
- PR #20 P1 review fix: the authenticated LB4 path now also queries read-only `GET /sampling-markets`, paginates it through `next_cursor`, and derives venue state from live CLOB market fields instead of hardcoding `trading_enabled`. Missing, malformed, empty, non-accepting, closed, archived, or disabled sampling-market state fails closed through `venue_state_not_open`.
- PR #20 P1 review fix: authenticated `GET /data/orders` and `GET /trades` now paginate through `next_cursor` until the terminal cursor/empty cursor before computing open-order and trade lifecycle gates. Non-advancing cursors or more than 50 readback pages fail closed.
- PR #20 P1 review fix: authenticated `GET /trades` now sends the configured funder/proxy address as required `maker_address`, keeping trade readback scoped to the account under preflight.
- PR #20 P2 review fix: LB4 account preflight normalizes the configured CLOB REST host by trimming whitespace and trailing slashes before the canonical `https://clob.polymarket.com` gate check, avoiding false `clob_host_mismatch` blocks for equivalent local config formatting.
- `GET /balance-allowance` parsing accepts both documented singular `allowance` responses and live plural `allowances` maps. Plural maps are aggregated by the lowest returned allowance so the gate remains fail-closed if any returned spender allowance is below the configured threshold. Very large allowance values saturate to `u64::MAX` for threshold comparison only.
- Polymarket error docs currently say `GET /balance-allowance` `signature_type` must be `EOA`, `POLY_PROXY`, or `GNOSIS_SAFE`, but the official `py_clob_client_v2` sends numeric values `0`, `1`, or `2`. The approved-host SDK check with `signature_type=1` returned `balance=45091977` and max allowances for the configured signer/funder, so LB4 readback now matches the official v2 client numeric query shape and tests that mapping.
- Local validate flag: `cargo run --offline -- --config config/default.toml validate --local-only --live-readback-preflight`.
- Local fail-closed output confirmed `live_beta_readback_preflight_lb3_hold_released=true`, `live_beta_readback_preflight_legal_access_approved=false`, `live_beta_readback_preflight_deployment_geoblock_passed=false`, `live_beta_readback_preflight_status=blocked`, `live_beta_readback_preflight_live_network_enabled=false`, and block reasons `deployment_geoblock_not_recorded,legal_access_not_recorded`.
- Approved-host geoblock check from this Mexico session returned `geoblock_blocked=false`, `geoblock_country=MX`, `geoblock_region=CHP`, and `live_beta_geoblock_gate=passed`.
- Initial approved-host command `cargo run --offline -- --config config/local.toml validate --live-readback-preflight` recorded legal/access and deployment geoblock prerequisites as true, then failed closed before authenticated endpoint calls because the three approved env handles were not present in that shell.
- Follow-up operator-shell run recorded `live_beta_secret_presence_status=ok`, all three approved handles as `present=true`, `geoblock_country=MX`, `geoblock_region=CMX`, and `live_beta_geoblock_gate=passed`, then failed closed while parsing the authenticated `GET /balance-allowance` success body because the live response used plural `allowances` instead of singular `allowance`.
- Approved-host rerun after the plural `allowances` parser correction completed authenticated read-only CLOB network readback with `live_beta_readback_preflight_live_network_enabled=true`, `open_order_count=0`, `trade_count=0`, `reserved_pusd_units=0`, `available_pusd_units=0`, `venue_state=trading_enabled`, and `heartbeat=not_started_no_open_orders`, then failed closed with block reasons `allowance_below_required,balance_below_required`.
- The apparent LB4 account-state blockers were traced to a request-shape mismatch: Rust used named `signature_type=POLY_PROXY`, while the official v2 client uses numeric `signature_type=1`. Operator-side official SDK readback for the same signer/funder returned funded account state: `balance=45091977` with effectively unlimited allowances. No order posting, canceling, cancel-all, live trading, or secret value handling was introduced.
- Approved-host rerun after aligning Rust with the official v2 client numeric `signature_type` query shape passed naturally with `live_beta_readback_preflight_status=passed`, `live_beta_readback_preflight_live_network_enabled=true`, empty `live_beta_readback_preflight_block_reasons`, `open_order_count=0`, `trade_count=3`, `reserved_pusd_units=0`, `available_pusd_units=45091977`, `venue_state=trading_enabled`, `heartbeat=not_started_no_open_orders`, and runtime shutdown `command_status=ok`.
- LB4 local checks passed: `cargo test --offline readback`, `cargo test --offline balance`, `cargo test --offline allowance`, `cargo test --offline heartbeat`, `cargo run --offline -- --config config/default.toml validate --local-only`, expected fail-closed LB4 preflight validate, `cargo fmt --check`, `cargo test --offline` (164 lib tests, 6 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, required safety/no-secret scans, and `.env` guard.
- LB4 approved-host continuation checks passed: `cargo fmt --check`, `cargo test --offline readback`, `cargo test --offline balance`, `cargo test --offline allowance`, `cargo test --offline heartbeat`, `cargo test --offline secret`, `cargo test --offline redaction`, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo test --offline` (169 lib tests, 6 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, trailing-whitespace scan, required safety/no-secret scans, and `.env` guard. The plural `allowances` parser correction passed `cargo fmt --check`, `cargo test --offline readback`, `cargo test --offline balance`, and `cargo test --offline allowance`. The PR #20 review fixes passed `cargo test --offline readback` with 28 lib tests and 1 main test covering venue-state readback derivation, paginated readback blockers, required trade `maker_address`, and paginated sampling-market venue classification; `cargo test --offline lb4_account_preflight_normalizes_clob_host_before_gate_evaluation`; then `cargo test --offline` with 175 lib tests and 7 main tests.
- Final LB4 PASS closeout checks passed after the approved-host success evidence: `git diff --check`, `cargo fmt --check`, `cargo test --offline` (175 lib tests, 7 main tests), `cargo clippy --offline -- -D warnings`, required safety/no-secret scans excluding ignored `.env` and `config/local.toml`, and gitignore guards for `.env` and `config/local.toml`.
- LB4 safety result: no live order placement, order post, live cancel, cancel-all, wallet/private-key material, API-key value, secret value, authenticated order write client, geoblock bypass, live-trading path, or strategy/risk/freshness weakening was added.
- Mandatory hold: do not start LB5 or LB6 until the human explicitly authorizes the next phase.

## Blockers And Risks

- M4 API verification sections 3, 5, and 10 are complete for M4 scope.
- M5 API verification sections 7, 8, 11, and 12 are complete for M5 signal/risk scope.
- Final start/end settlement artifact verification is complete for the M9 current-window RTDS paper sample; this does not remove the separate future live-beta release requirements.
- Polymarket geoblock is host/session-specific; prior M2 evidence observed blocked `US/CA`, while the current read-only M5 recheck observed unblocked `MX/CHP`. Trading-capable modes must remain fail-closed on blocked, malformed, or unreachable geoblock checks.
- CLOB V2 post-cutover endpoint was rechecked on 2026-04-28; use `https://clob.polymarket.com` for CLOB REST unless official docs/live read-only checks change again.
- Polymarket RTDS Chainlink reference ingestion is now the first path for settlement-source paper validation. Direct authenticated Chainlink Data Streams remains a fallback only if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.
- More bounded RTDS Chainlink paper sessions across additional market windows are useful before claiming strategy robustness, but M9 paper/replay evidence now covers current-window selection, natural risk-reviewed paper fills, deterministic replay, and post-market settlement reconciliation.

## Next Concrete Action

- LB0 is approved and complete via `verification/2026-04-29-live-beta-lb0-approval-scope-lock.md`.
- LB1 is complete via `verification/2026-04-29-live-beta-lb1-kill-gates.md`.
- LB2 is complete via `verification/2026-04-29-live-beta-lb2-auth-secret-handling.md`.
- LB3 is complete for dry-run payload construction via `verification/2026-04-30-live-beta-lb3-signing-dry-run.md`.
- Current branch is `live-beta/lb4-approved-host-readback`, based on merged PR #19 commit `a4a54d2d1a3876435be73cd7935f80d1d0928549`.
- LB4 approved-host geoblock is PASS from this Mexico session, and legal/access approval for this LB4 evidence attempt is recorded.
- LB4 approved-host authenticated readback/account preflight is PASS for the approved Mexico host/session only.
- Next concrete action is to finish LB4 branch verification/PR closeout. Do not start LB5 or LB6 until the human explicitly authorizes the next phase.
- Mandatory hold: LB5/LB6 remain blocked pending explicit human authorization.
- Continue M9/RTDS paper evidence only as strategy robustness evidence, not as live profitability proof.

## Update Checklist

When updating this file, include:

- Current branch and short commit.
- Clean/dirty worktree status and any unrelated user changes to preserve.
- Last completed milestone and active milestone.
- Next required exit gate.
- Latest verification evidence paths and outcomes.
- Current blockers, risks, and API assumptions.
- One concrete next action.
