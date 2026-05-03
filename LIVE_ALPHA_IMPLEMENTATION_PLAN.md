# LIVE_ALPHA_IMPLEMENTATION_PLAN.md

# Implementation Plan: Polymarket Live Alpha Release Gate

**Repo:** `jonahgrigoryan/polymarket-15m-arb-bot`
**Base state:** `main` at `26144dc`, after PR #27 / LB7 handoff
**Related PRD:** `LIVE_ALPHA_PRD.md`
**Date:** 2026-05-03
**Status:** Draft for approval
**Scope:** Post-LB7 phased implementation plan
**Primary goal:** Progress from one non-filled post-only canary to controlled live fill proof, durable reconciliation, shadow live execution, and tiny maker-only micro autonomy.

---

## 1. Purpose

This document decomposes `LIVE_ALPHA_PRD.md` into phase-gated implementation work.

The Live Beta sequence LB0-LB7 is complete. It proved:

- fail-closed live-mode gates,
- secret handling and redaction,
- signing dry run,
- authenticated readback,
- cancel readiness,
- one human-approved post-only GTD maker canary,
- successful cancel/readback handoff,
- runbook and observability hardening.

It did **not** prove:

- live fill accounting,
- trade-state reconciliation,
- fee accounting,
- balance delta reconciliation,
- conditional token position accounting,
- partial-fill handling,
- cancel-after-partial-fill handling,
- settlement follow-up,
- autonomous quote lifecycle management.

LA0 is the post-LB7 scope-lock and approval gate only. It does not authorize live orders, live cancels, cancel-all, strategy-selected live trading, cap reset, or live execution expansion. LA1 and LA2 must pass before any controlled fill canary; LA3 is the first possible controlled fill canary and only after explicit approval; LA5 or later is the first possible maker-only micro autonomy and only after prior evidence gates. Strategy-selected live trading remains deferred behind a separate robustness gate and explicit approval.

This implementation plan is therefore intentionally focused on **state correctness before strategy autonomy**.

The next engineering target is not:

```text
remove the canary cap and start autonomous trading
```

The target is:

```text
prove one real fill end-to-end,
then shadow the live executor,
then enable tiny maker-only autonomy under strict caps.
```

---

## 2. Strategy Boundary

The strategy remains:

```text
resolution-source-informed market making on short-duration prediction markets
```

The initial live execution posture is:

```text
maker-only
post-only
GTD
BTC/ETH/SOL 15-minute markets only
dedicated Live Alpha wallet only
strictly capped
fail-closed
```

Selective taker execution is a later gate. It is not part of the initial maker-only Live Alpha rollout.

---

## 3. Global Rules

These rules apply to every phase.

1. Do not start a phase until the previous phase has a dated verification note.
2. Do not skip hold points.
3. Keep default builds unable to place live orders.
4. Do not globally flip `LIVE_ORDER_PLACEMENT_ENABLED=true`.
5. Any live-placement-enabled build must require an explicit non-default compile-time feature plus runtime config plus CLI intent plus approval artifact.
6. Do not bypass geoblock checks.
7. Do not trade from restricted regions.
8. Do not store secrets in repo files, configs, logs, reports, screenshots, verification notes, or shell history.
9. Do not print private keys, API secrets, passphrases, signed payloads, auth headers, or unredacted SDK responses.
10. Use a dedicated Live Alpha wallet only.
11. Halt on ambiguous venue, geoblock, account, heartbeat, order, trade, balance, position, or settlement state.
12. Treat network ambiguity after submit or cancel as dangerous; reconcile before doing anything else.
13. Never assume a submit failed because the HTTP response failed.
14. Never assume a cancel succeeded until venue readback confirms it.
15. Never sell conditional tokens unless local and venue position state prove inventory exists.
16. Do not add batch order placement in Live Alpha unless separately approved.
17. Do not add cancel-all as a normal runtime behavior. It may exist only as a separately approved incident procedure.
18. Do not use paper or replay P&L as evidence of live profitability.
19. Do not use fill count as a success metric without fill quality, adverse selection, fees, and P&L.
20. Recheck official Polymarket docs before any phase that depends on order types, heartbeat, fees, WebSocket semantics, or geographic eligibility.

---

## 4. Compile-Time and Runtime Gating Policy

The existing repo has:

```rust
pub const LIVE_ORDER_PLACEMENT_ENABLED: bool = false;
```

Do **not** replace this with an unconditional `true`.

Recommended approach:

```rust
#[cfg(feature = "live-alpha-orders")]
pub const LIVE_ORDER_PLACEMENT_ENABLED: bool = true;

#[cfg(not(feature = "live-alpha-orders"))]
pub const LIVE_ORDER_PLACEMENT_ENABLED: bool = false;
```

Then still require all runtime gates:

```text
LIVE_ORDER_PLACEMENT_ENABLED == true
LIVE_ALPHA_ENABLED == true
LIVE_ALPHA_MODE is approved for the phase
config intent enabled
CLI intent enabled
kill switch inactive
geoblock PASS
account preflight PASS
heartbeat PASS
live-alpha approval artifact present
phase-specific approval present
risk limits PASS
reconciliation PASS
```

Default commands such as the following must remain unable to place orders:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline
cargo run --offline -- --config config/default.toml paper
```

Only explicitly approved live-alpha runs may use:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> <approved-live-command>
```

The feature must not be enabled by default in `Cargo.toml`.

---

## 5. Phase Order

| Phase | Name | Live order allowed? | Autonomous? | Mandatory hold? |
|---|---|---:|---:|---:|
| LA0 | Approval and scope lock | No | No | Yes |
| LA1 | Live Alpha gates, journal, reconciliation foundation | No | No | No |
| LA2 | Heartbeat, user events, and crash recovery | No order placement | No | No |
| LA3 | Controlled live fill canary | One approved fill attempt | No | Yes |
| LA4 | Shadow live executor | No | No | No |
| LA5 | Maker-only micro autonomy | Yes, tiny post-only GTD only | Yes, tightly capped | Yes |
| LA6 | Quote manager and cancel/replace | Yes, tightly capped | Yes, tightly capped | Yes |
| LA7 | Selective taker gate | Separately approved only | Limited | Yes |
| LA8 | Scale decision report | TBD | TBD | Yes |

