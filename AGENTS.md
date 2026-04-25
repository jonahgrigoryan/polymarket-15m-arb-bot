# Project Instructions: Polymarket 15m Arbitrage Bot

These instructions apply to this project folder and augment the parent `AGENTS.md`.

## Project Goal

Build the system described in `PRD.md`: a replay-first and paper-trading-first Rust/Tokio system for BTC, ETH, and SOL 15-minute Polymarket up/down markets.

Optimize for correctness, auditability, deterministic replay, and risk control before live speed.

## Hard Boundaries

- Do not add live order placement unless the user explicitly requests a separate live-trading phase.
- Do not implement or suggest geoblock bypassing.
- Treat Polymarket access, fees, APIs, and CLOB V2 behavior as time-sensitive; verify against official docs or live read-only endpoints before relying on assumptions.
- Do not use unaudited third-party signing, wallet, or order-routing code in the hot path.
- Keep BTC, ETH, and SOL as the only default assets unless the user expands scope.

## Stack Defaults

- Hot path: Rust + Tokio.
- Live/replay engine: single Rust binary where practical.
- Research and notebooks: Python is acceptable outside the hot path.
- Tick/replay data: ClickHouse or QuestDB.
- Relational state: Postgres.
- Runtime: `systemd` on Linux for production-like runs.
- Observability: structured `tracing`, Prometheus, and Grafana.

## Implementation Principles

- Prefer small, explicit modules over speculative frameworks.
- Build paper/replay behavior before real execution behavior.
- Model fees, latency buffers, stale feeds, and adverse selection explicitly.
- Persist raw feed messages and normalized events so decisions can be replayed and audited.
- Every simulated order should have a logged reason: placed, skipped, canceled, filled, or halted.
- Use REST for startup/recovery/metadata and WebSockets for live market data.

## Testing Requirements

- Add unit tests for fee math, fair-probability calculations, order book updates, paper fills, and risk halts.
- Add integration tests for read-only market discovery and WebSocket ingestion when feasible.
- Replay tests must be deterministic for identical input data and config.
- Failure tests should cover stale feeds, WebSocket disconnects, bad metadata, rate limits, storage issues, and geoblock-blocked responses.

## Safety And Compliance

- Startup should include a geoblock/compliance check before any trading-capable mode.
- MVP must remain paper-only.
- Future live trading requires a new PRD or explicit release gate with legal/access review, key management, order signing audit, and live risk limits.

## Working Style

- Read `PRD.md` before making architectural changes.
- Keep changes surgical and tied to the PRD.
- If API behavior is uncertain, verify rather than guessing.
- If implementation choices materially affect latency, correctness, or compliance, surface the tradeoff before coding.
