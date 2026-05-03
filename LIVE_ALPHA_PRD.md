# LIVE_ALPHA_PRD.md

# Live Alpha PRD — Controlled Live Fill, Reconciliation, and Maker-Only Micro Autonomy

**Repo:** `jonahgrigoryan/polymarket-15m-arb-bot`
**Base state:** `main` at `26144dc`, after PR #27 / LB7 handoff
**Date:** 2026-05-03
**Status:** Draft for approval
**Owner:** Jonah
**Scope type:** Post-LB7 milestone scope
**Primary objective:** Move from one non-filled live canary order to controlled live trading capability without jumping directly into unbounded autonomous strategy trading.

---

## 1. Executive Summary

The project has completed the planned Live Beta sequence LB0–LB7. LB6 proved one tiny human-approved post-only canary submit/cancel lifecycle: exactly one live order was submitted, that exact order was canceled, no fill occurred, post-cancel open orders were `0`, post-cancel reserved pUSD was `0`, and the LB6 one-order cap is consumed. That sequence did **not** prove fill accounting, fee accounting, position accounting, partial-fill behavior, settlement, autonomous strategy routing, or strategy profitability.

This PRD defines the next gated phase: **Live Alpha**.

LA0 is approval and scope lock only. It establishes Live Alpha as the post-LB7 release track but does not authorize live orders, live cancels, cancel-all, strategy-selected live trading, cap reset, or any live execution expansion.

Live Alpha is not production rollout. It is a controlled progression through:

1. durable live order/trade journaling,
2. heartbeat and crash-safety handling,
3. one tiny controlled live fill canary,
4. reconciliation of trades, balances, positions, and reserved funds,
5. shadow live execution,
6. maker-only micro autonomy,
7. quote lifecycle management,
8. only later, selective taker execution.

The purpose is to convert the existing paper/replay strategy and LB6 live-order plumbing into a safe, auditable, capital-bounded live system.

Phase boundary:

- LA1 and LA2 must pass before any controlled fill canary.
- LA3 is the first possible controlled fill canary, and only after explicit human/operator approval.
- LA5 or later is the first possible maker-only micro autonomy, and only after the prior evidence gates pass.
- Strategy-selected live trading remains deferred behind a separate robustness gate and explicit approval.

---

## 2. Background

### 2.1 Existing project posture

The current MVP and Live Beta posture remains conservative:

- `LIVE_ORDER_PLACEMENT_ENABLED=false` globally by default.
- LB6 consumed the one-order canary authority.
- LB7 completed documentation, runbook, observability, and handoff work only.
- No strategy-selected autonomous live trading is currently approved.
- No multi-order live execution is currently approved.
- No taker execution is currently approved.
- No production rollout is currently approved.

### 2.2 Existing built components

The system already includes:

- market discovery for BTC/ETH/SOL 15-minute markets,
- CLOB market data integration,
- external reference feeds,
- RTDS / settlement-alignment work,
- stateful books,
- signal engine,
- risk engine,
- paper executor,
- deterministic replay,
- authenticated readback,
- live signing dry run,
- one live post-only canary order,
- exact cancel path,
- live beta rollback runbook,
- live beta observability extensions.

### 2.3 Remaining live gap

The critical missing proof is not order submission. The missing proof is **post-match lifecycle correctness**.

The project must prove:

- live fill detection,
- live trade status tracking,
- live fee accounting,
- pUSD balance movement,
- reserved balance release,
- conditional token position accounting,
- partial-fill handling,
- cancel-after-partial-fill handling,
- trade/readback/user-event reconciliation,
- settlement follow-up,
- halt behavior on mismatches.

---

## 3. Product Objective

Build a gated Live Alpha system that can safely progress from controlled live fill canary to tiny maker-only autonomous trading on BTC/ETH/SOL 15-minute Polymarket markets.

The strategy remains:

> **Resolution-source-informed market making on short-duration prediction markets.**

