# 2026-05-02 - Live Beta LB6 Exact Single-Order Cancel/Readback Patch

Branch: `live-beta/lb6-single-cancel-readback`

Commit: PR #23 branch head.

PR: `https://github.com/jonahgrigoryan/polymarket-15m-arb-bot/pull/23`

Scope: add the missing LB6 exact canary-order readback/cancel mechanism, then stop. This run does not submit an order and does not send a live cancel.

## Operator Boundary

The operator asked to patch the missing items after the prior LB6 final gate stopped fail-closed.

This patch does not authorize a live order, live cancel, cancel-all, autonomous live trading, strategy-to-live routing, or reuse of any stale approval. A fresh market/order approval prompt is still required after this patch is reviewed and merged.

## Official Docs Rechecked

- `https://docs.polymarket.com/api-reference/authentication`: authenticated CLOB trading endpoints require the five L2 `POLY_*` headers; L2 signatures are HMAC-SHA256 using the API secret, and order creation still requires a separately signed order payload.
- `https://docs.polymarket.com/api-reference/trade/get-single-order-by-id`: exact authenticated order readback is `GET /order/{orderID}` and returns status, maker, market, asset, side, fixed-unit sizes, price, outcome, expiration, order type, associated trades, and creation time.
- `https://docs.polymarket.com/api-reference/trade/cancel-single-order`: exact single-order cancel is `DELETE /order` with body field `orderID`; the response includes `canceled` and `not_canceled`.
- `https://docs.polymarket.com/api-reference/rate-limits`: `GET /order` and `DELETE /order` have documented CLOB limits; auth/rate-limit responses remain fail-closed.

## Implemented

- Added `src/live_beta_order_lifecycle.rs`.
  - Authenticated exact order readback for `GET /order/{orderID}`.
  - Exact single-order cancel execution for `DELETE /order`.
  - No cancel-all, bulk cancel, market-wide cancel, or autonomous order selection path.
  - Fail-closed readiness for geoblock, authenticated readback availability, missing L2 handles, missing human cancel approval, rollback readiness, invalid order hash, order/account/condition/token/side/price/size/type mismatch, terminal or unknown order state, partial/full match, associated trades, and zero remaining size.
  - Post-cancel readback must show the target order canceled.
- Added `parse_single_order` to `src/live_beta_readback.rs` using the same fixed-unit parsing as LB4 paginated order readback.
- Added `live-cancel` CLI path.
  - `--dry-run`: performs authenticated exact order readback and evaluates cancel readiness only.
  - `--human-approved --one-order`: final gated mode for the exact recorded canary order only.
  - Final mode requires the local one-order cap sentinel to contain the same venue order ID and canary approval hash.
- Updated LB6 canary readiness so future final canary submission also requires the LB6 exact single-order cancel path to be present and cancel-all to remain disabled.
- Preserved LB5 as offline readiness only: `src/live_beta_cancel.rs` still has no network dispatch or secret-loading surface.

## Safety Notes

- No live order was submitted in this patch.
- No live cancel was sent in this patch.
- `LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.
- `LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED=true` exists only in the new exact-order lifecycle module and remains unreachable without `live-cancel --human-approved --one-order`, matching cap state, geoblock PASS, L2 handles, exact order readback, and exact order eligibility.
- `LB6_CANCEL_ALL_ENABLED=false`.
- `.env` and `config/local.toml` remain ignored local files and were not read or committed.

## Focused Verification

```bash
cargo test --offline lifecycle
cargo test --offline canary
cargo test --offline cancel
cargo fmt --check
```

Result: PASS.

## Full Verification

```bash
cargo test --offline readback
cargo test --offline secret
cargo test --offline redaction
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
test ! -e .env || git check-ignore .env
test ! -e config/local.toml || git check-ignore config/local.toml
```

Result: PASS.

Full test result: 209 lib tests, 8 main tests, and 0 doc tests passed.

Local validation result: `validation_status=ok`, `live_order_placement_enabled=false`, and broader `live_beta_gate_status=blocked` by design.

LB5 readiness result remains offline-only: `live_beta_cancel_readiness_live_network_enabled=false`, `live_beta_cancel_readiness_cancel_all_enabled=false`, and `live_beta_cancel_readiness_request_constructable=false`.

## Safety Scan

Commands:

```bash
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|cancel-all|order client|clob.*order|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config/default.toml config/example.local.toml STATUS.md verification/2026-05-02-live-beta-lb6-single-cancel-readback.md
rg -n -i "(private[_ -]?key|seed phrase|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config/default.toml config/example.local.toml STATUS.md verification/2026-05-02-live-beta-lb6-single-cancel-readback.md LIVE_BETA_PRD.md LIVE_BETA_IMPLEMENTATION_PLAN.md
```

Result: PASS with expected hits only: existing paper order/cancel code, existing LB4 L2 header names, existing LB6 canary signing path and handle names, public fixture IDs, public Chainlink/Pyth IDs, documentation guardrails, LB5 offline cancel constants/tests, and the new gated LB6 exact single-order lifecycle path. No secret values, wallet key material, cancel-all endpoint, autonomous trading path, or strategy-to-live route was added.

## Result

LB6 exact single-order cancel/readback mechanism: implemented locally.

Live order submission: NOT PERFORMED.

Live cancel: NOT PERFORMED.

Next action: review/merge PR #23, then stop. After merge, run a fresh final recheck and generate a fresh canary approval prompt only if every gate passes naturally.
