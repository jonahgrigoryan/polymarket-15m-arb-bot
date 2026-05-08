# Project Instructions: Polymarket 15m Arbitrage Bot

These instructions apply to this project folder and augment the parent `AGENTS.md`.

## Project Goal

Build the system described in `PRD.md`: a replay-first and paper-trading-first Rust/Tokio system for BTC, ETH, and SOL 15-minute Polymarket up/down markets.

Optimize for correctness, auditability, deterministic replay, and risk control before live speed.

## Hard Boundaries

- Gated live order placement is allowed only under `LIVE_ALPHA_PRD.md` and `LIVE_ALPHA_IMPLEMENTATION_PLAN.md` (phase order, hold points, verification, compile-time features off-by-default such as `live-alpha-orders`, runtime gates, approval artifacts)—not by bypassing those gates or flipping `LIVE_ORDER_PLACEMENT_ENABLED` alone.
- Do not implement or suggest geoblock bypassing.
- Treat Polymarket access, fees, APIs, and CLOB V2 behavior as time-sensitive; verify against official docs or live read-only endpoints before relying on assumptions.
- Do not use unaudited third-party signing, wallet, or order-routing code in the hot path; live-path secrets and signing follow Live Alpha discipline and documented approval.
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
- MVP default is non-live (fail-closed): live placement requires explicit Live Alpha phase gates and approvals per `LIVE_ALPHA_*` docs, not ad hoc toggles.
- Any live execution continues to require legal/access review, key management, order-signing audit, and live risk limits as documented in those gates.

## Working Style

- Read `PRD.md` before making architectural changes.
- Keep changes surgical and tied to the PRD.
- If API behavior is uncertain, verify rather than guessing.
- If implementation choices materially affect latency, correctness, or compliance, surface the tradeoff before coding.

## Review Guidelines

- Treat any weakening of live gates, approval gates, feature gates, geoblock checks, heartbeat freshness, authenticated readback, journal integrity, reconciliation, risk limits, stale-data handling, kill switches, or secret handling as high priority.
- Treat any live-order, cancel, signing, wallet, or API-secret behavior that is not explicitly authorized by the current Live Alpha phase as high priority.
- Treat scope drift into the next milestone as a review finding. The current phase and stop point are defined by `STATUS.md` plus the relevant `LIVE_ALPHA_*` docs and dated verification note.
- Treat missing or insufficient regression tests for live-path safety changes as a review finding.

## Codex Autofix Guidelines

When an automated Codex workflow asks for P1/P2 review fixes:

- Read `AGENTS.md`, `STATUS.md`, and the relevant implementation and verification docs before editing.
- Address only the triggering P1/P2 review finding on the current PR branch unless the workflow prompt explicitly asks for more.
- Keep each change surgical and traceable to a review finding.
- Do not start the next milestone or broaden the PR scope.
- Do not weaken any safety, approval, feature, geoblock, heartbeat, readback, journal, reconciliation, risk, stale-data, kill-switch, or secret-handling gate.
- Add or update focused regression tests for each behavior fix.
- Run the narrow tests relevant to the changed files, then run `scripts/verify-pr.sh`.
- If `STATUS.md` or a dated verification note requires stricter phase-specific checks, run those too.
- If a review finding is incorrect, leave a concise PR comment explaining why instead of changing behavior.
