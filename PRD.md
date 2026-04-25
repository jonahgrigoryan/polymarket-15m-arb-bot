# PRD: Polymarket 15-Minute Crypto Replay/Paper Arbitrage Bot

## Summary

Build an MVP trading system for BTC, ETH, and SOL 15-minute Polymarket up/down markets, targeting replay and paper trading first, with legally eligible non-US Polymarket CLOB V2 access.

The system should optimize for correctness before live speed: accurate market discovery, settlement-source-aware signals, fee-aware execution simulation, full tick capture, and deterministic replay.

Current platform constraints to design around:

- CLOB V2 go-live is April 28, 2026 around 11:00 UTC, with new contracts, pUSD collateral, V2 signing/order fields, and wiped open orders.
- Makers are fee-free; takers pay crypto fees, so execution must be maker-first unless expected value clears all costs.
- Polymarket international blocks US order placement; the product must include geoblock compliance checks and must not bypass restrictions.
- Use Polymarket WebSockets for order book updates; REST is for startup, recovery, and metadata.

## Goals

- Discover active BTC, ETH, and SOL 15-minute crypto markets automatically.
- Maintain live Polymarket order books from WebSocket data.
- Ingest the actual resolution-source feed plus predictive CEX feeds.
- Produce settlement-source-aware fair probability estimates.
- Simulate maker-first and selective taker execution in paper mode.
- Record all ticks, market data, signals, orders, and simulated fills.
- Replay recorded sessions deterministically for strategy validation.
- Produce per-market P&L, edge capture, latency, and risk reports.

## Non-Goals

- No live order placement in the MVP.
- No US geoblock bypassing or restricted-region access.
- No custodial wallet management.
- No public-facing trading UI in the MVP.
- No unsupported assets beyond BTC, ETH, and SOL.
- No reliance on unaudited third-party signing or order-routing code.

## Target Users

- Primary: the system owner/operator validating whether the strategy has real edge before risking capital.
- Secondary: future engineering agents or developers extending the replay/paper system into a live controlled beta.

## Core Stack

- Language/runtime: Rust + Tokio for the hot path.
- Polymarket integration: official Polymarket CLOB V2 Rust client if verified against the V2 endpoint; otherwise a minimal custom Rust V2 client based on official docs.
- Research and analysis: Python notebooks/scripts outside the live engine.
- Storage:
  - ClickHouse or QuestDB for tick, book, signal, and replay data.
  - Postgres for markets, config, paper orders, fills, balances, and audit state.
- Deployment: single Rust binary under systemd.
- Hosting target: London/Ireland low-jitter host for realistic international API latency testing.
- Observability: Prometheus, Grafana, and structured tracing logs.

## Architecture

The MVP should be a modular Rust system with clear boundaries:

1. Market discovery
   - Poll Polymarket Gamma/keyset market endpoints for BTC, ETH, and SOL 15-minute up/down markets.
   - Extract condition IDs, token IDs, outcomes, start/end times, tick size, min order size, fee settings, and resolution source.
   - Refresh periodically and emit market lifecycle events.

2. Feed ingestion
   - Subscribe to Polymarket CLOB market WebSocket for order book snapshots, price changes, best bid/ask, trades, new-market, and resolved-market events.
   - Subscribe to the settlement-source feed for each active market where available.
   - Subscribe to Binance and Coinbase spot feeds as predictive inputs.
   - Normalize all feed messages into internal timestamped events.

3. State management
   - Maintain in-memory order books per token ID.
   - Maintain current market lifecycle state.
   - Maintain current reference price state per asset.
   - Persist raw and normalized events for replay.

4. Signal engine
   - Estimate fair probability using the active market's resolution source and time remaining.
   - Compare fair probability to CLOB best bid/ask and available depth.
   - Account for spread, taker fee, latency buffer, adverse-selection buffer, and market phase.
   - Emit paper-intended orders only when thresholds are met.

