# Live Alpha LA7 Live Taker Canary Path Verification

Date: 2026-05-09
Branch: `live-alpha/la7-taker-gate`
Scope: implement the separate one-order LA7 live taker canary path after reviewing the passed dry-run evidence. Do not execute live taker canary without a new live approval artifact.

## Decision

Status: IMPLEMENTED FOR REVIEW; LIVE TAKER CANARY REMAINS NO-GO.

The dry-run evidence prerequisite passed engineering review. A separate final-gated `live-alpha-taker-canary --human-approved` implementation now exists, but no live taker canary was executed and the existing dry-run approval artifact does not authorize one.

## Dry-Run Evidence Review

Inputs reviewed:

- Approval artifact: `verification/2026-05-08-live-alpha-la7-approval.md`
- Dry-run report: `reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_report.json`
- Dry-run decision: `reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_decision.json`

Verified report fields:

```text
status=passed
block_reasons=[]
not_submitted=true
baseline_gate_status=passed
reconciliation_status=passed
position_count=0
open_order_count=0
reserved_pusd_units=0
no_live_actions.submitted=false
no_live_actions.signed=false
no_live_actions.canceled=false
no_live_actions.batch_orders=false
no_live_actions.fok_or_fak=false
no_live_actions.retry_after_ambiguous_submit=false
```

Decision artifact shape:

```text
would_take=true
live_allowed=true
reason_codes=[]
side=buy
outcome=Down
best_ask=0.34
notional=1.70
taker_fee=0.07854
estimated_ev_after_costs_bps=1403.7742993931613
```

The existing approval artifact says `Status: LA7 TAKER DRY RUN APPROVED` and explicitly does not authorize live submission, signing, canceling, batch orders, FOK/FAK orders, retry after ambiguous submit, or any live taker canary.

## Official Docs Rechecked

Primary docs rechecked on 2026-05-09:

- Polymarket authentication: CLOB trading endpoints require L2 headers, and user orders still require local order signing.
  - https://docs.polymarket.com/api-reference/authentication
- L2 client methods: authenticated trading methods use SDK client initialization with signer, credentials, signature type, and funder.
  - https://docs.polymarket.com/trading/clients/l2
- Order creation: official examples use `createAndPostOrder`/`postOrder`; all orders are limit orders, and FOK/FAK are separate market order types.
  - https://docs.polymarket.com/trading/orders/create
- Order overview: post-only cannot be combined with FOK/FAK; order readback exposes status, market, asset, side, sizes, price, and order type; successful insert statuses include `matched`, `live`, `delayed`, and `unmatched`.
  - https://docs.polymarket.com/trading/orders/overview

Implementation decision: LA7 live taker uses the official `polymarket_client_sdk_v2` path already present in the repo, submits one BUY GTC marketable limit with a strict worst-price limit, and does not use batch orders, cancel-all, FOK, FAK, post-only, or retry after ambiguous submit.

## Implementation Summary

- `src/live_taker_gate.rs`
  - Added live-only approval parsing with `Status: LA7 TAKER LIVE CANARY APPROVED`.
  - Live approval requires expiry plus exact dry-run report/decision paths and hashes.
  - Added official SDK submit helper for one BUY GTC marketable-limit taker canary.
  - Added no-network submit-shape validation for approval/decision binding, no batch, no FOK/FAK, no retry policy, exact approval hash format, and approval bounds.
- `src/main.rs`
  - Preserved the dry-run path.
  - Added `live-alpha-taker-canary --human-approved --approval-sha256`.
  - Added dry-run evidence review before live gates.
  - Added fresh pre-submit geoblock/readback/baseline/heartbeat/inventory/reconciliation/market/book/reference/taker checks.
  - Added create-new one-order cap reservation before submit, consumed cap update after submit, and immediate post-submit readback/reconciliation report writing.
- `runbooks/live-alpha-runbook.md`
  - Documented the new live command shape, required live approval fields, dry-run evidence binding, cap behavior, and post-submit expectations.
- `STATUS.md`
  - Updated LA7 handoff to distinguish implemented live path from the still-blocked live execution approval state.

## Verification Log

Dry-run evidence review commands:

```text
jq '{status, block_reasons, not_submitted, baseline_gate_status, reconciliation_status, position_count, open_order_count, reserved_pusd_units, no_live_actions}' reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_report.json
jq '.' reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_decision.json
```

Focused tests:

```text
cargo test --offline live_taker_gate
cargo test --offline live_alpha_taker_canary
cargo test --offline live_alpha_taker_live_review
```

