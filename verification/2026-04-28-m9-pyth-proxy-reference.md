# M9 Pyth Proxy Reference Feed Verification

Date: 2026-04-28
Branch: `m9/reference-feed-auth`

## Scope

This note records a temporary M9 paper/replay validation path that uses Pyth Hermes as a proxy reference feed while authorized Chainlink Data Streams access remains unavailable.

This is not live-trading scope and not final settlement-source evidence:

- No live Polymarket orders were added.
- No wallet, signing, private-key, trading API-key, or Chainlink credential handling was added.
- Current sampled BTC/ETH/SOL Polymarket 15-minute markets still cite Chainlink Data Streams as their resolution source.
- Pyth proxy sessions are labeled `live_readiness_evidence=false` and `settlement_reference_evidence=false`.

## Pyth Source

Public unauthenticated Hermes endpoint used for temporary proxy validation:

```text
https://hermes.pyth.network/v2/updates/price/latest
```

Configured IDs:

| Asset | Pyth price ID |
| --- | --- |
| BTC/USD | `0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43` |
| ETH/USD | `0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace` |
| SOL/USD | `0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d` |

Sources checked:

- `https://docs.pyth.network/price-feeds/core/price-feeds/price-feed-ids`
- `https://docs.pyth.network/price-feeds/core/fetch-price-updates`
- `https://docs.pyth.network/price-feeds/core/how-pyth-works/hermes`
- `https://docs.pyth.network/price-feeds/core/api-instances-and-providers/hermes`
- `https://docs.pyth.network/price-feeds/core/best-practices`
- `https://pythdata.app`

## Implementation Summary

- Added disabled-by-default `[reference_feed]` config.
- Added explicit opt-in config at `config/pyth-proxy.example.toml`.
- Added read-only `PythHermesClient` and parser for latest Hermes prices.
- Converted Pyth fixed-point `price` and `conf` using `expo`.
- Enforced `publish_time` freshness through `reference_feed.max_staleness_ms`.
- Persisted Hermes raw payloads and normalized `ReferenceTick` events under local file-backed sessions.
- Tagged proxy ticks with:
  - `provider = "pyth"`
  - `matches_market_resolution_source = false`
  - `source = <asset Chainlink resolution URL>` so the existing signal engine can evaluate proxy sessions without changing current market eligibility rules.
- Added report metadata:
  - `reference_feed_mode = "pyth_proxy"`
  - `reference_provider = "pyth"`
  - `matches_market_resolution_source = false`
  - `live_readiness_evidence = false`
  - `settlement_reference_evidence = false`
- Fixed live runtime snapshot wiring so Gamma market IDs can read CLOB order books keyed by condition ID.

## Runtime Evidence

Command:

```text
cargo run -- --config config/pyth-proxy.example.toml paper --run-id m9-pyth-proxy-smoke-20260428c --feed-message-limit 1 --cycles 1
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-smoke-20260428c
```

Result:

| Field | Value |
| --- | --- |
| Session dir | `reports/sessions/m9-pyth-proxy-smoke-20260428c` |
| Raw messages | 12 |
| Normalized events | 21 total rows: 18 feed/reference events plus 3 market-discovery lifecycle events |
| Reference ticks | 3 |
| Signal evaluations | 9 |
| Missing-reference skips | 6 early evaluations before proxy ticks arrived |
| Post-reference skips | 3 `stale_book` fail-closed decisions |
| Paper orders/fills | 0 / 0 |
| Replay status | deterministic |
| Replay/report fingerprint | `sha256:45b10220dcad3cdecf428f53a8d57cdc1b078583a30aade83df3484e471f4ba3` |

Interpretation:

- BTC/ETH/SOL Pyth proxy `ReferenceTick`s were persisted and replayed deterministically.
- Signal evaluation no longer failed entirely on `missing_reference_price`; after proxy reference ticks arrived, evaluation proceeded to the next fail-closed gate.
- The bounded smoke generated no paper orders because live books were stale under the current 1-second book freshness risk threshold by the time the proxy reference ticks were recorded.
- This validates reference-feed plumbing and deterministic replay behavior, not final strategy profitability or settlement-source correctness.

