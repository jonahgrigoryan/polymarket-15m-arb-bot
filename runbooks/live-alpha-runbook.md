# Live Alpha Runbook

Live Alpha is approval-gated by phase. LA2 adds heartbeat state, user-event parsing, and startup recovery checks only. It does not authorize live order placement, live cancels, cancel-all, controlled fill canaries, maker autonomy, strategy-selected live trading, or LA3 work.

## Scope Boundary

- `LIVE_ORDER_PLACEMENT_ENABLED` remains false.
- The `live-alpha-orders` feature remains off by default.
- Heartbeat POST remains disabled unless explicitly approved for the heartbeat endpoint only.
- User WebSocket support is parser-only in LA2; no live subscription is started by default.
- Unknown venue state halts. LA2 does not submit cancels or run a cancel loop.

## Local Validation

Use local validation before any approved-host check:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline live_heartbeat
cargo test --offline live_user_events
cargo test --offline startup_recovery
```

Expected local posture:

- `live_order_placement_enabled=false`
- `live_alpha_enabled=false`
- `live_alpha_mode=disabled`
- `live_alpha_heartbeat_required=true`
- `live_alpha_gate_status=blocked`

## Startup Recovery

For any non-disabled Live Alpha mode, startup recovery must prove all of the following before live-capable work can continue:

- geoblock passed;
- account preflight passed;
- balance and allowance readback passed;
- open-order readback passed;
- recent-trade readback passed;
- journal replay passed;
- position reconstruction passed;
- reconciliation passed.

Any failed or unknown check produces a halt-required recovery report and durable event plan:

```text
LiveStartupRecoveryStarted
LiveStartupRecoveryFailed
LiveRiskHalt
```

A fully proven startup emits:

```text
LiveStartupRecoveryStarted
LiveStartupRecoveryPassed
```

## Heartbeat

LA2 heartbeat state tracks:

- `heartbeat_id`
- `last_sent_at`
- `last_acknowledged_at`
- `expected_interval_ms`
- `max_staleness_ms`
- `associated_open_orders`
- `heartbeat_enabled`
- `heartbeat_failure_action`

Operational interpretation:

- `healthy`: gate may proceed only if every other gate also passes.
- `not_started`, `unknown`, `rejected`, or `stale`: live-capable modes stay blocked.
- stale or rejected heartbeat must trigger halt-and-reconcile handling before any resume.

Inspect heartbeat-related code and evidence:

```text
rg -n -i "postHeartbeat|heartbeat|LiveHeartbeatStale" src runbooks verification
cargo test --offline live_heartbeat
```

## User Events

LA2 parses official user-channel order and trade events from fixtures:

- order `PLACEMENT`
- order `UPDATE`
- order `CANCELLATION`
- trade `MATCHED`
- trade `MINED`
- trade `CONFIRMED`
- trade `RETRYING`
- trade `FAILED`

Inspect parser coverage:

```text
cargo test --offline live_user_events
rg -n -i "wss://ws-subscriptions-clob|MATCHED|MINED|CONFIRMED|RETRYING|FAILED" src/live_user_events.rs
```

## Approved-Host Read-Only Check

Run only with explicit approval and approved host/session context. Do not print secret values or auth headers.

```text
cargo run -- --config <approved-live-alpha-config> validate --live-readback-preflight
```

Record exact output in a verification note using counts and statuses only:

- geoblock country/region and blocked state;
- open-order count;
- recent-trade count;
- balance and allowance pass/fail;
- heartbeat state;
- recovery/reconciliation result.

## LA7 Taker Gate Evidence Ladder

LA7 has four separate GO states. Do not collapse them. Missing evidence at any step means the current state remains `NO-GO` for live taker execution.

### GO-ready locally

This is local/shadow readiness only. It does not authorize approved-host dry-runs or live taker orders.

Required commands:

```text
cargo test --offline live_taker_gate
cargo test --offline fee_model
cargo test --offline depth_check
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker
git diff --check
```

Required artifacts:

- `verification/YYYY-MM-DD-live-alpha-la7-taker-gate.md`
- `reports/sessions/<shadow-run-id>/shadow_taker_report.json`
- `reports/sessions/<shadow-run-id>/shadow_taker_decisions.jsonl`

The verification note must state that taker remains disabled by default, `LIVE_ORDER_PLACEMENT_ENABLED=false` for local evidence, maker/taker P&L is separated in reports, and no live order/cancel/cancel-all occurred.

### GO for approved dry-run

This is approved-host read-only plus dry-run readiness. It still does not authorize live taker submission.

Required approved-host/read-only command:

```text
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-YYYY-MM-DD-wallet-baseline-001
```

Required artifacts:

- `artifacts/live_alpha/LA7-YYYY-MM-DD-wallet-baseline-001/account_baseline.redacted.json`
- `artifacts/live_alpha/LA7-YYYY-MM-DD-wallet-baseline-001/orders.redacted.json`
- `artifacts/live_alpha/LA7-YYYY-MM-DD-wallet-baseline-001/trades.redacted.json`
- `artifacts/live_alpha/LA7-YYYY-MM-DD-wallet-baseline-001/balances.redacted.json`
- `artifacts/live_alpha/LA7-YYYY-MM-DD-wallet-baseline-001/positions.redacted.json`
- `verification/YYYY-MM-DD-live-alpha-la7-approval.md`

The dated verification note must record current official-doc/API evidence for authentication, user orders/trades readback, fees, market/order semantics, geoblock, and rate limits; approved host and wallet/funder evidence; `baseline_id` and `baseline_hash`; startup recovery and reconciliation binding to that baseline; position evidence status; geoblock/compliance PASS; heartbeat PASS; inventory/risk PASS; and zero open orders/reserved pUSD before dry-run.

The approval artifact for dry-run must bind the host, wallet/funder, baseline ID/hash, market slug, condition ID, token ID, side, outcome, worst price, max size/notional/fee/slippage, no-trade cutoff, no retry loop, and immediate reconciliation requirement. Do not invent an approval ID.

This branch has only the dry-run LA7 taker canary surface. Do not substitute `live-alpha-fill-canary`, `live-canary`, or any maker/quote-manager command. The dry-run command shape is:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-taker-canary --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-alpha-la7-approval.md
```

