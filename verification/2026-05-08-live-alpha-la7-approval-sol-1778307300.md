# Live Alpha LA7 Taker Dry-Run Approval

Date: 2026-05-08
Branch: `live-alpha/la7-taker-gate`
Status: LA7 TAKER DRY RUN APPROVED

This artifact authorizes a dry-run-only LA7 taker canary review for the exact market and bounds below. It does not authorize live submission, signing, canceling, batch orders, FOK/FAK orders, retry after ambiguous submit, or any live taker canary.

## Required Fields

| Field | Value |
| --- | --- |
| approval_id | LA7-2026-05-08-taker-dry-run-003 |
| baseline_id | LA7-2026-05-08-wallet-baseline-003 |
| baseline_capture_run_id | 18adab7ed4f41d38-170f4-0 |
| baseline_hash | sha256:fff55e06dc3983e30fea11ceff7bfa63f45e50f9d3d42bd85d2e8060cb9e3d5e |
| wallet | 0x280ca8b14386Fe4203670538CCdE636C295d74E9 |
| funder | 0xB06867f742290D25B7430fD35D7A8cE7bc3a1159 |
| market_slug | sol-updown-15m-1778307300 |
| condition_id | 0x351c7064aa3e814a8252619d0934ea0bd57774fc65b6ba24a46aa68188de78e0 |
| token_id | 21248074218639388557786179973749722783035602047162029149201517054291314612112 |
| outcome | Up |
| side | BUY |
| max_size | 5.0 |
| max_notional | 2.70 |
| worst_price | 0.54 |
| max_fee | 0.10 |
| max_slippage_bps | 100 |
| no_near_close_cutoff_seconds | 600 |
| max_orders_per_day | 1 |
| retry_after_ambiguous_submit | forbidden |
| batch_orders | forbidden |
| cancel_all | forbidden |

## Snapshot Context

Captured at approximately `2026-05-09T06:14:30Z` / `2026-05-08T23:14:30-0700`.

| Field | Value |
| --- | --- |
| market_window_utc | 2026-05-09T06:15:00Z to 2026-05-09T06:30:00Z |
| near_close_cutoff_utc | 2026-05-09T06:20:00Z |
| best_bid | 0.43 |
| best_ask | 0.44 |
| visible_ask_size_at_best | 39250 |
| estimated_notional_at_best | 2.20 |

## Decision

Dry-run only. Live taker remains `NO-GO` until a separate live approval artifact exists.