In practical execution terms:

> **Maker-first fair-value quoting with later optional selective taker execution, gated by live expectancy evidence and strict risk limits.**

---

## 4. Strategic Thesis

The bot estimates fair probability for short-duration crypto prediction markets using external reference price/state and settlement-source alignment. It compares this fair probability to Polymarket CLOB prices and places orders only when expected value clears configured thresholds.

The initial live strategy must be maker-only:

- post-only,
- GTD,
- small size,
- short TTL,
- strict stale-quote cancellation,
- no taker orders,
- no unlimited multi-order mode,
- no production sizing.

Selective taker execution is a later phase. It must only be enabled if maker-only live results prove that the model has positive expectancy and reconciliation is reliable.

---

## 5. Goals

### 5.1 Primary goals

1. Define a new post-LB7 approval scope.
2. Preserve the existing fail-closed live safety posture.
3. Build durable live order, trade, balance, position, and settlement journaling.
4. Add heartbeat lifecycle handling as a first-class safety module.
5. Execute one controlled tiny live fill canary.
6. Reconcile the fill end-to-end.
7. Build a shadow live executor that emits live intents without placing orders.
8. Add live-specific risk controls independent of paper risk controls.
9. Enable tiny maker-only autonomous live trading after the fill gate passes.
10. Add quote lifecycle management for cancel/replace behavior.
11. Prepare, but do not initially enable, selective taker execution.

### 5.2 Secondary goals

1. Preserve replayability of live decisions.
2. Compare paper fills versus real live fills.
3. Track adverse selection.
4. Track divergence between expected and actual fill quality.
5. Establish evidence needed to decide whether scaling toward 1,000+ fills/day is realistic.

---

## 6. Non-Goals

This PRD explicitly does **not** authorize:

- production rollout,
- unlimited live order placement,
- flipping `LIVE_ORDER_PLACEMENT_ENABLED=true` globally,
- removing all live caps,
- strategy-selected taker execution in the first phase,
- broad multi-asset expansion beyond BTC/ETH/SOL,
- new high-frequency infrastructure,
- colocated latency optimization,
- margin/borrow/short selling behavior,
- bypassing geoblock or compliance controls,
- use from restricted regions,
- use of non-dedicated wallets,
- large capital deployment,
- trading thousands of times per day as an immediate milestone.

---

## 7. Key Principle

Do not treat “live executor” as one module that simply replaces the paper executor.

The correct live architecture is:

```text
strategy signal
  → strategy intent
  → live-specific risk approval
  → quote decision
  → execution adapter
  → live order journal
  → live reconciliation
  → live position book
  → halt / continue decision
```

The live executor should be thin. It should submit, cancel, and read back. It should not own strategy logic, risk policy, or reconciliation truth.

---

## 8. Milestone Overview

| Milestone | Name | Purpose | Live orders? | Autonomous? |
|---|---|---:|---:|---:|
| LA0 | Approval + scope lock | Create explicit post-LB7 authority | No | No |
| LA1 | Live journal + reconciliation foundation | Persist and compare all live lifecycle state | No | No |
| LA2 | Heartbeat + crash safety | Prevent ambiguous stale/open order behavior | No | No |
| LA3 | Controlled fill canary | Prove one tiny real fill end-to-end | Yes, one controlled fill | No |
| LA4 | Shadow live executor | Generate live intents without posting | No | No |
| LA5 | Maker-only micro autonomy | Tiny post-only autonomous maker orders | Yes | Yes, tightly capped |
| LA6 | Quote manager cancel/replace | Manage stale quotes and replacements | Yes | Yes, tightly capped |
| LA7 | Selective taker gate | Add taker only if evidence supports it | Yes | Limited, separately approved |
| LA8 | Scale decision | Decide whether to increase size/assets/frequency | TBD | TBD |

---

## 9. Detailed Requirements

---

