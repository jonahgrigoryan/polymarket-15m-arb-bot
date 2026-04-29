# M9 Fresh RTDS Natural Paper Session

Date: 2026-04-29
Branch: `m9/rtds-natural-paper-validation`
Config: `config/polymarket-rtds-chainlink.example.toml`

## Scope

This run captured a fresh bounded Polymarket RTDS Chainlink-backed paper session after the condition-ID/Gamma-market-ID paper-path fix.

Safety boundaries held:

- No live order placement was added.
- No wallet, signing, private-key, trading API-key, or authenticated CLOB order-client path was added.
- `LIVE_ORDER_PLACEMENT_ENABLED=false` remained unchanged.
- Default signal/risk settings were unchanged.
- EV thresholds, freshness gates, and risk gates were not weakened.
- Paper orders were not forced, synthesized, seeded, or routed around signal/risk gates.

## Commands

```text
cargo run -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  paper --run-id m9-rtds-natural-20260429T021025Z-fresh \
  --feed-message-limit 10 --cycles 30

cargo run --offline -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  replay --run-id m9-rtds-natural-20260429T021025Z-fresh
```

## Session Result

| Run ID | Raw | Normalized | RTDS ticks | CLOB book events | Predictive ticks | Signal evals | Signal intents | Risk approvals | Risk rejections | Orders | Fills | P&L | Replay fingerprint |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `m9-rtds-natural-20260429T021025Z-fresh` | 1947 | 1655 | 365 | 700 | 570 | 1638 | 0 | 0 | 0 | 0 | 0 | 0.0 | `sha256:347e12676a7c0e36c01b2d5493c468366edd8a27f147354e324223f7f5ee25a3` |

RTDS reference tick split:

| Asset | RTDS ticks |
| --- | ---: |
| BTC | 118 |
| ETH | 125 |
| SOL | 122 |

CLOB book events count is `book_snapshot + best_bid_ask + book_delta`.

Report labels:

- `evidence_type=polymarket_rtds_chainlink_live_ingestion`
- `live_market_evidence=true`
- `reference_feed_mode=polymarket_rtds_chainlink`
- `reference_provider=polymarket_rtds_chainlink`
- `matches_market_resolution_source=true`
- `settlement_reference_evidence=true`
- `live_readiness_evidence=false`

## Skip Reasons

No signal intent reached risk. Risk approvals and rejections are both zero because all 1638 signal evaluations skipped before risk review.

Aggregate skip reason counts:

| Reason | Count |
| --- | ---: |
| `stale_reference_price` | 1023 |
| `stale_book` | 455 |
| `edge_below_minimum` | 180 |
| `maker_would_cross` | 120 |
| `missing_reference_price` | 70 |

Decision reason counts:

| Decision reason | Count |
| --- | ---: |
| `skip:stale_reference_price` | 1023 |
| `skip:stale_book` | 365 |
| `skip:missing_reference_price` | 70 |
| `skip:maker_would_cross,edge_below_minimum` | 60 |
| `skip:maker_would_cross,edge_below_minimum,stale_book` | 60 |
| `skip:edge_below_minimum` | 30 |
| `skip:edge_below_minimum,stale_book` | 30 |

## Replay Comparison

Replay status:

- `replay_status=deterministic`
- `replay_generated_paper_event_count=0`
- `replay_recorded_paper_event_count=0`
- `replay_determinism_fingerprint=sha256:347e12676a7c0e36c01b2d5493c468366edd8a27f147354e324223f7f5ee25a3`

Paper and replay report comparison:

| Field | Paper | Replay | Match |
| --- | ---: | ---: | --- |
| Orders | 0 | 0 | yes |
| Fills | 0 | 0 | yes |
| Total P&L | 0.0 | 0.0 | yes |
| Generated/recorded paper events | 0 | 0 | yes |

The paper and replay report files were byte-identical:

```text
347e12676a7c0e36c01b2d5493c468366edd8a27f147354e324223f7f5ee25a3  paper_report.json
347e12676a7c0e36c01b2d5493c468366edd8a27f147354e324223f7f5ee25a3  replay_report.json
```

## Verification Commands

Passed:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only
git diff --check
```

Focused safety scan:

```text
rg -n "\.post\(|\.put\(|\.delete\(" src
rg -n -i "private.?key|api.?key|wallet|order.?client|clob.?client|place.?order|submit.?order|create.?order|live.?trading|live.?order" src Cargo.toml config
```

Result:

- No `.post(`, `.put(`, or `.delete(` calls in `src`.
- Credential/live-order scan hits were limited to comments and `LIVE_ORDER_PLACEMENT_ENABLED=false` output/constant.

## Gate Result

- Polymarket RTDS Chainlink reference ingestion: PASS.
- Natural RTDS-backed paper trades: NOT EXERCISED.
- Reason: this fresh corrected-code run did not emit signal intents under unchanged gates, so no risk approval/rejection or paper order/fill could occur.
- Later root-cause review found this run selected pre-start `1777514400` markets almost 24 hours ahead of its execution window. See `verification/2026-04-29-m9-rtds-current-window-rootcause.md` for the current-window discovery/signal fix and clean natural paper order/fill evidence.
- Final M9 live-readiness: PARTIAL until natural risk-reviewed paper behavior and final start/end settlement artifacts are verified.
