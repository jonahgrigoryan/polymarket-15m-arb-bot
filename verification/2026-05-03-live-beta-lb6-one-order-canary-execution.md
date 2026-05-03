# 2026-05-03 LB6 One-Order Canary Execution

## Scope

Execute the reviewed LB6 pre-authorized one-order canary from updated `main` after PR #25 merged, then close it out with exact single-order cancellation and post-cancel readback.

No cancel-all, autonomous live trading, strategy-to-live routing, secret value, API-key value, seed phrase, or wallet/private-key material was added to repo/docs/logs/tests.

## Pre-Submit Gates

- Local `main` fast-forwarded to PR #25 merge commit `1d0f40a`.
- One-order cap sentinel was absent before submission.
- Approved-host LB4 account preflight passed before submission:
  - Run ID: `18abe5ab4bdc0780-16cec-0`
  - Geoblock: `passed`, `MX/CMX`
  - Open orders: `0`
  - Reserved pUSD units: `0`
  - Available pUSD units: `1614478`
  - Venue state: `trading_enabled`
  - Heartbeat: `not_started_no_open_orders`

## Canary Submission

The final pre-authorized envelope selected:

- Market slug: `eth-updown-15m-1777767300`
- Condition ID: `0x6455382f705a0cb742cab86603f6ade14a67442bd0cd7debcef18fb3f8bae8b1`
- Token ID: `108754796712694987030496168190461335721943804518169337367002311107585620439355`
- Outcome: `Up`
- Side: `BUY`
- Order type: `GTD`, post-only maker-only
- Price: `0.01`
- Size: `5`
- Notional: `0.05 pUSD`
- GTD expiry unix: `1777768020`
- Market end unix: `1777768200`
- Best bid: `0.35`
- Best ask: `0.37`
- Book age: `0 ms`
- Reference age: `987 ms`

`live-canary --preauthorized-envelope --one-order` passed all runtime gates:

- Run ID: `18abe606c2c70218-17c1f-0`
- Status: `ready_for_one_order_canary`
- Block reasons: none
- LB4 preflight: `true`
- Open orders before submission: `0`
- L2 handles present: `true`
- Canary private-key handle present: `true`
- One-order cap remaining before submission: `true`
- Approval hash: `sha256:04fe06d40a0e7e1b348878207c75bf1d5cb325c14ba8db7885fdf8e5b716a7ef`

Submission result:

- Status: `submitted`
- Venue order ID: `0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf`
- Venue status: `LIVE`
- Success: `true`
- Submitted order count: `1`

## Exact Cancel Closeout

Initial Rust `live-cancel --dry-run` did not send a cancel. It failed during exact readback because the Rust readback path still used `GET /order/{orderID}`, which returned HTTP `404` for this live order.

The official `py_clob_client_v2` client successfully read back the exact same order through its current endpoint:

- Pre-cancel status: `LIVE`
- Size matched: `0`
- Market, token, side, price, size, maker/funder, and order type matched the canary envelope.

The official client then sent a single exact-order cancel only:

- Method surface: `cancel_order(OrderPayload(orderID=<exact canary order id>))`
- Canceled IDs: `["0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf"]`
- Not canceled: `{}`
- Cancel-all was not used.

Post-cancel official readback:

- Order status: `CANCELED`
- Size matched: `0`
- Open orders by canary token: `[]`

Post-cancel LB4 account preflight passed:

- Run ID: `18abe61f679e9770-17f1b-0`
- Geoblock: `passed`, `MX/CMX`
- Open orders: `0`
- Reserved pUSD units: `0`
- Available pUSD units: `1614478`
- Trade count: `14`
- Venue state: `trading_enabled`
- Heartbeat: `not_started_no_open_orders`

## Follow-Up Patch

This closeout branch fixes the Rust exact-order readback shape found during the live canary:

- Exact order readback path is now `/data/order/{orderID}`, matching the official client behavior observed live.
- Single-order parser accepts current live SDK status strings such as `LIVE` and `CANCELED`.
- Single-order parser treats single-order `original_size` / `size_matched` as human decimal sizes, while paginated open-order readback keeps its existing fixed-unit parser.

Post-fix Rust dry-run readback against the canceled canary order passed transport/parsing and blocked correctly:

- Run ID: `18abe64ecbbe5270-262-0`
- Status: `blocked`
- Block reasons: `human_cancel_approval_missing,order_already_canceled`
- Order status: `canceled`
- Single cancel method remains `DELETE`
- Single cancel path remains `/order`
- Single order readback path is `/data/order/0x978bc4ba61cb0d4fefb55fd08ce594245ccc2678605ed17a3a4f593e4e89acdf`

## Current Outcome

LB6 one-order canary execution is complete:

- Exactly one live canary order was submitted.
- The canary order was canceled.
- No fill occurred.
- Post-cancel readback shows zero open orders and zero reserved pUSD.
- The local one-order cap sentinel is consumed and records the venue order ID.

## Verification

- `cargo fmt --check` PASS.
- `cargo test --offline readback` PASS: 29 lib tests + 1 main test.
- `cargo test --offline cancel` PASS: 22 lib tests.
- `cargo test --offline canary` PASS: 22 lib tests.
- `cargo run --offline -- --config config/default.toml validate --local-only` PASS, run ID `18abe6750b48a910-9e1-0`.
- `cargo test --offline` PASS: 218 lib tests + 8 main tests.
- `cargo clippy --offline -- -D warnings` PASS.
- `git diff --check` PASS.
- Safety/no-secret scans PASS with expected hits only: existing gated canary `post_order` path, exact single-order cancel/readback path, paper order/cancel simulation paths, disabled live-order gate strings, public condition/feed/order IDs, public Chainlink/Pyth IDs, header names, and secret handle names.
- Ignored-local guard PASS: `.env`, `config/local.toml`, and `reports/live-beta-lb6-one-order-canary-state.json` are gitignored.
