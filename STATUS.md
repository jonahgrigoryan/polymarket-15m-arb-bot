# Project Status Handoff

Last updated: 2026-05-13

## Purpose

`STATUS.md` is the current-state handoff file for coding agents. Use it to resume work without re-deriving the active branch, milestone, gate, verification state, blockers, and next action from scratch.

Authoritative sources remain:

- `AGENTS.md`: permanent project rules and safety boundaries.
- `PRD.md`: product requirements and project scope.
- `IMPLEMENTATION_PLAN.md`: milestone roadmap, build tasks, verification, and exit gates.
- `LIVE_TRADING_PRD.md`: final live-trading product scope draft.
- `LIVE_TRADING_IMPLEMENTATION_PLAN.md`: final live-trading implementation-plan draft.
- `API_VERIFICATION.md`: required external API verification checklist.
- `verification/*.md`: dated evidence logs.
- `STATUS.md`: current handoff context.

## Current Branch

- Branch: `live-trading/lt3-auth-signing-dry-run`.
- Current base: fresh `main`/`origin/main` at `f4f90c0` (`Merge pull request #45 from jonahgrigoryan/live-trading/lt2-gates-journal-evidence`).
- Workspace state: LT3 final-live auth, secret-handle validation, and signing dry-run support are locally implemented and ready for review/commit. The branch adds final-live secret handle/account binding config, a redacted `live-trading-signing-dry-run` CLI, LT3 signing artifact schemas, redacted local artifacts under `artifacts/live_trading/LT3-LOCAL-DRY-RUN/`, and verification note `verification/2026-05-13-live-trading-lt3-auth-signing-dry-run.md`. No live order submission, cancel submission, heartbeat POST, cap sentinel write, taker expansion, production sizing, multi-wallet deployment, asset expansion, cancel-all runtime behavior, authenticated write client, raw signature generation, auth header generation, or secret material logging is authorized or implemented.

## Milestones