Completion verification:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

Safety scans were also reviewed through `scripts/verify-pr.sh`. Expected hits were limited to documented/live-gated code paths, existing live-alpha/live-beta surfaces, and public test/docs values. No live taker canary command was executed.

All listed verification commands passed at the time recorded.

## Hold Point

Do not run the live command yet. The remaining live-execution prerequisite is a new, separate live approval artifact that includes:

```text
Status: LA7 TAKER LIVE CANARY APPROVED
approval_expires_at_unix
dry_run_report_path
dry_run_report_sha256
dry_run_decision_path
dry_run_decision_sha256
```

The operator must compute and pass the exact live approval artifact hash with `--approval-sha256`. A fresh eligible market, fresh book/reference, fresh readback, baseline-aware reconciliation, and an unused one-order cap must pass at runtime.

## 2026-05-08 Fresh Dry-Run And Live Approval Prep

Fresh dry-run-only approval artifact:

- `verification/2026-05-08-live-alpha-la7-approval-sol-1778293800.md`
- approval_id: `LA7-2026-05-08-taker-dry-run-002`
- market: SOL Up BUY, `sol-updown-15m-1778293800`
- no-near-close cutoff: `2026-05-09T02:35:00Z`

Fresh dry-run command:

```bash
set -a; source .env; set +a
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-taker-canary --dry-run --approval-id LA7-2026-05-08-taker-dry-run-002 --approval-artifact verification/2026-05-08-live-alpha-la7-approval-sol-1778293800.md
```

Fresh dry-run result:

- run_id: `18adc4bacd70fc48-16e2d-0`
- status: `passed`
- block_reasons: `[]`
- not_submitted: `true`
- baseline_gate_status: `passed`
- reconciliation_status: `passed`
- position_count: `0`
- open_order_count: `0`
- reserved_pusd_units: `0`
- no live submit/sign/cancel/batch/FOK/FAK/retry action occurred
- decision: `would_take=true`, `live_allowed=true`, `reason_codes=[]`
- best_ask: `0.47`
- notional: `2.35`
- taker_fee: `0.087185`

Dry-run evidence hashes:

```text
sha256:a01b9066efb4cb34a2280ee11320002d0ee267715505a2f26da6313035520225  reports/sessions/18adc4bacd70fc48-16e2d-0/live_alpha_taker_canary_dry_run_report.json
sha256:7b6d3633115abdc946012cd5a0ce98a0446940d4bb2276271a661b385b204aa3  reports/sessions/18adc4bacd70fc48-16e2d-0/live_alpha_taker_canary_dry_run_decision.json
```

Separate live approval artifact prepared:

- `verification/2026-05-08-live-alpha-la7-live-approval-sol-1778293800.md`
- approval_id: `LA7-2026-05-08-taker-live-001`
- approval_expires_at_unix: `1778294040` (`2026-05-09T02:34:00Z`)
- approval artifact hash: `sha256:6fb7b72fdb1933f9ce27d2ea9e1426a3512630a374dc12a6d7bad75da74911c6`

No `live-alpha-taker-canary --human-approved` command was executed. After the approval expiry, LA7 live taker is again `NO-GO` until another fresh dry-run and live approval artifact are created.

## 2026-05-08 Live Taker Canary Execution

The expired `sol-updown-15m-1778293800` live approval was not reused. A fresh approval/dry-run/live sequence was performed for `sol-updown-15m-1778307300`.

Fresh dry-run-only approval artifact:

- `verification/2026-05-08-live-alpha-la7-approval-sol-1778307300.md`
- approval_id: `LA7-2026-05-08-taker-dry-run-003`
- market: SOL Up BUY, `sol-updown-15m-1778307300`
- no-near-close cutoff: `2026-05-09T06:20:00Z`

Fresh dry-run command:

```bash
set -a; source .env; set +a
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-taker-canary --dry-run --approval-id LA7-2026-05-08-taker-dry-run-003 --approval-artifact verification/2026-05-08-live-alpha-la7-approval-sol-1778307300.md
```

Fresh dry-run result:

- run_id: `18add113b1c090e8-2ab1-0`
- status: `passed`
- block_reasons: `[]`
- not_submitted: `true`
- baseline_gate_status: `passed`
- reconciliation_status: `passed`
- position_count: `0`
- open_order_count: `0`
- reserved_pusd_units: `0`
- no live submit/sign/cancel/batch/FOK/FAK/retry action occurred during dry-run
- decision: `would_take=true`, `live_allowed=true`, `reason_codes=[]`
- best_ask: `0.41`
- notional: `2.05`
- taker_fee: `0.084665`

