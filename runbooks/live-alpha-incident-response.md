# Live Alpha Incident Response

Use this runbook when LA2 detects stale heartbeat, unknown startup state, readback/reconciliation mismatch, or any other live safety ambiguity. LA2 response is halt and investigate. It does not authorize live order placement, live cancels, cancel-all, controlled fill canaries, maker autonomy, strategy-selected live trading, or LA3 work.

## Immediate Halt

1. Stop live-capable work with the safest available operator control.
2. Preserve logs, journal files, and validation output.
3. Do not submit new orders.
4. Do not submit live cancels from LA2.
5. Do not use cancel-all.
6. Do not start an autonomous cancel loop.

Durable events for halt evidence:

```text
LiveHeartbeatStale
LiveStartupRecoveryFailed
LiveRiskHalt
```

## Stale Heartbeat

Heartbeat is unsafe when it is not started, unknown, rejected, or stale while heartbeat is required.

Procedure:

1. Record the last sent and acknowledged timestamps.
2. Record the current `heartbeat_id` value as a non-secret operational ID.
3. Record associated open-order IDs using redaction if needed.
4. Run local heartbeat tests if code changed:

```text
cargo test --offline live_heartbeat
```

5. Run read-only open-order and recent-trade inspection only from an approved host/session.
6. Reconcile before any resume.

LA2 heartbeat POST remains disabled unless explicit approval is given for only the heartbeat endpoint.

## Startup Recovery Failure

Startup recovery failure or unknown state must halt.

Capture:

- geoblock status;
- account preflight status;
- balance/allowance status;
- open-order readback status;
- recent-trade readback status;
- journal replay status;
- position reconstruction status;
- reconciliation mismatch list.

Local checks:

```text
cargo test --offline startup_recovery
cargo test --offline live_reconciliation
```

## Unknown Open Order

Unknown open order means venue readback shows an open order that the local journal cannot prove.

Procedure:

1. Keep live-capable gates blocked.
2. Record read-only open-order details.
3. Record recent-trade details.
4. Reconstruct journal state for the run.
5. Record `unknown_open_order` in the incident note.
6. Wait for human/operator approval for any next action.

No LA2 procedure may keep the unknown order alive intentionally, submit a replacement order, cancel all orders, or run autonomous cancel logic.

## Incident Note Template

```text
incident_id:
run_id:
detected_at_utc:
phase: LA2
trigger:
heartbeat_state:
startup_recovery_status:
open_order_count:
recent_trade_count:
balance_allowance_status:
reconciliation_mismatches:
durable_events:
operator_decision:
resume_allowed: false
```

## Resume Checklist

Resume remains blocked until:

- geoblock passes;
- account and approved-host context are confirmed;
- balance/allowance readback passes;
- open orders and recent trades reconcile with the journal;
- heartbeat is healthy if required;
- `LiveStartupRecoveryPassed` is recorded or planned;
- a human/operator explicitly approves the next phase.

Starting LA3 requires a separate branch and approval after LA2 is merged. LA2 completion alone does not authorize LA3.
