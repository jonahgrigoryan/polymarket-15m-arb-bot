# Live Alpha LA6 Approval Artifact

Status: LA6 APPROVAL BLOCKED - NOT AUTHORIZED FOR LIVE SUBMIT/CANCEL
Execution Gate Status: LA6 NOT RUN

## Operator Approval Recorded

- Approval status: `BLOCKED`.
- Approval ID: `LA6-2026-05-06-001`.
- Approval date: `NOT RUN`.
- Human action required before live run: fill all final fields below, re-run authenticated readback, and explicitly approve one LA6 session.

## Scope

- Phase: LA6 quote manager and cancel/replace.
- Branch: `live-alpha/la6-quote-manager`.
- Scope limit: maker-only post-only GTD quote lifecycle for BTC/ETH/SOL only, exact-order-ID cancel only, no taker, no FAK/FOK, no batch, no cancel-all, no LA7.

## Config-Bound Fields

| Field | Value |
| --- | --- |
| approval_id | `LA6-2026-05-06-001` |
| approved_wallet | `BLOCKED - NOT RUN` |
| approved_funder | `BLOCKED - NOT RUN` |
| approved_markets_assets | `BTC/ETH/SOL only - NOT LIVE AUTHORIZED` |
| approved_maximum_notional | `BLOCKED - NOT RUN` |
| max_orders | `BLOCKED - NOT RUN` |
| max_replacements | `BLOCKED - NOT RUN` |
| max_duration_sec | `BLOCKED - NOT RUN` |
| ttl_seconds | `BLOCKED - NOT RUN` |
| gtd_policy | `post-only GTD with Polymarket one-minute buffer - NOT LIVE AUTHORIZED` |
| cancel_policy | `exact order ID only; cancel-all disallowed - NOT LIVE AUTHORIZED` |
| no_trade_window_policy | `default exact-order-ID cancel or halt inside no-trade window; leaving open requires explicit final approval - NOT LIVE AUTHORIZED` |
| risk_limits | `BLOCKED - NOT RUN` |
| rollback_owner | `BLOCKED - NOT RUN` |
| monitoring_owner | `BLOCKED - NOT RUN` |
| authenticated_readback_evidence | `BLOCKED - NOT RUN` |
| operator_approval_timestamp | `BLOCKED - NOT RUN` |

## Live Readback Fields

| Field | Value |
| --- | --- |
| available_pusd_units | `NOT RUN` |
| reserved_pusd_units | `NOT RUN` |
| open_order_count | `NOT RUN` |
| trade_count | `NOT RUN` |
| heartbeat_status | `NOT RUN` |
| funder_allowance_units | `NOT RUN` |

## Execution Result

| Field | Value |
| --- | --- |
| run_id | `NOT RUN` |
| live_submit_cancel_path | `BLOCKED - NO LA6 LIVE RUN AUTHORIZED` |
| quotes_placed | `NOT RUN` |
| quotes_left_alone | `NOT RUN` |
| quotes_canceled | `NOT RUN` |
| quotes_replaced | `NOT RUN` |
| fills | `NOT RUN` |
| final_open_order_count | `NOT RUN` |
| final_reserved_pusd_units | `NOT RUN` |
| journal_replay_status | `NOT RUN` |

This artifact is intentionally incomplete and must fail closed in the human-approved LA6 command.