5. Paper execution
   - Simulate maker orders conservatively, including queue-position assumptions.
   - Simulate taker orders from observed book depth, including fees and slippage.
   - Track positions, realized/unrealized P&L, and per-market exposure.
   - Never submit real orders in MVP.

6. Risk engine
   - Enforce max exposure per market.
   - Enforce max exposure per asset.
   - Enforce global paper drawdown halt.
   - Halt on stale reference feed.
   - Halt on Polymarket WebSocket disconnect or degraded state.
   - Halt when geoblock check reports restricted access.
   - Warn on clock drift.

7. Replay engine
   - Reconstruct market sessions from recorded events.
   - Replay at 1x and accelerated speeds.
   - Guarantee deterministic strategy outputs for identical inputs and config.
   - Generate replay reports with trades, fills, P&L, latency, missed opportunities, and risk halts.

## Public Interfaces

### Configuration

The system should load a single config file with:

- Assets: BTC, ETH, SOL.
- Polymarket endpoints:
  - V2 CLOB REST endpoint.
  - CLOB market WebSocket endpoint.
  - Gamma market discovery endpoint.
  - Geoblock endpoint.
- Feed endpoints:
  - Resolution-source feeds.
  - Binance WebSocket feeds.
  - Coinbase WebSocket feeds.
- Paper wallet limits.
- Fee model and market-fee refresh interval.
- Strategy thresholds.
- Stale-feed thresholds.
- Risk limits.
- Storage connection strings.
- Replay mode flags.

### Event Types

The internal event bus should support:

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

### Market Model

Each market record should include:

- Market slug/title.
- Asset symbol.
- Condition ID.
- Outcome token IDs.
- Outcome labels.
- Start time.
- End time.
- Resolution source.
- Tick size.
- Minimum order size.
- Fees enabled.
- Fee parameters.
- Market lifecycle state.

### Execution Model

The MVP execution model is paper-only:

- Maker simulation should assume conservative queue position.
- Taker simulation should consume visible book depth.
- Taker simulation must include Polymarket crypto taker fees.
- The system must log why every order was placed, skipped, filled, canceled, or halted.

## Strategy Requirements

The default strategy should be maker-first:

- Place simulated maker quotes only when fair probability exceeds required edge.
- Cancel or avoid quotes when the reference feed is stale, book state is stale, or time remaining is below configured threshold.
- Simulate taker entries only when expected value clears:
  - Spread.
  - Taker fee.
  - Slippage.
  - Latency buffer.
  - Adverse-selection buffer.
  - Minimum EV threshold.

Recommended market phases:

- Opening phase: small size, wider threshold, focus on price discovery.
- Main phase: normal threshold and sizing.
- Late phase: reduce inventory and widen threshold.
- Final seconds: avoid new trades unless explicitly enabled in future versions.

## Risk Requirements

The system must include hard risk controls:

- Max paper loss per market.
- Max paper notional per market.
- Max paper notional per asset.
- Max total paper notional.
- Max correlated exposure across BTC, ETH, and SOL.
- Stale-feed kill switch.
- WebSocket disconnect kill switch.
- Order-rate guard.
- Clock-drift warning.
- Daily paper drawdown halt.

All risk halts must be persisted and visible in reports.

## Data And Storage

### Tick Store

Use ClickHouse or QuestDB for:

- Raw feed messages.
- Normalized order book events.
- Reference ticks.
- Predictive CEX ticks.
- Signal updates.
- Paper order events.
- Paper fills.
- Replay checkpoints.

### Relational Store

Use Postgres for:

- Market metadata.
- Runtime configuration snapshots.
- Paper orders.
- Paper fills.
- Paper positions.
- Paper balances.
- Risk events.
- Replay run metadata.

## Observability

Expose Prometheus metrics for:

- Feed latency by source.
- Message rate by source.
- WebSocket reconnect count.
- Book staleness.
- Reference-feed staleness.
- Signal count.
- Paper orders placed/canceled/filled.
- Paper P&L.
- Risk halt count.
- Replay determinism failures.

Grafana dashboards should show:

- Current active markets.
- Per-asset reference price.
- Polymarket best bid/ask.
- Fair probability vs market-implied probability.
- Paper positions.
- P&L by market and asset.
- Feed health.
- Risk state.

## Testing Plan

### Unit Tests

- V2 order-field construction.
- Fee calculation.
- Market lifecycle parsing.
- Fair-probability calculation.
- Order book state updates.
- Paper fill simulation.
- Risk halt logic.

### Integration Tests

- Gamma/keyset market discovery.
- CLOB market WebSocket subscription.
- Resolution-source feed ingestion.
- Binance/Coinbase feed ingestion.
- Geoblock check.
- Storage write/read path.

### Replay Tests

- Replay recorded BTC, ETH, and SOL sessions.
- Verify deterministic strategy outputs.
- Verify no trades occur on stale reference data.
- Verify fee-adjusted P&L is stable.
- Verify market lifecycle transitions are correct.

### Failure Tests

- Dropped Polymarket WebSocket.
- Delayed Binance/Coinbase feed.
- Missing or stale resolution-source feed.
- Bad market metadata.
- Book snapshot reset.
- Rate-limit responses.
- Geoblock-blocked response.
- Storage outage.

## Acceptance Criteria

The MVP is complete when:

- The system discovers active BTC, ETH, and SOL 15-minute markets.
- The system maintains live Polymarket order books from WebSocket data.
- The system records full tick and order book data.
- The system computes settlement-source-aware fair probability.
- The system generates paper maker/taker decisions with fee-aware EV.
- The system enforces risk halts.
- The system can replay a recorded session deterministically.
- The system produces per-market reports with P&L, fills, latency, edge, and halt reasons.
- No real orders can be submitted without a future explicit live-trading release gate.

## Release Plan

### Phase 1: Foundations

- Create Rust workspace and config loader.
- Implement market discovery.
- Implement Polymarket WebSocket ingestion.
- Implement storage schema and event persistence.
- Implement geoblock check.

### Phase 2: Signals And Paper Trading

- Implement reference and predictive feed ingestion.
- Implement fair probability model.
- Implement fee-aware EV calculation.
- Implement paper execution.
- Implement risk engine.

### Phase 3: Replay And Reporting

- Implement deterministic replay.
- Add replay reports.
- Add Prometheus metrics.
- Add Grafana dashboards.
- Run multi-day paper sessions.

### Phase 4: Live Readiness Review

- Audit V2 signing and order construction.
- Verify official Rust client or custom minimal V2 client.
- Audit all third-party dependencies in the hot path.
- Review legal and venue access.
- Define separate live-beta PRD before enabling real orders.

## Assumptions And Defaults

- MVP is replay and paper trading only.
- Venue path assumes legally eligible non-US access.
- Startup must call the Polymarket geoblock endpoint and disable trading if blocked.
- BTC, ETH, and SOL are the only v1 assets.
- No custom frontend in MVP.
- Grafana dashboards and CLI reports are sufficient.
- Default execution posture is maker-first.
- Taker simulation is allowed only for analysis and must include taker fees.
- `polymarket-hft` may be reviewed for architecture ideas but must not be used for production signing or routing without audit.

## Source Links

- Polymarket CLOB V2 migration: https://docs.polymarket.com/v2-migration
- Polymarket clients and SDKs: https://docs.polymarket.com/api-reference/clients-sdks
- Polymarket fees: https://docs.polymarket.com/trading/fees
- Polymarket market WebSocket: https://docs.polymarket.com/market-data/websocket/market-channel
- Polymarket geoblock docs: https://docs.polymarket.com/api-reference/geoblock
- Polymarket rate limits: https://docs.polymarket.com/api-reference/rate-limits