## Self-Verification Addendum

Additional bounded check on 2026-04-28:

```text
cargo run -- --config config/pyth-proxy.example.toml paper --run-id m9-pyth-proxy-self-verify-20260428a --feed-message-limit 1 --cycles 1
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-self-verify-20260428a
```

Result:

- Startup labels: `reference_feed_mode=pyth_proxy`, `reference_provider=pyth`, `settlement_reference_evidence=false`, `live_readiness_evidence=false`.
- Session dir: `reports/sessions/m9-pyth-proxy-self-verify-20260428a`.
- Captured: 12 raw messages, 21 normalized-event rows, 3 Pyth proxy `reference_tick`s, 0 paper orders, 0 fills, 0.000000 total P&L.
- Replay: deterministic with fingerprint `sha256:f05385206b87f7a30986b34002060d2169a75e168d83cad8f8e005ee7a830b6a`.
- Replay labels: `reference_feed_mode=pyth_proxy`, `reference_provider=pyth`, `matches_market_resolution_source=false`, `live_readiness_evidence=false`, `settlement_reference_evidence=false`.

## Natural Proxy Session Addendum

Longer natural proxy check on 2026-04-28, using existing gates and no forced trades:

```text
cargo run -- --config config/pyth-proxy.example.toml paper --run-id m9-pyth-proxy-natural-20260428a --feed-message-limit 5 --cycles 10
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-natural-20260428a
```

Result:

- Startup labels: `reference_feed_mode=pyth_proxy`, `reference_provider=pyth`, `settlement_reference_evidence=false`, `live_readiness_evidence=false`.
- Session dir: `reports/sessions/m9-pyth-proxy-natural-20260428a`.
- Captured: 220 raw messages, 352 normalized-event rows, 3 market records, 30 Pyth proxy `reference_tick`s.
- Signals: 123 evaluated, 0 emitted order intents, 123 skipped.
- Skip reasons: `missing_reference_price=12`, `stale_book=30`, `stale_reference_price=81`.
- Risk approvals/rejections: 0 / 0 because no signal reached paper-intent risk evaluation.
- Paper orders/fills: 0 / 0.
- Fees paid / total P&L: `0.000000` / `0.000000`.
- Replay: deterministic with fingerprint `sha256:e87608380e016b801462d5b915abcb8950094d38a0a04a7998ccd1d50f6641da`.
- Replay labels: `evidence_type=pyth_proxy_live_ingestion`, `live_market_evidence=true`, `reference_feed_mode=pyth_proxy`, `reference_provider=pyth`, `matches_market_resolution_source=false`, `live_readiness_evidence=false`, `settlement_reference_evidence=false`.

Interpretation:

- Pyth proxy live ingestion/reference plumbing remains PROXY-PASS.
- Natural live/proxy paper trades are NOT EXERCISED for this run because no natural signal survived freshness/edge gates to create a risk-reviewed paper intent.
- This does not weaken or bypass default strategy/risk settings and does not provide settlement-source or live-readiness evidence.

## Verification

Local checks passed after implementation:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- validate --local-only --config config/default.toml
cargo run --offline -- validate --local-only --config config/pyth-proxy.example.toml
```

Follow-up M9 lifecycle/proxy verification on 2026-04-28 also passed:

```text
cargo test --offline
cargo run --offline -- replay --run-id m9-deterministic-paper-lifecycle-20260428a
cargo run --offline -- --config config/pyth-proxy.example.toml replay --run-id m9-pyth-proxy-natural-20260428a
git diff --check
```

Full offline suite result: 114 tests passed.

## Gate Language

- Deterministic paper lifecycle fixture: PASS when the offline fixture produces order/fill/P&L artifacts and deterministic replay.
- Pyth proxy live ingestion/reference plumbing: PROXY-PASS when Pyth proxy ticks persist and replay deterministically with `live_readiness_evidence=false` and `settlement_reference_evidence=false`.
- Natural live/proxy paper trades: PASS only if natural trades occur; otherwise NOT EXERCISED with report skip reasons.
- M9 final live-readiness / settlement-source validation: PARTIAL until authorized Chainlink Data Streams access is available and Chainlink-backed paper plus replay succeeds.
