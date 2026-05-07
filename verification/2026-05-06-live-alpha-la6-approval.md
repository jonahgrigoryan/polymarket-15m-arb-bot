# Live Alpha LA6 Approval Artifact

Status: LA6 APPROVED AND EXECUTED FOR ONE RUN
Execution Gate Status: LA6 RUN COMPLETE

## Operator Approval Recorded

- Approval status: `APPROVED FOR THIS RUN ONLY`.
- Approval ID: `LA6-2026-05-07-005`.
- Approval timestamp: `2026-05-07T00:47:00-07:00`.
- Approval artifact used at runtime: `/tmp/p15m-la6-approval-005.md`.
- Approval artifact SHA-256: `sha256:b6e4171ddc5c56a962aa8a3c37689e7622c663ba528a08d5d37f0dec22f1f52f`.
- Approval cap state SHA-256: `sha256:ca6c355f74c9b2da77087be91d41fcb368005497865bc16e9f9413bb0a14972b`.

## Scope

- Phase: LA6 quote manager and cancel/replace.
- Branch: `live-alpha/la6-quote-manager`.
- Scope limit: maker-only post-only GTD quote lifecycle for BTC/ETH/SOL only, exact-order-ID cancel only, no taker, no FAK/FOK, no batch, no cancel-all, no LA7.

## Config-Bound Fields

| Field | Value |
| --- | --- |
| approval_id | `LA6-2026-05-07-005` |
| approved_wallet | `0x280ca8b14386Fe4203670538CCdE636C295d74E9` |
| approved_funder | `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159` |
| approved_markets_assets | `BTC/ETH/SOL only` |
| approved_maximum_notional | `2.56 pUSD config cap; submitted notional 1.00 pUSD` |
| max_orders | `1` |
| max_replacements | `1` |
| max_duration_sec | `300` |
| ttl_seconds | `30` |
| gtd_policy | `post-only GTD with Polymarket one-minute buffer` |
| cancel_policy | `exact order ID only; cancel-all disallowed` |
| no_trade_window_policy | `default exact-order-ID cancel or halt inside no-trade window; leaving open not approved` |
| risk_limits | `max_single_order_notional=2.56 max_total_live_notional=2.56 max_open_orders=1 max_submit_rate_per_min=1 max_cancel_rate_per_min=1` |
| rollback_owner | `Jonah / operator` |
| monitoring_owner | `Jonah / operator` |
| authenticated_readback_evidence | `18ad38ee3e0e5300-b6d4-0 and LA6 command initial readback` |
| operator_approval_timestamp | `2026-05-07T00:47:00-07:00` |

## Live Readback Fields

| Field | Value |
| --- | --- |
| available_pusd_units | `6314318` |
| reserved_pusd_units | `0` |
| open_order_count | `0` |
| trade_count | `23` |
| heartbeat_status | `not_started_no_open_orders` |
| funder_allowance_units | `18446744073709551615` |

## Execution Result

| Field | Value |
| --- | --- |
| run_id | `18ad38f9204f44e0-b76d-0` |
| command | `cargo run --features live-alpha-orders -- --config /tmp/p15m-la6-quote-manager.toml live-alpha-quote-manager --human-approved --approval-id LA6-2026-05-07-005 --approval-artifact /tmp/p15m-la6-approval-005.md --max-orders 1 --max-replacements 1 --max-duration-sec 300` |
| market | `btc-updown-15m-1778139900` |
| order_id | `0xea764a6d1846cef1602c37945c3734a35f99bb671ad38e9bc89236118a3e0ca9` |
| live_submit_cancel_path | `completed under live-alpha-orders feature and runtime gates` |
| quotes_placed | `1` |
| quotes_left_alone | `0` |
| quotes_canceled | `1 exact-order-ID cancel, confirmed on attempt 1` |
| quotes_replaced | `0` |
| fills | `0` |
| final_open_order_count | `0` |
| final_reserved_pusd_units | `0` |
| journal_replay_status | `quote reconciliation passed twice; final startup-recovery validate remains blocked by pre-existing account trade history outside this run` |

Notes: earlier same-session attempts were intentionally not reused as final LA6 evidence: `LA6-2026-05-07-001` hit the no-trade/runway gate with no order, `002` and `004` placed quotes that were terminal before explicit cancel dispatch, and `003` risk-rejected as `market_too_close_to_close`. The accepted LA6 exit evidence is approval/run `005`.
