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

- Branch: `m6/paper-executor-pnl`
- Short commit: `e6fa663`
- Worktree status: M6 paper executor/P&L implementation fixes and status updates are present but uncommitted.

## Milestones

- Last completed milestone: M6 - Paper Executor And P&L.
- Active milestone: M7 - Replay Engine And Reports.
- Next milestone: M7 - Replay Engine And Reports.

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

Stubbed runtime status remains intentional:

```text
paper_mode_status=stubbed_until_later_milestones
replay_status=stubbed_until_later_milestones
```

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
| Paper runtime | NA | Stubbed until later milestones. |
| Replay runtime | NA | Stubbed until later milestones. |
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
| Runtime scope lock | PASS | `paper` and `replay` runtime stubs remain unchanged; no strategy, paper execution, live order, signing, wallet, or private-key path was added. |

## Next Exit Gate

M7 is complete only when:

- Replay can reconstruct deterministic state and paper outcomes from recorded normalized events.
- Replay outputs fills, P&L, latency, missed opportunities, and risk halt report inputs.
- Identical replay input plus config produces identical outputs.

## M6 Current State

- `main.rs` still reports `paper_mode_status=stubbed_until_later_milestones` and `replay_status=stubbed_until_later_milestones`; this confirms M6 work has not been wired into runtime yet.
- `src/paper_executor.rs` has been split into `src/paper_executor/mod.rs`, `src/paper_executor/lifecycle.rs`, and `src/paper_executor/pnl.rs`.
- `PaperExecutor` consumes only risk-approved `PaperOrderIntent`s. A denied risk decision emits a paper audit rejection and creates no paper order.
- Paper lifecycle support covers open, partial, filled, canceled, expired, and rejected states with explicit audit events.
- Maker fills use conservative visible-queue assumptions from later trade ticks; taker fills consume visible executable book depth and per-market fee parameters.
- `PaperPositionBook` tracks positions by market/token/asset, average price, realized P&L, unrealized marks, settlement marks, fees, and deterministic exposure snapshots.
- Storage now has paper position and balance write APIs matching the existing Postgres M6 tables.
- Position and risk context exposure remain in `state::snapshot` and `risk_engine` from M5 context.
- Final start/end settlement artifact verification remains deferred to M7/reporting evidence, but M6 can trace P&L to explicit winning-token or split market outcomes.

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

M6 verification status: PASS.
- M6 local checks passed: `cargo fmt --check`, `cargo test --offline` (84 tests), and `cargo clippy --offline -- -D warnings`.
- M6 lifecycle tests cover maker fills, taker fills with per-market fees, partial fill math, cancel, expire, reject, and the no-risk-approval/no-order invariant.
- M6 P&L tests cover taker fee math, maker fee zero, position average price updates, realized P&L, unrealized P&L, deterministic replay of identical fills, settlement marking to winning/losing/split market outcomes, and storage-ready position snapshots.
- M6 storage tests cover paper order, fill, position, balance, and risk-event write paths.
- M6 safety scan found no source path for live order placement, signing, wallet, API key, or private-key handling. Matches were documentation/status text plus the live-order-disabled runtime flag.
- No new runtime verification was started; `paper` and `replay` runtime stubs remain unchanged.

## Blockers And Risks

- M4 API verification sections 3, 5, and 10 are complete for M4 scope.
- M5 API verification sections 7, 8, 11, and 12 are complete for M5 signal/risk scope.
- Final start/end settlement artifact verification remains deferred for paper P&L/reporting; this no longer blocks M5 because ambiguous or asset-mismatched resolution rules are ineligible at discovery, signal, and risk gates.
- Polymarket geoblock is host/session-specific; prior M2 evidence observed blocked `US/CA`, while the current read-only M5 recheck observed unblocked `MX/CHP`. Trading-capable modes must remain fail-closed on blocked, malformed, or unreachable geoblock checks.
- CLOB V2 cutover timing is time-sensitive; recheck endpoint assumptions if work continues after the April 28, 2026 cutover window.
- Final start/end settlement artifact verification is still required before reporting can claim live post-market reconciliation, but it does not block the M6 executor/P&L implementation gate.

## Next Concrete Action

Start M7 replay/reports. Keep settlement artifact verification as report evidence, and do not add live order placement, signing, wallet, or key handling.

## Update Checklist

When updating this file, include:

- Current branch and short commit.
- Clean/dirty worktree status and any unrelated user changes to preserve.
- Last completed milestone and active milestone.
- Next required exit gate.
- Latest verification evidence paths and outcomes.
- Current blockers, risks, and API assumptions.
- One concrete next action.
