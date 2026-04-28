# M9 Natural RTDS Paper Validation

Date: 2026-04-28
Branch: `m9/rtds-natural-paper-validation`

## Scope

This note records longer bounded Polymarket RTDS Chainlink-backed paper sessions using `config/polymarket-rtds-chainlink.example.toml`.

Safety boundaries held:

- No live order placement was added.
- No wallet, signing, private-key, trading API-key, or authenticated CLOB order-client path was added.
- Signal/risk thresholds were not weakened.
- Paper orders were not forced, synthesized, or bypassed around signal/risk gates.

## CLOB Endpoint Recheck

Official Polymarket docs checked on 2026-04-28:

- `https://docs.polymarket.com/api-reference/introduction` lists CLOB API as `https://clob.polymarket.com`.
- `https://docs.polymarket.com/v2-migration` says `https://clob-v2.polymarket.com` was the pre-cutover V2 test endpoint and that, on 2026-04-28 after go-live, V2 takes over `https://clob.polymarket.com`.

Live read-only checks:

```text
curl -sS -D - https://clob.polymarket.com/ok -o /tmp/polymarket-clob-ok.txt
curl -sS -D - https://clob-v2.polymarket.com/ok -o /tmp/polymarket-clob-v2-ok.txt
```

Observed:

- `https://clob.polymarket.com/ok` returned `HTTP/2 200` with body `"OK"`.
- `https://clob-v2.polymarket.com/ok` returned `HTTP/2 301` with `location: https://clob.polymarket.com/ok`.

Result: PASS. `config/polymarket-rtds-chainlink.example.toml` now uses `https://clob.polymarket.com`.

## Runtime Ordering Fixes

Longer natural run `m9-rtds-natural-20260428b` completed 4 cycles with 48 RTDS ticks, 0 orders, and 0 fills, but every post-reference evaluation still failed on freshness:

- replay fingerprint under current code: `sha256:4cd8ca9877c5965cdb41302cec6fa141326a00eaf6785f9378c964bdc2f1833e`
- skip reasons: `missing_reference_price=40`, `stale_book=48`, `stale_reference_price=95`

Diagnosis:

- RTDS reference ticks were captured after CLOB/predictive feed batches, so books could become stale before reference ticks were evaluated.
- CLOB book events use condition IDs, while discovered markets use Gamma market IDs. State could attach condition-ID books to Gamma markets, but replay evaluation was triggered on the raw condition ID, so fresh book updates were applied but not evaluated for the discovered market.

Fixes:

- After each asset's RTDS reference batch, the runtime now refreshes that asset's read-only CLOB `/book` snapshots.
- Replay now maps CLOB condition-ID book updates back to the corresponding discovered Gamma market before evaluation.
- Older config snapshots now default missing `reference_feed.polymarket_rtds_url` so every stored session can replay under current code.

Tests added:

- `replay::tests::fresh_book_after_reference_tick_evaluates_without_stale_book_skip`
- `config::tests::missing_rtds_url_defaults_for_older_reference_feed_snapshots`

## Completed Natural RTDS Sessions

All runs used unchanged default signal/risk settings.

| Run ID | Cycles | Raw | Normalized | RTDS ticks | Orders | Fills | P&L | Replay fingerprint | Skip reasons |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| `m9-rtds-natural-20260428c` | 4 | 168 | 217 | 48 | 0 | 0 | 0.0 | `sha256:8aff602ad9a9ba78f45f66a2e38bdfb621230c4bde9a63eb7a6697a6e07d8525` | `edge_below_minimum=12`, `edge_below_minimum+stale_book=12`, `missing_reference_price=32`, `stale_book=48`, `stale_reference_price=111` |
| `m9-rtds-natural-20260428d` | 4 | 168 | 205 | 48 | 0 | 0 | 0.0 | `sha256:20bba0230ba09694c567f1503c5e044b4ef9a361be563d403e4b20fd8b25b228` | `edge_below_minimum=12`, `edge_below_minimum+stale_book=12`, `missing_reference_price=24`, `stale_book=48`, `stale_reference_price=103` |
| `m9-rtds-natural-20260428e` | 3 | 126 | 156 | 36 | 0 | 0 | 0.0 | `sha256:746d6a18a0d6607d3738fd9a38e8efc919d0d1ab588635ddb03fe52ecf5c0dd4` | `edge_below_minimum=9`, `edge_below_minimum+stale_book=9`, `missing_reference_price=40`, `stale_book=36`, `stale_reference_price=58` |

Interpretation:

