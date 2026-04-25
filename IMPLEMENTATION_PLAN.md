# Implementation Plan: Polymarket 15-Minute Crypto Replay/Paper Arbitrage Bot

## Purpose

This plan turns `PRD.md` and `ARCHITECTURE.md` into an ordered build sequence.

The MVP goal is a replay-first and paper-trading-first system that can discover BTC, ETH, and SOL 15-minute Polymarket up/down markets, ingest live market data, maintain deterministic state, simulate fee-aware paper orders, persist all events, and replay sessions exactly.

Live order placement is not part of this implementation plan.

## MVP Defaults

These choices are fixed for the first implementation pass:

- Rust shape: one Cargo package with one binary and internal modules under `src/`.
- Async runtime: Tokio.
- Config format: TOML file plus environment-variable overrides.
- Normalized event format: Serde tagged enums encoded as JSON for v1.
- Tick store: ClickHouse first.
- Relational store: Postgres.
- Deployment target: single Linux `systemd` service.
- Runtime modes: `validate`, `paper`, and `replay`.
- Strategy model: deterministic baseline model first; no ML in MVP.
- Execution: paper-only; no wallet keys and no live order submission.

Rationale: this keeps the first system auditable and easy to replay. Binary/event serialization, multi-crate splitting, PTP, CPU pinning, and advanced models can come after replay shows durable edge.

## Milestones

```text
M0. Repository scaffold and configuration
M1. Storage schema and event model
M2. Market discovery and compliance checks
M3. Feed ingestion and normalization
M4. In-memory state and order books
M5. Signal engine and risk engine
M6. Paper executor and P&L
M7. Replay engine and reports
M8. Observability and production-like runbook
M9. Multi-session validation and live-readiness review
```

Each milestone should end with tests or a concrete runtime check.

## Milestone Exit Gate Policy

Do not start implementation work for the next milestone until the current milestone's exit gate is satisfied.

An exit gate is satisfied only when:

- All build tasks for the milestone are complete or explicitly deferred in writing.
- All milestone verification checks pass.
- All milestone done criteria are met.
- Required API verification sections are complete for milestones that depend on Polymarket behavior.
- Any failing, skipped, or deferred check is recorded with a reason and follow-up issue before moving on.

If a milestone depends on external APIs and verification is blocked by endpoint downtime or unclear docs, implementation may continue only for isolated local code that does not assume the unresolved behavior.

## Milestone Exit Gate Matrix

| Milestone | Exit Gate |
| --- | --- |
| M0: Scaffold/config | CLI, config validation, structured logs, and no-live-order invariant are working under `validate`. |
| M1: Event/storage | Event serialization is stable, migrations run, sample ClickHouse/Postgres write-read checks pass, and storage failures return halt-capable errors. |
| M2: Market discovery/compliance | API verification sections 1, 2, 3, 8, and 9 pass; active/upcoming BTC/ETH/SOL 15-minute markets can be listed; blocked geoblock status fails closed. |
| M3: Feed ingestion/normalization | API verification sections 4, 5, 9, and 10 pass; WebSockets connect read-only; raw and normalized events are persisted; feed staleness is observable. |
| M4: State/order books | API verification sections 3, 5, and 10 pass; book updates are deterministic; stale state is explicit; decision snapshots are coherent. |
| M5: Signal/risk | API verification sections 7, 8, 11, and 12 pass; fair probability, EV, and every risk gate have tests; no paper order can bypass risk. |
| M6: Paper executor/P&L | Maker/taker fill simulation, partial fills, fees, cancellations, and P&L are tested; every paper order has an audit trail; no live order path exists. |
| M7: Replay/reports | Captured and synthetic runs replay deterministically; report generation works offline; determinism checks fail on intentional event drift. |
| M8: Observability/runbook | Metrics endpoint, structured logs, graceful shutdown, and runbook commands work against test config. |
| M9: Multi-session validation | At least one full captured paper session per asset replays deterministically; findings document lists live-readiness blockers; live trading remains disabled. |

