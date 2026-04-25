# Architecture: Polymarket 15-Minute Crypto Replay/Paper Arbitrage Bot

## Purpose

This document defines the MVP architecture for the Polymarket 15-minute BTC, ETH, and SOL up/down market bot.

The MVP is replay-first and paper-trading-first. It must collect complete market data, normalize it, maintain deterministic state, generate fee-aware paper orders, enforce risk halts, and replay recorded sessions exactly.

Live order placement is out of scope for the MVP.

## System Overview

The system is a modular Rust/Tokio application with one primary event path:

```text
External feeds
  -> feed ingestion
  -> normalized events
  -> in-memory state
  -> signal engine
  -> risk engine
  -> paper executor
  -> storage
  -> replay and reports
```

The same normalized event format powers both live paper sessions and offline replay. This is the core architectural constraint: if a live paper decision cannot be reconstructed later from stored events and config, the implementation is incomplete.

## Runtime Modes

### Live Paper Mode

Live paper mode connects to external feeds, updates internal state, emits signals, simulates orders, and persists all raw and normalized data.

```text
Polymarket WebSocket
Binance WebSocket
Coinbase WebSocket
Resolution-source feed
Gamma/keyset REST
Geoblock REST
        |
        v
Feed ingestion + market discovery
        |
        v
Normalized event bus
        |
        v
State + signal + risk + paper execution
        |
        v
Tick store + relational store + metrics
```

### Replay Mode

Replay mode reads recorded normalized events and the exact config snapshot from a prior run, then rebuilds state and strategy decisions deterministically.

```text
Stored normalized events + config snapshot
        |
        v
Replay clock
        |
        v
State + signal + risk + paper execution
        |
        v
Replay report + determinism checks
```

Replay mode must not connect to external market data or submit any real orders.

### Read-Only Validation Mode

Read-only validation mode checks configuration, official endpoint reachability, geoblock status, market discovery, and WebSocket subscriptions without running strategy logic.

This mode is useful before starting live paper sessions.

## High-Level Data Flow

```text
1. Startup
   config -> validation -> geoblock check -> storage connection -> market discovery

2. Market data
   WebSockets/REST -> raw messages -> normalized events -> event bus -> storage

3. State
   normalized events -> order books + market state + reference state

4. Decision
   state -> fair probability -> EV calculation -> risk gate -> paper order intent

5. Paper execution
   paper order intent -> fill simulation -> position/P&L updates -> storage

6. Replay
   stored events + config -> replay clock -> same state/decision/execution path -> report
```

## Module Map

```text
config
  Owns runtime settings and config snapshots.

compliance
  Owns geoblock/startup eligibility checks.

market_discovery
  Owns Polymarket market metadata discovery and lifecycle events.

feed_ingestion
  Owns external WebSocket/REST connections and raw message capture.

normalization
  Converts venue-specific messages into internal events.

state
  Owns order books, market state, reference state, positions, and clocks.

signal_engine
  Computes fair probability, market-implied probability, and expected value.

risk_engine
  Applies hard trading and data-health gates.

paper_executor
  Simulates maker/taker orders, fills, cancellations, positions, and P&L.

storage
  Persists raw messages, normalized events, metadata, orders, fills, and snapshots.

replay
  Replays recorded events deterministically through the same strategy path.

metrics
  Exposes health, latency, trading, replay, and risk metrics.

reporting
  Produces session and per-market reports.
```

## Module Responsibilities

### Config

The config module loads and validates runtime settings before any trading-capable loop starts.

Responsibilities:

- Load a single config file.
- Validate required endpoints, assets, limits, and storage settings.
- Materialize a config snapshot at startup.
- Persist the config snapshot with each live paper or replay run.
- Prevent startup if required risk limits are missing.

Required config groups:

