# 2026-05-02 - Live Beta LB5 Cancel Readiness And Rollback/Runbook

Branch: `live-beta/lb5-cancel-readiness-rollback`
Base: `22d58dfbbf2e` (merged PR #20 on `main`)
Phase: LB5 - Cancel Path Readiness And Rollback/Runbook Minimum

## Operator Approval

The operator explicitly approved starting LB5 only on 2026-05-02 after LB4 approved-host readback/account preflight passed and PR #20 merged.

This approval does not authorize LB6, order posting, live cancel proof, cancel-all, autonomous live trading, or strategy-to-live routing.

## Official Docs Checked

- `https://docs.polymarket.com/api-reference/introduction`: CLOB API host remains `https://clob.polymarket.com`; authenticated order management requires authentication.
- `https://docs.polymarket.com/api-reference/authentication`: CLOB trading endpoints, cancellations, and heartbeat require L2 `POLY_*` headers.
- `https://docs.polymarket.com/api-reference/trade/cancel-single-order`: single-order cancel is `DELETE /order` with body field `orderID`; response includes `canceled` and `not_canceled`.
- `https://docs.polymarket.com/api-reference/trade/get-single-order-by-id`: single-order readback is `GET /order/{orderID}` and reports order status plus matched size.
- `https://docs.polymarket.com/api-reference/trade/cancel-multiple-orders`, `cancel-all-orders`, and `cancel-orders-for-a-market`: multiple, all, and market-wide cancels are separate endpoints and remain out of LB5 scope.
- `https://docs.polymarket.com/trading/orders/overview`: heartbeat can cancel open orders if not maintained; trade status and transaction-hash reconciliation remain required.
- `https://docs.polymarket.com/api-reference/rate-limits`: CLOB trading endpoints have separate rate limits; rate-limit responses must fail closed.

## Changes

- Added `src/live_beta_cancel.rs` as an offline LB5 readiness module only.
  - Single-order cancel draft shape: `DELETE /order` with `orderID`.
  - Live cancel network dispatch remains disabled: `LIVE_CANCEL_NETWORK_ENABLED=false`.
  - Cancel-all remains disabled and has no request builder.
  - Request construction is blocked unless LB4 preflight, LB5 approval, LB6 hold release, human canary approval, human cancel approval, one approved canary order ID, one open-order readback, heartbeat readiness, kill switch readiness, service stop readiness, cancel-plan acknowledgement, and live placement enablement are all present.
  - Even when fixture inputs satisfy those gates, the draft remains `network_enabled=false`; LB5 has no HTTP client and performs no venue request.
- Added `validate --local-only --live-cancel-readiness` output for local evidence.
- Added rollback minimum runbook: `runbooks/live-beta-lb5-rollback-runbook.md`.
- Updated `STATUS.md` for PR #20 merge closeout, LB5 active scope, and the mandatory hold before LB6.

## Cancel Fixture Coverage

Focused tests cover:

- default LB5 cancel readiness blocked before LB6 canary,
- single-order cancel draft requires LB6 gates and an approved canary order,
- successful `canceled` response,
- partial-fill ambiguity,
- already filled / partially filled,
- already canceled,
- missing order,
- unknown `not_canceled` reason,
- auth error,
- rate limit,
- no cancel-all path,
- no network dispatch or secret-loading surface in the LB5 cancel module,
- rollback runbook required-section coverage.

## Local Readiness Output

Command:

```text
cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness
```

Key output:

```text
live_order_placement_enabled=false
live_beta_gate_status=blocked
live_beta_cancel_readiness_status=blocked
live_beta_cancel_readiness_live_network_enabled=false
live_beta_cancel_readiness_cancel_all_enabled=false
live_beta_cancel_readiness_request_constructable=false
live_beta_cancel_readiness_single_cancel_method=DELETE
live_beta_cancel_readiness_single_cancel_path=/order
live_beta_cancel_readiness_single_order_readback_path_prefix=/order/
live_beta_cancel_readiness_block_reasons=live_order_placement_disabled,lb4_preflight_not_recorded,lb6_hold_not_released,human_canary_approval_missing,human_cancel_approval_missing,approved_canary_order_missing,single_open_order_not_verified,heartbeat_not_ready
```

Interpretation: LB5 readiness scaffolding is present, but live cancellation is blocked by design until LB6 approval and a single approved canary order exist.

## Rollback Minimum

Runbook path: `runbooks/live-beta-lb5-rollback-runbook.md`

Required sections present:

- Kill Switch,
- Service Stop,
- Open-Order Readback,
- Cancel Plan,
- Heartbeat Failure Handling,
- Incident Note Template,
- Artifact Checklist,
- LB6 Hold.

## Verification

- `cargo test --offline cancel`: PASS, 12 lib tests.
- `cargo test --offline rollback`: PASS, 1 lib test.
- `cargo test --offline runbook`: PASS, 1 lib test.
- `cargo test --offline readback`: PASS, 28 lib tests and 1 main test.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
- `cargo run --offline -- --config config/default.toml validate --local-only --live-cancel-readiness`: PASS.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 186 lib tests and 7 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- Trailing whitespace scan over tracked Rust/TOML/Markdown excluding `.git` and `target`: PASS, no output.

## Safety And No-Secret Scan

Order/cancel/live-trading scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
```

Expected hits:

- existing paper order/cancel simulation and paper reporting code,
- existing LB4 authenticated read-only order readback paths,
- `LIVE_ORDER_PLACEMENT_ENABLED=false` and blocked live gate output,
- new LB5 single-order cancel readiness constants/tests for `DELETE /order`, with no network dispatch,
- no cancel-all path.

Secret/key scan:

```text
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Expected hits:

- existing LB2 secret-handle metadata and redaction code,
- existing LB3 sanitized dry-run signing artifact code,
- existing LB4 read-only L2 header construction for authenticated GETs,
- public placeholder addresses in config examples,
- no private-key value, API-key value, secret value, wallet key material, order posting, live cancel proof, cancel-all, or strategy-to-live route.

Whole-repo no-secret scan:

```text
rg -n -i "(private[_ -]?key|seed phrase|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" .
```

Expected hits are public placeholder/reference IDs, documented header or handle names, prior verification scan text, and docs warning text. No secret values were found.

Ignored-local guard:

```text
.env_ignored=true
config_local_ignored=true
```

## Result

LB5 cancel readiness and rollback/runbook minimum: PASS for offline readiness only.

No live order was placed. No live cancel proof was performed. No cancel-all path was added. No secrets were added. `LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.

Mandatory hold: stop after LB5. Do not start LB6 until explicit human/operator approval for the exact one-order canary plan is recorded.
