# M9 Natural RTDS Paper Validation Continuation

Date: 2026-04-28
Branch: `m9/rtds-natural-paper-validation`
Config: `config/polymarket-rtds-chainlink.example.toml`

## Scope

This continuation ran longer bounded Polymarket RTDS Chainlink-backed paper sessions without changing default signal/risk thresholds.

Safety boundaries held:

- No live order placement was added.
- No wallet, signing, private-key, trading API-key, or authenticated CLOB order-client path was added.
- `LIVE_ORDER_PLACEMENT_ENABLED=false` remained unchanged.
- EV thresholds, freshness gates, and risk gates were not weakened.
- Paper orders were not forced, synthesized, seeded, or routed around signal/risk gates.

## Endpoint Recheck

The endpoint result did not change from the earlier 2026-04-28 verification, so `API_VERIFICATION.md` did not need another edit.

- Official Polymarket docs list the CLOB API base as `https://clob.polymarket.com`.
- Live read-only check: `https://clob.polymarket.com/ok` returned `200` and body `"OK"`.
- Live read-only check: `https://clob-v2.polymarket.com/ok` returned `301`.
- The RTDS config already uses `clob_rest_url = "https://clob.polymarket.com"`.

## Runtime Capture Fix

Before the fix, longer feed-message runs exposed two runtime capture issues:

| Run ID | Command shape | Result | Raw | Normalized | Replay fingerprint |
| --- | --- | --- | ---: | ---: | --- |
| `m9-rtds-natural-20260428T155349Z-a` | `--feed-message-limit 20 --cycles 30` | Failed before cycle completion on stale RTDS BTC update: `age_ms=19270`, `max_staleness_ms=5000` | 68 | 130 | `sha256:71d744bb0d5263874b5043b47938076de0b7784dd6c681e1dc21b80b3008f45c` |
| `m9-rtds-natural-20260428T155849Z-b` | `--feed-message-limit 20 --cycles 30` | Interrupted; CLOB WebSocket capture stayed alive on heartbeat-only traffic before completing a cycle | 6 | 9 | `sha256:b559e3030a82a61db331a7b7908f659c1ef86609d9e0971fcbe976bb7781522a` |
| `m9-rtds-natural-20260428T160309Z-c` | `--feed-message-limit 8 --cycles 30` | Interrupted; same pre-fix heartbeat-only capture behavior | 6 | 9 | `sha256:0c3ef217324b3c8e2141b2365bd66d4ca77ef41596055edcf03b79fbb1959f2a` |

Fixes:

- `ReadOnlyWebSocketClient::connect_and_capture` now has a wall-clock capture deadline for heartbeat-only WebSocket sessions.
- Paper capture treats a quiet CLOB WebSocket as non-fatal when read-only CLOB REST book snapshots were already recorded for the cycle.
- Polymarket RTDS Chainlink stale updates are skipped inside the capture batch, but fresh BTC/ETH/SOL ticks are still required before a cycle can complete.

Tests added:

- `feed_ingestion::tests::websocket_capture_deadline_is_wall_clock_bounded`
- `tests::paper_capture_allows_quiet_clob_websocket_when_snapshots_are_recorded`
- `tests::stale_polymarket_rtds_updates_are_skipped_without_relaxing_other_errors`

These changes do not relax signal/risk gates. Stale RTDS updates above `reference_feed.max_staleness_ms = 5000` remain rejected; stale books above `risk.stale_book_ms = 1000` remain rejected.

## Completed Sessions

All completed continuation runs used unchanged default signal/risk settings.