The dry-run command must reject missing `--approval-id`/`--approval-artifact`, non-final approval artifacts, mismatched baselines, non-BUY sides, retry-after-ambiguous-submit, batch orders, cancel-all, missing depth, stale book/reference, near-close markets, and any failed geoblock/readback/heartbeat/reconciliation/inventory/live-risk gate. It writes dry-run evidence only and must not submit, sign, cancel, batch, use FOK/FAK, or retry.

### GO for one approved live taker canary

This is exactly one live taker canary after the approved dry-run passes. It remains `NO-GO` until a separate live approval artifact exists and the reviewed live command passes fresh gates naturally.

The live command shape is:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-taker-canary --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-alpha-la7-live-approval.md --approval-sha256 sha256:<exact-artifact-hash>
```

Required live approval artifact fields include every dry-run approval field plus:

```text
approval_expires_at_unix
dry_run_report_path
dry_run_report_sha256
dry_run_decision_path
dry_run_decision_sha256
```

The live path must reject the dry-run approval artifact. It must bind the exact live approval hash supplied on the CLI, reject expired approvals, verify the dry-run report and decision hashes, prove the dry-run report still says `status=passed`, `block_reasons=[]`, `not_submitted=true`, baseline/reconciliation passed, zero positions/open orders/reserved pUSD, and no submit/sign/cancel/batch/FOK/FAK/retry occurred.

Before submission it must rerun geoblock, authenticated readback, baseline, heartbeat, inventory, reconciliation, fresh market, fresh book, fresh reference, depth, fee, slippage, worst-price, no-near-close, and live risk gates. Snapshot freshness must be aged at the final pre-submit decision point, after book/reference/predictive probes. It must reserve the one-order cap with create-new semantics before the official SDK submit call. The submit path is one BUY FAK market order through `polymarket_client_sdk_v2` with the approved worst-price limit and share amount; it is not batch, GTC/GTD, post-only, cancel-all, or retry-after-ambiguous-submit. After submission it must immediately run authenticated readback and baseline-aware reconciliation, write `live_alpha_taker_canary_live_report.json`, and leave the cap artifact consumed even if network ambiguity, cap update, or post-submit checks fail.

Operators must not run live taker execution without the exact live approval artifact, exact approval hash, fresh approved-host evidence, strict worst-price/depth/slippage/fee gates, no near-close unless separately approved, no retry after ambiguous submit, immediate readback, and one-order cap reservation.

### GO for post-canary LA7 exit

LA7 post-canary exit can be considered only after the canary, if performed, reconciles cleanly.

Required post-run evidence:

- exact run ID and approval ID;
- order ID, trade ID, market/condition/token/side/outcome, price, size, notional, and status;
- estimated fee, actual fee, fee delta, and fee-rate source;
- measured slippage and EV-after-costs at submit/fill;
- maker/taker P&L split;
- immediate reconciliation result bound to the journal and baseline artifact;
- post-run open-order count, reserved pUSD, available pUSD, and position evidence;
- explicit decision that taker remains disabled, remains shadow-only, or is ready for the next reviewed step.

If a reviewed `live-alpha-reconcile` command exists, run it after the canary:

```text
cargo run -- --config <approved-live-alpha-config> live-alpha-reconcile --run-id <run_id>
```

Until that command exists, use the approved-host read-only preflight plus the reconciliation runbook and record the gap:

```text
cargo run -- --config <approved-live-alpha-config> validate --live-readback-preflight
```

## Resume Requirements

Resume from halt only after:

- the halt reason is documented;
- journal replay and reconciliation pass;
- open orders and recent trades are read back;
- heartbeat is healthy if required;
- geoblock/account/balance/allowance checks pass;
- the next phase has explicit human/operator approval.

Do not resume by cancel-all, live order submission, autonomous cancel loops, or starting LA3.
