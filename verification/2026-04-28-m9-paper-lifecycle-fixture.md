# M9 Deterministic Paper Lifecycle Fixture Evidence

Date: 2026-04-28
Branch: `m9/reference-feed-auth`

## Scope

This note records deterministic fixture-backed paper order/fill lifecycle evidence. The fixture is offline and synthetic, and it exists to prove the local M4/M5/M6/M7 runtime path can produce auditable paper order, fill, position, balance, P&L, report, and replay artifacts without weakening live strategy or risk gates.

This is not live-market or settlement-source evidence:

- `evidence_type=deterministic_fixture`
- `live_market_evidence=false`
- `live_readiness_evidence=false`
- `settlement_reference_evidence=false`

No live Polymarket orders, signing, wallet/key handling, trading API-key handling, authenticated order-client code, or Chainlink credential handling were added.

## Implementation Summary

Added an explicit offline fixture path:

```text
cargo run --offline -- paper --run-id <run_id> --deterministic-fixture
```

The fixture writes a normal file-backed session under `reports/sessions/<run_id>`:

- `config_snapshot.json`
- `raw_messages.jsonl`
- `normalized_events.jsonl`
- `markets.jsonl`
- `paper_orders.jsonl`
- `paper_fills.jsonl`
- `paper_positions.jsonl`
- `paper_balances.jsonl`
- `risk_events.jsonl`
- `paper_report.json`
- `replay_report.json`
- Prometheus metrics files

The fixture input is a controlled BTC 15-minute up/down market with:

- Fresh asset-matched Chainlink resolution-source `ReferenceTick`
- Fresh Binance `PredictiveTick`
- Fresh executable CLOB `BookSnapshot`s
- Default strategy/risk config
- A maker path that would cross, causing the real signal engine to choose the taker path

The stored fixture is replayed through:

- `StateStore`
- `SignalEngine`
- `RiskEngine`
- `PaperExecutor`
- `PaperPositionBook` and P&L
- `ReplayEngine`
- Reporting and generated-vs-recorded paper-event comparison

## Evidence Run

Commands:

```text
cargo run --offline -- paper --run-id m9-deterministic-paper-lifecycle-20260428a --deterministic-fixture
cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a
```

Result:

| Field | Value |
| --- | --- |
| Run ID | `m9-deterministic-paper-lifecycle-20260428a` |
| Session dir | `reports/sessions/m9-deterministic-paper-lifecycle-20260428a` |
| Input fixture events | 6 |
| Recorded paper events | 2 |
| Market records | 1 |
| Paper orders | 1 |
| Paper fills | 1 |
| Filled notional | `5.100000` |
| Fees paid | `0.200000` |
| Realized P&L | `-0.200000` |
| Unrealized P&L | `-0.050000` |
| Total P&L | `-0.250000` |
| Paper event fingerprint | `sha256:5100fdb817c179770ca91b5691cb36813c0333c7e712dc41b023ac7143a0cbfb` |
| Replay determinism fingerprint | `sha256:29412f5cae3d50b892f420ad3b3a2a9a27cd878e343ac5fe16d8dc2635aa6a6a` |
| Replay status | deterministic |
| Generated-vs-recorded paper events | match |

Report summary from `replay_report.json`:

- Event rows: 8 total after generated paper events were recorded.
- Signal evaluations: 5.
- Emitted order intents: 1.
- Risk approvals/rejections: 1 / 0.
- Paper orders/fills: 1 / 1.
- P&L total: `-0.250000`, so the fixture result does not survive fees plus conservative mark.

## Gate Language

- Deterministic paper lifecycle fixture: PASS.
- Pyth proxy live ingestion/reference plumbing: separate PROXY-PASS evidence remains in `verification/2026-04-28-m9-pyth-proxy-reference.md`.
- Natural live/proxy paper trades: PASS only if natural trades occur; otherwise NOT EXERCISED with skip reasons.
- Final M9 live-readiness / settlement-source validation: PARTIAL until authorized Chainlink Data Streams access is available and Chainlink-backed paper plus replay succeeds.

## Verification

Latest local checks passed after this fixture work:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- validate --local-only --config config/default.toml
cargo run --offline -- validate --local-only --config config/pyth-proxy.example.toml
cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-natural-20260428a
git diff --check
```

Full offline suite result: 114 tests passed.