| Run ID | Command | Raw | Normalized | RTDS ticks | CLOB book events | Predictive ticks | Signal evals | Signal intents | Risk approvals | Risk rejections | Orders | Fills | P&L | Replay fingerprint |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `m9-rtds-natural-20260428T160726Z-d` | `paper --run-id ... --feed-message-limit 8 --cycles 30` | 1635 | 1467 | 363 | 628 | 450 | 1444 | 0 | 0 | 0 | 0 | 0 | 0.0 | `sha256:1c7f7c8b0e81e8dfbe0783272603a8a0e4478ae0ec92cf3e4762bc657b1d4906` |
| `m9-rtds-natural-20260428T163455Z-e` | `paper --run-id ... --feed-message-limit 10 --cycles 30` | 1939 | 1612 | 366 | 656 | 570 | 1595 | 8 | 0 | 8 | 0 | 0 | 0.0 | `sha256:52b2bf0921c3610a9e87068a8c43a372ca3dc87919bd3382b62f089140677002` |

CLOB book events count `book_snapshot + best_bid_ask + book_delta`.

Skip and risk reasons:

| Run ID | Signal skip reasons | Risk result |
| --- | --- | --- |
| `m9-rtds-natural-20260428T160726Z-d` | `edge_below_minimum=180`, `maker_would_cross=120`, `missing_reference_price=46`, `stale_book=453`, `stale_reference_price=855` | No signal intent reached risk |
| `m9-rtds-natural-20260428T163455Z-e` | `edge_below_minimum=172`, `maker_would_cross=98`, `missing_reference_price=34`, `stale_book=456`, `stale_reference_price=1015` | `stale_book=8` risk rejections |

Pre-fix paper/replay comparison:

- `m9-rtds-natural-20260428T160726Z-d`: paper and replay matched on order count, fill count, P&L, and generated/recorded paper event count.
- `m9-rtds-natural-20260428T163455Z-e`: paper and replay matched on order count, fill count, P&L, and generated/recorded paper event count.
- Both pre-fix replays reported `replay_generated_paper_event_count=0` and `replay_recorded_paper_event_count=0`.

## Root Cause And Fix

The second completed run naturally produced 8 signal candidates with positive EV. The first diagnosis incorrectly stopped at `stale_book` risk rejections.

The actual root cause was a market identifier mismatch across the paper path:

- Gamma-discovered markets use Gamma market IDs such as `2107306`.
- CLOB book snapshots are keyed by condition IDs such as `0x780c1a19ded6cc5cb86aa39a862349e6375f358b795b101908d2d3ff539a26de`.
- Signal evaluation could use the condition-ID books through the `DecisionSnapshot`.
- Risk and paper execution still compared the candidate intent's Gamma market ID directly against condition-ID book freshness and book snapshots.

Fixes:

- Risk now accepts condition-ID book freshness when it belongs to the current Gamma market.
- Replay normalizes the matching condition-ID `TokenBookSnapshot` to the Gamma market ID before handing it to `PaperExecutor`.
- Open maker order fill simulation also maps condition-ID book events back to open Gamma-market paper orders.
- Regression coverage now includes a condition-ID book opening a Gamma-market maker paper order after risk approval.

Post-fix replay of `m9-rtds-natural-20260428T163455Z-e` generates:

- 8 signal intents.
- 8 risk approvals.
- 8 open maker paper orders.
- 0 fills so far, because the generated orders are passive maker orders and the stored session has no later trade/queue evidence that fills them.

The post-fix replay intentionally diverges from the pre-fix recorded paper events for this run:

- Generated paper events: 8.
- Recorded paper events: 0.

That divergence is expected because the original paper capture was recorded before this fix. A new paper session is required to produce matching recorded paper order events under the corrected code.

## Replay Coverage

Before the condition-ID/Gamma-ID paper-path fix, every stored session with `config_snapshot.json` replayed deterministically. After the fix, older pre-fix sessions that naturally should have opened paper orders may diverge because their recorded paper-event files captured the old zero-order behavior.

