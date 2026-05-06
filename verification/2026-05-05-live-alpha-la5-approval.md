# Live Alpha LA5 Approval Artifact

Status: LA5 APPROVED FOR THIS RUN ONLY
Execution Gate Status: LA5 RUN COMPLETED

## Operator Approval Recorded

- Approved by: Jonah / operator.
- Approval status: `LA5 APPROVED FOR THIS RUN ONLY`.
- Approval ID: `LA5-2026-05-06-001`.
- Approval date: `2026-05-06`.
- Human action required after completion: PR merge only.
- Execution result: completed through the production CLI path with exactly three maker-only post-only GTD micro orders, journal replay, authenticated REST readback, and browser comparison.

## Scope

- Phase: LA5 maker-only micro autonomy.
- Branch: `live-alpha/la5-maker-micro`.
- Scope limit: maker-only post-only GTD micro autonomy, 3 sequential orders max, no taker, no FAK/FOK, no batch, no cancel-all, no LA6.

## Config-Bound Fields

| Field | Value |
| --- | --- |
| approved_wallet | `0x280ca8b14386Fe4203670538CCdE636C295d74E9` |
| approved_funder | `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159` |
| max_single_order_notional | `2.56` |
| max_total_live_notional | `2.56` |
| max_available_pusd_usage | `1.0` |
| max_reserved_pusd | `1.0` |
| max_fee_spend | `0.06` |
| max_orders | `3` |
| max_open_orders | `1` |
| max_duration_sec | `300` |
| no_trade_seconds_before_close | `600` |
| ttl_seconds | `30` effective quote TTL |
| venue_gtd_expiration_delta | `90` seconds (`60` second venue buffer + `30` second effective TTL) |
| signature_type | `1` |

## Live Readback Fields

Filled from LB4 authenticated REST readback plus Computer Use confirmation on Polymarket immediately before the LA5 run. REST run `18ad0a27b36aa900-289f-0` passed live network readback; Computer Use showed the logged-in Polymarket portfolio with Brazil region indicator, portfolio `$6.31`, cash `$6.31`, available to trade `$6.31`, no positions, and no open orders.

| Field | Value |
| --- | --- |
| available_pusd_units | `6314318` |
| reserved_pusd_units | `0` |
| open_order_count | `0` |
| heartbeat_status | `not_started_no_open_orders` |
| funder_allowance_units | `18446744073709551615` |

## Execution Result

| Field | Value |
| --- | --- |
| run_id | `18ad0a3c18e5fc58-2b73-0` |
| live_submit_path | `cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-maker-micro --human-approved --approval-id LA5-2026-05-06-001 --approval-artifact verification/2026-05-05-live-alpha-la5-approval.md --max-orders 3 --max-duration-sec 300` |
| orders_submitted | `3` |
| orders_accepted | `3` |
| orders_filled | `0` |
| final_open_order_count | `0` |
| final_reserved_pusd_units | `0` |
| final_available_pusd_units | `6314318` |
| post_run_readback_run_id | `18ad0a7c98a96638-31e9-0` |
| post_run_browser_state | portfolio `$6.31`; cash `$6.31`; available to trade `$6.31`; no positions; no open orders |
| journal_replay_status | `passed` |

| Seq | Order ID | Market | Side | Price | Size | Accepted | Final |
| --- | --- | --- | --- | ---: | ---: | --- | --- |
| 1 | `0x2e00d21ad4c0f3242847359acb90bbaca2672a3ba96ac652a80dcd73d0c5627a` | `btc-updown-15m-1778088600` | `BUY Up` | `0.17` | `5.0` | `LIVE` | `CANCELED` |
| 2 | `0x23d1ebf47455ccaa39474680c38b80c72ab5ea926ca4552a523da4efe73fa62d` | `btc-updown-15m-1778088600` | `BUY Up` | `0.17` | `5.0` | `LIVE` | `CANCELED` |
| 3 | `0x3b48a71c62b08b14e66c81ec509baa8f2b56004deaf39b17a94f4414b0dcc51e` | `btc-updown-15m-1778088600` | `BUY Up` | `0.17` | `5.0` | `LIVE` | `CANCELED` |

## Human-Facing Fields

| Field | Value |
| --- | --- |
| rollback_owner | Jonah / operator |
| monitoring_owner | Jonah / operator |
| approval_id | `LA5-2026-05-06-001` |
| approval_date | `2026-05-06` |

Status: LA5 APPROVED FOR THIS RUN ONLY
Approved: Jonah / operator approved the run scope; the authorized session completed exactly three maker-only post-only GTD micro orders and no additional LA5 or LA6 order authority remains in this artifact.
Date: 2026-05-06
Scope: Maker-only post-only GTD micro autonomy, 3 orders max, no taker, no FAK/FOK, no batch, no cancel-all, no LA6