- Last completed milestone: LT2 Final-Live Gates, Journal, And Evidence Schema is complete and merged to `main` via PR #45 at `f4f90c0`; LT1 Read-Only Final-Live Supervision is complete and merged to `main` via PR #44 at `27e3b7f`; LT0 approval/scope lock is complete and merged to `main` via PR #43 at `823f9ef`; final live-trading PRD and implementation-plan draft are merged to `main` via PR #42 at `f57979d`. LA8 scale decision report is complete and merged to `main` via PR #41 at `89ddf0c` with decision `NO-GO: lifecycle unsafe`. LA7 selective taker gate is complete and merged to `main` via PR #40 at `fd35522`. LA7 remains a tightly scoped taker gate and review milestone, not broad taker enablement. LA6 quote manager and cancel/replace remains complete and merged to `main` via PR #35 at `d5fd1d9`; LA5 maker-only micro autonomy remains complete and merged via PR #34 at `e051054`; LA4 shadow live executor remains complete and merged via PR #33 at `cfec8dd`; LA3 controlled fill canary remains complete and merged via PR #32 at `7b7f952`; LA2 heartbeat, user events, and crash recovery remains complete and merged via PR #31 at `f493c78`; LA1 gates, journal, and reconciliation foundation remains complete and merged via PR #29 at `c6f3c23`; LA0 approval and scope lock remains complete and merged via PR #28. LB7, LB6, LB4, and M9 statuses remain as previously documented.
- Active milestone: LT3 Auth, Secret Handles, And Signing Dry-Run is locally complete and ready for commit. `LIVE_TRADING_PRD.md` defines the product boundary as a gated path toward the original 1,000+ matched fills/day objective. `LIVE_TRADING_IMPLEMENTATION_PLAN.md` decomposes the LT0-LT14 sequence, including throughput readiness, staged volume ramps, and a 1,000+ fills/day candidate. LT3 adds final-live auth/secret handle validation and sanitized signing dry-run artifacts only; it does not authorize order placement, cancel placement, raw signing output, auth header output, or a submit-ready live order.
- M9 - Multi-Session Validation And Live-Readiness Review is PASS for paper/replay validation evidence only. M9 still does not authorize live trading, and the settled sample was negative after final reconciliation.
- Next exit gate: review, commit, push, and open an LT3 PR to `main`; after merge, refresh `main` and wait for explicit LT4 approval. Do not start LT4 maker shadow work, run live order/cancel behavior, generate an order intended for immediate submission, log private keys/API secrets/passphrases/raw signatures/auth headers, increase size, order rate, asset coverage, taker usage, runtime duration, multi-wallet deployment, or production rollout without explicit next approval.
- 2026-05-08 follow-up: fresh dry-run-only approval `LA7-2026-05-08-taker-dry-run-002` was created for SOL Up BUY on `sol-updown-15m-1778293800`, and dry-run `18adc4bacd70fc48-16e2d-0` passed with `not_submitted=true`, no block reasons, baseline/reconciliation passed, zero positions, zero open orders, `best_ask=0.47`, `notional=2.35`, and `taker_fee=0.087185`. A separate live approval artifact was then created at `verification/2026-05-08-live-alpha-la7-live-approval-sol-1778293800.md` with exact dry-run report/decision hashes and `approval_expires_at_unix=1778294040` (2026-05-09T02:34:00Z). No live taker command was executed. After that expiry, LA7 live taker returns to `NO-GO` until another fresh dry-run and live approval artifact are created.
- 2026-05-08 live taker canary: the expired `sol-updown-15m-1778293800` approval was not reused. Fresh dry-run-only approval `LA7-2026-05-08-taker-dry-run-003` was created for SOL Up BUY on `sol-updown-15m-1778307300`; dry-run `18add113b1c090e8-2ab1-0` passed with `not_submitted=true`, no block reasons, baseline/reconciliation passed, zero positions, zero open orders, `best_ask=0.41`, `notional=2.05`, and `taker_fee=0.084665`. Separate live approval `LA7-2026-05-08-taker-live-002` was created at `verification/2026-05-08-live-alpha-la7-live-approval-sol-1778307300.md` with exact dry-run hashes and `approval_expires_at_unix=1778307540` (2026-05-09T06:19:00Z). The live command submitted exactly one BUY GTC taker order. Venue response: `success=true`, `venue_status=MATCHED`, order id `0x8a768554d4a993f0d521b0def432c98525570470538b679946351370de0dcab9`, transaction hash `0x98480c5e82196ed5c6445fabb7063b96adc88855f3855479f653ada0287952c4`, making amount `1.35`, taking amount `5`, no batch/FOK/FAK/cancel-all/retry. The command failed closed after submit with `submitted_post_check_blocked` because immediate post-submit readback/reconciliation did not pass: `post_submit_readback_not_passed`, `post_submit_reconciliation_not_passed`, mismatches `unexpected_fill`, `nonterminal_venue_trade_status`, `baseline:current_readback_not_passed`. The one-order cap is consumed at `reports/live-alpha-la7-taker-canary-cap.json`. A post-submit read-only account baseline `LA7-2026-05-08-post-taker-live-001` passed and shows `trade_count=24`, `open_order_count=0`, `reserved_pusd_units=0`, `available_pusd_units=4904902`, `position_count=1` for 5 SOL Up shares on `sol-updown-15m-1778307300`, and `la7_live_gate_status=blocked` due to `baseline_positions_nonzero`. No second live attempt is authorized.
- 2026-05-08 post-resolution readback: after the canary market resolved, read-only baseline `LA7-2026-05-08-post-taker-resolved-001` passed with run id `18add20de02fdd58-42ec-0`, baseline hash `sha256:a4648ef83da8b61f46732feba109244b1fa10d6e5ae8ad9fa4446734e221c6f0`, `trade_count=26`, `open_order_count=0`, `reserved_pusd_units=0`, `available_pusd_units=3122653`, `position_evidence_complete=true`, and `position_count=0`. The trade artifact contains confirmed entries for the canary market, including the live order id `0x8a768554d4a993f0d521b0def432c98525570470538b679946351370de0dcab9` and transaction hash `0x98480c5e82196ed5c6445fabb7063b96adc88855f3855479f653ada0287952c4`. LA7 live gate status is back to `passed` from a flat-position/readback perspective, but the consumed one-order cap remains binding and the live run still records the immediate post-submit `submitted_post_check_blocked` result. Do not run another live taker canary from this branch state.
- 2026-05-09 post-submit reconciliation follow-up: `src/main.rs` now builds explicit LA7 post-submit local state from the submitted taker order and matching readback trades, including the observed SDK shape where submit returned `trade_ids=[]` but authenticated readback later returned the trade by `order_id`. Confirmed matched trades now reconcile as expected instead of `unexpected_fill`; expected nonterminal trade lifecycle status is recorded as `matched_pending_confirmation` and still blocks the live command from clean `submitted_reconciled` until later confirmation evidence exists. Focused tests cover matched-with-empty-submit-trade-ids, readback trade found by order ID, nonterminal pending status, missing matched trade fail-closed behavior, and flat resolved baseline not resetting the consumed one-order cap.
- LA8 decision summary: `NO-GO: lifecycle unsafe` for scaling. Reasons: paper/post-settlement P&L sample is negative (`-3.694100`), there is no live maker fill/P&L sample, LA7 immediate post-submit readback/reconciliation historically failed closed, paper/live P&L is not comparable from matched market-window evidence, and the LA7 one-order taker cap remains consumed. The report policy can now emit a future `GO: propose next PRD for broader scaling` only when evidence is clean and positive; current evidence is not. LA8 may aggregate and report evidence only. It must not treat the accepted LA7 taker canary as approval to run another taker canary, reset the consumed cap, expand taker behavior, or increase live size/rate/assets/duration. Any official live-order transition after LA8 needs a new PRD or implementation plan and a new approval scope.

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
- Historical hold: LB5 did not start until the operator explicitly authorized LB5 only on 2026-05-02 after PR #20 merged.

## LB5 Verification Status

