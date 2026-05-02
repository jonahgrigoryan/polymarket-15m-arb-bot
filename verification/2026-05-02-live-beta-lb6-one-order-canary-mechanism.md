# LB6 One-Order Canary Mechanism Verification

Date: 2026-05-02

Branch: `live-beta/lb6-one-order-canary`

Commit: pending

Scope: implement the LB6 one-order canary signing/submission mechanism only, then stop before submitting any order.

## Operator Boundary

The operator authorized LB6 preparation/mechanism work only.

This run does not authorize order submission, live cancel, cancel-all, a second order, autonomous live trading, strategy-to-live routing, or reuse of any expired canary approval.

No order was submitted in this run. No live cancel was sent in this run.

## Official Docs Rechecked

- `https://docs.polymarket.com/api-reference/authentication`: CLOB order posting requires L2 `POLY_*` headers, and methods that create orders still require a signed order payload from the user's private key.
- `https://docs.polymarket.com/api-reference/clients-sdks`: official clients are available for TypeScript, Python, and Rust; the Rust package is `polymarket_client_sdk_v2`.
- `https://docs.polymarket.com/trading/orders/overview`: orders are EIP-712 signed; manual signing is involved; official SDK clients are recommended for signing/submission. GTD is a limit order type, and post-only can only be used with GTC/GTD.
- `https://docs.polymarket.com/trading/orders/create`: SDK signing/submission is the recommended path; post-only orders are rejected if they would match immediately.
- `https://docs.polymarket.com/api-reference/trade/post-a-new-order`: one signed order is posted through `POST /order`.
- `https://docs.polymarket.com/api-reference/trade/cancel-single-order`: one order is canceled through `DELETE /order`, with response fields `canceled` and `not_canceled`.
- `https://docs.polymarket.com/api-reference/rate-limits`: CLOB trading endpoints have burst and sustained limits; rate-limit responses must fail closed.

## Implemented

- Added `src/live_beta_canary.rs`.
  - Exact canonical approval text generation.
  - Approval `sha256:<hex>` guard.
  - Approval expiry guard.
  - One-order cap state type for local non-secret sentinel storage.
  - GTD/post-only/maker-only checks.
  - Price, size, notional, tick alignment, and LB6 notional cap checks.
  - Best-ask-above-bid check for the post-only maker canary.
  - Geoblock, LB4 preflight, zero-open-orders, LB5 rollback, secret-handle, and official-SDK readiness checks.
  - Official `polymarket_client_sdk_v2` final signing/submission path for exactly one order.
- Added `live-canary` CLI path.
  - `--dry-run`: evaluates gates and prints the fresh final approval prompt/hash without submitting.
  - `--human-approved --one-order`: final gated mode; blocks unless exact approval text/hash and every pre-submit gate pass.
  - The approval prompt includes run ID, host, geoblock result, wallet/funder, signature type, pUSD available/reserved state, market slug, condition ID, token ID, outcome, side, price, size, notional, order type, GTD expiry, market end, fee estimate, book age, reference age, heartbeat, cancel plan, and rollback command.
  - Final mode validates non-empty/parseable local signing and L2 env values before reserving the one-order cap sentinel, so bad local credentials cannot consume the only canary attempt before any venue submission call.
- Added canary private-key handle metadata:
  - `P15M_LIVE_BETA_CANARY_PRIVATE_KEY`
  - The handle name is committed; no value is committed or printed.
- Added the official Rust SDK dependency:
  - `polymarket_client_sdk_v2 = "0.6.0-canary.1"` with `clob` feature.

## Gate Status

Mechanism status: implemented.

Current final submission status in this Codex shell: BLOCKED.

Observed missing handles in this shell:

- `P15M_LIVE_BETA_CANARY_PRIVATE_KEY`
- `P15M_LIVE_BETA_CLOB_L2_ACCESS`
- `P15M_LIVE_BETA_CLOB_L2_CREDENTIAL`
- `P15M_LIVE_BETA_CLOB_L2_PASSPHRASE`

Because the final gate was blocked, no fresh live-market approval prompt was produced in this run.

## Safety Notes

- `LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.
- LB6 uses a narrower compile-time canary-only gate: `LB6_ONE_ORDER_CANARY_SUBMISSION_ENABLED=true`.
- The canary submit path remains unreachable without exact approval text/hash, fresh expiry, geoblock PASS, LB4 preflight PASS, zero open orders, LB5 rollback readiness, all required handles, official SDK availability, and unused one-order cap.
- The one-order cap sentinel is reserved only after the final readiness report passes and local SDK input parsing validates the configured private-key handle, L2 key handle, L2 secret/passphrase handles, wallet/funder addresses, token ID, price, size, and GTD expiry without printing values.
- No cancel-all path was added.
- No strategy-selected order path was added.
- No live order was submitted.
- No live cancel was sent.
- No expired approval text was reused.

## Verification Commands

Focused verification:

```bash
cargo fmt --check
cargo test --offline canary
cargo test --offline signing
cargo test --offline cancel
cargo test --offline readback
cargo test --offline secret
cargo test --offline redaction
```

Full verification:

```bash
cargo fmt --check
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Additional checks:

```bash
git diff --cached --check
test ! -e .env || git check-ignore .env
test ! -e config/local.toml || git check-ignore config/local.toml
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|cancel-all|order client|clob.*order|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config/default.toml config/example.local.toml
rg -n -i "(private[_ -]?key|seed phrase|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config/default.toml config/example.local.toml STATUS.md verification/2026-05-02-live-beta-lb6-one-order-canary-mechanism.md LIVE_BETA_PRD.md LIVE_BETA_IMPLEMENTATION_PLAN.md
```

All required verification above passed. Safety/no-secret scan hits were expected handle names, documentation guardrails, public Chainlink price IDs, public fixture IDs, existing LB4 read-only auth header names, existing LB5 offline cancel-readiness constants/tests, and the new gated LB6 one-order SDK submit path.

Negative dry-run check:

```bash
cargo run --offline -- --config config/default.toml live-canary --dry-run --market-slug eth-updown-15m-demo --condition-id 0x0000000000000000000000000000000000000000000000000000000000000001 --token-id 1 --outcome Up --side BUY --price 0.01 --size 5 --notional 0.05 --gtd-expiry-unix 1777762400 --market-end-unix 1777763000 --best-ask 0.50 --book-age-ms 250 --reference-age-ms 250
```

Expected result: failed closed before any canary evaluation because `config/default.toml` intentionally has no approved LB4 readback account signature type. No order was submitted.

## Result

LB6 one-order canary mechanism: implemented and verified for mechanism-only PR review.

Live order submission: NOT PERFORMED.

Live cancel: NOT PERFORMED.

Next action: commit/push/open PR, then stop. After merge, perform a fresh immediate final recheck and produce a new exact approval prompt only if all final gates pass.
