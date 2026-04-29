# M9 RTDS Current-Window Root Cause And Paper Evidence

Date: 2026-04-29
Branch: `m9/rtds-natural-paper-validation`
Config: `config/polymarket-rtds-chainlink.example.toml`

## Root Cause

The zero-paper-trade run `m9-rtds-natural-20260429T021025Z-fresh` did not fail because Chainlink RTDS, risk, or paper execution was unreachable. It selected future 15-minute markets:

| Asset | Market ID | Slug | Selected window |
| --- | --- | --- | --- |
| BTC | `2111569` | `btc-updown-15m-1777514400` | 2026-04-30 02:00-02:15 UTC |
| ETH | `2111574` | `eth-updown-15m-1777514400` | 2026-04-30 02:00-02:15 UTC |
| SOL | `2111575` | `sol-updown-15m-1777514400` | 2026-04-30 02:00-02:15 UTC |

That run executed from 2026-04-29 02:10:42 UTC to 2026-04-29 02:40:27 UTC, so the selected markets were almost 24 hours pre-start. The runtime was treating Polymarket `active=true` / `acceptingOrders=true` as equivalent to the current 15-minute reference window. For these markets, `active` means open for orders, not that the market's slug interval has started.

The signal engine also failed to hard-skip pre-start markets: `now.saturating_sub(market.start_ts)` classified pre-start markets as `opening`.

## Fix

- Gamma discovery now asks for active, non-closed markets bounded by near-term `endDate` and ordered ascending by `endDate`, so current intervals are visible instead of far-future open-for-order intervals.
- Paper runtime now selects only markets where `market.start_ts <= now_wall_ts < market.end_ts`.
- Signal evaluation now emits hard skip `market_not_started` for pre-start markets.
- Paper generated-vs-recorded event fingerprinting now canonicalizes JSON float round trips before hashing, matching file-session replay semantics.

No live order placement, wallet/signing/API-key code, EV-threshold reduction, risk bypass, or forced paper order path was added.

## Live Read-Only API Check

Official docs used:

- `https://docs.polymarket.com/api-reference/markets/list-markets-keyset-pagination`
- `https://docs.polymarket.com/api-reference/introduction`

Read-only check against Gamma keyset with `end_date_min`, `end_date_max`, `order=endDate`, and `ascending=true` returned current interval markets including:

```text
eth-updown-15m-1777431600  2026-04-29T03:15:00Z
sol-updown-15m-1777431600  2026-04-29T03:15:00Z
btc-updown-15m-1777431600  2026-04-29T03:15:00Z
```

## Clean Paper Evidence

Run:

```text
cargo run -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  paper --run-id m9-rtds-current-window-rootcause-20260429T0312Z \
  --feed-message-limit 3 --cycles 1

cargo run --offline -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  replay --run-id m9-rtds-current-window-rootcause-20260429T0312Z
```

Selected current-window markets:

| Asset | Market ID | Slug | Window |
| --- | --- | --- | --- |
| BTC | `2101649` | `btc-updown-15m-1777431600` | 2026-04-29 03:00-03:15 UTC |
| ETH | `2101638` | `eth-updown-15m-1777431600` | 2026-04-29 03:00-03:15 UTC |
| SOL | `2101644` | `sol-updown-15m-1777431600` | 2026-04-29 03:00-03:15 UTC |

Result:

| Run ID | Report events | RTDS ticks | Predictive ticks | Signal evals | Signal intents | Risk approvals | Orders | Fills | Filled notional | Fees | Total P&L | Fingerprint |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `m9-rtds-current-window-rootcause-20260429T0312Z` | 42 | 6 | 5 | 40 | 1 | 1 | 1 | 1 | 0.200000 | 0.014112 | -0.064112 | `sha256:4b77dc55d8aef8ac96b704186308f21ff531c220a47437614cf6441bb450bffd` |

Paper and replay reports are byte-identical:

```text
4b77dc55d8aef8ac96b704186308f21ff531c220a47437614cf6441bb450bffd  paper_report.json
4b77dc55d8aef8ac96b704186308f21ff531c220a47437614cf6441bb450bffd  replay_report.json
```

## Startup-Log Confirmed Rerun

The paper startup log now prints each selected market's asset, market ID, slug, start/end UTC, and selection `now` UTC before the first paper cycle.

Run:

```text
cargo run -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  paper --run-id m9-rtds-current-window-startuplog-20260429T035356Z \
  --feed-message-limit 3 --cycles 1

cargo run --offline -- \
  --config config/polymarket-rtds-chainlink.example.toml \
  replay --run-id m9-rtds-current-window-startuplog-20260429T035356Z
```

Startup market selection:

| Asset | Market ID | Slug | Start UTC | End UTC | Selection now UTC | Acceptance |
| --- | --- | --- | --- | --- | --- | --- |
| BTC | `2101765` | `btc-updown-15m-1777434300` | 2026-04-29T03:45:00Z | 2026-04-29T04:00:00Z | 2026-04-29T03:54:19.245Z | `start <= now < end` |
| ETH | `2101771` | `eth-updown-15m-1777434300` | 2026-04-29T03:45:00Z | 2026-04-29T04:00:00Z | 2026-04-29T03:54:19.245Z | `start <= now < end` |
| SOL | `2101772` | `sol-updown-15m-1777434300` | 2026-04-29T03:45:00Z | 2026-04-29T04:00:00Z | 2026-04-29T03:54:19.245Z | `start <= now < end` |

Result:

| Run ID | Raw | Normalized | RTDS ticks | Predictive ticks | Signal evals | Signal intents | Risk approvals | Orders | Fills | Filled notional | Fees | Total P&L | Fingerprint |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `m9-rtds-current-window-startuplog-20260429T035356Z` | 30 | 52 | 6 | 5 | 40 | 6 | 6 | 6 | 6 | 3.468000 | 0.226100 | -0.472100 | `sha256:2f07ba8506838e846f0f6b3ab29629c70346ee002da24a22c29ae895956bacf3` |

Skip reasons:

| Reason | Count |
| --- | ---: |
| `missing_reference_price` | 28 |
| `stale_book` | 6 |

Replay status:

- `replay_status=deterministic`
- `replay_generated_paper_event_count=12`
- `replay_recorded_paper_event_count=12`
- Paper and replay reports are byte-identical:

```text
2f07ba8506838e846f0f6b3ab29629c70346ee002da24a22c29ae895956bacf3  paper_report.json
2f07ba8506838e846f0f6b3ab29629c70346ee002da24a22c29ae895956bacf3  replay_report.json
```

Verification passed after adding the startup market-window log:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only
git diff --check
```

Focused safety scan stayed clean: no source `.post(`, `.put(`, or `.delete(` calls; credential/live-order scan hits were limited to comments and `LIVE_ORDER_PLACEMENT_ENABLED=false` output/constant.

## Gate Result

- Polymarket RTDS Chainlink reference ingestion: PASS.
- Natural RTDS-backed paper trades: PASS for risk-approved paper order/fill under unchanged gates.
- Final M9 live-readiness: PARTIAL until final start/end settlement artifacts and post-market reconciliation evidence are captured.
