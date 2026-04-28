# M9 Polymarket RTDS Chainlink Reference Feed Verification

Date: 2026-04-28
Branch: `main`

## Scope

This note records the M9 read-only reference-feed path using Polymarket RTDS Chainlink crypto prices.

This is still paper/replay validation only:

- No live Polymarket orders were added.
- No wallet, signing, private-key, trading API-key, or authenticated CLOB order-client path was added.
- No direct Chainlink credential handling was added.
- Live readiness remains disabled and final M9 live-readiness remains PARTIAL.

## Source

Polymarket RTDS docs identify:

- Endpoint: `wss://ws-live-data.polymarket.com`
- Topic: `crypto_prices_chainlink`
- Symbols: `btc/usd`, `eth/usd`, `sol/usd`
- Authentication: not required for the crypto Chainlink price stream
- Payload fields used by the bot: `symbol`, `timestamp`, and `value`

Implementation uses `polymarket_rtds_chainlink` as the first reference provider for M9 paper/replay validation. Direct authenticated Chainlink Data Streams remains only a fallback if RTDS is unavailable, delayed, insufficiently precise, or not accepted as settlement-source evidence.

## Implementation Summary

- Added disabled-by-default config support for `reference_feed.provider = "polymarket_rtds_chainlink"`.
- Added explicit opt-in config at `config/polymarket-rtds-chainlink.example.toml`.
- Added documented RTDS subscription builders for BTC/USD, ETH/USD, and SOL/USD.
- Added parser support for RTDS Chainlink messages and empty heartbeat frames.
- Persisted RTDS raw messages and normalized `ReferenceTick` events under file-backed sessions.
- Tagged normalized ticks with:
  - `provider = "polymarket_rtds_chainlink"`
  - `matches_market_resolution_source = true`
  - `source = <asset Chainlink resolution URL>` for compatibility with existing signal/risk resolution-source gates
- Added replay/report metadata:
  - `reference_feed_mode = "polymarket_rtds_chainlink"`
  - `reference_provider = "polymarket_rtds_chainlink"`
  - `matches_market_resolution_source = true`
  - `settlement_reference_evidence = true`
  - `live_readiness_evidence = false`

The provider/source label is Polymarket RTDS Chainlink, not direct Chainlink Data Streams. The `ReferencePrice.source` URL remains the market-resolution URL because the existing M5 gates intentionally require the tick to match each market's `resolution_source`.

## Runtime Evidence

Command:

```text
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml paper --run-id m9-rtds-chainlink-smoke-20260428b --feed-message-limit 5 --cycles 1
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml replay --run-id m9-rtds-chainlink-smoke-20260428b
```

Result:

| Field | Value |
| --- | --- |
| Session dir | `reports/sessions/m9-rtds-chainlink-smoke-20260428b` |
| Raw messages | 36 |
| Normalized events | 40 total rows |
| RTDS Chainlink reference ticks | 12 |
| Assets with RTDS ticks | BTC, ETH, SOL |
| Signal evaluations | 24 |
| Signal order intents | 0 |
| Skip reasons | `missing_reference_price=12`, `stale_book=12` |
| Risk approvals/rejections | 0 / 0 |
| Paper orders/fills | 0 / 0 |
| Fees paid / total P&L | `0.000000` / `0.000000` |
| Replay status | deterministic |
| Replay/report fingerprint | `sha256:2523c96dfd1f80901e2c402a6b454f66201c6c8232f3377f09e15b334b0ed575` |

Replay metadata:

```text
evidence_type=polymarket_rtds_chainlink_live_ingestion
live_market_evidence=true
reference_feed_mode=polymarket_rtds_chainlink
reference_provider=polymarket_rtds_chainlink
matches_market_resolution_source=true
settlement_reference_evidence=true
live_readiness_evidence=false
```

Interpretation:

- Polymarket RTDS Chainlink is reachable without private credentials from this environment.
- BTC/ETH/SOL `ReferenceTick`s are persisted and replay deterministically.
- Signal evaluation no longer fails entirely on `missing_reference_price`; after RTDS reference ticks arrive, evaluation proceeds to the next fail-closed gate.
- The bounded run produced 0 paper orders/fills because natural signals skipped on `stale_book` after reference ticks arrived. This is not a risk bypass and not a strategy-performance pass.

## Gate Language

- Polymarket RTDS Chainlink reference ingestion: PASS for read-only M9 reference plumbing and deterministic replay.
- Natural RTDS-backed paper trades: NOT EXERCISED for `m9-rtds-chainlink-smoke-20260428b` because no order intents reached risk approval.
- Final M9 live-readiness / settlement-source validation: PARTIAL until Chainlink-source paper sessions naturally exercise risk-reviewed paper behavior and final start/end settlement artifacts are verified.

Live trading remains disabled.
