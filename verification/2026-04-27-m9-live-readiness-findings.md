# M9 Live-Readiness Findings

Date: 2026-04-27
Branch: `m9/multi-session-validation`
Base commit: `610de7f`

## Scope

M9 remains validation and live-readiness review only. This patch wires real paper and replay runtime paths, but it does not add live order placement, signing, wallet/key handling, API-key handling, authenticated CLOB order clients, or live trading.

`paper` now runs a read-only file-backed paper session:

- validates config and geoblock,
- persists a config snapshot and run ID,
- discovers eligible BTC/ETH/SOL 15-minute markets,
- records raw and normalized read-only feed events,
- replays the stored session through state, signal, risk, and paper execution paths,
- records generated paper events when they exist,
- writes paper orders/fills/positions/balance/P&L artifacts and Prometheus metrics,
- flushes the session directory on graceful bounded completion.

`replay --run-id <id>` now loads a stored session from `reports/sessions/<run_id>`, rebuilds state offline, regenerates signal/risk/paper outputs, compares generated paper events against recorded paper events, writes a replay report, and fails on divergence.

## Gate Result

M9 is still PARTIAL.

The runtime capture/replay mechanics now pass with real live-read-only sessions, deterministic paper order/fill/P&L lifecycle evidence now passes through the same state, signal, risk, paper executor, position, replay, and reporting path, and Polymarket RTDS Chainlink reference ingestion now persists BTC/ETH/SOL reference ticks. Final live-readiness remains PARTIAL because natural RTDS-backed paper trades and final start/end settlement artifacts have not yet been verified.

| Gate item | Status | Evidence / decision |
| --- | --- | --- |
| At least one captured paper session covers BTC, ETH, and SOL and can be replayed | PASS for capture/replay mechanics | Runtime run `m9-runtime-smoke-20260427b` selected one BTC, ETH, and SOL market, captured live read-only feeds, persisted a file-backed session, and replayed deterministically. |
| Replay determinism passes for selected sessions | PASS | `replay --run-id m9-runtime-smoke-20260427b` produced matching report fingerprint `sha256:f1446dc2b3a6bb4862df7cfd9c9cd6b5629655ff5869dc1ee227153d4b5b7d60`. |
| Reports identify whether strategy performance survives fees and conservative fills | PARTIAL | Pyth and RTDS natural runs produced 0 order intents/fills under unchanged gates; fee/P&L behavior remains covered by storage-backed/file-backed fixture tests but not by natural reference-backed runtime trades. |
| Live-readiness blockers are listed before real orders | PASS | Blockers remain listed below. |
| Live trading remains disabled | PASS | `LIVE_ORDER_PLACEMENT_ENABLED=false`; no live order, signing, wallet/key, API-key, authenticated CLOB order-client, or live-trading path exists. |
| Deterministic paper lifecycle fixture | PASS | File-backed fixture run `m9-deterministic-paper-lifecycle-20260428a` produced 1 risk-approved taker order, 1 fill, position/balance/P&L artifacts, matching generated-vs-recorded paper events, and deterministic replay fingerprint `sha256:29412f5cae3d50b892f420ad3b3a2a9a27cd878e343ac5fe16d8dc2635aa6a6a`. |
| Natural live/proxy paper trades | NOT EXERCISED | Natural Pyth proxy run `m9-pyth-proxy-natural-20260428a` produced 0 orders/fills. Report skip reasons were `missing_reference_price=12`, `stale_book=30`, and `stale_reference_price=81`. |
| Polymarket RTDS Chainlink reference ingestion | PASS for reference plumbing | Opt-in run `m9-rtds-chainlink-smoke-20260428b` persisted 12 BTC/ETH/SOL RTDS Chainlink `ReferenceTick`s and replayed deterministically with fingerprint `sha256:2523c96dfd1f80901e2c402a6b454f66201c6c8232f3377f09e15b334b0ed575`; `live_readiness_evidence=false`. |
| Natural RTDS-backed paper trades | NOT EXERCISED | The same RTDS run produced 0 orders/fills because signals skipped `missing_reference_price=12` before reference ticks and `stale_book=12` after reference ticks. |

## Runtime Session Evidence

Command:

```text
cargo run -- --config config/default.toml paper --run-id m9-runtime-smoke-20260427b --feed-message-limit 1 --cycles 1
cargo run --offline -- --config config/default.toml replay --run-id m9-runtime-smoke-20260427b
```

Session directory:

```text
reports/sessions/m9-runtime-smoke-20260427b
```

Captured files:

```text
config_snapshot.json
markets.jsonl
normalized_events.jsonl
paper_balances.jsonl
paper_fills.jsonl
paper_metrics.prom
paper_orders.jsonl
paper_positions.jsonl
paper_report.json
raw_messages.jsonl
replay_metrics.prom
replay_report.json
risk_events.jsonl
```

Session counts:

| Artifact | Count |
| --- | ---: |
| Selected markets | 3 |
| Raw messages | 11 |
| Normalized events | 18 |
| Market records | 3 |
| Paper balances | 1 |
| Paper orders | 0 |
| Paper fills | 0 |
| Paper positions | 0 |
| Risk events | 0 |

Report summary from `replay_report.json`:

| Field | Value |
| --- | --- |
| Event types | 12 `book_snapshot`, 3 `market_discovered`, 3 `predictive_tick` |
| Signal evaluations | 6 |
| Order intents | 0 |
| Signal skips | 6 `missing_reference_price` |
| Signal counts by asset | BTC=2, ETH=2, SOL=2 |
| Paper orders/fills | 0 / 0 |
| Total paper P&L | 0.000000 |
| Replay fingerprint | `sha256:f1446dc2b3a6bb4862df7cfd9c9cd6b5629655ff5869dc1ee227153d4b5b7d60` |

Interpretation:

- The runtime path is no longer a placeholder.
- It captures and persists live read-only market/feed data.
- Replay is deterministic from the stored session.
- The strategy correctly refuses to emit a paper order when the settlement/reference source price is missing.
- This is not yet a strategy-performance pass because no real paper order/fill/P&L path was exercised by live reference-backed data.

## Fixture Evidence

M9 also keeps storage-backed fixture tests that exercise paper order/fill/P&L behavior across BTC, ETH, and SOL:

```text
cargo test --offline m9_storage_backed_fixture_sessions_replay_for_default_assets --lib -- --nocapture
cargo test --offline m9_storage_backed_fixture_paper_event_determinism_fails_when_recorded_event_is_missing --lib
```

Fixture result:

| Asset | Run ID | Report fingerprint | Paper-event fingerprint | Fills | Fees paid | Total P&L |
| --- | --- | --- | --- | ---: | ---: | ---: |
| BTC | `m9-btc-captured-paper-fixture` | `sha256:5d902f0a82481f8f7482247c71ccb2fbd482945c0255054ab1c0741338f9ffb5` | `sha256:b96ea689336f413c0c9e21aae4cdf31c2b3908ede82064b335f2f6849170f3d8` | 1 | 0.200000 | -0.250000 |
| ETH | `m9-eth-captured-paper-fixture` | `sha256:e3544a62b85c3619a455d8ebb18b48a3c68ea18d33c82467e3550d317a3325dc` | `sha256:b24c0089378088ba98b23ae508eab794c2a9b8723f87640d442dce80b69a8f96` | 1 | 0.200000 | -0.250000 |
| SOL | `m9-sol-captured-paper-fixture` | `sha256:2f36b64fa6a854af2f61e37dcb63fa5f9e38745b26db7052eb6307bb71005c37` | `sha256:1bd0a4533fc30d8c4f5c2c15526bd4d5638814cd0078297ae7d4cba0959e762e` | 1 | 0.200000 | -0.250000 |

These fixtures prove deterministic replay and paper accounting, but they are not a substitute for real runtime sessions with verified resolution/reference ticks.

### File-Backed Lifecycle Fixture Addendum

Follow-up fixture evidence on 2026-04-28:

```text
cargo run --offline -- paper --run-id m9-deterministic-paper-lifecycle-20260428a --deterministic-fixture
cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a
```

Result:

- Labels: `evidence_type=deterministic_fixture`, `live_market_evidence=false`, `live_readiness_evidence=false`, `settlement_reference_evidence=false`.
- Artifact files: config snapshot, raw/normalized event JSONL, market JSONL, paper order/fill/position/balance/risk JSONL, paper report, replay report, and metrics under `reports/sessions/m9-deterministic-paper-lifecycle-20260428a`.
- Counts: 1 paper order, 1 fill, filled notional `5.100000`, fees paid `0.200000`, total P&L `-0.250000`.
- Generated-vs-recorded paper events matched with fingerprint `sha256:5100fdb817c179770ca91b5691cb36813c0333c7e712dc41b023ac7143a0cbfb`.
- Replay was deterministic with fingerprint `sha256:29412f5cae3d50b892f420ad3b3a2a9a27cd878e343ac5fe16d8dc2635aa6a6a`.

See `verification/2026-04-28-m9-paper-lifecycle-fixture.md`.

## Live-Readiness Blockers Before Real Orders

Real orders remain blocked by design until a separate live-beta PRD and explicit release gate exist.

### Reference Feed Access Recheck

Follow-up verification on 2026-04-28 confirmed that the missing live reference price is an external access blocker, not a remaining runtime stub.

- Current captured BTC/ETH/SOL markets cite Chainlink Data Streams pages as their resolution source.
- The corresponding Chainlink streams are BTC/USD, ETH/USD, and SOL/USD reference-price Data Streams.
- Chainlink's real-time Data Streams REST and WebSocket APIs require authenticated access headers.
- An unauthenticated REST probe against the BTC feed ID returned missing `Headers.UserId`, `Headers.Timestamp`, and `Headers.HmacSignature`.
- The public `data.chain.link` pages are delayed informational pages and must not be used as the paper strategy's settlement-reference feed.