# LA0 — Approval and Scope Lock

## 9.1 Objective

Create explicit authority for all work beyond LB7.

## 9.2 Requirements

Create:

```text
LIVE_ALPHA_PRD.md
LIVE_ALPHA_IMPLEMENTATION_PLAN.md
verification/YYYY-MM-DD-live-alpha-la0-approval-scope.md
```

The approval note must state:

- approved wallet,
- approved host/environment,
- approved assets,
- approved maximum pUSD funding,
- approved maximum single-order notional,
- approved maximum daily loss,
- approved maximum open orders,
- approved live order types,
- approved phases,
- prohibited phases,
- rollback owner,
- monitoring owner.

## 9.3 Default posture

All new live code must default to disabled.

Required defaults:

```text
LIVE_ORDER_PLACEMENT_ENABLED=false
LIVE_ALPHA_ENABLED=false
LIVE_ALPHA_FILL_CANARY_ENABLED=false
LIVE_ALPHA_SHADOW_EXECUTOR_ENABLED=false
LIVE_ALPHA_MAKER_MICRO_ENABLED=false
LIVE_ALPHA_TAKER_ENABLED=false
LIVE_ALPHA_SCALE_ENABLED=false
```

## 9.4 Exit criteria

LA0 passes only if:

- the new PRD exists,
- implementation plan exists,
- approval artifact exists,
- all live-alpha flags default to disabled,
- no new code path can place live orders without explicit compile-time and runtime approval.

---

# LA1 — Live Order Journal and Reconciliation Foundation

## 10.1 Objective

Build a durable live state layer before adding more live trading behavior.

## 10.2 New modules

```text
src/live_order_journal.rs
src/live_reconciliation.rs
src/live_position_book.rs
src/live_balance_tracker.rs
src/execution_intent.rs
```

## 10.3 Event model

The system must persist each live lifecycle event.

Minimum event types:

```text
LiveIntentCreated
LiveIntentRejectedByRisk
LiveIntentApprovedByRisk
LiveOrderSubmitRequested
LiveOrderSubmitAccepted
LiveOrderSubmitRejected
LiveOrderReadbackObserved
LiveOrderPartiallyFilled
LiveOrderFilled
LiveOrderCancelRequested
LiveOrderCancelAccepted
LiveOrderCancelRejected
LiveOrderCanceled
LiveOrderExpired
LiveTradeObserved
LiveTradeMatched
LiveTradeMined
LiveTradeConfirmed
LiveTradeFailed
LiveBalanceSnapshot
LiveBalanceDeltaObserved
LiveReservedBalanceObserved
LivePositionOpened
LivePositionReduced
LivePositionClosed
LiveSettlementObserved
LiveReconciliationPassed
LiveReconciliationMismatch
LiveRiskHalt
```

## 10.4 Required persisted fields