| Run ID | Generated/recorded paper events | Replay fingerprint |
| --- | --- | --- |
| `m9-deterministic-paper-lifecycle-20260428a` | 2 / 2 | `sha256:cb09d75d882eecd251859e41e380b694d4d5bbc708475fc5846d9d3817799490` |
| `m9-pyth-proxy-natural-20260428a` | 0 / 0 | `sha256:69f56f57cff22e9c9a443a78c9f568b148376389e08f72dcf0db46c94f0d2a98` |
| `m9-pyth-proxy-self-verify-20260428a` | 0 / 0 | `sha256:fa5492b7ba5a22f918b554a62472e8c0c9cbbd10dd5ca2e8fbd6cd0f7ace606c` |
| `m9-pyth-proxy-smoke-20260428a` | 0 / 0 | `sha256:22374d060a0fc1e562cf95c0bae9f5b47b9b5717b2eb12d1bd5a4045d0ed33aa` |
| `m9-pyth-proxy-smoke-20260428b` | 0 / 0 | `sha256:5d258c96353a81679d7326e71f28371fdaa68aae3bad34483ae39ea7683f863a` |
| `m9-pyth-proxy-smoke-20260428c` | 0 / 0 | `sha256:1b12edf41a127d6cd9412447d32e2efb9c7566753132069154732b608a89452d` |
| `m9-rtds-chainlink-smoke-20260428a` | 0 / 0 | `sha256:bc7bc925be893bcef624cd95bca06e5ccc65b99774654ee7857b377a798c728a` |
| `m9-rtds-chainlink-smoke-20260428b` | 0 / 0 | `sha256:8a4dce14a349b92dcf10dfb7dbce1f079f667b2fe91689fb6e93d0fa91f3e0df` |
| `m9-rtds-natural-20260428T155349Z-a` | 0 / 0 | `sha256:71d744bb0d5263874b5043b47938076de0b7784dd6c681e1dc21b80b3008f45c` |
| `m9-rtds-natural-20260428T155849Z-b` | 0 / 0 | `sha256:b559e3030a82a61db331a7b7908f659c1ef86609d9e0971fcbe976bb7781522a` |
| `m9-rtds-natural-20260428T160309Z-c` | 0 / 0 | `sha256:0c3ef217324b3c8e2141b2365bd66d4ca77ef41596055edcf03b79fbb1959f2a` |
| `m9-rtds-natural-20260428T160726Z-d` | 0 / 0 | `sha256:1c7f7c8b0e81e8dfbe0783272603a8a0e4478ae0ec92cf3e4762bc657b1d4906` |
| `m9-rtds-natural-20260428T163455Z-e` | post-fix: 8 / 0 divergence expected | see `replay_report_diverged.json` |
| `m9-rtds-natural-20260428a` | 0 / 0 | `sha256:c91adabbc9bb8262bf8d2e9b10953ba6016737912aa0ebd59feabf06ae82ffea` |
| `m9-rtds-natural-20260428b` | 0 / 0 | `sha256:4cd8ca9877c5965cdb41302cec6fa141326a00eaf6785f9378c964bdc2f1833e` |
| `m9-rtds-natural-20260428c` | 0 / 0 | `sha256:8aff602ad9a9ba78f45f66a2e38bdfb621230c4bde9a63eb7a6697a6e07d8525` |
| `m9-rtds-natural-20260428d` | 0 / 0 | `sha256:20bba0230ba09694c567f1503c5e044b4ef9a361be563d403e4b20fd8b25b228` |
| `m9-rtds-natural-20260428e` | 0 / 0 | `sha256:746d6a18a0d6607d3738fd9a38e8efc919d0d1ab588635ddb03fe52ecf5c0dd4` |
| `m9-runtime-smoke-20260427b` | 0 / 0 | `sha256:34f9df68115b922af638b365259d64cea4aa1f37fa80a8690bb3fe0cba539548` |

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
- Natural RTDS-backed paper order opening: FIXED in replay for the stored `m9-rtds-natural-20260428T163455Z-e` inputs.
- Natural RTDS-backed paper fills: NOT EXERCISED.
- Reason: post-fix replay of the stored inputs opens 8 passive maker orders, but no later fill evidence exists in the pre-fix captured session.
- Final M9 live-readiness: PARTIAL until natural risk-reviewed paper orders/fills and final start/end settlement artifacts are verified.
