# 2026-05-12 Live Trading LT1 Read-Only Supervision

Branch: `live-trading/lt1-read-only-supervision`

Scope: LT1 read-only final-live supervision only.

## Decision

LT1 is implementation-complete locally and ready for commit after final verification.

The implementation adds a final-live read-only command:

```text
live-trading-preflight --read-only --baseline-id <id>
```

It writes redacted artifacts under:

```text
artifacts/live_trading/<baseline_id>/
```

It does not authorize or perform live order signing, order submission, cancel submission, heartbeat POST, or cap sentinel writes.

## Official Documentation Recheck

Checked on 2026-05-12:

- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/api-reference/geoblock
- https://docs.polymarket.com/api-reference/rate-limits
- https://docs.polymarket.com/api-reference/trade/get-user-orders
- https://docs.polymarket.com/trading/orderbook
- https://docs.polymarket.com/api-reference/markets/get-clob-market-info
- https://docs.polymarket.com/api-reference/core/get-current-positions-for-a-user
- https://docs.polymarket.com/api-reference/core/get-trades-for-a-user-or-markets

Relevant assumptions used for LT1:

- Gamma/Data API and public CLOB orderbook/price reads are read-only.
- CLOB authenticated account/order/trade readback uses L2 `POLY_*` headers.
- CLOB order placement, cancellations, and heartbeat are authenticated trading operations and remain out of LT1 scope.
- Geoblock readback is `GET https://polymarket.com/api/geoblock`, not a CLOB API host endpoint.
- Current rate limits include CLOB readback and public market-data endpoints; LT1 local dry-run does not approach those limits.

## Implementation Summary

- Added disabled-by-default `[live_trading]` config:
  - `enabled = false`
  - `approved_host = ""`
  - `approved_country = ""`
  - `approved_region = ""`
- Added `live_trading_preflight` module with deterministic redacted artifact hashing and fail-closed gate evaluation.
- Added CLI command `live-trading-preflight --read-only --baseline-id <id>`.
- Reused existing read-only account baseline and CLOB readback primitives.
- Added local fail-closed artifact generation for default config.
- Updated `STATUS.md` to point to LT1 and the commit/PR hold before LT2.

## Local LT1 Artifact

Command:

```text
cargo run --offline -- --config config/default.toml live-trading-preflight --read-only --baseline-id LT1-LOCAL-DRY-RUN
```

Result:

- Status: `blocked`
- Run ID: `18aef17982931638-edf6-0`
- Preflight artifact: `artifacts/live_trading/LT1-LOCAL-DRY-RUN/final_live_preflight.redacted.json`
- Preflight hash: `sha256:262150134a1dcf27ec6fb4491df06f5a6d9e29c80d951c4d8fe1c9e32e66da49`
- Account baseline artifact: `artifacts/live_trading/LT1-LOCAL-DRY-RUN/account_baseline.redacted.json`
- Account baseline hash: `sha256:8cfdec2b16a006452c2fd5acfb1c71b224a931acd25afbb73c03229a247f2ad3`
- Geoblock result: `not_checked` because `config/default.toml` keeps final live trading disabled.
- Open order count: `0`
- Trade count: `0`
- Position count: `0`
- Reserved pUSD units: `0`
- Available pUSD units: `25000000` from local fail-closed sample evidence only.

Block reasons:

```text
account_readback_not_live_network,
account_readback_not_passed,
approved_country_not_configured,
approved_host_not_configured,
approved_region_not_configured,
book_freshness_not_passed,
final_live_config_disabled,
geoblock_not_passed,
l2_secret_handles_not_present,
market_discovery_freshness_not_passed,
predictive_freshness_not_passed,
reference_freshness_not_passed
```

No-live-action proof:

```text
submitted_orders=false
signed_orders_for_submission=false
submitted_cancels=false
heartbeat_posts=false
cap_writes=false
```

## Approved-Host Readback

Not run in this local LT1 branch state.

Reason: no approved final-live config with `[live_trading].enabled=true`, approved host/jurisdiction, account handles, and L2 readback secret handles is present in the repo state. The local dry-run intentionally fails closed instead of reaching for live network account readback.

## Verification

| Check | Status | Notes |
| --- | --- | --- |
| `cargo run --offline -- --config config/default.toml validate --local-only` | PASS | Run ID `18aef17d7a5d9908-f188-0`; `validation_status=ok`; live order placement remains false. |
| `cargo test --offline live_trading_preflight` | PASS | 3 module tests and 1 CLI parse test passed. |
| `cargo test --offline live_account_baseline` | PASS | 12 tests passed. |
| `cargo test --offline live_alpha_preflight` | PASS | 6 tests passed. |
| `cargo run --offline -- --config config/default.toml live-trading-preflight --read-only --baseline-id LT1-LOCAL-DRY-RUN` | PASS | Produced blocked redacted LT1 artifacts with no live actions. |
| `cargo fmt --check` | PASS | Formatting clean after LT1 edits. |
| `cargo test --offline` | PASS | 426 lib tests, 93 main tests, and 0 doc tests passed. |
| `cargo clippy --offline -- -D warnings` | PASS | Passed after tightening one LT1 option comparison. |
| `scripts/verify-pr.sh` | PASS | Formatting, full tests, clippy, diff whitespace check, and built-in safety scope scan passed. |

Additional LT1 safety scans were run over order/cancel, secret/wallet, and geoblock-related terms:

- `/tmp/lt1_order_scan.txt`: 1416 lines.
- `/tmp/lt1_secret_scan.txt`: 1747 lines.
- `/tmp/lt1_geo_scan.txt`: 1084 lines.

Targeted review of new LT1 hits found only read-only command/docs/no-live-action field names, final-live proof metadata, default fail-closed geoblock status, jurisdiction fields, and existing Live Alpha/Beta paths. No new order submission, cancel submission, signing-for-submission, secret value exposure, or geoblock bypass was added.

## Safety Boundary

LT1 remains read-only. The implementation does not add:

- order signing for submission,
- order submission,
- cancel submission,
- heartbeat POST,
- cap sentinel writes,
- taker expansion,
- final-live order/cancel client,
- live order feature enablement,
- production sizing or rate increase.

## Exit Gate

LT1 local implementation exits with a redacted baseline/preflight artifact and a fail-closed final-live gate report. The gate is blocked on default config, which is expected.

Next action: commit LT1, push, open PR to `main`, review, merge, refresh `main`, then wait for explicit approval before starting LT2.
