# 2026-05-03 LB7 Runbook Handoff

## Scope

LB7 is runbook, observability, rollback hardening, incident workflow, and STATUS handoff only.

No new live order was submitted. No live cancel was sent. No cancel-all, strategy-selected live trading, taker/FOK/FAK/marketable limit path, multi-order path, cap increase, broader market/asset scope, secret value, API-key value, seed phrase, or wallet/private-key material was added.

## Base State

- Branch: `live-beta/lb7-runbook-handoff`.
- Base: updated `main` at PR #26 merge commit `2031332`.
- PR #26: merged, `Record LB6 canary execution closeout`.
- LB6 evidence: `verification/2026-05-03-live-beta-lb6-one-order-canary-execution.md`.

## LB6 Handoff Facts

- One canary submitted: yes.
- Exact order canceled: yes.
- Fill occurred: no.
- Post-cancel open orders: `0`.
- Post-cancel reserved pUSD: `0`.
- One-order cap: consumed.
- Canary interpretation: order-lifecycle/access evidence only, not profitability evidence.

## Runbook Hardening

Updated `runbooks/live-beta-lb5-rollback-runbook.md` with LB6 closeout lessons:

- Exact single-order readback path is `GET /data/order/{orderID}`.
- Exact single-order cancel path remains `DELETE /order` with body field `orderID`.
- Official `py_clob_client_v2` behavior observed during closeout:
  - official SDK readback saw the canary order as `LIVE` with matched size `0`;
  - official SDK single-order cancel returned the exact canary order ID in `canceled` and `{}` in `not_canceled`;
  - post-cancel official SDK readback saw `CANCELED` and no open canary-token orders.
- Rust/SDK readback disagreement procedure:
  - stop all live actions;
  - preserve sanitized outputs;
  - do not submit, cancel, retry, or broaden the action surface based on only one client;
  - compare order/account/trade/reserved-balance fields;
  - use the stricter safety interpretation until human review;
  - patch or document Rust compatibility before further approved live action.

## Observability Coverage

Updated `docs/m8-observability-runbook.md` with a live-beta observability addendum covering:

| Area | Required coverage |
| --- | --- |
| Live mode | `LIVE_ORDER_PLACEMENT_ENABLED`, live beta gate, canary gate, one-order cap state |
| Compliance | geoblock status, country/region, approval scope, host/session |
| Safety controls | kill switch, service stop, human approval hash, post-only/GTD/maker-only checks, one-open-order cap, cancel-all disabled |
| Heartbeat | state, age where available, failures, ambiguity blockers |
| Orders | attempts, accepts, rejects, order ID, market/condition ID, token ID, side, price, size, notional, type, expiry, venue status |
| Cancels | exact cancel attempts, exact canceled ID, `DELETE /order` response, `not_canceled`, proof cancel-all was not used |
| Fills/trades | fills, matched size, trade statuses, transaction hashes, fees, settlement follow-up |
| Readback | `/data/order/{orderID}`, open orders, Rust/SDK readback status, mismatches |
| Account state | available pUSD, reserved pUSD, balance/reserved mismatch, allowances, open notional, balance delta |
| P&L | realized P&L and settlement P&L without treating one canary as strategy-performance evidence |

## Tests Added

- `live_beta_cancel::tests::rollback_runbook_contains_lb7_closeout_lessons`
- `metrics::tests::observability_runbook_covers_live_beta_handoff_signals`

These tests lock the LB7 handoff docs. No production runtime behavior was changed for LB7.

## Verification

- `cargo test --offline metrics` PASS: 5 lib tests.
- `cargo test --offline reporting` PASS: 5 lib tests.
- `cargo test --offline rollback` PASS: 3 lib tests.
- `cargo run --offline -- --config config/default.toml validate --local-only` PASS:
  - `validation_status=ok`
  - `live_order_placement_enabled=false`
  - `live_beta_gate_status=blocked`
  - `live_beta_gate_block_reasons=live_order_placement_disabled,missing_config_intent,missing_cli_intent,kill_switch_active,geoblock_unknown,later_phase_approvals_missing`
- `cargo fmt --check` PASS.
- `cargo test --offline` PASS: 220 lib tests + 8 main tests.
- `cargo clippy --offline -- -D warnings` PASS.
- `git diff --check` PASS.
- Trailing whitespace check over edited Markdown/Rust/TOML files PASS.
- `.env` gitignore guard PASS.
- `config/local.toml` gitignore guard PASS.

## Safety And No-Secret Scans

Commands run:

```bash
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
rg -n -i --hidden -g '!.git' -g '!target' -g '!.env' -g '!config/local.toml' "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
```

Expected hits only:

- existing LB6 gated canary `post_order` and exact single-order cancel/readback paths;
- `DELETE /order`, `/data/order/{orderID}`, and `/data/orders` readback/cancel strings;
- paper order/cancel simulation and reporting paths;
- disabled live-order gate strings;
- approved secret handle names and L2 header names, not values;
- public placeholder addresses, public canary/order/condition IDs already recorded in LB6 evidence, and public Pyth/Chainlink IDs;
- safety scan command text in docs/verification notes.

No new live order, live cancel, cancel-all, secret value, API-key value, seed phrase, wallet/private-key material, geoblock bypass, strategy-to-live route, broader order type, multi-order path, cap increase, or market/asset expansion was found in the LB7 diff.

## Result

LB7 PASS for handoff/runbook/observability readiness only.

Next gate: review/merge the LB7 PR. Any future live-beta expansion, repeated canary, live cancel, order placement, strategy-selected live trading, cap reset, or production rollout requires a new explicit human/operator approval and a new milestone scope.
