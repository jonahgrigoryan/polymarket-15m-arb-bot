# Live Alpha LA7 Taker Dry-Run Approval

Date: 2026-05-08
Branch: `live-alpha/la7-taker-gate`
Status: LA7 TAKER DRY RUN APPROVED

This artifact authorizes a dry-run-only LA7 taker canary review for the exact market and bounds below. It does not authorize live submission, signing, canceling, batch orders, FOK/FAK orders, retry after ambiguous submit, or any live taker canary.

## Required Fields

| Field | Value |
| --- | --- |
| approval_id | LA7-2026-05-08-taker-dry-run-001 |
| baseline_id | LA7-2026-05-08-wallet-baseline-003 |
| baseline_capture_run_id | 18adab7ed4f41d38-170f4-0 |
| baseline_hash | sha256:fff55e06dc3983e30fea11ceff7bfa63f45e50f9d3d42bd85d2e8060cb9e3d5e |
| wallet | 0x280ca8b14386Fe4203670538CCdE636C295d74E9 |
| funder | 0xB06867f742290D25B7430fD35D7A8cE7bc3a1159 |
| market_slug | btc-updown-15m-1778273100 |
| condition_id | 0xa58b8cfde3f7aa75b19d95e891f0133507f4caf71df647c7792277a5acaf62f8 |
| token_id | 31397586596402482044445491161773882475705477303446864072433092447405604929366 |
| outcome | Down |
| side | BUY |
| max_size | 5.0 |
| max_notional | 2.70 |
| worst_price | 0.48 |
| max_fee | 0.10 |
| max_slippage_bps | 100 |
| no_near_close_cutoff_seconds | 600 |
| max_orders_per_day | 1 |
| retry_after_ambiguous_submit | forbidden |
| batch_orders | forbidden |
| cancel_all | forbidden |

## Snapshot Context

Captured at approximately `2026-05-08T20:45:23Z` / `2026-05-08T13:45:23-0700`.

| Field | Value |
| --- | --- |
| market_window_utc | 2026-05-08T20:45:00Z to 2026-05-08T21:00:00Z |
| near_close_cutoff_utc | 2026-05-08T20:50:00Z |
| best_bid | 0.46 |
| best_ask | 0.47 |
| visible_ask_size_at_best | 6 |
| estimated_notional_at_best | 2.35 |
| estimated_taker_fee_at_best | 0.087185 |

## Decision

Dry-run only. Live taker remains `NO-GO`.