## M0: Repository Scaffold And Configuration

### Build Tasks

- Initialize a Rust binary package in the project folder.
- Add internal modules:
  - `config`
  - `compliance`
  - `market_discovery`
  - `feed_ingestion`
  - `normalization`
  - `state`
  - `signal_engine`
  - `risk_engine`
  - `paper_executor`
  - `storage`
  - `replay`
  - `metrics`
  - `reporting`
- Add a CLI with three subcommands:
  - `validate`
  - `paper`
  - `replay`
- Add `config/default.toml`.
- Add `config/example.local.toml` for local overrides without secrets.
- Add structured logging with `tracing`.
- Add a run ID generated at startup and carried through logs/events.

### Required Config Sections

- `runtime`
- `assets`
- `polymarket`
- `feeds`
- `storage`
- `strategy`
- `risk`
- `paper`
- `metrics`
- `replay`

### Verification

- `cargo test` passes.
- `cargo run -- validate --config config/default.toml` loads config and prints validation status.
- Invalid config fails fast with a clear error.
- No module contains live order placement code.

### Done Criteria

- A developer can run the binary in validation mode.
- Config validation blocks missing endpoints, missing assets, and missing risk limits.
- Startup logs include run ID, mode, assets, and config path.

## M1: Storage Schema And Event Model

### Build Tasks

- Define the normalized event enum:
  - `MarketDiscovered`
  - `MarketUpdated`
  - `MarketResolved`
  - `BookSnapshot`
  - `BookDelta`
  - `BestBidAsk`
  - `LastTrade`
  - `ReferenceTick`
  - `PredictiveTick`
  - `SignalUpdate`
  - `PaperOrderPlaced`
  - `PaperOrderCanceled`
  - `PaperFill`
  - `RiskHalt`
  - `ReplayCheckpoint`
- Define common event envelope fields:
  - `run_id`
  - `event_id`
  - `event_type`
  - `source`
  - `source_ts`
  - `recv_wall_ts`
  - `recv_mono_ns`
  - `ingest_seq`
  - `market_id`
  - `asset`
  - `payload`
- Define domain models:
  - `Asset`
  - `Market`
  - `OutcomeToken`
  - `OrderBookLevel`
  - `OrderBookSnapshot`
  - `ReferencePrice`
  - `SignalDecision`
  - `PaperOrder`
  - `PaperFill`
  - `RiskState`
- Add ClickHouse schema migrations for append-only event/tick tables.
- Add Postgres schema migrations for relational state.
- Add storage traits:
  - append raw message
  - append normalized event
  - upsert market
  - insert config snapshot
  - insert paper order/fill
  - insert risk event
  - read run events for replay

### Verification

- Unit tests serialize and deserialize every normalized event variant.
- Storage migration tests run against local containers or test databases.
- A sample event can be written and read back from ClickHouse.
- A sample market and config snapshot can be written and read back from Postgres.

### Done Criteria

- Event schema is stable enough for replay.
- Every persisted event has a run ID and deterministic ordering fields.
- Storage write failures return explicit errors that callers can use to halt.

## M2: Market Discovery And Compliance Checks

### Build Tasks

- Implement Polymarket geoblock check in `compliance`.
- Implement read-only Polymarket market discovery using Gamma/keyset endpoints.
- Filter discovered markets to:
  - BTC
  - ETH
  - SOL
  - 15-minute duration
  - up/down binary outcomes
- Extract required metadata:
  - market slug/title
  - asset
  - condition ID
  - token IDs
  - outcome labels
  - start time
  - end time
  - tick size
  - minimum order size
  - fee settings
  - resolution source
  - lifecycle state
- Mark markets ineligible if required metadata is missing.
- Persist discovered markets to Postgres.
- Emit market lifecycle normalized events.

### Verification

- `validate` mode runs geoblock check and market discovery without strategy execution.
- Unit tests cover market filtering.
- Unit tests cover missing metadata and ineligible-market handling.
- Integration test can discover current matching markets when endpoints are reachable.