- RTDS Chainlink reference ingestion remains PASS.
- The post-fix sessions reached unchanged EV evaluation (`edge_below_minimum`) where reference/book inputs overlapped.
- Natural RTDS-backed paper trades remain NOT EXERCISED because no order intent reached risk approval and no paper orders/fills occurred.
- Some windows still failed freshness naturally under `stale_reference_ms=1000` and `stale_book_ms=1000`; those gates were not relaxed.

## Replay All Stored Sessions

Every stored session with `config_snapshot.json` was replayed twice. The second replay summary matched the first exactly (`determinism_diff_status=0`).

| Run ID | Replay fingerprint |
| --- | --- |
| `m9-deterministic-paper-lifecycle-20260428a` | `sha256:cb09d75d882eecd251859e41e380b694d4d5bbc708475fc5846d9d3817799490` |
| `m9-pyth-proxy-natural-20260428a` | `sha256:69f56f57cff22e9c9a443a78c9f568b148376389e08f72dcf0db46c94f0d2a98` |
| `m9-pyth-proxy-self-verify-20260428a` | `sha256:fa5492b7ba5a22f918b554a62472e8c0c9cbbd10dd5ca2e8fbd6cd0f7ace606c` |
| `m9-pyth-proxy-smoke-20260428a` | `sha256:22374d060a0fc1e562cf95c0bae9f5b47b9b5717b2eb12d1bd5a4045d0ed33aa` |
| `m9-pyth-proxy-smoke-20260428b` | `sha256:5d258c96353a81679d7326e71f28371fdaa68aae3bad34483ae39ea7683f863a` |
| `m9-pyth-proxy-smoke-20260428c` | `sha256:1b12edf41a127d6cd9412447d32e2efb9c7566753132069154732b608a89452d` |
| `m9-rtds-chainlink-smoke-20260428a` | `sha256:bc7bc925be893bcef624cd95bca06e5ccc65b99774654ee7857b377a798c728a` |
| `m9-rtds-chainlink-smoke-20260428b` | `sha256:8a4dce14a349b92dcf10dfb7dbce1f079f667b2fe91689fb6e93d0fa91f3e0df` |
| `m9-rtds-natural-20260428a` | `sha256:c91adabbc9bb8262bf8d2e9b10953ba6016737912aa0ebd59feabf06ae82ffea` |
| `m9-rtds-natural-20260428b` | `sha256:4cd8ca9877c5965cdb41302cec6fa141326a00eaf6785f9378c964bdc2f1833e` |
| `m9-rtds-natural-20260428c` | `sha256:8aff602ad9a9ba78f45f66a2e38bdfb621230c4bde9a63eb7a6697a6e07d8525` |
| `m9-rtds-natural-20260428d` | `sha256:20bba0230ba09694c567f1503c5e044b4ef9a361be563d403e4b20fd8b25b228` |
| `m9-rtds-natural-20260428e` | `sha256:746d6a18a0d6607d3738fd9a38e8efc919d0d1ab588635ddb03fe52ecf5c0dd4` |
| `m9-runtime-smoke-20260427b` | `sha256:34f9df68115b922af638b365259d64cea4aa1f37fa80a8690bb3fe0cba539548` |

Paper/replay comparison:

- For `m9-rtds-natural-20260428d` and `m9-rtds-natural-20260428e`, `paper_report.json` and `replay_report.json` are byte-identical.
- For older completed sessions, paper/replay reports may differ in regenerated signal diagnostics under the fixed condition-ID evaluation path, but paper order count, fill count, and total P&L match.
- Partial captures `m9-rtds-chainlink-smoke-20260428a` and `m9-rtds-natural-20260428a` replay deterministically but are not counted as completed natural validation sessions.

## Verification Commands

Passed:

```text
cargo test --offline
cargo clippy --offline -- -D warnings
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/polymarket-rtds-chainlink.example.toml validate --local-only
git diff --check
```

Safety scan:

- No `.post(`, `.put(`, or `.delete(` calls in `src`.
- Focused scan found no source path for private keys, API keys, wallets, authenticated order clients, live order placement, or live trading.
- Expected matches only: comments warning against credentials/live trading and `LIVE_ORDER_PLACEMENT_ENABLED=false` output/constant.

## Gate Result

- Polymarket RTDS Chainlink reference ingestion: PASS.
- Natural RTDS-backed paper trades: NOT EXERCISED.
- Final M9 live-readiness: PARTIAL until natural risk-reviewed paper orders/fills and final start/end settlement artifacts are verified.
