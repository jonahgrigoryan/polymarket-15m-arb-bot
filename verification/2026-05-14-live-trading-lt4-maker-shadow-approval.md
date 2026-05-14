# 2026-05-14 Live Trading LT4 Maker Shadow And Approval-Envelope Dry-Run

## Scope

LT4 adds final live-trading maker quote evaluation and approval-envelope dry-run support only. It does not place orders, cancel orders, sign for submit, post heartbeat, write caps, enable taker behavior, add batch order behavior, add cancel-all behavior, emit raw signatures, emit order-submit auth headers, or expose secret material.

## Official Documentation Recheck

Rechecked on 2026-05-14:

- Polymarket Authentication: https://docs.polymarket.com/api-reference/authentication
  - L1 signing and L2 API credentials remain separate.
  - Even with L2 authentication headers, order-creating methods still require signed order payloads.
  - LT4 does not create raw signatures, create submit auth headers, or submit order payloads.
- Polymarket Orderbook: https://docs.polymarket.com/trading/orderbook
  - The orderbook is public/read-only and includes bid/ask levels, `tick_size`, `min_order_size`, `neg_risk`, and orderbook hash fields.
- Polymarket Order Overview: https://docs.polymarket.com/trading/orders/overview
  - Post-only orders are limit orders that should only rest on the book.
  - Post-only must not be combined with FOK/FAK and can only be used with GTC/GTD.
  - Crossing/marketable post-only orders are rejected rather than executed.
- Polymarket CLOB market info: https://docs.polymarket.com/api-reference/markets/get-clob-market-info
  - CLOB market metadata includes minimum order size, minimum tick size, maker base fee, taker base fee, and fee detail fields.
- Polymarket Rate Limits: https://docs.polymarket.com/api-reference/rate-limits
  - Rate limits can throttle/delay requests; LT4 treats stale or unavailable state as a blocker rather than relying on delayed data.
- Polymarket Geoblock: https://docs.polymarket.com/api-reference/geoblock
  - Deployment-host geoblock remains `GET https://polymarket.com/api/geoblock`.
  - Blocked, stale, unavailable, or unapproved geoblock state fails closed.

## Implementation Summary

- Added `src/live_trading_maker.rs` with:
  - final-live maker dry-run artifact schema `lt4.live_trading_maker_shadow.v1`,
  - maker quote/candidate checks for market identifier completeness, price-times-size notional consistency, edge-at-submit, fee presence, tick-size alignment, min size, book/reference/predictive age, no-trade window, post-only order type, post-only marketability, single-order notional cap, matched paper/live shadow decision, baseline binding, heartbeat, unresolved live order state, incidents, and caps,
  - approval-envelope Markdown generation with host, wallet/funder, baseline, market, side, order type, price, size, expiry, fee, and cap fields present even when blocked,
  - no-submit/no-cancel/no-sign-for-submit/no-cap-write/no-taker/no-batch/no-cancel-all proof fields.
- Added CLI:
  - `live-trading-maker-canary --dry-run --approval-id <id> --approval-artifact <path>`
- Added final-live config fields under `[live_trading]` for LT4 baseline and allowance binding:
  - `baseline_id`
  - `baseline_capture_run_id`
  - `baseline_artifact_path`
  - `baseline_hash`
  - `required_collateral_allowance_units`
- Fixed the LT3/LT4 readiness hardening issue: final-live authenticated readback now sources `required_collateral_allowance_units` from `config.live_trading` instead of `config.live_beta.readback_account` while authenticating with final-live handles/account binding.
- Updated `STATUS.md` for the LT4 branch, current blocker state, and the mandatory stop before LT5.

## LT4 Local Dry-Run Result

Command:

```text
cargo run --offline -- --config config/default.toml live-trading-maker-canary --dry-run --approval-id LT4-LOCAL-DRY-RUN --approval-artifact verification/2026-05-14-live-trading-lt4-approval-candidate.md
```

Result:

- Command status: PASS
- Artifact status: `blocked`
- Run ID: `18af722fa03a9350-bca2-0`
- Approval envelope path: `verification/2026-05-14-live-trading-lt4-approval-candidate.md`
- Approval envelope file hash: `sha256:102da6fd6a7f5141c91ef72f37e2266b1a14fd2344ec99814afb992ec08324ec`
- Dry-run report path: `artifacts/live_trading/LT4-LOCAL-DRY-RUN/maker_dry_run.redacted.json`
- Dry-run report file hash: `sha256:918737cf6c8156f439327ccfbe83f99e02d8d06be9407504f8eec43b07b21441`
- Dry-run artifact hash: `sha256:1aadc8c278aa48b8bf3438848cb3acc872304d486405ff7eadd70a0f7cf39866`
- No-submit proof:
  - `not_submitted=true`
  - `network_post_enabled=false`
  - `network_cancel_enabled=false`
  - `signed_order_for_submission=false`
  - `raw_signature_generated=false`
  - `order_submit_auth_headers_generated=false`
  - `taker_submission_enabled=false`
  - `batch_order_path_enabled=false`
  - `cancel_all_path_enabled=false`
  - `cap_writes=false`

Interpretation: tracked default config remains fail-closed. The LT4 dry-run writes complete redacted evidence and a complete approval-envelope candidate, but the envelope is explicitly `LT4 BLOCKED - NOT APPROVED FOR SUBMISSION`.

## Market-Window Candidate