Dry-run evidence hashes:

```text
sha256:3301413c015895af83476acd57b9d7dd4e030cfb0f5a3bd7d09d2470281438c1  reports/sessions/18add113b1c090e8-2ab1-0/live_alpha_taker_canary_dry_run_report.json
sha256:20cc92049e0207e9d5dfa0ea820a6777ece2fc803d7150c081c737684a95446f  reports/sessions/18add113b1c090e8-2ab1-0/live_alpha_taker_canary_dry_run_decision.json
```

Separate live approval artifact:

- `verification/2026-05-08-live-alpha-la7-live-approval-sol-1778307300.md`
- approval_id: `LA7-2026-05-08-taker-live-002`
- approval_expires_at_unix: `1778307540` (`2026-05-09T06:19:00Z`)
- approval artifact hash: `sha256:b3a5dd85609eb1b39db67ee11eaca2085a2c4c40de395c1a2f06aeccdae35c3d`

Live command:

```bash
set -a; source .env; set +a
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-taker-canary --human-approved --approval-id LA7-2026-05-08-taker-live-002 --approval-artifact verification/2026-05-08-live-alpha-la7-live-approval-sol-1778307300.md --approval-sha256 sha256:b3a5dd85609eb1b39db67ee11eaca2085a2c4c40de395c1a2f06aeccdae35c3d
```

Live command result:

- run_id: `18add124ab8912e8-301f-0`
- live report: `reports/sessions/18add124ab8912e8-301f-0/live_alpha_taker_canary_live_report.json`
- status: `submitted_post_check_blocked`
- block_reasons: `post_submit_readback_not_passed`, `post_submit_reconciliation_not_passed`
- venue response: `success=true`, `venue_status=MATCHED`
- order_id: `0x8a768554d4a993f0d521b0def432c98525570470538b679946351370de0dcab9`
- transaction_hash: `0x98480c5e82196ed5c6445fabb7063b96adc88855f3855479f653ada0287952c4`
- making_amount: `1.35`
- taking_amount: `5`
- submitted_order_count: `1`
- order_type: `GTC`
- post_only: `false`
- batch_orders: `false`
- FOK/FAK: `false`
- cancel_all: `false`
- retry_after_ambiguous_submit: `false`

The one-order cap was consumed:

- `reports/live-alpha-la7-taker-canary-cap.json`
- approval_id: `LA7-2026-05-08-taker-live-002`
- venue_status: `MATCHED`
- consumed: `true`

Post-submit read-only baseline:

```bash
set -a; source .env; set +a
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-2026-05-08-post-taker-live-001
```

Post-submit baseline result:

- run_id: `18add133173cb630-34cf-0`
- output_dir: `artifacts/live_alpha/LA7-2026-05-08-post-taker-live-001`
- status: `passed`
- trade_count: `24`
- open_order_count: `0`
- reserved_pusd_units: `0`
- available_pusd_units: `4904902`
- position_evidence_complete: `true`
- position_count: `1`
- position: 5 SOL Up shares on `sol-updown-15m-1778307300`
- baseline_hash: `sha256:b67cbd0c3d254e1dddef152050e67ee6f288fc0587863bc8e066732e2ba72034`
- la7_live_gate_status: `blocked`
- la7_live_gate_block_reasons: `baseline_positions_nonzero`

Interpretation: the one-order LA7 taker canary did submit and match. LA7 does not pass post-canary exit yet because immediate post-submit readback/reconciliation failed closed and the account now has a nonzero canary position. No retry, cancel, second live order, batch order, FOK/FAK order, or cancel-all was run.

## 2026-05-08 Post-Resolution Readback

After the canary market resolved, a fresh read-only baseline was captured:

```bash
set -a; source .env; set +a
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-2026-05-08-post-taker-resolved-001
```

Resolved baseline result:

- run_id: `18add20de02fdd58-42ec-0`
- output_dir: `artifacts/live_alpha/LA7-2026-05-08-post-taker-resolved-001`
- captured_at_rfc3339: `2026-05-09T06:33:04Z`
- status: `passed`
- block_reasons: `[]`
- trade_count: `26`
- open_order_count: `0`
- reserved_pusd_units: `0`
- available_pusd_units: `3122653`
- position_evidence_complete: `true`
- position_count: `0`
- baseline_hash: `sha256:a4648ef83da8b61f46732feba109244b1fa10d6e5ae8ad9fa4446734e221c6f0`
- LA7 live gate status from flat-position/readback perspective: `passed`