For every live intent:

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
risk_decision
risk_limits_snapshot
created_at
```

For every live order:

```text
order_id
intent_id
venue_status
original_size
matched_size
remaining_size
price
side
outcome
token_id
condition_id
created_at
updated_at
submitted_at
readback_at
cancel_requested_at
canceled_at
expired_at
raw_sdk_response_redacted
raw_rest_response_redacted
```

For every live trade:

```text
trade_id
order_id
market
asset
side
price
size
fee
fee_rate
maker_or_taker
status
transaction_hash
matched_at
mined_at
confirmed_at
failed_at
raw_trade_response_redacted
```

For balances:

```text
p_usd_available
p_usd_reserved
p_usd_total
conditional_token_positions
balance_snapshot_at
source
```

## 10.5 Reconciliation logic

The reconciliation layer must compare:

```text
local journal state
official SDK readback
authenticated REST readback
open orders endpoint
trades endpoint
user WebSocket events, if available
balance/allowance readback
reserved balance
position book
```

## 10.6 Mismatch policy

Any mismatch in the following must trigger a live halt:

- unknown open order,
- order accepted locally but missing in venue readback after timeout,
- venue order present but missing locally,
- unexpected fill,
- unexpected partial fill,
- expected cancel not reflected,
- reserved balance not released after cancel timeout,
- trade status failed/retrying beyond allowed window,
- pUSD balance delta inconsistent with trade records,
- token position inconsistent with trade records,
- SDK and Rust readback disagreement on critical state.

## 10.7 Exit criteria

LA1 passes only if:

- live journal writes durable records,
- journal survives process restart,
- reconciliation can run without placing orders,
- mismatch simulation triggers halt,
- redaction tests protect secrets and signed payloads,
- existing paper/replay behavior remains unchanged.

---

# LA2 — Heartbeat and Crash Safety

## 11.1 Objective

Promote heartbeat handling to a first-class live safety system.

## 11.2 New module

```text
src/live_heartbeat.rs
```

## 11.3 Requirements

The heartbeat module must track:

```text
heartbeat_id
last_sent_at
last_acknowledged_at
expected_interval
max_staleness
associated_open_orders
heartbeat_enabled
heartbeat_failure_action
```

## 11.4 Required behavior

If heartbeat becomes stale or ambiguous:

1. stop placing new orders,
2. reconcile open orders,
3. verify whether open orders remain open, canceled, expired, or filled,
4. trigger halt if venue state cannot be proven,
5. write a durable event.

## 11.5 Crash recovery

On startup in live-alpha mode, the bot must:

1. perform geoblock check,
2. perform account preflight,
3. read open orders,
4. read recent trades,
5. read balances and allowances,
6. reconstruct local position state,
7. compare against journal,
8. cancel or halt according to approved recovery policy.

## 11.6 Exit criteria

LA2 passes only if:

- stale heartbeat triggers halt,
- startup detects unknown open orders,
- startup detects unreconciled fills,
- no live order placement is possible until preflight and reconciliation pass.

---

# LA3 — Controlled Live Fill Canary

## 12.1 Objective

Prove one tiny real fill end-to-end.

This is the most important next live milestone. LB6 proved live submit and cancel, but not live fill lifecycle correctness.

## 12.2 Scope

Authorized behavior:

- one human-approved fill canary,
- one market only,
- minimum viable size,
- strict maximum notional,
- strict worst-price limit,
- no retry loop,
- no autonomous strategy selection,
- no cancel/replace loop,
- no scaling,
- immediate reconciliation.

## 12.3 Order type

The fill canary may use a marketable limit, FAK, or FOK style path only if separately approved in the LA0 approval artifact.

The order must include:

```text
max_notional
max_price
max_fee_estimate
max_slippage
market_slug_binding
token_id_binding
asset_binding
side_binding
size_binding
human_approval_id
```

## 12.4 Required preflight

Immediately before the fill canary:

```text
geoblock check
account balance readback
allowance readback
open orders readback
recent trades readback
market status check
book freshness check
reference freshness check
heartbeat freshness check
journal health check
risk limits check
```

## 12.5 Required post-fill reconciliation

After the canary, the system must reconcile:

```text
order accepted
trade observed
matched size
remaining size
fee / fee rate
maker/taker status
balance delta
reserved balance
token position
trade status
transaction hash, if available
open order state
journal state
```

## 12.6 Settlement follow-up

For resolved markets, the system must later record:

```text
market resolution
position settlement value
realized P&L
settlement discrepancy, if any
```

## 12.7 Exit criteria

LA3 passes only if:

- exactly one controlled fill canary occurs,
- fill is reconciled end-to-end,
- no unexpected open orders remain,
- no reserved pUSD remains unexpectedly locked,
- no SDK/Rust disagreement remains unresolved,
- no unexplained balance or position mismatch remains,
- a verification artifact is committed.

---

# LA4 — Shadow Live Executor

## 13.1 Objective

Build the live executor interface without allowing it to place orders.

## 13.2 New module

```text
src/live_executor.rs
```

## 13.3 Shadow behavior

The shadow live executor receives risk-approved strategy intents and writes what it would have done.

It must not submit orders.

It must answer:

```text
Would this order be live-eligible?
Would it violate live risk?
Would it violate inventory constraints?
Would it be post-only safe?
Would it accidentally cross the book?
Would it exceed pUSD balance?
Would it exceed reserved balance limits?
Would it exceed open-order count?
Would it be too close to market close?
Would it use a stale book?
Would it use a stale reference?
Would it require cancel/replace?
```

## 13.4 Required output

Each shadow decision must persist:

```text
shadow_intent_id
strategy_snapshot_id
live_eligibility
risk_decision
reason_codes
would_submit
would_cancel
would_replace
expected_price
expected_size
expected_notional
expected_fee
expected_edge
expected_ttl
```

## 13.5 Exit criteria

LA4 passes only if:

- shadow mode runs through normal runtime loop,
- no live order placement occurs,
- every rejected intent has a reason,
- every would-submit intent has complete risk context,
- paper and shadow-live decisions can be compared.

---

# LA5 — Maker-Only Micro Autonomous Trading

## 14.1 Objective

Allow tiny autonomous live maker orders after LA3 and LA4 pass.

## 14.2 Scope

Authorized behavior:

```text
post-only GTD only
maker-only only
BTC/ETH/SOL only
approved wallet only
approved host only
one small order per market/token according to cap
minimum or near-minimum size only
short TTL
strict no-trade window near market end
strict quote staleness limits
automatic halt on mismatch
```

## 14.3 Prohibited behavior

LA5 does not allow:

```text
taker orders
FAK/FOK strategy orders
batch order spam
unbounded cancel/replace
production sizing
more assets
multiple wallets
restricted-region access
inventory-blind sells
```

## 14.4 Inventory-aware side mapping

The live system must not treat all sells as valid.

Required behavior:

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

Any SELL intent must check conditional token inventory before approval.

## 14.5 Live risk limits

Required configurable live limits:

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
max_cancel_rate
max_submit_rate
max_reconciliation_lag_ms
max_book_staleness_ms
max_reference_staleness_ms
no_trade_seconds_before_close
```

