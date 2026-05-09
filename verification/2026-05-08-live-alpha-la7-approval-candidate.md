# Live Alpha LA7 Taker Canary Approval Candidate

Date: 2026-05-08
Branch: `live-alpha/la7-taker-gate`
Status: NOT APPROVED; NOT EXECUTABLE.

This file records one bounded read-only candidate snapshot. It is not a human approval artifact and must not be used to submit an order. The branch has no reviewed `live-alpha-taker-canary` command, and the candidate expired under the no-near-close rule at 2026-05-08T18:50:00Z.

## Account Binding

| Field | Value |
| --- | --- |
| approval_candidate_id | `LA7-2026-05-08-taker-candidate-001` |
| baseline_id | `LA7-2026-05-08-wallet-baseline-003` |
| baseline_capture_run_id | `18adab7ed4f41d38-170f4-0` |
| baseline_hash | `sha256:fff55e06dc3983e30fea11ceff7bfa63f45e50f9d3d42bd85d2e8060cb9e3d5e` |
| baseline_artifact_path | `artifacts/live_alpha/LA7-2026-05-08-wallet-baseline-003/account_baseline.redacted.json` |
| wallet | `0x280ca8b14386Fe4203670538CCdE636C295d74E9` |
| funder | `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159` |
| signature_type | `poly_proxy` |
| geoblock_blocked | `false` |
| open_order_count | `0` |
| trade_count | `23` |
| reserved_pusd_units | `0` |
| available_pusd_units | `6323882` |
| position_evidence_complete | `true` |
| position_count | `0` |
| la7_live_gate_status | `passed` |

## Browser Context

Chrome portfolio inspection was read-only. It showed `$6.32` cash/available, `No positions found.`, and `No open orders found.`

Browser context is supporting evidence only. Authenticated CLI baseline artifacts remain the binding source of truth.

## Candidate Market Snapshot

| Field | Value |
| --- | --- |
| captured_at_utc | `2026-05-08T18:45:20Z..2026-05-08T18:45:30Z` |
| market_slug | `btc-updown-15m-1778265900` |
| market_question | `Bitcoin Up or Down - May 8, 2:45PM-3:00PM ET` |
| condition_id | `0x07244c875f32a72be3548edc3b7e8216bf7e605757ae933267c3f77ea6a8e41a` |
| market_active | `true` |
| market_closed | `false` |
| accepting_orders | `true` |
| enable_order_book | `true` |
| outcome | `Down` |
| side | `BUY` |
| token_id | `720986806552495213604739819630143891197564154591059404160911245803995872816` |
| book_hash | `2ad524c5a3bac134bc5f6926ae2d01ded6e5050d` |
| book_timestamp_ms | `1778265923000` |
| best_bid | `0.47` |
| best_ask | `0.48` |
| visible_ask_size_at_best | `381.12` |
| fee_config | `{r:0.07,e:1,to:true}` |

## Candidate Bounds

| Field | Value |
| --- | --- |
| canary_size_shares | `5.0` |
| estimated_notional_pusd | `2.40` |
| max_notional_pusd | `2.56` |
| max_slippage_bps | `100` |
| worst_price_limit | `0.49` |
| estimated_worst_price | `0.48` |
| estimated_slippage_bps | `0` |
| estimated_taker_fee_pusd | `0.087360` |
| required_max_fee_spend_pusd_at_least | `0.10` |
| max_orders_per_day | `1` |
| no_near_close_cutoff_seconds | `600` |
| market_close_utc | `2026-05-08T19:00:00Z` |
| candidate_expires_by_no_near_close_utc | `2026-05-08T18:50:00Z` |
| retry_after_ambiguous_submit | `forbidden` |
| batch_orders | `forbidden` |
| cancel_all | `forbidden` |
| immediate_reconciliation_required | `true` |

## Decision

This is a useful shape for the real approval artifact, but it is not usable as approval:

- the candidate is time-bound and expired at 2026-05-08T18:50:00Z;
- the current local live-alpha risk config still has `max_fee_spend=0.06`, below the estimated `0.087360` taker fee;
- no reviewed `live-alpha-taker-canary` CLI exists on this branch;
- no live taker dry-run passed against this exact candidate;
- no live taker canary occurred.

The real approval artifact must be freshly regenerated for a then-current market window after the reviewed canary command exists.