### Done Criteria

- The system can list currently active or upcoming BTC/ETH/SOL 15-minute markets.
- Blocked geoblock response prevents paper mode startup.
- Missing or ambiguous resolution source prevents signals for that market.

## M3: Feed Ingestion And Normalization

### Build Tasks

- Implement Polymarket market WebSocket client.
- Subscribe to active market token IDs from market discovery.
- Parse and normalize:
  - book snapshots
  - book deltas or price changes
  - best bid/ask
  - last trade price
  - market lifecycle updates when available
- Implement Binance WebSocket client for BTC, ETH, and SOL spot feeds.
- Implement Coinbase WebSocket client for BTC, ETH, and SOL spot feeds.
- Implement resolution-source feed adapter as a separate interface.
- Persist raw inbound messages.
- Emit normalized events with source timestamps and local receive timestamps.
- Add bounded reconnect/backoff behavior.
- Add feed health state and staleness tracking.

### Verification

- Unit tests parse captured sample messages from each feed.
- Integration test connects to each configured WebSocket in read-only mode.
- Feed disconnect simulation emits a stale/degraded state.
- Raw messages and normalized events are both persisted.

### Done Criteria

- Live paper mode can ingest Polymarket, Binance, Coinbase, and resolution-source data.
- Every normalized event is replayable.
- Feed staleness is observable and can trigger risk halts later.

## M4: In-Memory State And Order Books

### Build Tasks

- Implement per-token order book state.
- Apply book snapshots and deltas deterministically.
- Track:
  - best bid
  - best ask
  - spread
  - visible depth
  - last trade
  - last update timestamp
- Implement market lifecycle state.
- Implement reference and predictive price state by asset/source.
- Implement coherent decision snapshots for the signal engine.
- Track data freshness by source and market.

### Verification

- Unit tests apply snapshot and delta sequences.
- Unit tests verify same event sequence gives same book state.
- Unit tests cover stale book and stale reference detection.
- Replay of a short synthetic sequence reconstructs expected state.

### Done Criteria

- Signal engine can request a read-only snapshot containing market, book, reference, predictive, and position state.
- State updates are deterministic.
- Stale state is explicit, not inferred silently by downstream modules.

## M5: Signal Engine And Risk Engine

### Build Tasks

- Implement deterministic baseline fair-probability model:
  - use market start/end time
  - use current resolution-source price
  - use current distance from market start/reference threshold when available
  - use realized volatility from recent predictive CEX ticks as a configurable input
  - output fair probability and confidence status
- Implement market-implied probability from CLOB best bid/ask.
- Implement expected value calculation including:
  - spread
  - taker fee
  - slippage
  - latency buffer
  - adverse-selection buffer
  - minimum edge threshold
- Implement market phase classification:
  - opening
  - main
  - late
  - final seconds
- Emit `SignalUpdate` for evaluated opportunities and skipped decisions.
- Implement risk gates:
  - max paper loss per market
  - max paper notional per market
  - max paper notional per asset
  - max total paper notional
  - correlated exposure guard
  - stale reference halt
  - stale book halt
  - geoblock halt
  - order-rate guard
  - daily drawdown halt

### Verification

- Unit tests for fair-probability outputs on controlled inputs.
- Unit tests for EV calculation with maker and taker cases.
- Unit tests for every risk gate.
- Golden-file tests for representative signal decisions.

### Done Criteria

- Every signal decision includes a reason and required inputs.
- Risk engine can reject any intent with a persisted reason.
- No paper order can be created without passing risk.

## M6: Paper Executor And P&L

### Build Tasks

- Implement paper order lifecycle:
  - create
  - open
  - partially filled
  - filled
  - canceled
  - expired
- Implement conservative maker fill simulation.
- Implement taker fill simulation from visible book depth.
- Apply taker fees to taker fills.
- Track positions by:
  - market
  - token
  - asset
- Track realized and unrealized paper P&L.
- Persist paper orders, fills, positions, and balances.
- Emit paper execution normalized events.