---

## 6. Required Verification Baseline

Every phase must run the checks that apply to changed files plus:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
git status --short --branch
```

Every phase must also run focused safety scans.

Order/cancel/live behavior scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading|FOK|FAK|GTD|GTC|post[_ -]?only)" src Cargo.toml config runbooks *.md
```

Secret scan:

```text
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
```

Live-alpha gate scan:

```text
rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
```

Any new scan hit must be explained in the phase verification note. A scan hit is not automatically a blocker if the current phase explicitly allows that code and it is proven unreachable without the required gates.

---

## 7. Artifact and Directory Conventions

Recommended new paths:

```text
LIVE_ALPHA_PRD.md
LIVE_ALPHA_IMPLEMENTATION_PLAN.md

src/execution_intent.rs
src/live_alpha_config.rs
src/live_alpha_gate.rs
src/live_order_journal.rs
src/live_reconciliation.rs
src/live_position_book.rs
src/live_balance_tracker.rs
src/live_heartbeat.rs
src/live_user_events.rs
src/live_executor.rs
src/live_risk_engine.rs
src/live_quote_manager.rs
src/live_taker_gate.rs
src/live_alpha_metrics.rs

runbooks/live-alpha-runbook.md
runbooks/live-alpha-fill-canary-runbook.md
runbooks/live-alpha-reconciliation-runbook.md
runbooks/live-alpha-rollback-runbook.md
runbooks/live-alpha-incident-response.md

verification/YYYY-MM-DD-live-alpha-la0-approval-scope.md
verification/YYYY-MM-DD-live-alpha-la1-journal-reconciliation.md
verification/YYYY-MM-DD-live-alpha-la2-heartbeat-crash-safety.md
verification/YYYY-MM-DD-live-alpha-la3-controlled-fill-canary.md
verification/YYYY-MM-DD-live-alpha-la4-shadow-live-executor.md
verification/YYYY-MM-DD-live-alpha-la5-maker-micro-autonomy.md
verification/YYYY-MM-DD-live-alpha-la6-quote-manager.md
verification/YYYY-MM-DD-live-alpha-la7-taker-gate.md
verification/YYYY-MM-DD-live-alpha-la8-scale-decision.md
```

Runtime artifacts should be grouped by run ID:

```text
artifacts/live_alpha/<run_id>/config.redacted.toml
artifacts/live_alpha/<run_id>/journal.jsonl
artifacts/live_alpha/<run_id>/preflight.json
artifacts/live_alpha/<run_id>/orders.redacted.json
artifacts/live_alpha/<run_id>/trades.redacted.json
artifacts/live_alpha/<run_id>/balances.redacted.json
artifacts/live_alpha/<run_id>/positions.redacted.json
artifacts/live_alpha/<run_id>/reconciliation.json
artifacts/live_alpha/<run_id>/metrics.prom
```

---

# LA0 — Approval and Scope Lock

## Objective

Create explicit authority for all work beyond LB7.

## Allowed Changes

- Add or update `LIVE_ALPHA_PRD.md`.
- Add this `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`.
- Update `STATUS.md` to say the next phase is Live Alpha approval, not production live trading.
- Add `verification/YYYY-MM-DD-live-alpha-la0-approval-scope.md`.
- Add issue/checklist text if desired.

## Explicitly Disallowed Changes

- Source code changes.
- Config changes that enable live order placement.
- New secrets, wallets, private keys, API keys, passphrases, or signer values.
- New live order placement code.
- New taker code.
- Strategy-to-live routing.
- Any second canary or new live order before LA3 approval.

## Required Implementation Notes

The approval note must record:

```text
approved wallet
approved host
approved assets
approved maximum wallet funding
approved maximum single-order notional
approved maximum daily loss
approved maximum open orders
approved order types per phase
approved canary type for LA3
prohibited phases
rollback owner
monitoring owner
legal/access owner
```

Also record that:

```text
LB6 did not prove live fill lifecycle.
LB7 did not approve expanded beta authority.
Live Alpha is not production rollout.
LA0 approval does not authorize live order placement, live canceling, cancel-all, or live trading.
```

## Verification Commands

```text
git status --short --branch
git diff --check
rg -n "LIVE_ALPHA|Live Alpha|live alpha" LIVE_ALPHA_PRD.md LIVE_ALPHA_IMPLEMENTATION_PLAN.md STATUS.md verification
test ! -e .env || git check-ignore .env
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la0-approval-scope.md
```

Must include:

```text
approved scope
explicit non-authorization of production trading
explicit non-authorization of taker strategy
wallet/host/access status
capital caps
order caps
required next phase
reviewer/approver identity
```

## Exit Gate

LA0 exits only when the PRD and implementation plan are approved as planning artifacts.

Approval of LA0 does not approve any live order.

## Hold Point

Mandatory. Stop after LA0 until human/operator approval to start LA1.

---

# LA1 — Live Alpha Gates, Journal, and Reconciliation Foundation

## Objective

Build the durable live-state foundation before placing additional live orders.

This phase may add live-capable data structures and read-only reconciliation. It must not place orders.

## Allowed Changes

- Live Alpha config section with all live-alpha modes defaulting disabled.
- Live Alpha gate evaluation.
- Compile-time feature scaffold, default off.
- Execution intent type.
- Durable live order journal.
- Live balance tracker.
- Live position book.
- Reconciliation engine.
- Metrics for reconciliation health.
- Tests and fixtures.
- Read-only use of existing LB4 account/order/trade readback if already approved.

## Explicitly Disallowed Changes

- Live order placement.
- Live cancel expansion beyond already approved runbook procedures.
- Taker orders.
- FAK/FOK/marketable limit live calls.
- Strategy-selected live orders.
- Any change that makes default builds capable of live placement.
- Batch orders.
- Cancel-all as normal runtime behavior.

## Implementation Steps

### LA1.1 Add config model

Add:

```text
src/live_alpha_config.rs
```

Implement:

```rust
pub enum LiveAlphaMode {
    Disabled,
    FillCanary,
    Shadow,
    MakerMicro,
    QuoteManager,
    TakerGate,
    Scale,
}
```