## 14.6 Halt triggers

The bot must halt live placement on:

```text
geoblock failure
account preflight failure
heartbeat stale
unknown open order
unknown fill
unexpected partial fill
reserved balance mismatch
balance mismatch
position mismatch
trade status failure
SDK/Rust disagreement
book stale
reference stale
market close proximity
max daily loss reached
max open orders reached
cancel failure
submit/readback mismatch
network ambiguity after submit
```

## 14.7 Exit criteria

LA5 passes only if:

- maker-only live orders are placed under tiny caps,
- all live orders are journaled,
- all fills/cancels/expirations are reconciled,
- no taker orders occur,
- no inventory-invalid sells occur,
- no unresolved open-order mismatch remains,
- verification artifact summarizes live behavior and P&L.

---

# LA6 — Quote Manager and Cancel/Replace

## 15.1 Objective

Build the quote lifecycle system needed for market making.

## 15.2 New module

```text
src/live_quote_manager.rs
```

## 15.3 Responsibilities

The quote manager owns decisions to:

```text
place quote
leave quote
cancel quote
replace quote
expire quote
halt quote
skip market
```

It must consider:

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
```

## 15.4 Cancel/replace policy

A quote may be replaced only when:

```text
fair value moved beyond configured tolerance
book moved enough to change post-only safety
edge fell below threshold
position/inventory changed
time-to-close entered a stricter zone
risk limits changed
```

## 15.5 Anti-churn protections

The quote manager must enforce:

```text
minimum quote lifetime
maximum cancel rate
maximum replacement rate
minimum edge improvement for replacement
cooldown after failed submit
cooldown after failed cancel
cooldown after reconciliation mismatch
```

## 15.6 Exit criteria

LA6 passes only if:

- stale quotes are canceled automatically,
- replacement is rate-limited,
- cancel confirmations are reconciled,
- reserved balance release is verified,
- quote churn metrics are reported,
- no uncontrolled cancel/replace loops occur.

---

# LA7 — Selective Taker Gate

## 16.1 Objective

Add taker execution only after maker-only live evidence supports it.

## 16.2 Entry criteria

LA7 may begin only if:

- LA3 controlled fill passed,
- LA5 maker-only micro autonomy passed,
- LA6 quote manager passed,
- maker-only live results show no unresolved lifecycle bugs,
- paper/live divergence is understood,
- adverse selection is measured,
- live risk controls have not produced unexplained mismatches.

## 16.3 Taker rule

A taker order is allowed only when:

```text
expected_value >
  spread
  + taker_fee
  + slippage
  + latency_buffer
  + adverse_selection_buffer
  + minimum_profit_buffer