See `verification/2026-04-28-reference-feed-access.md`.

### Temporary Pyth Proxy Recheck

Follow-up implementation on 2026-04-28 added a Pyth Hermes proxy reference feed for paper/replay testing only.

- Pyth proxy mode is disabled by default and requires explicit config opt-in.
- Pyth proxy sessions are labeled `reference_feed_mode=pyth_proxy`, `reference_provider=pyth`, `matches_market_resolution_source=false`, `live_readiness_evidence=false`, and `settlement_reference_evidence=false`.
- Bounded proxy run `m9-pyth-proxy-smoke-20260428c` persisted BTC/ETH/SOL proxy `ReferenceTick`s and replayed deterministically.
- Natural proxy run `m9-pyth-proxy-natural-20260428a` persisted 30 proxy `ReferenceTick`s across 10 cycles and replayed deterministically, but produced 0 paper orders/fills because signals skipped on `missing_reference_price`, `stale_book`, and `stale_reference_price`.
- The proxy run validates paper runtime/reference plumbing, not Polymarket settlement-source correctness.

See `verification/2026-04-28-m9-pyth-proxy-reference.md`.

### Polymarket RTDS Chainlink Recheck

Follow-up implementation on 2026-04-28 added Polymarket RTDS Chainlink as the first reference provider for paper/replay validation.

- RTDS mode is disabled by default and requires explicit config opt-in at `config/polymarket-rtds-chainlink.example.toml`.
- The provider subscribes read-only to `crypto_prices_chainlink` for `btc/usd`, `eth/usd`, and `sol/usd`.
- RTDS sessions are labeled `reference_feed_mode=polymarket_rtds_chainlink`, `reference_provider=polymarket_rtds_chainlink`, `matches_market_resolution_source=true`, `settlement_reference_evidence=true`, and `live_readiness_evidence=false`.
- Bounded run `m9-rtds-chainlink-smoke-20260428b` persisted 12 RTDS Chainlink `ReferenceTick`s across BTC/ETH/SOL and replayed deterministically.
- The run produced 0 paper orders/fills because no natural signal survived unchanged freshness gates; after reference ticks arrived the skip reason moved to `stale_book`.
- Direct authenticated Chainlink Data Streams credentials remain a fallback only if Polymarket RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.

See `verification/2026-04-28-m9-polymarket-rtds-chainlink-reference.md`.

Required blockers before any real-order phase:

- Separate live-beta PRD and explicit user approval.
- Legal/access review for deployment jurisdiction and operator.
- Deployment-host geoblock verification; trading-capable modes must fail closed on blocked, malformed, or unreachable geoblock checks.
- Additional Polymarket RTDS Chainlink-backed paper sessions where unchanged runtime freshness permits risk-reviewed paper intents, or direct Chainlink Data Streams fallback if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.
- Explicitly approved credential-handling scope before any authenticated direct Data Streams ingestion is implemented.
- Real BTC, ETH, and SOL paper sessions where reference ticks allow signal/risk decisions to create or reject paper intents.
- Offline replay of those real sessions with generated-vs-recorded paper-event determinism.
- Final start/end settlement artifact verification for paper P&L/reporting reconciliation.
- API section 6 before live beta: signing/auth/order-create/order-post/order-cancel and current CLOB V2 fields.
- Key management and wallet custody design.
- Signing audit, including current V2 signing rules after cutover.
- Rate-limit and order-endpoint behavior verification for live-beta throughput assumptions.
- Current production CLOB endpoint recheck after the April 28, 2026 V2 cutover window.

## Final Verification

Local gates:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- validate --local-only --config config/default.toml
```

Result: PASS. Full offline suite result was 105 tests passed.

Follow-up M9 lifecycle/proxy check on 2026-04-28: PASS. Full offline suite result was 114 tests passed. Additional passed checks:

```text
cargo run --offline -- validate --local-only --config config/pyth-proxy.example.toml
cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-natural-20260428a
git diff --check
```

Runtime gates:

```text
cargo run -- --config config/default.toml paper --run-id m9-runtime-smoke-20260427b --feed-message-limit 1 --cycles 1
cargo run --offline -- --config config/default.toml replay --run-id m9-runtime-smoke-20260427b
```

Result: PASS for runtime capture/replay mechanics.

Follow-up M9 RTDS Chainlink check on 2026-04-28: PASS for reference plumbing. Full offline suite result after RTDS implementation was 120 tests passed. Additional passed checks:

```text
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml paper --run-id m9-rtds-chainlink-smoke-20260428b --feed-message-limit 5 --cycles 1
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml replay --run-id m9-rtds-chainlink-smoke-20260428b
```

Result: PASS for RTDS Chainlink reference ingestion and deterministic replay; NOT EXERCISED for natural paper trades.

Safety scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Result: PASS for the no-live-order boundary. Source hits are paper-only lifecycle names, disabled safety flags, and config/documentation references; no live order placement, signing, wallet/key handling, API-key handling, authenticated CLOB order-client, or live-trading path was added.