Add config sections:

```toml
[live_alpha]
enabled = false
mode = "disabled"
approved_host_required = true
approved_wallet_required = true
geoblock_required = true
heartbeat_required = true

[live_alpha.risk]
max_wallet_funding_pusd = "0.00"
max_available_pusd_usage = "0.00"
max_reserved_pusd = "0.00"
max_single_order_notional = "0.00"
max_per_market_notional = "0.00"
max_per_asset_notional = "0.00"
max_total_live_notional = "0.00"
max_open_orders = 0
max_open_orders_per_market = 0
max_open_orders_per_asset = 0
max_daily_realized_loss = "0.00"
max_daily_unrealized_loss = "0.00"
max_fee_spend = "0.00"
max_submit_rate_per_min = 0
max_cancel_rate_per_min = 0
max_reconciliation_lag_ms = 0
max_book_staleness_ms = 0
max_reference_staleness_ms = 0
no_trade_seconds_before_close = 0

[live_alpha.fill_canary]
enabled = false
human_approval_required = true
max_notional = "0.00"
max_price = "0.00"
allow_fok = false
allow_fak = false
allow_marketable_limit = false

[live_alpha.maker]
enabled = false
post_only = true
order_type = "GTD"
ttl_seconds = 0
min_edge_bps = 0
replace_tolerance_bps = 0
min_quote_lifetime_ms = 0

[live_alpha.taker]
enabled = false
max_notional = "0.00"
min_ev_after_all_costs_bps = 0
max_slippage_bps = 0
max_orders_per_day = 0
```

All defaults must be inert.

### LA1.2 Add gate evaluator

Add:

```text
src/live_alpha_gate.rs
```

Gate inputs:

```text
compile_time_live_enabled
live_alpha_enabled
live_alpha_mode
config_intent_enabled
cli_intent_enabled
kill_switch_active
geoblock_status
account_preflight_status
heartbeat_status
reconciliation_status
approval_status
phase_status
```

Gate output:

```text
allowed: bool
block_reasons: Vec<LiveAlphaBlockReason>
```

Block reasons should include at least:

```text
compile_time_live_disabled
live_alpha_disabled
mode_disabled
missing_config_intent
missing_cli_intent
kill_switch_active
geoblock_blocked
geoblock_unknown
account_preflight_failed
heartbeat_failed
reconciliation_failed
approval_missing
phase_not_approved
```

### LA1.3 Add execution intent type

Add:

```text
src/execution_intent.rs
```

This type should be shared by paper, shadow-live, and live execution.

Minimum fields:

```text
intent_id
strategy_snapshot_id
market_slug
condition_id
token_id
asset_symbol
outcome
side
price
size
notional
order_type
time_in_force
post_only
expiry
fair_probability
edge_bps
reference_price
reference_source_timestamp
book_snapshot_id
best_bid
best_ask
spread
created_at
```

### LA1.4 Add durable journal

Add:

```text
src/live_order_journal.rs
```

Use append-only JSONL unless there is a strong reason not to.

Requirements:

```text
schema_version
run_id
event_id
event_type
created_at
payload
redaction_status
```

Journal writes must be durable enough for crash recovery:

```text
write event
flush
fsync if practical for live event boundaries
```

### LA1.5 Add state reducers

Implement reducers that reconstruct:

```text
LiveIntentState
LiveOrderState
LiveTradeState
LiveBalanceState
LivePositionState
LiveRiskHaltState
```

from the journal.

### LA1.6 Add reconciliation engine

Add:

```text
src/live_reconciliation.rs
```

Inputs:

```text
local journal state
open orders readback
single order readback
trades readback
balance/allowance readback
position readback if available
user WebSocket events if available later
```

Outputs:

```text
ReconciliationPassed
ReconciliationMismatch
RiskHaltRecommended
```

Mismatch categories:

```text
unknown_open_order
missing_venue_order
unexpected_fill
unexpected_partial_fill
cancel_not_confirmed
reserved_balance_mismatch
balance_delta_mismatch
position_mismatch
trade_status_failed
sdk_rust_disagreement
```

### LA1.7 Add tests

Required tests:

```text
cargo test --offline live_alpha_config
cargo test --offline live_alpha_gate
cargo test --offline execution_intent
cargo test --offline live_order_journal
cargo test --offline live_reconciliation
cargo test --offline live_position_book
cargo test --offline live_balance_tracker
cargo test --offline redaction
```

Test cases:

```text
default config cannot place orders
live-alpha disabled blocks all modes
compile-time feature absent blocks placement
journal append/replay reconstructs state
journal redacts secrets
unknown open order triggers halt
unexpected fill triggers halt
reserved balance mismatch triggers halt
position mismatch triggers halt
SDK/Rust disagreement triggers halt
```

## Verification Commands

```text
cargo test --offline live_alpha_config
cargo test --offline live_alpha_gate
cargo test --offline execution_intent
cargo test --offline live_order_journal
cargo test --offline live_reconciliation
cargo test --offline live_position_book
cargo test --offline live_balance_tracker
cargo test --offline redaction
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|/order|/orders|/cancel|FOK|FAK)" src Cargo.toml config
rg -n -i "(private[_ -]?key|secret|passphrase|mnemonic|seed|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
```

