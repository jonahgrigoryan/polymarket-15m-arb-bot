# Live Alpha Reconciliation Runbook

This runbook covers LA2 journal, readback, and reconciliation handling. LA2 must fail closed on unknown or unproven live state. It does not authorize live order placement, live cancels, cancel-all, controlled fill canaries, maker autonomy, strategy-selected live trading, or LA3 work.

## Inspect Journal

Use the configured journal path for the run under review. Keep output free of secret values.

```text
jq -c 'select(.run_id == "<run-id>")' <journal.jsonl>
rg -n '"event_type":"live_startup_recovery_failed"|"event_type":"live_heartbeat_stale"|"event_type":"live_risk_halt"' <journal.jsonl>
```

Check for the durable LA2 events:

- `LiveStartupRecoveryStarted`
- `LiveStartupRecoveryPassed`
- `LiveStartupRecoveryFailed`
- `LiveHeartbeatStale`
- `LiveRiskHalt`

## Inspect Open Orders

Use approved-host read-only account readback only after approval:

```text
cargo run -- --config <approved-live-alpha-config> validate --live-readback-preflight
```

Expected evidence to record:

- open-order count;
- order IDs redacted or shortened if needed;
- order statuses;
- matched size;
- remaining size;
- market and asset identifiers.

If any open order is not known in the local journal, treat it as `unknown_open_order` and halt.

## Inspect Recent Trades

Recent trades must be compared against local journal trade/order mappings.

Record:

- trade count;
- trade lifecycle status;
- related order ID when available;
- transaction hash presence without exposing credentials.

Nonterminal venue trade statuses `MATCHED`, `MINED`, and `RETRYING` are not final fill evidence. LA2 reconciliation keeps them fail-closed until the venue state is proven terminal.

## Inspect Balances And Allowances

Record balance and allowance pass/fail from read-only preflight. Do not print raw auth headers or secret values.

Reconcile:

- available pUSD;
- reserved pUSD from open orders;
- conditional-token balances;
- local position reconstruction.

Any balance, reserved-balance, allowance, or conditional-token drift that cannot be explained by the journal must halt.

## Reconciliation Checks

Run focused tests locally:

```text
cargo test --offline live_reconciliation
cargo test --offline startup_recovery
```

Fail-closed mismatch categories include:

- unknown open order;
- missing venue order;
- unknown venue order status;
- unexpected fill;
- unexpected partial fill;
- cancel not confirmed;
- reserved balance mismatch;
- balance delta mismatch;
- position mismatch;
- missing venue trade;
- unknown venue trade status;
- nonterminal venue trade status;
- failed trade status;
- trade/order mismatch;
- SDK/Rust disagreement.

## Unknown Open Order Procedure

1. Stop live-capable work through the gate or kill switch.
2. Capture journal events for the run.
3. Capture approved-host read-only open orders and recent trades.
4. Run reconciliation locally.
5. Record `LiveStartupRecoveryFailed` and `LiveRiskHalt` evidence.
6. Escalate for human/operator decision.

Do not cancel the order in LA2 unless a separate approved phase explicitly authorizes that exact cancel path. Do not use cancel-all.

## Reconciliation Pass Procedure

LA2 can mark recovery passed only when:

- all startup recovery checks pass;
- no unknown open order exists;
- no unreconciled fill exists;
- no nonterminal trade is treated as final;
- heartbeat is healthy if required;
- durable recovery events are present or planned for the run.
