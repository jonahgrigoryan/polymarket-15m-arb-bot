# 2026-05-02 LB6 Pre-Authorized Canary Envelope

## Scope

Add a reviewed `live-canary --preauthorized-envelope --one-order` path so the operator does not need to paste a fresh exact prompt/hash for each 15-minute market window.

This evidence note is for the code-review mechanism only. No live order was submitted, no live cancel was sent, and no cancel-all/autonomous trading path was added.

## Official References Rechecked

- https://docs.polymarket.com/api-reference/trade/post-a-new-order
- https://docs.polymarket.com/trading/orders/overview
- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/api-reference/trade/cancel-single-order

## Envelope Constraints

The pre-authorized mode is intentionally narrower than general LB6:

- ETH 15-minute market slug only: `eth-updown-15m-<start_unix>`.
- Current market window only; the configured market end must equal start plus 900 seconds.
- Outcome `Up`.
- Side `BUY`.
- Order type `GTD`.
- Post-only and maker-only.
- Price `0.01`.
- Size `5`.
- Notional `0.05 pUSD`.
- Tick size `0.01`.
- GTD expiry must be before the final market minute.
- Best ask must remain above `0.01`.
- Book and reference ages must remain under configured stale thresholds.
- Reserved pUSD must be zero.
- Available pUSD must exceed the canary notional.

The runtime still requires geoblock PASS, LB4 authenticated readback/account preflight PASS, zero open orders, L2 handles present, canary private-key handle present, LB5 rollback readiness, the LB6 exact single-order cancel path, official SDK availability, and an unused local one-order cap sentinel.

## Safety Result

- No live order submitted.
- No live cancel sent.
- No cancel-all path added.
- No strategy-to-live or autonomous trading route added.
- No secret values, API-key values, seed phrases, or wallet/private-key material added to repo/docs/tests.
- Existing exact `--human-approved --one-order` prompt/hash path remains available.

## Verification

- `cargo fmt --check` PASS.
- `cargo test --offline canary` PASS: 20 focused canary/cancel/lifecycle tests, including pre-authorized envelope fixed-shape/current-window checks.
- `cargo test --offline lifecycle` PASS.
- `cargo test --offline cancel` PASS.
- `cargo test --offline readback` PASS.
- `cargo test --offline secret` PASS.
- `cargo test --offline redaction` PASS.
- `cargo run --offline -- --config config/default.toml validate --local-only` PASS, run_id `18abe22cc1df5cc0-1115c-0`.
- `cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness` PASS, run_id `18abe22f2c7d3500-11211-0`; cancel readiness remains fail-closed without an approved canary order.
- `cargo test --offline` PASS: 213 lib tests + 8 main tests.
- `cargo clippy --offline -- -D warnings` PASS.
- `git diff --check` PASS.
- Safety scans PASS with expected hits only: existing gated canary `post_order` path, exact single-order cancel/readback path, paper order/cancel simulation paths, disabled live-order gate strings, public condition/feed IDs, and secret handle names.
- Ignored-local guard PASS: `.env` and `config/local.toml` are gitignored.