Any order/cancel hits must be documented as existing LB code or LA1 inert definitions only.

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la1-journal-reconciliation.md
```

Must include:

```text
config defaults
gate decision examples
journal path
journal replay result
redaction result
mismatch fixture results
safety scan results
confirmation that no live order was placed
```

## Exit Gate

LA1 exits only when:

- Live Alpha config defaults are inert,
- gate evaluation blocks by default,
- journal writes and replays,
- reconciliation mismatch fixtures halt,
- existing paper/replay behavior is unchanged,
- no live order placement path is reachable.

---

# LA2 — Heartbeat, User Events, and Crash Recovery

## Objective

Build live-session safety before new live fills.

This phase may use heartbeat and authenticated readback from the approved host. It must not place orders.

## Allowed Changes

- Heartbeat module.
- Heartbeat state tracking.
- User WebSocket parser and fixture tests.
- Startup crash recovery preflight.
- Live Alpha incident halt state.
- Read-only account/order/trade/balance/allowance readback.
- Heartbeat POST only if specifically approved and only for heartbeat endpoint.
- Runbook updates.

## Explicitly Disallowed Changes

- Live order placement.
- Strategy-selected live orders.
- Taker orders.
- FAK/FOK/marketable limit orders.
- Cancel-all.
- Any autonomous cancel loop.
- Any change that keeps unapproved orders alive.

## Implementation Steps

### LA2.1 Add heartbeat module

Add:

```text
src/live_heartbeat.rs
```

State:

```text
heartbeat_id
last_sent_at
last_acknowledged_at
expected_interval_ms
max_staleness_ms
associated_open_orders
heartbeat_enabled
heartbeat_failure_action
```

Actions:

```text
HeartbeatNotStarted
HeartbeatHealthy
HeartbeatStale
HeartbeatRejected
HeartbeatUnknown
```

### LA2.2 Add heartbeat gating

The live gate must block if:

```text
heartbeat_required == true
and heartbeat state is not healthy
and the current mode can place or maintain live orders
```

### LA2.3 Add user WebSocket parser

Add:

```text
src/live_user_events.rs
```

Parse user channel events:

```text
order PLACEMENT
order UPDATE
order CANCELLATION
trade MATCHED
trade MINED
trade CONFIRMED
trade RETRYING
trade FAILED
```

Initial implementation may be parser-only with fixture tests. Full network subscription can be phased if needed, but LA3 should have either user WS or REST trade polling as the required confirmation path.

### LA2.4 Add startup crash recovery

On startup for any live-alpha mode other than disabled, perform:

```text
geoblock check
account preflight
balance/allowance readback
open orders readback
recent trades readback
journal replay
position reconstruction
reconciliation
```

If unknown live state exists, enter halt mode.

### LA2.5 Add halt state

Add durable events:

```text
LiveRiskHalt
LiveHeartbeatStale
LiveStartupRecoveryStarted
LiveStartupRecoveryPassed
LiveStartupRecoveryFailed
```

### LA2.6 Add runbooks

Create or update:

```text
runbooks/live-alpha-runbook.md
runbooks/live-alpha-reconciliation-runbook.md
runbooks/live-alpha-incident-response.md
```

Required procedures:

```text
how to inspect journal
how to inspect open orders
how to inspect recent trades
how to inspect balances
how to inspect heartbeat
how to handle unknown open order
how to handle stale heartbeat
how to halt
how to resume
```

## Verification Commands

```text
cargo test --offline live_heartbeat
cargo test --offline live_user_events
cargo test --offline live_reconciliation
cargo test --offline startup_recovery
cargo test --offline risk_halt
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(postHeartbeat|heartbeat|wss://ws-subscriptions-clob|user.*channel|MATCHED|MINED|CONFIRMED|RETRYING|FAILED)" src runbooks verification
rg -n -i "(createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|submit.*order|place.*order|FOK|FAK)" src Cargo.toml config
```

Optional approved live read-only/heartbeat check from approved host:

```text
cargo run -- --config <approved-live-alpha-config> live-alpha-preflight --read-only
cargo run -- --config <approved-live-alpha-config> live-alpha-heartbeat-check
```

The final command names may differ. Record exact commands in the verification note.

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la2-heartbeat-crash-safety.md
```

Must include:

```text
heartbeat behavior
startup recovery behavior
open orders readback result
recent trades readback result
balance/allowance result
journal reconstruction result
user event parser fixture result
halts triggered by stale/unknown states
safety scan results
confirmation that no live order was placed
```

## Exit Gate

LA2 exits only when:

- heartbeat staleness blocks live-capable modes,
- startup recovery detects unknown open orders,
- user event fixtures parse correctly,
- readback/reconciliation can run without order placement,
- no live order placement occurred.

---

# LA3 — Controlled Live Fill Canary

## Objective

Execute exactly one controlled tiny live fill attempt and reconcile it end-to-end.

This is the first new live order authority after LB6. It is not autonomous trading.

## Allowed Changes

- Controlled fill canary command.
- Human approval prompt and approval log.
- Optional FOK/FAK/marketable-limit canary path if explicitly approved in LA0.
- Immediate order/trade/balance/position/readback reconciliation.
- Settlement follow-up artifact.
- Incident note if any state is ambiguous.

## Explicitly Disallowed Changes

- More than one fill attempt without new approval.
- Strategy-selected live order.
- Autonomous live trading.
- Maker micro mode.
- Quote manager mode.
- Batch orders.
- Retry loop after failed or ambiguous submit.
- Taker strategy.
- Scaling.

## Implementation Steps

### LA3.1 Add fill canary command

Add a CLI command such as:

```text
live-alpha-fill-canary --dry-run
live-alpha-fill-canary --human-approved --approval-id <id>
```

Final command names may differ, but they must be recorded in the verification note.

### LA3.2 Separate fill canary from LB6 canary

Do not reuse the LB6 one-order cap as if it grants new authority.

Create a new LA3-specific envelope:

```text
LiveAlphaFillCanaryEnvelope
```

Required fields:

```text
approval_id
run_id
host_id
wallet_id
geoblock_result
account_preflight_id
heartbeat_status
market_slug
condition_id
token_id
asset_symbol
outcome
side
order_type
price
amount_or_size
max_notional
max_slippage_bps
max_fee_estimate
book_snapshot_id
reference_snapshot_id
created_at
```

### LA3.3 Preflight

Immediately before the canary:

```text
geoblock check
approved host check
approved wallet check
kill switch check
heartbeat check
account balance readback
allowance readback
open orders readback
recent trades readback
market status check
book freshness check
reference freshness check
risk limits check
journal health check
```

If any preflight item fails, no order is submitted.

### LA3.4 Human approval prompt

The approval prompt must show:

```text
run ID
host
wallet/funder
geoblock result
pUSD available
pUSD reserved
open orders
recent trades count
market slug
condition ID
token ID
outcome
side
order type
price / worst-price limit
amount or size
max notional
max fee estimate
book age
reference age
heartbeat state
cancel/reconciliation plan
rollback command
```

### LA3.5 Submit one canary

Use the official SDK path already proven for LB6 signing/submission unless there is a documented reason not to.

If FOK/FAK is approved, remember:

```text
FOK = fill entirely or cancel
FAK = fill what is available immediately and cancel the rest
price = worst-price limit for market orders
```

If a marketable limit is approved, it must have a strict worst-price limit and visible-depth check.

### LA3.6 Reconcile immediately

After submission:

```text
write submit event
read order status
read trades by market/token/time/order if possible
read open orders
read balances
read positions if available
consume user WS events if active
compare all sources
write reconciliation result
```

Required outcomes:

```text
filled and reconciled
not filled and canceled/expired cleanly
failed before submit
failed after submit with incident note
```

### LA3.7 Settlement follow-up

For a market that later resolves, record:

```text
resolution
position settlement value
realized P&L
settlement discrepancy if any
```

## Verification Commands

Before canary:

```text
cargo run -- --config <approved-live-alpha-config> live-alpha-preflight --read-only
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-fill-canary --dry-run
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

During canary:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-fill-canary --human-approved --approval-id <id>
cargo run -- --config <approved-live-alpha-config> live-alpha-readback --run-id <run_id>
cargo run -- --config <approved-live-alpha-config> live-alpha-reconcile --run-id <run_id>
```

After canary:

```text
cargo run --offline -- live-alpha-replay-journal --run-id <run_id>
git diff --check
```

Command names are placeholders until implementation. Final names must be documented before use.

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la3-controlled-fill-canary.md
```

Must include:

```text
approval id
exact command sequence
geoblock result
account preflight
heartbeat result
market and order intent
venue order ID
trade ID, if any
order status transitions
trade status transitions
maker/taker status
fee / fee rate
balance before/after
reserved balance before/after
position before/after
open orders after run
journal replay result
reconciliation result
settlement follow-up plan
incident note if any ambiguity exists
```

## Exit Gate

LA3 exits only when:

- exactly one controlled fill attempt has occurred,
- the result is reconciled or incident-documented,
- no unexpected open orders remain,
- reserved pUSD state is understood,
- position state is understood,
- no SDK/Rust disagreement remains unresolved.

## Hold Point

Mandatory. Stop after LA3 for human review.

No maker micro autonomy may begin from LA3 code alone.

---

# LA4 — Shadow Live Executor

## Objective

Build the live executor interface and route strategy intents to shadow-live decisions without placing orders.

## Allowed Changes

- `LiveExecutor` abstraction.
- Shadow execution mode.
- Strategy intent routing into shadow live.
- Paper + shadow comparison artifacts.
- Live eligibility decisions.
- Reason-coded rejection.
- Metrics for would-submit/would-cancel/would-replace.

## Explicitly Disallowed Changes

- Live order placement.
- Live cancel/replace automation.
- Taker strategy.
- Maker micro live orders.
- Batch orders.
- Any strategy-to-live path that can submit.

## Implementation Steps

### LA4.1 Add executor interface

Add:

```text
src/live_executor.rs
```

Recommended trait:

```rust
pub trait ExecutionSink {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision;
}
```

Modes:

```text
DisabledExecution
PaperExecution
ShadowLiveExecution
LiveMakerExecution
LiveTakerExecution
```

Only `ShadowLiveExecution` is implemented in LA4.

### LA4.2 Add shadow decision model

Persist:

```text
shadow_decision_id
intent_id
would_submit
would_cancel
would_replace
live_eligible
risk_eligible
post_only_safe
inventory_valid
balance_valid
book_fresh
reference_fresh
market_time_valid
reason_codes
expected_order_type
expected_price
expected_size
expected_notional
expected_edge_bps
expected_fee
```

### LA4.3 Integrate into runtime

Do not simply replace paper execution.

Recommended flow:

```text
signal_engine
  -> existing risk_engine
  -> execution_intent builder
  -> paper_executor
  -> shadow_live_executor
  -> decision/journal/replay artifacts
```

Paper remains the execution path. Shadow-live only records would-have-done decisions.

### LA4.4 Add reason codes

At minimum:

```text
edge_too_small
book_stale
reference_stale
market_too_close_to_close
post_only_would_cross
insufficient_pusd
insufficient_inventory_for_sell
max_open_orders_reached
max_market_notional_reached
max_asset_notional_reached
heartbeat_not_healthy
reconciliation_not_clean
geoblock_not_passed
mode_not_approved
```

### LA4.5 Add comparison report

For each run:

```text
paper fills
shadow would-submit count
shadow would-cancel count
shadow rejected count by reason
paper/live-intent divergence
estimated fee exposure
estimated reserved pUSD exposure
```

## Verification Commands

```text
cargo test --offline live_executor
cargo test --offline shadow_live
cargo test --offline execution_intent
cargo test --offline live_risk_engine
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(postOrder|postOrders|createAndPostOrder|createAndPostMarketOrder|submit.*order|place.*order|FOK|FAK)" src Cargo.toml config
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la4-shadow-live-executor.md
```

Must include:

```text
runtime command
duration
markets observed
paper fills
shadow would-submit count
shadow would-cancel count
shadow rejection reasons
live risk decision examples
confirmation that no live order was placed
safety scan results
```

## Exit Gate

LA4 exits only when:

- shadow mode runs through the normal strategy loop,
- every shadow action is journaled,
- no live order can be placed,
- paper behavior is unchanged,
- shadow-live decisions are explainable.

---

# LA5 — Live Risk Engine and Maker-Only Micro Autonomy

## Objective

Enable tiny autonomous post-only maker orders after LA3 and LA4 have passed.

This is the first autonomous live strategy phase. It must be deliberately boring.

## Allowed Changes

- Live-specific risk engine.
- Inventory-aware side mapping.
- Maker-only post-only GTD submission.
- One or very few open orders under strict caps.
- Short TTL.
- Automatic stale quote cancellation only within approved micro mode.
- Full reconciliation after every submit/cancel/fill.
- Metrics and runbook updates.

## Explicitly Disallowed Changes

- Taker orders.
- FAK/FOK strategy orders.
- Marketable strategy orders.
- Batch orders.
- Production sizing.
- More assets beyond BTC/ETH/SOL.
- Multiple wallets.
- Inventory-blind sells.
- Unlimited order loops.
- Scaling toward 1,000 trades/day.

## Implementation Steps

### LA5.1 Add live risk engine

Add:

```text
src/live_risk_engine.rs
```

Inputs:

```text
execution_intent
account balance
reserved balance
open orders
positions
market exposure
asset exposure
daily P&L
fees
book freshness
reference freshness
heartbeat
reconciliation state
geoblock state
```

Outputs:

```text
LiveRiskApproved
LiveRiskRejected(reason_codes)
LiveRiskHalt(reason)
```

Required limits:

```text
max_wallet_funding_pusd
max_available_pusd_usage
max_reserved_pusd
max_single_order_notional
max_per_market_notional
max_per_asset_notional
max_total_live_notional
max_open_orders
max_open_orders_per_market
max_open_orders_per_asset
max_daily_realized_loss
max_daily_unrealized_loss
max_fee_spend
max_submit_rate
max_cancel_rate
max_reconciliation_lag_ms
max_book_staleness_ms
max_reference_staleness_ms
no_trade_seconds_before_close
```

### LA5.2 Add inventory-aware side mapping

Required mapping:

```text
Bullish Up:
  prefer BUY Up

Bearish Up:
  if holding Up: SELL Up to reduce or exit
  if not holding Up: BUY Down

Bullish Down:
  prefer BUY Down

Bearish Down:
  if holding Down: SELL Down to reduce or exit
  if not holding Down: BUY Up
```

SELL intents require conditional-token inventory and allowance.

### LA5.3 Add maker micro live path

Use official SDK submission path.

Allowed order shape:

```text
post-only = true
order type = GTD
side = inventory-aware BUY or inventory-valid SELL
expiry = now + required venue buffer + configured TTL
size <= approved cap
notional <= approved cap
price conforms to tick size
book proves non-marketable
```

### LA5.4 Add cancel-after-submit safety

After each submit:

```text
write submit event
read order
verify open/matched/rejected status
read open orders
read balances/reserved
read trades
reconcile
```

If order remains open beyond TTL or becomes stale:

```text
request cancel
read order
read open orders
read reserved balance
reconcile
```

### LA5.5 Add micro run mode

Example command:

```text
live-alpha-maker-micro --max-orders <N> --max-duration-sec <S>
```

Initial recommended caps:

```text
max_orders = 1 to 3
max_open_orders = 1
max_duration_sec = short session
max_single_order_notional = tiny
```

Actual values must come from approval artifact.

### LA5.6 Add live metrics

Add:

```text
src/live_alpha_metrics.rs
```

Metrics:

```text
live_orders_submitted_total
live_orders_accepted_total
live_orders_rejected_total
live_orders_filled_total
live_orders_canceled_total
live_unknown_open_orders_total
live_reconciliation_mismatches_total
live_risk_halts_total
live_balance_mismatch_total
live_position_mismatch_total
live_reserved_balance_mismatch_total
live_submit_latency_ms
live_cancel_latency_ms
live_readback_latency_ms
live_edge_at_submit_bps
live_edge_at_fill_bps
live_realized_pnl
live_unrealized_pnl
live_fee_spend
```

## Verification Commands

Before live micro run:

```text
cargo test --offline live_risk_engine
cargo test --offline inventory
cargo test --offline live_executor
cargo test --offline live_order_journal
cargo test --offline live_reconciliation
cargo test --offline live_heartbeat
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-maker-micro --dry-run
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Approved live micro run:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-preflight --read-only
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-maker-micro --human-approved --max-orders <N> --max-duration-sec <S>
cargo run -- --config <approved-live-alpha-config> live-alpha-reconcile --run-id <run_id>
```

After run:

```text
cargo run --offline -- live-alpha-replay-journal --run-id <run_id>
cargo run --offline -- live-alpha-report --run-id <run_id>
git diff --check
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la5-maker-micro-autonomy.md
```

Must include:

```text
approval id
exact command sequence
config snapshot redacted
run duration
markets touched
orders submitted
orders accepted
orders rejected
orders filled
orders canceled
open orders after run
reserved pUSD after run
balance before/after
positions before/after
P&L
fees
risk halts
mismatches
paper/shadow comparison
go/no-go decision for LA6
```

## Exit Gate

LA5 exits only when:

- maker-only micro autonomy runs under strict caps,
- no taker orders occur,
- no inventory-invalid sells occur,
- every submit/cancel/fill is journaled,
- every live state is reconciled,
- no unexplained open order remains,
- no unexplained reserved balance remains.

## Hold Point

Mandatory. Stop after LA5 for human review.

---

# LA6 — Quote Manager and Cancel/Replace

## Objective

Build the quote lifecycle system needed for market making.

The quote manager decides whether to place, leave, cancel, replace, or halt a quote. It must prevent uncontrolled churn.

## Allowed Changes

- Live quote manager.
- Stale quote detection.
- Cancel/replace policy.
- Anti-churn rate limits.
- Maker-only replacement orders.
- Quote lifecycle metrics.
- Expanded but still capped maker-only runs.

## Explicitly Disallowed Changes

- Taker strategy.
- FAK/FOK strategy.
- Batch orders.
- Unlimited cancel/replace.
- Cancel-all as normal runtime behavior.
- Production sizing.
- Multi-wallet deployment.
- Asset expansion beyond BTC/ETH/SOL.

## Implementation Steps

### LA6.1 Add quote manager

Add:

```text
src/live_quote_manager.rs
```

Inputs:

```text
fair probability
edge threshold
current best bid/ask
spread
own open orders
own inventory
time remaining
market status
book freshness
reference freshness
order age
post-only safety
risk limits
cancel rate limits
submit rate limits
reconciliation status
```

Outputs:

```text
PlaceQuote
LeaveQuote
CancelQuote
ReplaceQuote
ExpireQuote
HaltQuote
SkipMarket
```

### LA6.2 Add quote state

Track:

```text
quote_id
intent_id
order_id
market
token_id
side
price
size
fair_probability_at_submit
edge_bps_at_submit
submitted_at
last_validated_at
cancel_requested_at
replaced_by_quote_id
status
```

### LA6.3 Add replace conditions

Replace only when:

```text
fair value moved beyond replace_tolerance_bps
book moved enough to affect post-only safety
edge fell below threshold
position/inventory changed
time-to-close entered stricter zone
risk limits changed
existing quote age exceeded TTL
```

### LA6.4 Add anti-churn rules

Enforce:

```text
minimum quote lifetime
maximum cancel rate
maximum replacement rate
minimum edge improvement for replacement
cooldown after failed submit
cooldown after failed cancel
cooldown after reconciliation mismatch
```

### LA6.5 Add no-trade window

No new live orders within:

```text
no_trade_seconds_before_close
```

Existing quotes inside this window must either be canceled or left according to approved risk policy. Default should be cancel/halt unless approved otherwise.

### LA6.6 Add market WebSocket integration

Use existing CLOB market data if already implemented. Ensure quote manager consumes:

```text
book snapshots
price_change
best_bid_ask
last_trade_price
tick_size_change
market_resolved
```

If user WebSocket is available, consume order/trade updates as reconciliation hints but still verify by REST/readback.

## Verification Commands

```text
cargo test --offline live_quote_manager
cargo test --offline cancel_replace
cargo test --offline anti_churn
cargo test --offline post_only_safety
cargo test --offline no_trade_window
cargo test --offline live_reconciliation
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-quote-manager --dry-run
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Approved live run:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-preflight --read-only
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-quote-manager --human-approved --max-orders <N> --max-replacements <R> --max-duration-sec <S>
cargo run -- --config <approved-live-alpha-config> live-alpha-reconcile --run-id <run_id>
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la6-quote-manager.md
```

Must include:

```text
approval id
exact command sequence
config snapshot redacted
markets touched
quotes placed
quotes left alone
quotes canceled
quotes replaced
fills
open orders after run
reserved pUSD after run
cancel rate
replacement rate
anti-churn triggers
risk halts
mismatches
P&L
paper/shadow/live divergence
go/no-go decision for LA7
```

## Exit Gate

LA6 exits only when:

- stale quotes are canceled or left according to policy,
- replacements are rate-limited,
- cancel confirmations are reconciled,
- reserved balance release is verified,
- no uncontrolled cancel/replace loop occurs,
- live maker-only P&L and adverse selection are measured.

## Hold Point

Mandatory. Stop after LA6 for human review.

---

# LA7 — Selective Taker Gate

## Objective

Add taker execution only after maker-only evidence supports it.

This phase is optional and must be separately approved.

## Entry Criteria

LA7 may start only if:

```text
LA3 controlled fill canary passed
LA5 maker-only micro autonomy passed
LA6 quote manager passed
no unresolved lifecycle bugs
paper/live divergence is understood
adverse selection is measured
fee model is implemented
taker gate approval exists
```

## Allowed Changes

- Taker evaluator in shadow mode.
- Fee and slippage cost model.
- Visible-depth checker.
- Worst-price-limit enforcement.
- One tiny controlled taker canary if separately approved.
- Taker P&L reporting separate from maker P&L.

## Explicitly Disallowed Changes

- General taker strategy before shadow evidence.
- Unlimited taker orders.
- Taker retry loop after ambiguous submit.
- Taker near close unless separately approved.
- Production sizing.
- Scaling trade count.

## Implementation Steps

### LA7.1 Add taker evaluator

Add:

```text
src/live_taker_gate.rs
```

Taker order is eligible only when:

```text
expected_value >
  spread
  + taker_fee
  + slippage
  + latency_buffer
  + adverse_selection_buffer
  + minimum_profit_buffer
```

### LA7.2 Add fee model

Implement fee query and calculation.

Track:

```text
fees_enabled
fee_rate
estimated_fee
actual_fee
maker_or_taker
fee_delta
```

Do not include fees in orders; reconcile fees after match.

### LA7.3 Add visible-depth checker

Before any taker canary:

```text
book has sufficient visible size
worst-price limit is set
max market impact respected
max slippage respected
market status valid
reference fresh
book fresh
```

### LA7.4 Run taker in shadow first

For at least one shadow session, report:

```text
would-take count
rejected-by-fee count
rejected-by-depth count
rejected-by-slippage count
rejected-by-latency-buffer count
estimated EV after costs
```

### LA7.5 Optional one taker canary

Only if approved:

```text
one tiny taker order
strict worst-price limit
max notional
max fee
no retry loop
immediate reconciliation
```

## Verification Commands

Shadow:

```text
cargo test --offline live_taker_gate
cargo test --offline fee_model
cargo test --offline depth_check
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Optional approved taker canary:

```text
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-taker-canary --dry-run
cargo run --features live-alpha-orders -- --config <approved-live-alpha-config> live-alpha-taker-canary --human-approved --approval-id <id>
cargo run -- --config <approved-live-alpha-config> live-alpha-reconcile --run-id <run_id>
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la7-taker-gate.md
```

Must include:

```text
entry criteria evidence
shadow taker report
fee model evidence
depth check evidence
approval id if live canary occurred
order/trade details if live canary occurred
fees
slippage
P&L
reconciliation result
decision whether taker remains disabled or can proceed
```

## Exit Gate

LA7 exits only when:

- taker remains disabled by default,
- taker shadow evidence is complete,
- optional taker canary reconciles cleanly if performed,
- maker and taker P&L are separated,
- taker cannot bypass live risk controls.

## Hold Point

Mandatory. Stop after LA7 for human review.

---

# LA8 — Scale Decision Report

## Objective

Decide whether scaling is justified.

This phase is primarily reporting and decision-making, not automatic expansion.

## Allowed Changes

- Live Alpha scale report.
- Metrics aggregation.
- Paper/live comparison.
- Adverse selection report.
- P&L and fee attribution.
- Recommendation for next PRD if scaling is justified.

## Explicitly Disallowed Changes

- Increasing size without new approval.
- Increasing order rate without new approval.
- Increasing asset coverage without new approval.
- Increasing taker usage without new approval.
- Multi-wallet deployment.
- Production rollout.

## Implementation Steps

### LA8.1 Add scale report command

Example:

```text
live-alpha-scale-report --from <date> --to <date>
```

Report:

```text
realized P&L
unrealized P&L
paper/live divergence
fill count
cancel count
quote update count
maker/taker split
fee spend
slippage
adverse selection rate
average edge at submit
average edge at fill
edge decay after fill
open-order mismatch count
reconciliation mismatch count
halt count
unknown state count
latency distribution
market liquidity distribution
per-asset P&L
per-market-window P&L
```

### LA8.2 Define scale recommendation

Possible decisions:

```text
NO-GO: lifecycle unsafe
NO-GO: negative expectancy
NO-GO: paper/live divergence unexplained
HOLD: more maker-only data required
HOLD: taker shadow only
GO: increase duration only
GO: increase order cap only
GO: increase size only
GO: add selective taker only
GO: propose next PRD for broader scaling
```

### LA8.3 One-dimensional scaling rule

Any future scale phase may increase only one dimension at a time:

```text
size
order count
asset count
timeframe count
taker usage
runtime duration
```

## Verification Commands

```text
cargo test --offline live_alpha_report
cargo run --offline -- live-alpha-scale-report --from <date> --to <date>
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

## Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-alpha-la8-scale-decision.md
```

Must include:

```text
period covered
capital used
orders/fills/cancels/replacements
maker/taker split
P&L
fees
slippage
adverse selection
mismatches
halts
major bugs
go/no-go decision
next proposed PRD or hold decision
```

## Exit Gate

LA8 exits only when a documented go/no-go decision exists.

## Hold Point

Mandatory. Scaling requires a new approval scope.

---

## 8. PR / Branch Strategy

Recommended branch names:

```text
live-alpha/la0-scope-lock
live-alpha/la1-journal-reconciliation
live-alpha/la2-heartbeat-crash-safety
live-alpha/la3-fill-canary
live-alpha/la4-shadow-executor
live-alpha/la5-maker-micro
live-alpha/la6-quote-manager
live-alpha/la7-taker-gate
live-alpha/la8-scale-report
```

Recommended PR style:

```text
one phase per PR
no mixing docs-only approval with live-capable source changes
no mixing maker and taker changes
no mixing quote manager with scale changes
no merging if verification note is missing
```

Every PR description should include:

```text
phase
scope
allowed changes
explicit disallowed changes
commands run
safety scan results
verification artifact path
hold point status
```

---

## 9. Minimum Interfaces

These are suggested interfaces to keep the architecture clean. Exact Rust names may differ.

### Execution intent

```rust
pub struct ExecutionIntent {
    pub intent_id: String,
    pub strategy_snapshot_id: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub asset_symbol: String,
    pub outcome: String,
    pub side: IntentSide,
    pub price: Decimal,
    pub size: Decimal,
    pub notional: Decimal,
    pub order_type: IntentOrderType,
    pub time_in_force: TimeInForce,
    pub post_only: bool,
    pub expiry_unix: Option<i64>,
    pub fair_probability: Decimal,
    pub edge_bps: i64,
    pub reference_price: Decimal,
    pub reference_source_timestamp_ms: i64,
    pub book_snapshot_id: String,
    pub best_bid: Option<Decimal>,
    pub best_ask: Option<Decimal>,
    pub spread: Option<Decimal>,
    pub created_at_ms: i64,
}
```

### Live risk decision

```rust
pub enum LiveRiskDecision {
    Approved(LiveRiskApproval),
    Rejected { reasons: Vec<LiveRiskRejectReason> },
    Halt { reason: LiveRiskHaltReason },
}
```

### Reconciliation result

```rust
pub enum LiveReconciliationResult {
    Passed {
        run_id: String,
        checked_at_ms: i64,
    },
    Mismatch {
        run_id: String,
        mismatches: Vec<LiveReconciliationMismatch>,
    },
    HaltRequired {
        run_id: String,
        reason: LiveRiskHaltReason,
    },
}
```

### Quote action

```rust
pub enum QuoteAction {
    Place(ExecutionIntent),
    Leave { order_id: String },
    Cancel { order_id: String, reason: String },
    Replace { cancel_order_id: String, new_intent: ExecutionIntent },
    Skip { reason: String },
    Halt { reason: String },
}
```

---

## 10. Documentation References to Recheck Before Live Phases

Recheck these before LA3, LA5, LA6, and LA7:

```text
Polymarket create order docs:
https://docs.polymarket.com/trading/orders/create

Polymarket L2 client methods:
https://docs.polymarket.com/trading/clients/l2

Polymarket user WebSocket channel:
https://docs.polymarket.com/market-data/websocket/user-channel

Polymarket market WebSocket channel:
https://docs.polymarket.com/market-data/websocket/market-channel

Polymarket fees:
https://docs.polymarket.com/trading/fees

Polymarket geoblock:
https://docs.polymarket.com/api-reference/geoblock
```

Do not rely on memory for live order semantics. These docs can change.

---

## 11. Final Live Alpha Acceptance Criteria

Live Alpha is complete only when:

1. LA0 approval exists.
2. Default builds cannot place live orders.
3. Live Alpha config defaults are disabled.
4. Live order journal is durable and replayable.
5. Reconciliation detects mismatches and halts.
6. Heartbeat staleness halts live-capable modes.
7. Startup recovery detects unknown live state.
8. One controlled live fill canary is reconciled.
9. Shadow live executor runs without posting.
10. Maker-only micro autonomy runs under strict caps.
11. Quote manager can cancel/replace without uncontrolled churn.
12. All live orders, fills, cancels, balances, reserved funds, and positions are reconciled.
13. Taker execution remains disabled unless LA7 separately approves it.
14. Scale decision is evidence-based.

---

## 12. Final Product Decision

Implement Live Alpha in this order:

```text
LA0 approval
LA1 gates + journal + reconciliation
LA2 heartbeat + user events + crash recovery
LA3 one controlled live fill canary
LA4 shadow live executor
LA5 maker-only micro autonomy
LA6 quote manager cancel/replace
LA7 optional selective taker gate
LA8 scale decision
```

Do not optimize for 1,000 trades/day yet. Optimize for:

```text
known state
small capital
clean reconciliation
positive expectancy evidence
controlled progression
```

That is the fastest responsible path from LB7 to a real live trading bot.
