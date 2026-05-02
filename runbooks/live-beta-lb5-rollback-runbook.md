# Live Beta LB5 Rollback Runbook

Date: 2026-05-02
Scope: LB5 cancel readiness and rollback minimum before any LB6 canary order.

This runbook does not authorize LB6, order posting, live cancel proof, cancel-all, autonomous trading, or strategy-to-live routing. `LIVE_ORDER_PLACEMENT_ENABLED=false` remains required until a later approved phase changes it.

## Preconditions

- LB4 approved-host readback/account preflight is PASS and recorded.
- LB5 cancel readiness tests and this runbook are reviewed.
- LB6 human approval is not assumed. No live order or live cancel may occur from LB5.
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

Before any LB6 cancel action, read back account state from the approved host:

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

## Cancel Plan

LB5 only builds and tests the disabled single-order cancel readiness path. It does not perform a live cancel.

For LB6 only, if the one approved post-only GTD canary remains open:

- Cancel only the exact approved canary order ID.
- Use single-order cancellation only.
- Do not use cancel-all.
- Do not cancel by market.
- Do not cancel more than one order.
- Immediately read back the order after the cancel attempt.
- Confirm reserved pUSD is released or record the mismatch.
- Read back trades and balances. If the order filled before cancel, reconcile trade status, transaction hash, fees, and settlement instead of treating cancel status as sufficient.

If cancel fails, is rate-limited, returns auth/geoblock errors, reports unknown state, or returns a response where the order is neither canceled nor clearly terminal, stop the service, preserve artifacts, and escalate using the incident note template below.

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
- Cancel readiness output.
- Heartbeat output or blocker.
- Service stop output.
- Balance and reserved-balance output.
- Trade readback output with terminal statuses and transaction hashes when present.
- Settlement reconciliation note if any fill occurred.
- Safety/no-secret scan output.

## LB6 Hold

Stop after LB5. LB6 requires separate explicit human/operator approval for the exact canary order plan and must still keep the scope to one post-only tiny GTD maker canary.