### Default Maker Simulation Rule

For MVP, maker orders should be conservative:

- If a simulated maker order joins the best price, assume it is behind visible size already resting at that price.
- Count subsequent traded volume at that price toward queue progress.
- Fill only after observed trade volume exceeds the assumed queue ahead.
- If the price moves away before enough volume trades, leave the order open or cancel according to strategy rules.

This is intentionally pessimistic so replay results are less likely to overstate edge.

### Verification

- Unit tests for maker queue simulation.
- Unit tests for taker depth consumption.
- Unit tests for partial fills.
- Unit tests for fee-adjusted P&L.
- Scenario tests for order expiration and cancellation.

### Done Criteria

- Every paper order has a complete audit trail.
- P&L can be traced to fills and market outcomes.
- Paper executor contains no live order submission path.

## M7: Replay Engine And Reports

### Build Tasks

- Implement replay run loading by `run_id`.
- Load config snapshot and normalized events in deterministic order.
- Implement replay clock.
- Reuse live state, signal, risk, and paper execution code.
- Generate per-market replay report:
  - market metadata
  - signals
  - paper orders
  - fills
  - P&L
  - skip reasons
  - risk halts
  - feed staleness windows
  - latency summary
- Add determinism check mode:
  - compare replay-generated paper events to prior paper events
  - fail if outputs diverge outside explicitly allowed fields

### Replay Ordering Rule

Use this ordering for MVP:

1. `recv_mono_ns`
2. `ingest_seq`
3. `event_id`

If source timestamps conflict with receive order, preserve recorded ingestion order for deterministic replay and report the source timestamp gap as a diagnostic.

### Verification

- Replay of a synthetic run produces exact expected events.
- Replay of a captured run is deterministic across two executions.
- Determinism check fails when an event is removed or reordered.
- Report generation works without external network access.

### Done Criteria

- Any paper session can be replayed offline.
- Replay produces stable outputs with the same config and events.
- Reports are readable enough to decide whether the strategy has edge.

## M8: Observability And Production-Like Runbook

### Build Tasks

- Add Prometheus endpoint.
- Export metrics:
  - feed message rate
  - feed latency
  - WebSocket reconnect count
  - book staleness
  - reference-feed staleness
  - signal count
  - paper order count
  - paper fill count
  - paper P&L
  - risk halt count
  - storage write failures
  - replay determinism failures
- Add structured log fields:
  - run ID
  - mode
  - market ID
  - asset
  - source
  - event type
  - reason
- Add example Grafana dashboard JSON or dashboard notes.
- Add `systemd` service template.
- Add runbook for:
  - validate mode
  - starting paper mode
  - stopping safely
  - running replay
  - interpreting reports
  - handling feed/storage failures

### Verification

- Metrics endpoint returns expected metrics.
- Logs include run ID and mode.
- Runbook commands work locally against test config.
- Paper mode can shut down gracefully.

### Done Criteria

- A production-like paper session can be operated without reading source code.
- Feed, storage, and risk failures are visible in metrics and logs.

## M9: Multi-Session Validation And Live-Readiness Review

### Build Tasks

- Run multiple paper sessions across BTC, ETH, and SOL markets.
- Replay each session and verify deterministic outputs.
- Compare paper results by:
  - asset
  - market phase
  - maker vs taker simulation
  - edge threshold
  - feed staleness
  - time remaining
- Identify false-positive signals and missed opportunities.
- Audit dependency list for hot-path risks.
- Audit any Polymarket SDK usage.
- Confirm no live order path exists.
- Produce a live-readiness findings document.

### Verification

- At least one full paper session can be captured and replayed for each asset.
- Replay determinism passes for all selected sessions.
- Reports identify whether strategy performance survives fees and conservative fills.
- Live-readiness review explicitly lists remaining blockers before real orders.

### Done Criteria

- The MVP can answer whether the baseline strategy is promising.
- The system can support a future live-beta PRD without architectural rewrite.
- Live trading remains disabled.