- Assets: BTC, ETH, SOL.
- Polymarket CLOB V2 REST endpoint.
- Polymarket market WebSocket endpoint.
- Polymarket Gamma/keyset discovery endpoint.
- Polymarket geoblock endpoint.
- Binance and Coinbase WebSocket endpoints.
- Resolution-source feed settings.
- Paper wallet and exposure limits.
- Fee model and fee refresh settings.
- Strategy thresholds.
- Stale-feed thresholds.
- Storage connection settings.
- Runtime mode.

### Compliance

The compliance module prevents trading-capable modes from running when access is blocked.

Responsibilities:

- Call the Polymarket geoblock endpoint at startup.
- Persist the geoblock result.
- Disable live paper mode if the response indicates blocked access.
- Emit a `RiskHalt` or startup failure with a clear reason.

The module must not contain or suggest any bypass behavior.

### Market Discovery

The market discovery module finds active BTC, ETH, and SOL 15-minute up/down markets.

Responsibilities:

- Poll Polymarket Gamma/keyset endpoints.
- Filter to BTC, ETH, and SOL 15-minute up/down markets.
- Extract condition IDs, token IDs, outcomes, start/end times, tick size, min size, fee settings, and resolution source.
- Emit `MarketDiscovered`, `MarketUpdated`, and `MarketResolved` events.
- Refresh active market state on a configurable interval.
- Persist market metadata to Postgres.

The module should not infer missing resolution-source details silently. If resolution source is missing or ambiguous, the market should be marked ineligible for strategy decisions until resolved.

### Feed Ingestion

The feed ingestion module owns external connections and raw message capture.

Responsibilities:

- Maintain Polymarket market WebSocket subscriptions for active token IDs.
- Maintain Binance and Coinbase WebSocket subscriptions for BTC, ETH, and SOL.
- Maintain resolution-source feed subscriptions or polling clients.
- Reconnect with bounded backoff.
- Emit feed-health events.
- Persist raw inbound messages before or alongside normalization.

Feed ingestion does not calculate trading signals. Its output is raw messages plus normalized events.

### Normalization

The normalization module converts source-specific messages into internal event types.

Responsibilities:

- Parse Polymarket book snapshots, price changes, best bid/ask, trades, new market events, and resolved market events.
- Parse reference and predictive price ticks.
- Attach source timestamps where available.
- Attach local receive timestamps using a monotonic clock plus wall-clock time.
- Validate event ordering where source sequence numbers exist.
- Emit normalized events to the internal event bus.

Normalized events must be stable enough to support deterministic replay.

### Order Book State

The order book state module owns per-token in-memory books.

Responsibilities:

- Maintain bids and asks by price level for each outcome token.
- Apply snapshots and deltas.
- Track best bid, best ask, spread, depth, and last trade.
- Detect stale books.
- Detect sequence gaps or state resets where source data supports it.
- Expose read-only book snapshots to the signal engine.

The order book module should be deterministic: the same event sequence must produce the same book state.

### Market And Reference State

The state module owns non-book state required for decisions.

Responsibilities:

- Maintain active market lifecycle state.
- Maintain current reference-source price per asset.
- Maintain predictive CEX prices per asset and source.
- Track feed freshness and source latency.
- Track current paper positions and balances.
- Expose immutable decision snapshots to the signal engine.

Decision snapshots should be internally consistent: a signal evaluation should see one coherent view of book, market, reference, and position state.

### Signal Engine

The signal engine produces paper order intents.

Responsibilities:

- Estimate fair probability from the market's resolution source, current reference price, time remaining, and configured model.
- Calculate market-implied probability from Polymarket best bid/ask and depth.
- Calculate expected value after spread, taker fee, slippage, latency buffer, adverse-selection buffer, and minimum edge.
- Classify market phase: opening, main, late, final seconds.
- Emit `SignalUpdate` for every evaluated opportunity.
- Emit a paper order intent only when thresholds pass.
- Emit skip reasons when no order is generated.

The signal engine should not update positions or fills. It only proposes paper intents.

### Risk Engine

The risk engine gates all paper order intents and can halt strategy decisions.

Responsibilities:

- Enforce max paper loss per market.
- Enforce max paper notional per market.
- Enforce max paper notional per asset.
- Enforce max total paper notional.
- Enforce correlated exposure limits across BTC, ETH, and SOL.
- Halt on stale reference data.
- Halt on stale or disconnected Polymarket books.
- Halt on geoblock-blocked startup state.
- Halt on excessive order rate.
- Warn or halt on clock drift according to config.
- Persist every halt and rejection reason.

The risk engine is authoritative. Paper executor must not accept an order intent that has not passed risk checks.

### Paper Executor

The paper executor simulates order placement, cancellation, fills, positions, and P&L.

Responsibilities:

- Accept only risk-approved paper order intents.
- Simulate maker order placement with conservative queue assumptions.
- Simulate maker fills based on subsequent trades/book movement according to configured rules.
- Simulate taker fills by consuming visible book depth.
- Apply taker fees when taker simulation is used.
- Track open paper orders.
- Track positions by market, token, and asset.
- Track realized and unrealized paper P&L.
- Emit `PaperOrderPlaced`, `PaperOrderCanceled`, and `PaperFill` events.
- Persist order and fill state.

The paper executor must never submit real orders in the MVP.

### Storage

The storage layer persists both high-volume tick data and relational state.

Responsibilities:

- Write raw feed messages.
- Write normalized events.
- Write market metadata.
- Write config snapshots.
- Write paper orders, fills, positions, balances, and risk events.
- Provide ordered event reads for replay.
- Provide report queries.

Storage should be append-first for event data. Corrections should be represented as new events rather than in-place mutation where practical.

### Replay

The replay module reconstructs a session from persisted data.

Responsibilities:

- Load the config snapshot for a run.
- Load normalized events in deterministic order.
- Drive the system with a replay clock.
- Reuse the same state, signal, risk, and paper execution logic as live paper mode.
- Compare generated paper decisions against prior recorded decisions when requested.
- Produce deterministic replay reports.

Replay must not depend on wall-clock time except for controlled replay pacing.

### Metrics

The metrics module exposes Prometheus metrics and structured logs.

Responsibilities:

- Feed message rates by source.
- Feed latency by source.
- WebSocket reconnect counts.
- Book staleness.
- Reference-source staleness.
- Signal counts by market and action.
- Paper order/fill counts.
- Paper P&L metrics.
- Risk halt counts.
- Replay determinism results.
- Storage write failures.

Metrics should be operationally useful but must not be the source of record. The source of record is persisted events plus config snapshots.

## Internal Event Flow

All major modules communicate through normalized events and explicit state snapshots.

```text
                        +------------------+
                        |  market_discovery |
                        +---------+--------+
                                  |
                                  v
                         Market lifecycle events
                                  |
                                  v
+----------------+       +--------+--------+       +----------------+
| WebSocket/REST | ----> | normalization   | ----> | event bus      |
+----------------+       +--------+--------+       +--------+-------+
                                  |                         |
                                  v                         v
                           raw + normalized             storage
                           event persistence                |
                                                            v
                                                       replay input
```

The decision path consumes normalized events after they update state:

```text
normalized events
      |
      v
state update
      |
      v
decision snapshot
      |
      v
signal_engine
      |
      v
risk_engine
      |
      v
paper_executor
      |
      v
paper events -> storage -> replay/reporting
```

## Event Ordering And Time

Every normalized event should include:

- Source name.
- Source event timestamp if provided.
- Local receive wall-clock timestamp.
- Local receive monotonic timestamp.
- Market ID or asset ID where applicable.
- Sequence number where applicable.
- Run ID.

Ordering rules:

- Per-source sequence numbers should be respected where available.
- Local receive monotonic timestamp is used as the fallback ordering key.
- Replay ordering must be stable and documented.
- If ordering is ambiguous, replay should preserve recorded ingestion order.

Clock handling:

- Use monotonic time for latency measurement and stale-feed checks.
- Use wall-clock time for reporting and market lifecycle alignment.
- Warn on detected drift.
- Start with `chrony`/NTP; PTP is a later optimization.

## State Ownership