Confirmed readback entries for the canary market included:

- original live taker order id: `0x8a768554d4a993f0d521b0def432c98525570470538b679946351370de0dcab9`
- original transaction hash: `0x98480c5e82196ed5c6445fabb7063b96adc88855f3855479f653ada0287952c4`
- two additional confirmed entries on the same canary market in the resolved trade artifact

Interpretation: the resolved account readback is now flat, with no open orders, no reserved pUSD, and no remaining canary position. This resolves the earlier `baseline_positions_nonzero` blocker. It does not reset the consumed one-order cap, and it does not erase the live command's immediate post-submit `submitted_post_check_blocked` result.

## 2026-05-09 Post-Submit Reconciliation Follow-Up

Official docs rechecked for the follow-up:

- Authentication: CLOB trading endpoints require L2 authenticated headers and order creation still requires local order signing.
- Create order: successful placement can return `status=matched`, `transactionsHashes`, and `tradeIDs=[]`.
- Order overview / L2 client: matched orders create trades; trade history includes the taker order ID, transaction hash, and lifecycle statuses `MATCHED`, `MINED`, `CONFIRMED`, `RETRYING`, and `FAILED`.

Patch outcome:

- `src/main.rs` now seeds LA7 post-submit reconciliation with the submitted taker order's expected trade evidence before comparing venue readback.
- Readback trades are mapped back to the submitted order by `order_id`, submit response `trade_ids`, or transaction hash, covering the observed SDK response where `trade_ids=[]`.
- Post-submit authenticated readback now uses a bounded poll before final classification. It retries only propagation-shaped states: `matched_pending_confirmation` or `submitted_order_trade_missing_from_readback`.
- A confirmed readback trade for the submitted order no longer triggers `unexpected_fill`.
- An expected submitted-order trade still in `MATCHED`/nonterminal readback is classified as `matched_pending_confirmation`; it remains not cleanly passed and requires later confirmation evidence.
- If a `MATCHED`/successful submission has no matching trade in readback, reconciliation remains fail-closed with `submitted_order_trade_missing_from_readback`.
- The resolved flat baseline path does not reset or bypass the consumed one-order cap.
- A pre-SDK/submission-attempt cap reservation with no venue order ID remains consumed, so SDK failure or network ambiguity cannot enable a retry or second live taker canary.

Focused verification:

```bash
cargo fmt --check
cargo test --offline la7_post_submit
cargo test --offline la7_resolved_flat_baseline_does_not_reset_consumed_taker_cap
cargo test --offline la7_taker_cap_pre_submit_reservation_blocks_retry_after_failure_or_ambiguity
cargo test --offline live_taker_gate
cargo test --offline fee_model
cargo test --offline depth_check
cargo test --offline live_alpha_taker_canary
cargo test --offline live_alpha_taker_live_review
cargo test --offline live_reconciliation
cargo test --offline live_account_baseline
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

Observed result on 2026-05-09: all commands above passed. The focused `la7_post_submit` filter ran 4 tests including the bounded poll policy; the cap filters covered both resolved-flat success state and pre-submit failure/ambiguity state; the full offline suite passed 414 lib tests and 86 main tests.

Safety scans were rerun with the Live Alpha order/cancel surface, secret/key surface, and gate/reconciliation regexes from `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`. Hits remain expected documented live-gated code paths, public fixture/order IDs, public approval/readback metadata, secret handle names, and prior verification scan text. No secret values, cap reset, second live submit path, batch/FOK/FAK path, cancel-all behavior, or LA8 scope was added.

Human review decision: LA7 accepted after post-canary review. The code-level false `unexpected_fill` classification is fixed for the next report generation path and future post-submit checks now poll within a bounded window, but the already executed live canary's historical report remains `submitted_post_check_blocked`. Human review accepted the later flat/post-resolution baseline, confirmed canary trade evidence, and offline verification suite as sufficient LA7 closure evidence without rewriting the immediate post-submit failure. Taker remains disabled by default, the one-order cap remains consumed, and no LA8, second live canary, cap reset, or broad taker enablement is authorized from this branch state. Proceed only to the reviewed PR/merge path.

Cleanup note: raw redacted baseline artifacts under `artifacts/live_alpha/*` were local evidence and were removed from the working tree after this verification note captured the relevant baseline IDs, run IDs, hashes, counts, and canary order/transaction evidence. Ignored `reports/` outputs remain local runtime evidence and are not staged for the LA7 commit.