PASS for offline cancel readiness and rollback/runbook minimum only.
- Evidence file: `verification/2026-05-02-live-beta-lb5-cancel-readiness-rollback.md`.
- Runbook: `runbooks/live-beta-lb5-rollback-runbook.md`.
- Operator approval to start LB5 only was recorded on 2026-05-02. The same approval explicitly did not authorize LB6, live order posting, live cancel proof, cancel-all, autonomous live trading, or strategy-to-live routing.
- Official docs rechecked for LB5 scope: CLOB host `https://clob.polymarket.com`; single-order cancel is authenticated `DELETE /order` with body field `orderID`; response includes `canceled` and `not_canceled`; multiple, all, and market-wide cancels are separate endpoints and remain out of scope; heartbeat may auto-cancel open orders if not maintained; rate limits must fail closed.
- LB5 added offline single-order cancel readiness only. `LIVE_CANCEL_NETWORK_ENABLED=false`, `cancel_all_enabled=false`, and `LIVE_ORDER_PLACEMENT_ENABLED=false` remain true in validation evidence.
- Local readiness command: `cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness`.
- Local readiness output confirmed `live_beta_cancel_readiness_status=blocked`, `live_beta_cancel_readiness_live_network_enabled=false`, `live_beta_cancel_readiness_cancel_all_enabled=false`, `live_beta_cancel_readiness_request_constructable=false`, `live_beta_cancel_readiness_single_cancel_method=DELETE`, `live_beta_cancel_readiness_single_cancel_path=/order`, and block reasons `live_order_placement_disabled,lb4_preflight_not_recorded,lb6_hold_not_released,human_canary_approval_missing,human_cancel_approval_missing,approved_canary_order_missing,single_open_order_not_verified,heartbeat_not_ready`.
- Cancel fixture tests cover default blocked readiness before LB6 canary, request construction requiring LB6 gates plus an approved canary order, success, extra canceled order IDs in a one-order cancel response, duplicate canceled order IDs in a one-order cancel response, extra `not_canceled` order IDs in a one-order cancel response, partial-fill ambiguity, already filled, already canceled, missing order, auth error, rate limit, unknown response, no cancel-all path, no network dispatch/secret-loading surface, and runbook minimums.
- Rollback minimum includes kill switch command, service stop command, open-order readback procedure, cancel plan, heartbeat failure handling, incident note template, artifact checklist, and LB6 hold.
- LB5 checks passed: `cargo test --offline cancel` (15 lib tests), `cargo test --offline rollback` (1 lib test), `cargo test --offline runbook` (1 lib test), `cargo test --offline readback` (28 lib tests, 1 main test), `cargo run --offline -- --config config/default.toml validate --local-only`, LB5 cancel-readiness validate, `cargo fmt --check`, `cargo test --offline` (189 lib tests, 7 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, trailing-whitespace scan, required safety/no-secret scans, and ignored-local guards for `.env` and `config/local.toml`.
- LB5 safety result: no live order placement, order post, live cancel proof, cancel-all, wallet/private-key material, API-key value, secret value, authenticated order write client, geoblock bypass, live-trading path, or strategy/risk/freshness weakening was added.
- Mandatory hold: stop after LB5. Do not start LB6 until the human/operator explicitly approves the exact one-order canary plan.

## LB6 Mechanism Status

IMPLEMENTED for mechanism only; final canary submission remains BLOCKED in this run.
- Evidence file: `verification/2026-05-02-live-beta-lb6-one-order-canary-mechanism.md`.
- PR #22 merged to `main` at `cc5d965a98cbc07b63027cdcd31ac9a56e5e1431` on 2026-05-02.
- Scope: one-order canary signing/submission mechanism only. No order was submitted and no live cancel was sent in this run.
- Official docs rechecked for LB6 scope: CLOB authentication requires L1 private-key signing for order payloads plus L2 headers for posting signed orders; official SDK clients are recommended for signing/submission; `POST /order` creates one order; `DELETE /order` cancels one order; post-only is valid only with GTC/GTD and is rejected if marketable; CLOB rate limits still require fail-closed handling.
- Added `src/live_beta_canary.rs` with exact approval text/hash gating, approval-expiry gating, atomic one-order cap reservation, post-only/GTD/maker-only checks, price/size/notional checks, book/reference freshness checks, side-aware non-marketable bid/ask checks, geoblock/LB4/open-order/LB5/secret/SDK checks, and an official `polymarket_client_sdk_v2` final submission path.
- Added `live-canary` CLI path with `--dry-run` and `--human-approved --one-order` modes. Dry-run prints the final approval prompt/hash and never submits. The approval prompt includes run ID, host, geoblock result, wallet/funder, signature type, pUSD available/reserved state, order intent, book/reference age, heartbeat, cancel plan, and rollback command. Final gated mode blocks unless the exact approval text/hash, fresh expiry, zero open orders, LB4 preflight, geoblock PASS, LB5 rollback readiness, L2 handles, canary private-key handle, official SDK path, fresh book/reference ages, and one-order cap all pass.
- Final mode validates non-empty/parseable local signing and L2 env values before reserving the one-order cap sentinel, so bad local credentials cannot consume the only canary attempt before any venue submission call.
- Global `LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged. LB6 uses a narrower compile-time canary gate, `LB6_ONE_ORDER_CANARY_SUBMISSION_ENABLED=true`, inside `src/live_beta_canary.rs`; this path is still unreachable without the exact final gates above.
- Latest approved-host readback recheck after PR #22 merge: PASS from Mexico host/session with `geoblock_country=MX`, `geoblock_region=CHP`, `live_beta_readback_preflight_open_order_count=0`, `live_beta_readback_preflight_reserved_pusd_units=0`, `live_beta_readback_preflight_available_pusd_units=1614478`, `live_beta_readback_preflight_venue_state=trading_enabled`, and `live_beta_readback_preflight_heartbeat=not_started_no_open_orders`.
- Latest LB6 dry-run generated approval hash `sha256:4f620792ae4c8c1579254b2873c9e9aae0e045c5bf5ba298fcf6fdf30b0877ea` for `eth-updown-15m-1777761000`; the operator approved that prompt in chat.
- No order was submitted after approval. The immediate final safety check stopped fail-closed because the runtime still has no approved live single-order cancel network path: `live_beta_cancel_readiness_live_network_enabled=false`, `live_beta_cancel_readiness_cancel_all_enabled=false`, and `live_beta_cancel_readiness_request_constructable=false`. The one-order cap sentinel remains absent.
- PR #23 for the LB6 exact single-order cancel/readback patch merged to `main` at `a1524e1` on 2026-05-02. It added `src/live_beta_order_lifecycle.rs`, exact authenticated `GET /order/{orderID}` readback, exact `DELETE /order` single-order cancel execution, `live-cancel` CLI dry-run/final-gated modes, and a canary readiness check that the exact single-order cancel path exists while cancel-all remains disabled.
- `live-cancel --dry-run` is readback/readiness only. `live-cancel --human-approved --one-order` requires the local one-order cap sentinel to match the exact venue order ID and canary approval hash, a nonexpired approval timestamp, geoblock PASS, L2 handles, exact order readback, and an unmatched/live target order before sending any cancel. It can only send `DELETE /order` for that exact order ID and must read the order back afterward.
- LB5 remains offline readiness only: `src/live_beta_cancel.rs` still reports `live_beta_cancel_readiness_live_network_enabled=false` and still has no network dispatch or secret-loading surface.
- Evidence file for the follow-up patch: `verification/2026-05-02-live-beta-lb6-single-cancel-readback.md`.
- Safety result: no live order submitted, no live cancel sent, no cancel-all path, no autonomous live trading, no strategy-to-live route, no secret values in repo/logs/docs/chat, and no expired market approval reused.

## LB6 Pre-Authorized Canary Envelope Status

IMPLEMENTED; PR #24 merged to `main` at `c8d0bfc` on 2026-05-02. No live order or cancel was submitted in this run.
- Evidence file: `verification/2026-05-02-live-beta-lb6-preauthorized-canary-envelope.md`.
- Branch: `live-beta/lb6-preauthorized-canary-envelope`.
- Scope: adds `live-canary --preauthorized-envelope --one-order` as a reviewed alternative to the exact prompt/hash loop for time-sensitive 15-minute markets.
- The pre-authorized envelope is intentionally narrow: ETH 15-minute market slug only, current market window only, `Outcome=Up`, `Side=BUY`, `Order type=GTD`, post-only maker-only, `Price=0.01`, `Size=5`, `Notional=0.05 pUSD`, `tick_size=0.01`, GTD expiry before the final market minute, side-aware non-marketable best ask, fresh book/reference ages, zero reserved pUSD, and available pUSD above the canary notional.
- The runtime still requires geoblock PASS, LB4 approved-host account preflight PASS, zero open orders, L2 handles present, canary private-key handle present, LB5 rollback readiness, exact LB6 single-order cancel path availability, official SDK availability, an unused local one-order cap sentinel, and fresh discovery binding that proves the supplied condition ID and Up token ID belong to the supplied ETH 15-minute slug.
- Existing `live-canary --human-approved --one-order` exact prompt/hash gating remains available and unchanged for non-envelope approvals.
- Safety result for this patch: no live order submitted, no live cancel sent, no cancel-all path, no autonomous live trading, no strategy-to-live route, no secret values, and no private-key material added to repo/docs/tests.

## LB6 Pre-Authorized Slug Binding Fix Status

IMPLEMENTED for PR review only; no live order or cancel was submitted in this run.
- Evidence file: `verification/2026-05-02-live-beta-lb6-preauthorized-slug-binding.md`.
- Branch: `live-beta/lb6-preauthorized-slug-binding`.
- After PR #24 merge, local `main` was fast-forwarded to `c8d0bfc`.
- Approved-host LB4 readback PASS was reconfirmed with run ID `18abe3d240c43b40-15285-0`: geoblock `MX/CMX`, open orders `0`, reserved pUSD `0`, available pUSD units `1614478`, venue `trading_enabled`, heartbeat `not_started_no_open_orders`.
- A pre-authorized canary command failed closed before any order call with run ID `18abe4137d932ef0-15382-0`: `best_ask_not_above_bid,reference_stale`, `live_beta_canary_not_submitted=true`, and `live_beta_canary_one_order_cap_remaining=true`.
- The next ETH market existed at Gamma's direct slug endpoint, but PR #24's pre-authorized binding used paged keyset discovery and could miss the exact current ETH slug within configured page limits.
- This patch adds an exact Gamma `/markets/slug/<slug>` lookup for the LB6 pre-authorized binding while keeping broad keyset discovery unchanged. A 404 from the slug endpoint is treated as a missing binding, not a fatal discovery error, so stale/typo slugs still fail closed through normal readiness reporting.
- Verification passed: `cargo fmt --check`, `cargo test --offline market_discovery`, `cargo test --offline canary`, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo test --offline` (217 lib tests + 8 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, safety/no-secret scans, ignored-local guards, and one-order cap absent check.
- Safety result for this patch: no live order submitted, no live cancel sent, no cancel-all path, no autonomous live trading, no strategy-to-live route, no secret values, and no private-key material added to repo/docs/tests.

## LB6 One-Order Canary Execution Status

EXECUTED and merged to `main` via PR #26 at `2031332` on 2026-05-03 UTC.
- Evidence file: `verification/2026-05-03-live-beta-lb6-one-order-canary-execution.md`.
- Branch: `live-beta/lb6-canary-live-closeout`.
- PR: #26 `Record LB6 canary execution closeout`.
- Pre-submit LB4 readback PASS run ID `18abe5ab4bdc0780-16cec-0`: geoblock `MX/CMX`, open orders `0`, reserved pUSD `0`, available pUSD units `1614478`, venue `trading_enabled`, heartbeat `not_started_no_open_orders`.
- `live-canary --preauthorized-envelope --one-order` passed all runtime gates with run ID `18abe606c2c70218-17c1f-0` and submitted exactly one order:
  - Market slug `eth-updown-15m-1777767300`.
  - Condition ID `0x6455382f705a0cb742cab86603f6ade14a67442bd0cd7debcef18fb3f8bae8b1`.
  - Up token ID `108754796712694987030496168190461335721943804518169337367002311107585620439355`.
  - Side `BUY`, price `0.01`, size `5`, notional `0.05 pUSD`, GTD expiry `1777768020`.
  - Best bid `0.35`, best ask `0.37`, book age `0 ms`, reference age `987 ms`.
  - Approval hash `sha256:04fe06d40a0e7e1b348878207c75bf1d5cb325c14ba8db7885fdf8e5b716a7ef`.
  - Venue order ID `0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf`, venue status `LIVE`, success `true`, submitted order count `1`.
- Initial Rust `live-cancel --dry-run` did not cancel. It failed at exact readback because the Rust path still used `GET /order/{orderID}`, which returned HTTP `404` for this live order.
- Official `py_clob_client_v2` exact readback confirmed the canary order was `LIVE`, matched size `0`, and matched the approved market/token/side/price/size/order-type envelope.
- Official `py_clob_client_v2` exact single-order cancel was used for only this order ID. Result: canceled `[0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf]`, `not_canceled={}`. No cancel-all was used.
- Post-cancel official readback showed order status `CANCELED`, matched size `0`, and no open orders for the canary token.
- Post-cancel LB4 readback PASS run ID `18abe61f679e9770-17f1b-0`: geoblock `MX/CMX`, open orders `0`, reserved pUSD `0`, available pUSD units `1614478`, trade count `14`, venue `trading_enabled`, heartbeat `not_started_no_open_orders`.
- Closeout patch fixes the Rust exact readback surface:
  - Single-order readback path is now `/data/order/{orderID}`, matching the official client behavior observed live.
  - Single-order parser accepts current live SDK status strings such as `LIVE` and `CANCELED`.
  - Single-order parser treats single-order `original_size` / `size_matched` as human decimal sizes; paginated open-order readback keeps the existing fixed-unit parser.
- Post-fix Rust `live-cancel --dry-run` against the canceled canary order passed transport/parsing and blocked correctly with run ID `18abe64ecbbe5270-262-0`: `human_cancel_approval_missing,order_already_canceled`, order status `canceled`, readback path `/data/order/0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf`.
- Current outcome: LB6 one-order canary is complete, the canary order is canceled, no fill occurred, post-cancel open orders are zero, post-cancel reserved pUSD is zero, and the local one-order cap sentinel is consumed.
- Safety result: no second order was attempted, no cancel-all was used, no autonomous live trading was added, no strategy-to-live route was added, and no secret values were committed or printed.

## LB7 Handoff Status

PASS for runbook, observability, rollback hardening, incident workflow, and STATUS handoff only.
- Branch: `live-beta/lb7-runbook-handoff`.
- Evidence file: `verification/2026-05-03-live-beta-lb7-runbook-handoff.md`.
- PR: #27 merged to `main` at `26144dc` on 2026-05-03.
- Scope: updated the handoff after merged PR #26, folded LB6 closeout lessons into the rollback runbook, reviewed live-beta observability coverage, and preserved the next approval gate.
- Runbook update: exact single-order readback path is `GET /data/order/{orderID}`, exact cancel path remains `DELETE /order`, official `py_clob_client_v2` closeout behavior is recorded, and Rust/SDK readback disagreement now halts live action pending human review.
- Observability update: `docs/m8-observability-runbook.md` now lists live-beta coverage for live mode, geoblock, kill switch, heartbeat age/failures, order attempts/accepts/rejects, cancels, fills, readback mismatches, balance/reserved mismatches, open notional, realized P&L, and settlement P&L.
- Tests added: `live_beta_cancel::tests::rollback_runbook_contains_lb7_closeout_lessons` and `metrics::tests::observability_runbook_covers_live_beta_handoff_signals`.
- LB7 checks passed: `cargo test --offline metrics` (5 lib tests), `cargo test --offline reporting` (5 lib tests), `cargo test --offline rollback` (3 lib tests), `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo fmt --check`, `cargo test --offline` (220 lib tests + 8 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, trailing-whitespace scan, safety/no-secret scans, and ignored-local guards for `.env` and `config/local.toml`.
- Safety result: no new live order, live cancel, cancel-all, secret value, API-key value, seed phrase, wallet/private-key material, geoblock bypass, strategy-to-live route, broader order type, multi-order path, cap increase, or market/asset expansion was added.
- LB7 does not authorize another canary, live cancel, cancel-all, strategy-selected live trading, taker/FOK/FAK/marketable limit paths, multi-order paths, higher caps, broader markets/assets, or profitability claims from the LB6 canary.
- Required handoff facts from LB6: one canary submitted, exact order canceled, no fill, post-cancel open orders `0`, post-cancel reserved pUSD `0`, and local one-order cap consumed.

## Live Alpha LA0 Status

PASS for approval and scope lock only.
- Branch: `live-alpha/la0-approval-scope`.
- Evidence file: `verification/2026-05-03-live-alpha-la0-approval-scope.md`.
- Scope: establish Live Alpha as the post-LB7 release track, add/finalize `LIVE_ALPHA_PRD.md` and `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`, preserve the LB7 handoff facts, and record the next gate.
- LA0 does not authorize live order placement, live canceling, cancel-all, strategy-selected live trading, resetting/bypassing the consumed LB6 one-order cap, enabling `LIVE_ORDER_PLACEMENT_ENABLED=true` globally, or starting LA1/LA2/LA3 work.
- Required sequencing: LA1 and LA2 must pass before any controlled fill canary; LA3 is the first possible controlled fill canary and only after explicit approval; LA5 or later is the first possible maker-only micro autonomy and only after prior evidence gates; strategy-selected live trading remains behind a separate robustness gate.
- LA0 checks passed: `cargo fmt --check`, `cargo test --offline` (220 lib tests + 8 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, and safety/no-secret scans.
- Exact next action after LA0 PR merge: stop and obtain explicit human/operator approval to start LA1 from fresh updated `main`.

## Live Alpha LA1 Status

PASS for gates, journal, and reconciliation foundation only.
- Branch: `live-alpha/la1-gates-journal-reconciliation`.
- Evidence file: `verification/2026-05-03-live-alpha-la1-journal-reconciliation.md`.
- Scope: inert Live Alpha config defaults, Live Alpha gate evaluation, compile-time feature scaffold default-off, execution intent shape with notional consistency validation, append-only live journal with redaction/replay/reducers, live balance tracker, live position book, reconciliation engine, and reconciliation-health metrics. Review-hardening fixes normalize redaction keys before sensitive-field matching, reject malformed typed journal payloads during replay reduction, require live-order-capable modes to have their matching submode enabled, scope journal state replay by `run_id`, keep rejected submissions and failed trades out of fill evidence, verify exact trade ID to order ID consistency, require local trades to be present in venue trade readback, include conditional-token balance drift, reject prices above `1.0`, and block non-order-capable Live Alpha modes through `can_place_live_orders()`.
- Default validation result: `live_order_placement_enabled=false`, `live_alpha_enabled=false`, `live_alpha_mode=disabled`, `live_alpha_compile_time_orders_enabled=false`, `live_alpha_gate_status=blocked`.
- Gate block reasons by default: `live_order_placement_disabled,compile_time_live_disabled,live_alpha_disabled,mode_disabled,missing_config_intent,missing_cli_intent,kill_switch_active,geoblock_unknown,account_preflight_unknown,heartbeat_unknown,reconciliation_unknown,approval_missing,phase_not_approved`.
- Reconciliation mismatch fixtures halt fail-closed for unknown open order, missing venue order, unknown venue order status, unexpected fill, filled venue order without a matching local trade order, unexpected partial fill, cancel not confirmed, reserved balance mismatch, balance delta mismatch including conditional-token drift, position mismatch, missing venue trade, unknown venue trade status, trade status failed, trade order mismatch, and same-source SDK/Rust disagreement.
- LA1 checks passed: focused LA1 filters, `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo fmt --check`, `cargo test --offline` (267 lib tests + 8 main tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, and safety/no-secret scans.
- LA1 does not authorize live order placement, live canceling, cancel-all, controlled fill canaries, maker autonomy, strategy-selected live trading, resetting/bypassing the consumed LB6 one-order cap, enabling `LIVE_ORDER_PLACEMENT_ENABLED=true` globally, or starting LA2/LA3 work.
- LA1 is merged to `main` via PR #29 at `c6f3c23`.
- Exact next action after LA1 PR merge was completed: start LA2 from fresh updated `main` only after explicit human/operator approval.

## Live Alpha LA2 Status

PASS for heartbeat, user events, and crash recovery only.
- Branch: `live-alpha/la2-heartbeat-crash-safety`.
- Evidence file: `verification/2026-05-04-live-alpha-la2-heartbeat-crash-safety.md`.
- Runbooks: `runbooks/live-alpha-runbook.md`, `runbooks/live-alpha-reconciliation-runbook.md`, and `runbooks/live-alpha-incident-response.md`.
- Scope: heartbeat state/evaluation with official `postHeartbeat` timing modeled but network heartbeat POST disabled, parser-only user-channel event handling, startup recovery evaluation for geoblock/account/balance/open-order/recent-trade/journal/position/reconciliation checks, durable halt/recovery events, and readback-to-reconciliation conversion.
- Official Polymarket docs rechecked for user WebSocket events, heartbeat behavior, L2 auth, geoblock, order/cancel/readback, trade lifecycle, fees, and rate limits before coding.
- Focused checks passed: `cargo test --offline live_heartbeat`, `cargo test --offline live_user_events`, `cargo test --offline live_reconciliation`, `cargo test --offline startup_recovery`, `cargo test --offline risk_halt`, `cargo test --offline live_alpha_gate`, `cargo test --offline live_beta_readback`, `cargo test --offline readback_missing_trader_side_does_not_use_taker_order_for_account_maker`, and `cargo test --offline startup_recovery_validate_path_persists_journal_events`. PR review fixes derive readback trade `order_id` from official `trader_side` when present, do not fall back to a counterparty `taker_order_id` when missing `maker_orders` still prove the configured account is maker-side, scope startup reconciliation local order evidence to the open-order readback snapshot, and persist emitted startup recovery journal events durably.
- Final checks passed: `cargo run --offline -- --config config/default.toml validate --local-only` (run ID `18ac576299093bd0-f6a4-0`), `cargo fmt --check`, `cargo test --offline` (294 lib tests, 18 main tests, 0 doc tests), `cargo clippy --offline -- -D warnings`, `git diff --check`, `git status --short --branch`, and required scope/no-secret scans.
- Optional approved-host live read-only/heartbeat check was not run for LA2.
- LA2 authorizes no live order placement, no live cancels, no cancel-all, no controlled fill canary, no maker autonomy, no strategy-selected live trading, no LA3 work, no global `LIVE_ORDER_PLACEMENT_ENABLED=true`, no default `live-alpha-orders` feature enablement, and no reset/bypass of the consumed LB6 one-order cap.
- Exact next action after LA2 PR merge: stop and obtain explicit human/operator approval before starting LA3 from fresh updated `main`.

## Live Alpha LA3 Status

LIVE FILL CANARY EXECUTED AND SETTLEMENT-FOLLOWED; STOP AFTER LA3.
- Branch: `live-alpha/la3-controlled-fill-canary`.
- Evidence files: `verification/2026-05-04-live-alpha-la3-controlled-fill-canary.md` and `verification/2026-05-04-live-alpha-la3-approval.md`.
- Scope completed so far: branch creation from fresh `main`, LA3 planning-source re-read, repository approval-artifact search, current browser/account/geoblock readback, current public market/book lookup, refreshed local approval artifact, approval-artifact parser, fail-closed LA3 preflight, Polymarket RTDS Chainlink reference freshness, dry-run/final CLI gates, one-attempt official-SDK FAK submit path, LA3 journal events, local cap sentinel handling, and immediate post-submit reconciliation scaffolding.
- Approval artifact: `LA3-2026-05-04-004` approved exactly one BTC 15m `BUY` `FAK` fill canary for market `btc-updown-15m-1777925700`, outcome `Down`, `2.56 pUSD` max notional, `0.06 pUSD` max fee, worst price `0.51`, no retry, no second attempt, no cancel-all, no FOK, and no later-phase work.
- Funded/L2 retry evidence: read-only preflight run `18ac72d16a872788-a4b3-0` proved geoblock PASS for `BR/SP`, authenticated account preflight PASS, live network enabled, available pUSD units `12185950`, allowance sufficient, zero open orders, and 14 recent trades. It still blocked before submit with `missing_cli_intent,best_ask_exceeds_worst_price`.
- Dry-run retry evidence: runs `18ac72e5a0e2e970-a59d-0`, `18ac72f1447f0478-a64d-0`, and `18ac72f8f3b1e080-a6bd-0` completed with `live_alpha_fill_canary_not_submitted=true`; each blocked on fast-moving book bounds (`best_ask_exceeds_worst_price` or `slippage_exceeds_approval`) before the configured no-trade cutoff.
- Final dry-run `18ac742809bd7b70-af72-0` passed with no blockers, fresh book/reference, account preflight PASS, available pUSD units `10070772`, allowance sufficient, zero open orders, and compile-time `live-alpha-orders=true`.
- Final submit `18ac742fd4a83e40-b000-0` passed preflight and submitted exactly one order: venue order ID `0xd16026c677ff8b5d0f8cc89a1c75bebc61fd047d71232d0b323a2c50acd5b6a0`, venue status `MATCHED`, success `true`, making amount `2.56`, taking amount `5.12`, transaction hash `0x94fd00369403b3c6835c31956df2788d5e6d1a0c5e4b4c6647b0abf820be4077`, reconciliation status `filled_and_reconciled`, matching trade ID `495feb52-5706-4660-9f52-fa0449fda520`, and zero open orders after run.
- Post-run read-only preflight `18ac7437fce59170-b065-0` confirmed the attempt cap is consumed, available pUSD units `7418612`, reserved pUSD units `0`, open order count `0`, recent trade count `17`, account preflight PASS, and heartbeat `not_started_no_open_orders`.
- Settlement follow-up confirmed Gamma `outcomePrices=["0","1"]`, so `Down` won. Public activity showed the original `BUY` trade at 2026-05-04T19:42:04Z for `5.12` Down shares, price `0.5`, `usdcSize=2.65216`, and a `REDEEM` at 2026-05-04T20:32:30Z for `5.12 pUSD`. Authenticated readback run `18ac77308a4b2f70-c7b9-0` reported final available pUSD units `12538612`, reserved pUSD units `0`, open orders `0`, and account preflight PASS.
- Settlement P&L: final pre-submit available pUSD was `10.070772`, post-fill/pre-redeem available pUSD was `7.418612`, final post-redeem available pUSD was `12.538612`, settlement value was `5.120000 pUSD`, total trade cost was `2.652160 pUSD`, and realized P&L was `+2.467840 pUSD`.
- Fee discrepancy: official Polymarket fee docs define `C` as number of shares traded. The activity readback implies `0.092160 pUSD` fee/extra cost (`2.652160 - 2.560000`), matching `5.12 * 0.072 * 0.5 * 0.5`; this exceeded the approval artifact's `0.06 pUSD` max fee estimate. This is documented in the approval artifact and verification note, and LA3 preflight now computes the official crypto taker-fee estimate from shares traded and fails closed when the approved max fee is too low.
- Final local checks passed for the implemented code: `cargo run --offline -- --config config/default.toml validate --local-only`, `cargo fmt --check`, `cargo test --offline`, `cargo clippy --offline -- -D warnings`, `git diff --check`, and the required Live Alpha order/cancel/no-secret/gate scans.
- Exact next action: human review plus final LA3-only verification/PR handling. No second LA3 canary or later-phase work is authorized.

## Blockers And Risks

- M4 API verification sections 3, 5, and 10 are complete for M4 scope.
- M5 API verification sections 7, 8, 11, and 12 are complete for M5 signal/risk scope.
- Final start/end settlement artifact verification is complete for the M9 current-window RTDS paper sample; this does not remove the separate future live-beta release requirements.
- Polymarket geoblock is host/session-specific; prior M2 evidence observed blocked `US/CA`, earlier LA3 evidence observed unblocked `MX/CMX`, and the funded/L2 retry observed unblocked `BR/SP`. Trading-capable modes must remain fail-closed on blocked, malformed, or unreachable geoblock checks.
- CLOB V2 post-cutover endpoint was rechecked on 2026-04-28; use `https://clob.polymarket.com` for CLOB REST unless official docs/live read-only checks change again.
- Polymarket RTDS Chainlink reference ingestion is now the first path for settlement-source paper validation. Direct authenticated Chainlink Data Streams remains a fallback only if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.
- More bounded RTDS Chainlink paper sessions across additional market windows are useful before claiming strategy robustness, but M9 paper/replay evidence now covers current-window selection, natural risk-reviewed paper fills, deterministic replay, and post-market settlement reconciliation.
- LB5 was offline readiness only; LB6 now has one live canary/cancel proof for the exact reviewed canary envelope. This does not authorize autonomous trading, repeated canaries, strategy-to-live routing, wider order sizes, cancel-all, or production live trading.
- The local one-order cap sentinel is consumed for LB6. Do not submit another canary unless a new explicit milestone/gate resets the cap policy and records a new approval scope.
- LB7 is a handoff/runbook/observability phase only. It must not expand the beta or convert the LB6 lifecycle probe into strategy performance evidence.
- Live Alpha starts with LA0 scope lock and evidence gates, not immediate live trading. LA2 preserves all geoblock, readback, heartbeat, risk, stale-data, approval, and no-secret gates.

## Next Concrete Action

- LB0 is approved and complete via `verification/2026-04-29-live-beta-lb0-approval-scope-lock.md`.
- LB1 is complete via `verification/2026-04-29-live-beta-lb1-kill-gates.md`.
- LB2 is complete via `verification/2026-04-29-live-beta-lb2-auth-secret-handling.md`.
- LB3 is complete for dry-run payload construction via `verification/2026-04-30-live-beta-lb3-signing-dry-run.md`.
- LB7 is complete and merged via PR #27 at `26144dc`.
- LA0 is complete and merged via PR #28 at `3bf2048`.
- LA1 is complete and merged via PR #29 at `c6f3c23`.
- Current branch is `live-alpha/la3-controlled-fill-canary` for LA3 controlled fill canary implementation within the local approval artifact.
- LB4 approved-host geoblock is PASS from the earlier Mexico session, and legal/access approval for that LB4 evidence attempt is recorded. Current LA3 retry geoblock evidence observed `BR/SP`.
- LB4 approved-host authenticated readback/account preflight is PASS for the approved Mexico host/session only; current LA3 account preflight also proved authenticated account readiness from the current shell but did not authorize a submit without a passing LA3 dry-run.
- LB5 cancel readiness and rollback/runbook minimum are PASS for offline readiness only; LB6 has now proven one exact live canary submission and exact single-order cancel closeout.
- Next concrete action is human review plus final LA3-only verification/PR handling; do not invoke another human-approved submit and do not start LA4/LA5 unless a new explicit phase approval is recorded.
- Mandatory boundary: LA3 may proceed only within `verification/2026-05-04-live-alpha-la3-approval.md`. Do not add live cancel expansion, cancel-all, FOK, GTC/GTD marketable-limit path, SELL path, taker strategy, retry loop, maker autonomy, one-order cap reset/bypass, strategy-selected live trading, wider live trading, repeated canary, live strategy routing, production rollout, or later-phase work.
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