## Suggested File Layout

```text
polymarket-15m-arb-bot/
  AGENTS.md
  PRD.md
  ARCHITECTURE.md
  IMPLEMENTATION_PLAN.md
  Cargo.toml
  config/
    default.toml
    example.local.toml
  migrations/
    clickhouse/
    postgres/
  src/
    main.rs
    config/
    compliance/
    market_discovery/
    feed_ingestion/
    normalization/
    state/
    signal_engine/
    risk_engine/
    paper_executor/
    storage/
    replay/
    metrics/
    reporting/
  tests/
    fixtures/
    integration/
  reports/
    .gitkeep
  runbooks/
    PAPER_MODE.md
    REPLAY.md
```

## Dependency Guidelines

Use stable, common Rust crates unless there is a strong reason not to:

- Async/runtime: `tokio`
- HTTP: `reqwest`
- WebSocket: `tokio-tungstenite` or a compatible maintained client
- Serialization: `serde`, `serde_json`, `toml`
- CLI: `clap`
- Errors: `thiserror`, `anyhow` at application boundaries
- Logging: `tracing`, `tracing-subscriber`
- Metrics: `prometheus` or a maintained Prometheus exporter crate
- Time: `time` or `chrono`, plus `std::time::Instant` for monotonic timing
- Postgres: `sqlx` or `tokio-postgres`
- ClickHouse: maintained ClickHouse HTTP/native client

Do not add unofficial trading/HFT SDKs to the hot path until they are audited.

## Testing Strategy

### Test Levels

- Unit tests for pure calculations and state transitions.
- Parser tests using captured sample messages.
- Storage tests against local test databases.
- Integration tests for read-only endpoints and WebSocket subscriptions.
- Replay tests for deterministic outcomes.
- Failure tests for stale feeds, disconnects, bad metadata, rate limits, and storage errors.

### Minimum Test Coverage By Milestone

- M0: config validation and CLI parsing.
- M1: event serialization and storage read/write.
- M2: market filtering and geoblock handling.
- M3: feed parsing and reconnect behavior.
- M4: order book determinism.
- M5: fair probability, EV, and risk gates.
- M6: maker/taker fill simulation and P&L.
- M7: replay determinism.
- M8: metrics and graceful shutdown.
- M9: captured-session replay.

## Operational Safety Rules

- Paper mode must fail closed when required data is stale or missing.
- Storage failure must halt new paper decisions because decisions must remain auditable.
- Market metadata ambiguity must disable strategy for that market.
- Geoblock-blocked status must prevent trading-capable modes.
- No private key handling is needed for MVP.
- Any future live order feature requires a separate PRD and explicit user approval.

## First Coding Sequence

Start coding in this exact order:

1. Scaffold Rust binary, CLI, config loader, and structured logs.
2. Define domain models and normalized event enum.
3. Add Postgres and ClickHouse migrations.
4. Implement storage traits with local test write/read.
5. Implement geoblock check and market discovery in `validate` mode.
6. Implement Polymarket WebSocket ingestion and raw/normalized persistence.
7. Implement CEX/reference feed ingestion.
8. Implement order book state and decision snapshots.
9. Implement baseline signal and risk checks.
10. Implement paper executor.
11. Implement replay.
12. Add metrics, reports, and runbooks.

Do not start with strategy optimization. The first objective is a trustworthy data and replay loop.

## MVP Acceptance Checklist

- `validate` mode checks config, geoblock, storage, and market discovery.
- `paper` mode records live market data and simulated decisions.
- `replay` mode can replay a recorded run offline.
- BTC, ETH, and SOL markets are discovered and filtered correctly.
- Polymarket order books are maintained from WebSocket data.
- Reference and predictive feeds are ingested and timestamped.
- Fair probability and EV calculations are logged for each decision.
- Risk halts are enforced and persisted.
- Paper fills and P&L are reproducible in replay.
- Prometheus metrics expose feed, strategy, risk, storage, and replay health.
- There is no live order placement path.
