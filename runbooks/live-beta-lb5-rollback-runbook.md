# Live Beta Rollback Runbook

Date: 2026-05-03
Scope: LB5 cancel readiness and rollback minimum, updated by LB7 with LB6 one-order canary closeout lessons.

This runbook does not authorize another canary order, order posting, live cancel proof, cancel-all, autonomous trading, strategy-to-live routing, wider markets/assets, higher caps, or taker/FOK/FAK/marketable-limit paths. `LIVE_ORDER_PLACEMENT_ENABLED=false` remains required unless a later approved phase changes that boundary.

## Preconditions

- LB4 approved-host readback/account preflight is PASS and recorded.
- LB5 cancel readiness tests and this runbook are reviewed.
- LB6 one-order canary closeout is recorded before this LB7 update: one canary was submitted, the exact order was canceled, no fill occurred, post-cancel open orders were `0`, post-cancel reserved pUSD was `0`, and the local one-order cap is consumed.
- No additional live order or live cancel may occur without a new explicit approval record and milestone scope.
- The operator has an approved config with secret handles only. No private keys or credential values may be printed, logged, pasted, or committed.

## Kill Switch

Use the kill switch before any rollback action that follows a live canary incident:

```bash
LIVE_ORDER_PLACEMENT_ENABLED=false
```

Confirm runtime validation still reports:

```text
live_order_placement_enabled=false
live_beta_gate_status=blocked
```

If a service exists, keep the config `live_beta.kill_switch_active = true` before restart. Do not open a new order while the kill switch is active.

## Service Stop

For a production-like `systemd` deployment, stop the service before manual reconciliation:

```bash
sudo systemctl stop polymarket-15m-arb-bot
sudo systemctl status polymarket-15m-arb-bot --no-pager
```

For a foreground process, send `Ctrl-C`, wait for graceful shutdown output, and save the terminal transcript as an artifact.

## Open-Order Readback

Before any future approved live cancel action, read back account state from the approved host:

```bash
set -a && source .env && set +a
cargo run -- --config config/local.toml validate --live-readback-preflight
```

Required operator checks:

- geoblock gate is `passed`.
- readback preflight is `passed`.
- exactly one canary order is open if cancel is needed.
- no unexpected open orders exist.
- heartbeat state is healthy or the heartbeat failure is recorded as the incident trigger.
- order ID, market, asset ID, side, original size, matched size, and reserved balance are captured.

If readback is blocked, unknown, stale, malformed, rate-limited, or inconsistent, stop and write an incident note. Do not guess order state.

For exact single-order readback, use the current Rust path:

```text
GET /data/order/{orderID}
```

The LB6 closeout found that the older Rust assumption `GET /order/{orderID}` returned `404` for the live canary order, while the official `py_clob_client_v2` client successfully read back that same order and the Rust parser now matches `/data/order/{orderID}`. Keep this path explicit in future evidence.

If Rust readback and official SDK readback disagree:

- Stop all live actions and preserve both sanitized outputs.
- Do not submit, cancel, retry, or broaden the action surface based on only one client.
- Compare order ID, market/condition ID, token ID, side, price, original size, matched size, order type, status, maker/funder, open-order list, reserved pUSD, and trade list.
- Treat the stricter state as authoritative for safety. If either client sees an open or matched order, keep the beta halted until human review.
- Patch or document the Rust compatibility gap before any further approved live action.

## Cancel Plan

LB5 only builds and tests the disabled single-order cancel readiness path. It does not perform a live cancel.

For a future explicitly approved single-order cancel, if the one approved post-only GTD canary remains open:

- Cancel only the exact approved canary order ID.
- Use single-order cancellation only: `DELETE /order` with body field `orderID`.
- Do not use cancel-all.
- Do not cancel by market.
- Do not cancel more than one order.
- Immediately read back the order after the cancel attempt.
- Confirm reserved pUSD is released or record the mismatch.
- Read back trades and balances. If the order filled before cancel, reconcile trade status, transaction hash, fees, and settlement instead of treating cancel status as sufficient.

If cancel fails, is rate-limited, returns auth/geoblock errors, reports unknown state, or returns a response where the order is neither canceled nor clearly terminal, stop the service, preserve artifacts, and escalate using the incident note template below.

The LB6 closeout used official `py_clob_client_v2` single-order behavior only after Rust dry-run readback failed closed: official SDK readback showed the exact canary order as `LIVE` with matched size `0`, official single-order cancel returned that exact order ID in `canceled` and `{}` in `not_canceled`, and post-cancel official readback showed `CANCELED` with no open canary-token orders. This is lifecycle evidence only, not profitability evidence.

## Heartbeat Failure Handling

If heartbeat becomes unhealthy, missing, stale, or ambiguous:

- Stop opening new orders.
- Keep the service stopped until a human review decides whether to resume heartbeat.
- Use readback to determine whether the venue auto-canceled any open order.
- Do not assume auto-cancel succeeded without order readback and reserved-balance reconciliation.

## Incident Note Template

Create a dated note under `verification/` with:

```text
Date:
Branch/commit:
Host/session:
Operator:
Run ID:
Geoblock result:
LIVE_ORDER_PLACEMENT_ENABLED:
Kill switch state:
Service stop command/result:
Open-order readback result:
Order ID:
Market/condition ID:
Asset/token ID:
Side/price/size/notional:
Order type/expiry:
Heartbeat state:
Cancel decision:
Rust exact readback path:
Rust readback result:
Official SDK readback result:
Rust/SDK mismatch:
Cancel request status:
Cancel response summary:
Post-cancel readback:
Trades/trade statuses:
Transaction hashes:
Balance/reserved-balance delta:
Fees:
Settlement follow-up:
Blocker or incident summary:
Next approval needed:
```

Do not include private keys, raw L2 credentials, API-key values, signatures, or credential-derived values.

## Artifact Checklist

- Git commit and branch.
- `STATUS.md` state before and after the incident.
- Approved config path with secret handle names only.
- Geoblock output.
- Readback preflight output.
- Open-order readback output.
- Exact single-order readback output from `GET /data/order/{orderID}`.
- Official SDK readback output if used for closeout or disagreement triage.
- Cancel readiness output.
- Exact single-order cancel output from `DELETE /order`, if a future approved milestone allows the cancel.
- Heartbeat output or blocker.
- Service stop output.
- Balance and reserved-balance output.
- Trade readback output with terminal statuses and transaction hashes when present.
- Settlement reconciliation note if any fill occurred.
- Safety/no-secret scan output.

## Post-LB7 Hold

Stop after LB7 handoff. The LB6 one-order cap is consumed. Any new live action requires a new explicit human/operator approval, a new milestone scope, a fresh geoblock/account preflight, and a fresh incident/rollback artifact plan.