```

## 16.4 Required taker controls

```text
strict worst-price limit
visible-depth check
max market impact
max taker notional
max taker orders per day
max taker fee spend
no taker near close unless separately approved
no retry loop after ambiguous submit
immediate reconciliation
```

## 16.5 Exit criteria

LA7 passes only if:

- tiny taker orders reconcile cleanly,
- fee accounting is correct,
- slippage is measured,
- taker P&L is separated from maker P&L,
- taker mode can be disabled independently,
- no taker order can bypass maker/live risk controls.

---

# LA8 — Scale Decision

## 17.1 Objective

Decide whether to scale toward higher volume.

## 17.2 Required evidence

Scaling is allowed only if the system can report:

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

## 17.3 Volume target realism

The system should not treat 1,000–1,500 trades/day as the next milestone.

With BTC/ETH/SOL 15-minute windows:

```text
3 assets × 96 windows/day = 288 market windows/day
```

Reaching 1,000 fills/day requires roughly:

```text
1,000 / 288 ≈ 3.5 fills per market window
```

That likely requires:

- repeated intra-window fills,
- reliable cancel/replace,
- some selective taker execution,
- enough venue liquidity,
- positive expectancy after fees and slippage,
- expansion beyond the first maker-only micro phase.

## 17.4 Scale gates

Scaling may increase only one dimension at a time:

```text
size
order count
asset count
timeframe count
taker usage
runtime duration
```

Do not increase multiple dimensions in the same approval gate.

---

## 18. Functional Requirements Summary

| ID | Requirement | Priority |
|---|---|---:|
| FR-001 | Add Live Alpha approval flags defaulting disabled | P0 |
| FR-002 | Add durable live order journal | P0 |
| FR-003 | Add live reconciliation layer | P0 |
| FR-004 | Add live balance tracker | P0 |
| FR-005 | Add live position book | P0 |
| FR-006 | Add heartbeat module | P0 |
| FR-007 | Add startup crash recovery reconciliation | P0 |
| FR-008 | Add one controlled fill canary path | P0 |
| FR-009 | Add shadow live executor | P0 |
| FR-010 | Add live-specific risk engine | P0 |
| FR-011 | Add inventory-aware side mapping | P0 |
| FR-012 | Add maker-only micro live execution | P1 |
| FR-013 | Add quote manager | P1 |
| FR-014 | Add cancel/replace lifecycle | P1 |
| FR-015 | Add selective taker gate | P2 |
| FR-016 | Add scale decision report | P2 |

---

## 19. Non-Functional Requirements

## 19.1 Safety

- Fail closed by default.
- Halt on ambiguity.
- Never assume a submit failed just because the network response failed.
- Never assume a cancel succeeded until venue readback confirms it.
- Never sell conditional tokens unless inventory exists.
- Never use live order placement from restricted regions.
- Never continue after unknown open orders.

## 19.2 Observability

Required metrics:

```text
live_alpha_enabled
live_orders_submitted_total
live_orders_accepted_total
live_orders_rejected_total
live_orders_canceled_total
live_orders_filled_total
live_orders_partially_filled_total
live_unknown_open_orders_total
live_reconciliation_mismatches_total
live_risk_halts_total
live_balance_mismatch_total
live_position_mismatch_total
live_reserved_balance_mismatch_total
live_heartbeat_stale_total
live_submit_latency_ms
live_cancel_latency_ms
live_readback_latency_ms
live_fill_latency_ms
live_edge_at_submit_bps
live_edge_at_fill_bps
live_adverse_selection_rate
live_realized_pnl
live_unrealized_pnl
live_fee_spend
live_slippage_bps
quote_cancel_replace_rate
```

## 19.3 Auditability

Every live action must have:

```text
intent id
risk decision id
config snapshot
market snapshot
book snapshot
reference snapshot
account snapshot
order id, if submitted
trade id, if filled
human approval id, if required
```

## 19.4 Security

- No secrets in logs.
- No signed payloads in logs unless redacted.
- No raw private keys in config files.
- Environment handles only.
- Redaction tests must cover logs, errors, debug prints, artifacts, and verification notes.

---

## 20. Config Requirements

Example configuration:

```toml
[live_alpha]
enabled = false
mode = "disabled" # disabled | fill_canary | shadow | maker_micro | quote_manager | taker_gate
approved_host_required = true
approved_wallet_required = true
geoblock_required = true
heartbeat_required = true