- Candidate status: no safe maker candidate exists under tracked default config.
- Market slug: blocked/missing.
- Condition ID: blocked/missing.
- Token ID: blocked/missing.
- Outcome: blocked/missing.
- Side: `BUY` placeholder only.
- Order type: `GTD`.
- Post-only: `true`.
- Price/size/notional: blocked/unavailable.
- Expiry: generated placeholder only; not submit-ready.
- Tick size/min size: blocked/unavailable.
- Book/reference/predictive state: unavailable/stale by LT4 policy.

Blockers:

- `account_binding_missing`
- `approved_host_not_matched`
- `condition_id_missing`
- `edge_at_submit_below_threshold`
- `fee_not_known`
- `final_live_config_disabled`
- `geoblock_not_passed`
- `geoblock_stale_or_not_checked`
- `heartbeat_required_not_fresh`
- `market_slug_missing`
- `max_single_order_notional_missing`
- `missing_baseline_binding`
- `near_close_market`
- `notional_invalid`
- `outcome_missing`
- `paper_shadow_comparison_not_feasible`
- `post_only_marketability_unknown`
- `price_invalid`
- `required_collateral_allowance_missing`
- `size_invalid`
- `stale_book`
- `stale_predictive`
- `stale_reference`
- `token_id_missing`
- `unknown_min_size`
- `unknown_tick_size`
- `unresolved_live_order_state_unknown`

## Expected Order Shape

If a future approved-host LT4 dry-run has a safe candidate, the envelope must bind exactly one maker order with:

- one post-only `GTD` or `GTC` maker order,
- side `BUY` or `SELL`,
- concrete market slug, condition ID, token ID, and outcome,
- price aligned to the current tick size,
- size at or above current minimum order size,
- notional equal to price times size within tolerance and at or below approved cap,
- expiry outside the GTD one-minute security threshold when order type is `GTD`,
- known maker fee estimate,
- fresh book/reference/predictive state,
- fresh geoblock and heartbeat state,
- zero unresolved live orders and zero unreviewed incidents,
- complete baseline binding and cap fields.

## Maker/Taker Status

- Maker dry-run status: `blocked` under tracked default config.
- Taker status: disabled in LT4.
- Batch order path: disabled in LT4.
- Cancel-all path: disabled in LT4.

## Comparable Paper/Live Plan

Comparable paper/live review is not feasible from the tracked default local dry-run because no approved-host current market window, account baseline, fresh book/reference/predictive state, or same-window paper decision is bound. A pass-capable LT4 candidate must compare the proposed final-live maker decision against the paper decision for the same market window. If the paper/live decisions are not available, it keeps `paper_shadow_comparison_not_feasible` as a blocker; if they are available but diverge, it blocks with `paper_live_shadow_mismatch`.

## Verification Commands

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test --offline live_trading_maker` | PASS | 8 lib tests and 2 CLI tests passed, covering complete candidate pass, marketable post-only blocking, missing market identifier blocking, over-cap single-order notional blocking, stale notional mismatch/cap blocking, paper/live shadow mismatch blocking, missing baseline/tick/stale-state blocking, approval envelope field coverage, CLI parse, and dry-run-only command guard. |
| `cargo test --offline live_quote_manager` | PASS | 37 existing quote-manager tests passed. |
| `cargo test --offline live_trading_readback_uses_final_live_allowance_requirement` | PASS | Regression proves final-live readback allowance requirement is sourced from `config.live_trading`, not `config.live_beta.readback_account`. |
| `cargo run --offline -- --config config/default.toml live-trading-maker-canary --dry-run --approval-id LT4-LOCAL-DRY-RUN --approval-artifact verification/2026-05-14-live-trading-lt4-approval-candidate.md` | PASS | Wrote blocked LT4 approval candidate and redacted report with no-submit proof. |
| `shasum -a 256 verification/2026-05-14-live-trading-lt4-approval-candidate.md artifacts/live_trading/LT4-LOCAL-DRY-RUN/maker_dry_run.redacted.json` | PASS | Hashes recorded above. |
| `rg -n -i "private[_ -]?key\|api[_ -]?key\|passphrase\|poly_signature\|signature\"\\s*:\\s*\"0x\|secret\|P15M_\|POLY_\|auth header\|authorization" verification/2026-05-14-live-trading-lt4-approval-candidate.md artifacts/live_trading/LT4-LOCAL-DRY-RUN/maker_dry_run.redacted.json` | PASS | No matches in generated LT4 artifacts. |
| `git diff --check` | PASS | Whitespace check passed. |
| `scripts/verify-pr.sh` | PASS | Formatting, full tests, clippy, diff whitespace, safety scan, no-secret scan, and ignored-local-secret-file checks passed. Full tests covered 457 lib tests, 114 bin tests, and 0 doc tests. |

## Safety Scan

- No raw private keys, API secrets, passphrases, raw signatures, or auth header values are emitted by the LT4 approval candidate or dry-run report.
- New source references to order submission, cancel submission, taker submission, batch orders, and cancel-all are dry-run proof fields, disallowed-scope strings, tests, documentation, or pre-existing Live Alpha/Beta paths. LT4 adds no live write client and no final-live submit/cancel dispatch.
- LT4 dry-run records `network_post_enabled=false`, `network_cancel_enabled=false`, `signed_order_for_submission=false`, `raw_signature_generated=false`, `order_submit_auth_headers_generated=false`, `taker_submission_enabled=false`, `batch_order_path_enabled=false`, `cancel_all_path_enabled=false`, and `cap_writes=false`.
- The local default-config artifact is blocked, not approved for LT5.

## Exit Gate

LT4 is implemented locally for review and verification. The default local dry-run correctly blocks because no safe maker candidate exists in tracked config, while still producing a complete approval envelope and redacted report. Stop here after verification; do not start LT5 or run a real maker canary without explicit human/operator approval of the exact LT5 artifact.