Each mutable state area should have one owner:

```text
Market metadata       -> market_discovery/state
Order books           -> order_book_state
Reference prices      -> state
Predictive CEX prices -> state
Risk status           -> risk_engine
Paper orders/fills    -> paper_executor
Config snapshot       -> config/storage
```

Other modules should consume immutable snapshots or event streams instead of mutating shared state directly.

## Storage Architecture

### Tick Store

Use ClickHouse or QuestDB for high-volume append-only data:

- Raw feed messages.
- Normalized market events.
- Order book events.
- Reference and predictive ticks.
- Signal updates.
- Paper order events.
- Paper fill events.
- Replay checkpoints.

### Relational Store

Use Postgres for relational state:

- Markets.
- Config snapshots.
- Run metadata.
- Paper orders.
- Paper fills.
- Paper positions.
- Paper balances.
- Risk events.
- Replay runs.

### Write Path

```text
raw external message
      |
      +--> raw message sink
      |
      v
normalized event
      |
      +--> tick store
      |
      +--> state update
      |
      +--> relational store when event changes relational state
```

The system should prefer durable event capture over perfect downstream processing. If reporting fails but raw and normalized events are persisted, the run remains recoverable.

## Replay Architecture

Replay reads from persisted normalized events and drives the same state and decision modules.

```text
run_id
  -> load config snapshot
  -> load ordered normalized events
  -> initialize replay clock
  -> apply events to state
  -> evaluate signal/risk/paper execution
  -> compare or report outputs
```

Replay outputs:

- Market-by-market P&L.
- Fills and missed fills.
- Signal count and skip reasons.
- Risk halts.
- Feed staleness windows.
- Determinism check result.
- Config used.

## Failure Boundaries

### Feed Failure

If a primary feed disconnects or becomes stale:

- Emit feed-health event.
- Mark dependent markets ineligible for new paper orders.
- Cancel or expire relevant paper maker orders according to config.
- Persist the halt or degraded state.

### Market Metadata Failure

If required market metadata is missing:

- Persist the market as discovered but ineligible.
- Do not generate signals for that market.
- Emit a clear skip reason.

### Storage Failure

If raw or normalized event persistence fails:

- Degrade to halt mode.
- Stop generating new paper orders.
- Continue attempting graceful shutdown or recovery.

The system should not make paper decisions that cannot be audited later.

### Risk Failure

If risk state cannot be computed:

- Reject all new paper order intents.
- Persist a risk halt if storage is available.
- Emit a structured error.

## Security And Compliance Boundaries

- No live private keys are required for the MVP.
- No live order placement exists in the MVP.
- No geoblock bypassing.
- Geoblock status is checked before trading-capable modes.
- Future live trading requires a separate design and explicit release gate.

## MVP Deployment Shape

Initial production-like deployment should be simple:

```text
systemd
  -> polymarket-15m-arb-bot binary
       -> Postgres
       -> ClickHouse or QuestDB
       -> Prometheus scrape endpoint
       -> structured logs
```

Kubernetes is not needed for the MVP. CPU pinning, PTP, kernel tuning, and bare-metal optimization should wait until replay/paper results show a real edge.

## Open Implementation Decisions

These decisions should be resolved during implementation planning before coding:

- ClickHouse vs QuestDB as the first tick store.
- Exact config file format.
- Exact internal event serialization format.
- First version of the fair-probability model.
- Conservative maker queue simulation rule.
- Replay ordering tie-breaker when timestamps collide.
- Whether to keep all modules in one Rust crate initially or use a small workspace.

## Architecture Acceptance Criteria

The architecture is implemented correctly when:

- Live paper mode and replay mode share the same normalized event model.
- Every paper decision can be traced to a config snapshot, state snapshot, signal calculation, and risk decision.
- Raw and normalized events are persisted for every live paper session.
- Replaying the same run with the same config produces deterministic outputs.
- No module can submit real orders in the MVP.
- Feed, risk, and storage failures halt new paper orders instead of silently continuing.