[live_alpha.risk]
max_wallet_funding_pusd = "10.00"
max_available_pusd_usage = "2.00"
max_reserved_pusd = "2.00"
max_single_order_notional = "0.10"
max_per_market_notional = "0.25"
max_per_asset_notional = "0.50"
max_total_live_notional = "1.00"
max_open_orders = 3
max_open_orders_per_market = 1
max_open_orders_per_asset = 1
max_daily_realized_loss = "0.50"
max_daily_unrealized_loss = "0.50"
max_fee_spend = "0.10"
max_submit_rate_per_min = 3
max_cancel_rate_per_min = 6
max_reconciliation_lag_ms = 5000
max_book_staleness_ms = 1000
max_reference_staleness_ms = 1000
no_trade_seconds_before_close = 60

[live_alpha.fill_canary]
enabled = false
human_approval_required = true
max_notional = "0.05"
max_price = "0.99"
allow_fok = false
allow_fak = false
allow_marketable_limit = false

[live_alpha.maker]
enabled = false
post_only = true
order_type = "GTD"
ttl_seconds = 30
min_edge_bps = 100
replace_tolerance_bps = 25
min_quote_lifetime_ms = 5000

[live_alpha.taker]
enabled = false
max_notional = "0.05"
min_ev_after_all_costs_bps = 250
max_slippage_bps = 50
max_orders_per_day = 1
```

The numeric values above are placeholders. Approval artifacts must set the actual values.

---

## 21. Testing and Verification Philosophy

The user has already tested the Polymarket API/CLOB integration through LB1–LB7. This PRD does not require repeating generic API tests.

However, new live-risk-bearing code must be validated because it introduces new failure modes.

Required validation is limited to:

- new module unit tests,
- fail-closed config tests,
- journal persistence tests,
- reconciliation mismatch tests,
- inventory-aware sell rejection tests,
- heartbeat stale tests,
- shadow executor dry run,
- one controlled live fill canary,
- verification artifacts after each live phase.

The purpose is not to “test the API again.” The purpose is to avoid letting a new autonomous live path trade from an unproven state machine.

---

## 22. Verification Artifacts

Each live phase must produce a verification note:

```text
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

Each verification note must include:

```text
scope
config snapshot
wallet funding cap
runtime duration
markets touched
orders submitted
orders filled
orders canceled
open orders after run
reserved pUSD after run
balance before/after
positions before/after
P&L
fees
mismatches
halts
lessons
go/no-go decision
```

---

## 23. Kill Switches

Required kill switches:

```text
GLOBAL_LIVE_KILL_SWITCH
LIVE_ALPHA_KILL_SWITCH
LIVE_ALPHA_MAKER_KILL_SWITCH
LIVE_ALPHA_TAKER_KILL_SWITCH
LIVE_ALPHA_CANCEL_REPLACE_KILL_SWITCH
```

Any kill switch must:

1. stop new order placement,
2. allow approved safe cancellation or reconciliation,
3. write a halt event,
4. require explicit operator action to resume.

---

## 24. Runbook Updates

Update or create:

```text
runbooks/live-alpha-runbook.md
runbooks/live-alpha-fill-canary-runbook.md
runbooks/live-alpha-reconciliation-runbook.md
runbooks/live-alpha-rollback-runbook.md
runbooks/live-alpha-incident-response.md
```

Minimum runbook sections:

```text
preflight checklist
approved config checklist
how to run fill canary
how to inspect journal
how to inspect open orders
how to inspect balances
how to inspect positions
how to handle SDK/Rust disagreement
how to handle unknown open order
how to handle partial fill
how to handle failed cancel
how to handle stale heartbeat
how to halt
how to resume
how to verify zero open orders
how to verify reserved pUSD release
```

---

## 25. Acceptance Criteria

Live Alpha is accepted only when:

1. LA0 approval exists.
2. Live journal is durable.
3. Reconciliation can detect mismatches.
4. Heartbeat failures halt safely.
5. Startup recovery detects unknown live state.
6. One controlled fill canary passes.
7. Shadow live executor runs without posting.
8. Maker-only micro live mode runs under strict caps.
9. Quote manager can cancel/replace without uncontrolled churn.
10. All live orders are reconciled.
11. No unexplained balance, reserved balance, order, trade, or position mismatch remains.
12. Taker execution remains disabled unless separately approved.
13. Scale decision is based on evidence, not trade-count ambition.

---

## 26. Open Questions

1. What exact pUSD funding cap should the dedicated Live Alpha wallet have?
2. Should the controlled fill canary use FOK, FAK, or a marketable limit?
3. What minimum order size is acceptable for the current venue constraints?
4. Should the first fill canary buy Up, buy Down, or choose based on best liquidity?
5. How long after fill should settlement follow-up wait before verification is complete?
6. What is the acceptable maximum reconciliation delay?
7. Should maker-only micro mode allow one order per market or one order per token?
8. What adverse selection threshold blocks progression to taker mode?
9. What paper/live divergence threshold blocks scaling?
10. What is the maximum daily loss that is psychologically and financially acceptable?

---

## 27. Recommended Implementation Order

```text
1. Commit LIVE_ALPHA_PRD.md
2. Commit LIVE_ALPHA_IMPLEMENTATION_PLAN.md
3. Add live-alpha config flags defaulting disabled
4. Add live_order_journal
5. Add live_reconciliation
6. Add live_position_book
7. Add live_balance_tracker
8. Add live_heartbeat
9. Add startup crash recovery checks
10. Add controlled fill canary path
11. Run one approved tiny fill canary
12. Add shadow live executor
13. Add live risk engine
14. Add maker-only micro mode
15. Add quote manager
16. Add cancel/replace policy
17. Analyze maker-only live evidence
18. Decide whether to open selective taker gate
19. Decide whether to scale
```

---

## 28. Final Product Decision

The next milestone is not:

```text
Remove canary cap and start autonomous trading.
```

The next milestone is:

```text
Live Alpha: prove one real fill end-to-end, build reconciliation, shadow live execution, then enable tiny maker-only autonomy under strict caps.
```

This is the fastest responsible path from LB7 to a real live trading bot.
