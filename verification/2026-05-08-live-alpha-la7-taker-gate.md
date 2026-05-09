# Live Alpha LA7 Selective Taker Gate Verification

Date: 2026-05-08
Branch: `live-alpha/la7-taker-gate`
Base: `6e85e08b80f204f5320332232802a75048d820d8`
Scope: LA7 selective taker gate and shadow taker evidence only.

## Decision

Status: IMPLEMENTED FOR SHADOW, DRY-RUN CANARY REVIEW, AND SEPARATE FINAL-GATED LIVE PATH REVIEW; LIVE TAKER CANARY REMAINS NO-GO.

No live taker canary occurred. No live order was submitted. No live cancel was sent. No batch order, cancel-all, FOK/FAK expansion, retry loop after ambiguous submit, production sizing, or general taker strategy was added.

2026-05-09 addendum: the separate final-gated live path is now implemented for review in `live-alpha-taker-canary --human-approved`, but no live taker canary occurred and the existing dry-run approval artifact remains non-executable for live submission. See `verification/2026-05-09-live-alpha-la7-live-taker-canary-path.md`.

LA7 now has a reviewed dry-run canary CLI surface and a separate final-gated live implementation, but live taker execution remains blocked until:

- a real approved-host LA7 account baseline artifact is captured and configured by exact baseline ID, capture run ID, and redacted artifact path,
- position evidence is complete or explicitly blocks live canary,
- heartbeat, reconciliation, inventory, geoblock/compliance, and live risk checks pass,
- a separate fresh bounded approval artifact authorizes a dry-run canary,
- that dry-run canary passes with no blockers,
- a separate live approval artifact exists, binds the exact dry-run report/decision hashes, is unexpired, and is passed to the live command with its exact `--approval-sha256`.

## Entry Criteria Evidence

- `git status --short --branch`: `## live-alpha/la7-taker-gate`
- `git rev-parse HEAD`: `6e85e08b80f204f5320332232802a75048d820d8`
- `git rev-parse main`: `6e85e08b80f204f5320332232802a75048d820d8`
- `git merge-base HEAD main`: `6e85e08b80f204f5320332232802a75048d820d8`
- Current inputs checked: `PRD.md`, `LIVE_ALPHA_PRD.md`, `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`, `STATUS.md`, `verification/2026-05-06-live-alpha-la6-quote-manager.md`, `verification/2026-05-06-live-alpha-la6-approval.md`.
- LA6 evidence remains maker-only. It did not authorize LA7, live taker, FAK/FOK strategy orders, batch orders, cancel-all, production sizing, or cap reset.

## Official Docs Rechecked

- Polymarket authentication: L1/L2 auth separation and private endpoint requirements.
  - https://docs.polymarket.com/api-reference/authentication
- User orders and trades readback: authenticated user-order/trade evidence remains required for account-history-aware reconciliation.
  - https://docs.polymarket.com/api-reference/trade/get-user-orders
  - https://docs.polymarket.com/api-reference/trade/get-trades
- Order model and marketable order behavior: all orders are limit orders; market orders use FOK/FAK immediate execution with a documented worst-price field; FOK/FAK remain separate market order types and are not added by LA7.
  - https://docs.polymarket.com/trading/orders/create
  - https://docs.polymarket.com/developers/CLOB/orders/onchain-order-info
- Fees: taker-only fee model and `fee = C * feeRate * p * (1 - p)` formula match the existing local `fee_paid` primitive used by LA7 when market `fd.r` is available. The `fee-rate` endpoint is reachable but remains semantically ambiguous for exact LA7 fee estimates; see addendum.
  - https://docs.polymarket.com/trading/fees
  - https://docs.polymarket.com/api-reference/market-data/get-fee-rate
- Single-order readback and heartbeat docs were rechecked, but authenticated behavior was not probed in this run.
  - https://docs.polymarket.com/api-reference/trade/get-single-order-by-id
  - https://docs.polymarket.com/api-reference/trade/send-heartbeat
- V2 migration docs were rechecked for pUSD, order field, timestamp, and fee-at-match-time behavior.
  - https://docs.polymarket.com/v2-migration
- Market/user WebSocket and endpoint locations remain unchanged for read-only shadow evidence.
  - https://docs.polymarket.com/developers
- Geolocation/geoblock docs remain a required safety input before live-capable behavior.
  - https://docs.polymarket.com/api-reference/geoblock
- Rate limits remain time-sensitive and reinforce bounded read-only capture/replay behavior.
  - https://docs.polymarket.com/api-reference/rate-limits

No checked doc conflicted with the LA7 shadow-only implementation because no live taker submit/cancel/signing/readback path was opened. Live-capable taker behavior remains blocked on the API ambiguities and read-only evidence gaps below.

## Current Official API Verification Addendum

Timestamp: 2026-05-08T08:12:10Z to 2026-05-08T08:13:13Z.

Method: official Polymarket docs plus unauthenticated/read-only endpoint probes only. No L2 credentials, private keys, signatures, order posts, order cancels, heartbeat POSTs, or authenticated readback calls were used.

### Public endpoint shape

Checks:

```text
GET https://clob.polymarket.com/ok
GET https://clob.polymarket.com/time
GET https://polymarket.com/api/geoblock
GET https://gamma-api.polymarket.com/markets/slug/{asset}-updown-15m-1778227200
GET https://clob.polymarket.com/clob-markets/{condition_id}
GET https://clob.polymarket.com/fee-rate?token_id={token_id}
GET https://clob.polymarket.com/book?token_id={token_id}
```

Observed sanitized results:

- CLOB health/time: `/ok` returned `200 "OK"` and `/time` returned a Unix timestamp.
- Geoblock: response shape contained `blocked`, `country`, `ip`, and `region`; the command output redacted IP/location and recorded only `blocked=false`.
- Current sampled BTC/ETH/SOL 15-minute slugs existed, were `active=true`, `closed=false`, `acceptingOrders=true`, `feesEnabled=true`, `orderMinSize=5`, and `orderPriceMinTickSize=0.01`.
- `getClobMarketInfo` for sampled BTC/ETH/SOL returned `mos=5`, `mts=0.01`, `mbf=1000`, `tbf=1000`, `fd={"r":0.07,"e":1,"to":true}`, `itode=true`, and Up/Down token rows.
- `fee-rate` for each sampled Up token returned `base_fee=1000`.
- Public `/book` returned visible bid/ask levels for the sampled Up tokens.

### Confirmed for LA7 assumptions

- Fees: official fee docs state fees are applied at match time, not included in orders; makers pay zero; only takers pay; current crypto formula is `fee = C * feeRate * p * (1 - p)` with `feeRate=0.07`; `getClobMarketInfo(conditionID).fd` exposes `{r,e,to}`. This matches the local `fee_paid(fill_size, fill_price, Taker, raw_fee_config)` formula when `fd.r` is available.
- Order types and worst price: official order docs state all orders are limit orders; market orders are FOK/FAK immediate execution; BUY market order `amount` is dollars to spend; SELL market order `amount` is shares; `price` on market orders is the worst-price/slippage-protection limit. This matches LA7's shadow-only visible-depth and worst-price-limit modeling for BUY candidates.
- Readback fields: official docs for `/data/orders` and `/trades` include order status, market/asset/side/price/original size/size matched/order type, trade status, `taker_order_id`, `fee_rate_bps`, `trader_side`, `transaction_hash`, and `maker_orders`. These remain the minimum fields for LA7 reconciliation and fee attribution.
- Heartbeat/geoblock/rate limits: official docs require geoblock checks before order placement; heartbeat is authenticated `POST /heartbeats` and missing heartbeat can auto-cancel open orders; CLOB readback and trading endpoints are rate-limited/throttled. LA7 must continue to fail closed on unknown/failed geoblock, stale/missing heartbeat, and rate-limit/degraded readback.
- V2 fields: official V2 migration docs state production is `https://clob.polymarket.com`; V2 uses pUSD collateral; order uniqueness uses `timestamp` in milliseconds; `nonce`, `feeRateBps`, and user-set `taker` are removed from signed orders; `metadata` and `builder` are added; fees are operator-set at match time.

### Ambiguities / blockers

- Fee-rate endpoint semantics: official `GET /fee-rate` docs describe `base_fee` as basis points, and live read-only samples returned `base_fee=1000`, matching `tbf=1000`. This does not directly match the crypto fee-doc formula/table rate of `0.07` or the sampled `fd.r=0.07`. Decision: exact LA7 taker fee estimates should use `getClobMarketInfo().fd.r` plus the official formula when available and must block if `fd` is missing or if `fd`, `fee-rate`, and trade readback disagree. Do not treat `base_fee=1000` or `tbf=1000` alone as the final taker fee formula until Polymarket clarifies or approved-host readback proves the mapping.
- Single-order readback path: official docs fetched on 2026-05-08 document authenticated `GET /order/{orderID}`. The repo's prior LB6 live closeout and current Rust readback constant use `GET /data/order/{orderID}` because the live closeout observed `/order/{orderID}` returning `404` while the official SDK path worked. This cannot be safely reverified here without authenticated readback. Decision: live LA7 taker canary remains BLOCKED until an approved-host read-only SDK/Rust comparison verifies the exact single-order readback path for a known order ID and records any docs/live behavior disagreement.
- Heartbeat POST cannot be verified in this run because it is authenticated and could affect live session state. Decision: keep heartbeat behavior BLOCKED for live taker unless the approved-host phase evidence proves healthy heartbeat handling without exposing secrets.
- Authenticated account/order/trade/balance/position readback cannot be verified in this run because the task disallows secrets/authentication. Decision: live taker canary remains BLOCKED until the LA7 account baseline artifact, position evidence, and account-history-aware reconciliation are captured from the approved host.

## Implementation Summary

- Added `src/live_taker_gate.rs`.
  - Evaluates BUY taker eligibility per outcome from a read-only `DecisionSnapshot`.
  - LA7 shadow taker is intentionally BUY-only in this changeset. SELL/reduce-only taker requires live inventory and venue position evidence that is still incomplete for the current wallet, so SELL/reduce/inventory-aware taker remains a scoped `NO-GO` before any full live taker enablement.
  - Uses existing `SignalEngine` fair probability and existing `paper_executor::fee_paid` fee primitive.
  - Requires:
    - `expected_value > spread + taker_fee + slippage + latency_buffer + adverse_selection_buffer + minimum_profit_buffer`
    - visible ask depth for at least the minimum order size,
    - worst-price limit,
    - max taker notional,
    - max slippage,
    - max orders per day,
    - max fee spend,
    - no near-close taker,
    - fresh reference,
    - fresh book,
    - active market status,
    - live alpha mode/gates,
    - geoblock/compliance,
    - heartbeat,
    - reconciliation,
    - baseline readiness,
    - inventory cleanliness,
    - live risk readiness.
  - Fails closed to the stricter positive limit when both LA7 and broader risk caps are present for single-order taker notional, book freshness, and reference freshness; falls back to the broader risk limit only when the LA7-specific limit is zero.
  - Reports would-take count, live-allowed count, fee/depth/slippage/latency rejects, EV after costs, estimated fee/notional, and maker/taker paper P&L split.
- Updated `src/replay.rs`.
  - Adds shadow taker replay collection without changing paper order/fill outputs.
  - Keeps baseline/live-risk readiness false in shadow replay so live taker cannot accidentally become allowed.
- Updated `src/main.rs`.
  - Adds `paper --shadow-live-alpha --shadow-taker`.
  - Requires `--shadow-live-alpha` for `--shadow-taker`.
  - Writes `shadow_taker_decisions.jsonl` and `shadow_taker_report.json`.
  - Adds dry-run-only `live-alpha-taker-canary --dry-run`.
  - Requires `--approval-id` and `--approval-artifact`.
  - Parses the final LA7 taker approval fields, loads the configured baseline artifact, binds one market/condition/token/outcome/BUY candidate, evaluates depth/fee/slippage/EV/safety gates, writes dry-run artifacts, and has no submit/sign/cancel/batch/FOK/FAK/retry path.
  - 2026-05-09 addendum: adds separate `live-alpha-taker-canary --human-approved` final-gated implementation requiring a live-only approval artifact, exact `--approval-sha256`, unexpired approval, dry-run report/decision hash binding, fresh pre-submit safety checks, create-new one-order cap reservation, official SDK BUY GTC submit, immediate readback/reconciliation, and consumed cap/live report artifacts. This addendum does not execute or authorize a live taker canary.
- Updated `src/live_alpha_config.rs`.
  - Taker config may validate only under `live_alpha.enabled=true` and `mode=taker_gate`.
  - Enabled taker mode now also requires config-pinned `baseline_id`, `baseline_capture_run_id`, and `baseline_artifact_path`.
  - Default config remains inert and taker disabled.
- Updated `src/live_account_baseline.rs`, `src/live_startup_recovery.rs`, and `src/main.rs`.
  - Preserves the LA7 account-baseline prerequisite in `live-alpha-account-baseline`.
  - Validates baseline hash, configured baseline ID, configured capture run ID, wallet/funder/signature binding, current readback count consistency, zero open orders, zero reserved pUSD, and baseline trade presence in current readback.
  - Makes startup recovery load the configured baseline for enabled LA7 taker mode and reconcile history-bearing accounts by ignoring only explicitly baselined trade IDs.
  - Keeps live taker blocked while baseline position evidence is incomplete.

## Shadow Taker Runtime Evidence

Command:

```bash
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker
```

Result: PASS.

Key output:

```text
run_id=18ad8867a1c168b0-17678-0
paper_shadow_live_alpha_enabled=true
paper_shadow_taker_enabled=true
live_order_placement_enabled=false
paper_completed_cycles=1
paper_order_count=0
paper_fill_count=0
paper_total_pnl=0.000000
shadow_live_decision_count=0
shadow_taker_report_path=reports/sessions/18ad8867a1c168b0-17678-0/shadow_taker_report.json
shadow_taker_evaluation_count=256
shadow_taker_would_take_count=0
shadow_taker_live_allowed_count=0
shadow_taker_rejected_by_fee_count=0
shadow_taker_rejected_by_depth_count=0
shadow_taker_rejected_by_slippage_count=0
shadow_taker_rejected_by_latency_buffer_count=0
shadow_taker_estimated_ev_after_costs_bps_average=none
shadow_taker_estimated_fee=0.000000
shadow_taker_estimated_notional=0.000000
shadow_taker_paper_maker_fill_count=0
shadow_taker_paper_taker_fill_count=0
shadow_taker_paper_maker_fees_paid=0.000000
shadow_taker_paper_taker_fees_paid=0.000000
```

Follow-up shadow run after this verification pass:

```text
run_id=18ada5a8ce3668d8-d7ec-0
paper_shadow_live_alpha_enabled=true
paper_shadow_taker_enabled=true
live_order_placement_enabled=false
paper_completed_cycles=1
paper_order_count=0
paper_fill_count=0
paper_total_pnl=0.000000
shadow_taker_report_path=reports/sessions/18ada5a8ce3668d8-d7ec-0/shadow_taker_report.json
shadow_taker_evaluation_count=236
shadow_taker_would_take_count=0
shadow_taker_live_allowed_count=0
shadow_taker_rejected_by_fee_count=0
shadow_taker_rejected_by_depth_count=0
shadow_taker_rejected_by_slippage_count=0
shadow_taker_rejected_by_latency_buffer_count=0
shadow_taker_estimated_ev_after_costs_bps_average=none
shadow_taker_estimated_fee=0.000000
shadow_taker_estimated_notional=0.000000
shadow_taker_paper_maker_fill_count=0
shadow_taker_paper_taker_fill_count=0
shadow_taker_paper_maker_fees_paid=0.000000
shadow_taker_paper_taker_fees_paid=0.000000
```

Shadow taker report reason counts (first run):

```text
baseline_not_ready=256
book_stale=86
heartbeat_not_healthy=256
live_alpha_disabled=256
live_risk_controls_not_passed=256
max_orders_per_day_exceeded=256
missing_book=9
missing_fair_probability=247
reconciliation_not_clean=256
reference_stale=256
taker_disabled=256
taker_gate_mode_not_enabled=256
```

Interpretation: default config stayed fail-closed. Shadow evaluation ran, but no candidate met the configured/live-readiness gates and no live taker path opened.

Second run reason counts:

```text
baseline_not_ready=236
book_stale=91
heartbeat_not_healthy=236
live_alpha_disabled=236
live_risk_controls_not_passed=236
market_too_close_to_close=224
max_orders_per_day_exceeded=236
missing_best_ask=27
missing_best_bid=33
missing_book=9
missing_fair_probability=167
reconciliation_not_clean=236
reference_stale=236
taker_disabled=236
taker_gate_mode_not_enabled=236
```

## Fee Model Evidence

Command:

```bash
cargo test --offline fee_model
```

Result: PASS, 1 filtered test.

Covered behavior: taker fee is charged through the existing local fee primitive, maker fees remain zero, and a taker candidate is rejected when the taker fee removes positive EV after all required costs and the minimum profit buffer.

## Depth Check Evidence

Command:

```bash
cargo test --offline depth_check
```

Result: PASS, 2 filtered tests.

Covered behavior: insufficient visible ask depth rejects a taker, and multi-level consumption rejects when worst price or slippage exceeds configured limits.

## Cap/Freshness Regression Evidence

Command:

```bash
cargo test --offline live_taker_gate
```

Result: PASS, 9 tests.

Added regression coverage:

- LA7 book/reference freshness limits are stricter than broader risk freshness limits when both are set.
- Broader risk freshness limits are used only when the LA7-specific freshness limit is zero.
- The effective taker single-order notional cap uses the stricter positive value between `live_alpha.taker.max_notional` and `live_alpha.risk.max_single_order_notional`.

These tests would fail under the previous `.max(...)` freshness behavior and under a taker-only notional cap that ignored the stricter live risk cap.

## BUY-Only Scope Boundary

Current LA7 shadow taker evaluates only `Side::Buy` candidates against visible ask depth. This is intentional for this shadow/code gate. Full SELL/reduce-only taker would require account baseline binding plus complete live conditional-token position evidence and inventory-aware venue reconciliation; the current baseline gate explicitly remains blocked while `position_evidence_complete=false`.

Decision: SELL/reduce/inventory-aware taker remains `NO-GO` and must not be enabled before a later reviewed LA7 follow-up with complete position evidence, inventory proofs, focused tests, and separate human approval.

## Verification Commands

```bash
cargo fmt --check
set -o pipefail
bash scripts/verify-pr.sh > /tmp/verify-pr-2026-05-08-live-alpha-la7-taker-gate.log 2>&1
echo EXIT:$?
tail -n 20 /tmp/verify-pr-2026-05-08-live-alpha-la7-taker-gate.log
cargo test --offline live_taker_gate
cargo test --offline fee_model
cargo test --offline depth_check
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Results:

- `bash scripts/verify-pr.sh > /tmp/verify-pr-2026-05-08-live-alpha-la7-taker-gate.log 2>&1; echo EXIT:$?; tail -n 20 ...`: PASS/complete, log is complete and contains `EXIT:0` at the end.
- `cargo fmt --check`: PASS.
- `cargo test --offline live_account_baseline`: PASS, 12 filtered library tests.
- `cargo test --offline startup_recovery`: PASS, 9 filtered library tests and 11 filtered main tests.
- `cargo test --offline live_alpha_config`: PASS, 7 filtered library tests.
- `cargo test --offline live_taker_gate`: PASS, 13 filtered library tests.
- `cargo test --offline live_alpha_taker_canary`: PASS, 3 filtered main tests.
- `cargo test --offline`: PASS, 412 library tests, 78 main tests, and 0 doc tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- Earlier shadow runtime evidence remains: `cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker` PASS, runs `18ad8867a1c168b0-17678-0` and `18ada5a8ce3668d8-d7ec-0`.

## Safety Scan

Commands:

```bash
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading|FOK|FAK|GTD|GTC|post[_ -]?only)" src Cargo.toml config runbooks *.md
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
test ! -e .env || git check-ignore .env
test ! -e config/local.toml || git check-ignore config/local.toml
```

Result: PASS.

Expected hits only:

- historical LA3/LA5/LA6/live-beta code and tests outside the LA7 taker gate,
- read-only account baseline no-secret guarantee text,
- `LIVE_ORDER_PLACEMENT_ENABLED=false` checks/output,
- `TakerGate.can_place_live_orders()` test assertion for the pre-existing enum behavior,
- feed capture variable names using "batch" for normalized read-only feed batches.
- new LA7 `src/live_taker_gate.rs` hits are shadow-only fee/depth/slippage/gate checks and do not submit, cancel, sign, or route live orders.
- new `live-alpha-taker-canary --dry-run` hits in `src/main.rs` are parser, gate-evaluation, report-writing, and explicit no-live-action fields only.
- new `src/live_account_baseline.rs` hits are redacted read-only baseline artifact fields, public wallet/funder identifiers in tests/notes, and explicit no-secret guarantees.

No new LA7 live submit path, live cancel path, cancel-all path, order batch path, FOK/FAK expansion, raw secret value, API-secret value, seed phrase, mnemonic, or private-key material was added.

## Live Canary Fields

- Approval ID: NONE.
- Live canary occurred: NO.
- Order details: NONE.
- Trade details: NONE.
- Fees: NONE.
- Slippage: NONE.
- P&L: NONE.
- Live reconciliation result: NOT RUN; no live taker order occurred.

## Approval Follow-Up

The operator granted broad permission to proceed with the human-approval workflow, but LA7 still requires bounded evidence tied to the exact account baseline, market, price, size, fee, and slippage limits.

Credential-loaded read-only baseline capture initially succeeded as `baseline-001`, but that capture correctly blocked because the proxy/funder wallet still had 5 positions. After those positions were cleared/redeemed, Data API checks showed `funder_position_count=0` and `wallet_position_count=0`.

The first recapture attempt for `baseline-002` failed on `/sampling-markets` response-body decoding. The read-only CLOB client now sends `Accept-Encoding: identity`; `cargo test --offline live_beta_readback` passed after that fix. The recapture then succeeded:

```text
baseline_id=LA7-2026-05-08-wallet-baseline-002
baseline_capture_run_id=18ada759309e3ad0-13d88-0
baseline_hash=sha256:22ab15276a4d8fe6418b20c2fefa27325fbb753bc5ced0acd9e9d9718c760737
open_order_count=0
trade_count=23
reserved_pusd_units=0
available_pusd_units=6323882
position_evidence_complete=true
position_count=0
la7_live_gate_status=passed
la7_live_gate_block_reasons=
```

After the browser-backed account tab was available, the agent used Chrome read-only portfolio navigation only. The Polymarket portfolio page showed `$6.32` cash/available, `No positions found.`, and `No open orders found.` This is supporting context only; it does not replace authenticated CLI artifacts.

The agent then recaptured the authenticated read-only baseline:

```text
baseline_id=LA7-2026-05-08-wallet-baseline-003
baseline_capture_run_id=18adab7ed4f41d38-170f4-0
baseline_hash=sha256:fff55e06dc3983e30fea11ceff7bfa63f45e50f9d3d42bd85d2e8060cb9e3d5e
geoblock_blocked=false
geoblock_country=BR
geoblock_region=SP
open_order_count=0
trade_count=23
reserved_pusd_units=0
available_pusd_units=6323882
position_evidence_complete=true
position_count=0
la7_live_gate_status=passed
la7_live_gate_block_reasons=
```

Read-only public market snapshot for a bounded BUY taker candidate:

```text
captured_at_utc=2026-05-08T18:45:20Z..2026-05-08T18:45:30Z
approval_status=NOT_APPROVED_CANDIDATE_ONLY
market_slug=btc-updown-15m-1778265900
condition_id=0x07244c875f32a72be3548edc3b7e8216bf7e605757ae933267c3f77ea6a8e41a
market_question=Bitcoin Up or Down - May 8, 2:45PM-3:00PM ET
market_active=true
market_closed=false
accepting_orders=true
enable_order_book=true
fee_config={r:0.07,e:1,to:true}
outcome=Down
side=BUY
token_id=720986806552495213604739819630143891197564154591059404160911245803995872816
book_hash=2ad524c5a3bac134bc5f6926ae2d01ded6e5050d
book_timestamp_ms=1778265923000
best_bid=0.47
best_ask=0.48
visible_ask_size_at_best=381.12
canary_size_shares=5.0
estimated_notional_pusd=2.40
max_notional_pusd=2.56
max_slippage_bps=100
worst_price_limit=0.49
estimated_worst_price=0.48
estimated_slippage_bps=0
estimated_taker_fee_pusd=0.087360
required_max_fee_spend_pusd_at_least=0.10
max_orders_per_day=1
no_near_close_cutoff_seconds=600
market_close_utc=2026-05-08T19:00:00Z
candidate_expires_by_no_near_close_utc=2026-05-08T18:50:00Z
```

Decision: baseline state is now approval-eligible from the account-history and flat-position perspective. The market snapshot above is not an approval artifact and must not be used for execution. It expires under the 600-second no-near-close policy at 2026-05-08T18:50:00Z, requires increasing the LA7 taker fee cap from the current local `$0.06` to at least `$0.10`, and the branch still has no reviewed `live-alpha-taker-canary` CLI. Do not create or use a live taker approval artifact until a fresh market snapshot is captured, the exact bounds are reviewed, startup recovery/readback passes against the exact baseline, and a reviewed live taker canary command exists.

## Dry-Run Canary Engineering Review

Review date: 2026-05-08.

Review scope: engineering review only for a proposed `live-alpha-taker-canary --dry-run` path. No live taker canary, submit, sign, cancel, API-key creation, batch order, FOK/FAK order, or retry-after-ambiguous-submit action was run.

Step-by-step result:

1. Confirm scope: BLOCKED. The reviewed `live-alpha-taker-canary --dry-run` surface is absent from the CLI. Current LA7 execution surface is still `paper --shadow-live-alpha --shadow-taker` plus read-only `live-alpha-account-baseline`.
2. Confirm branch state: PASS. `git status --short --branch` reported `## live-alpha/la7-taker-gate`; `git rev-parse HEAD` and `git merge-base HEAD main` both returned `6e85e08b80f204f5320332232802a75048d820d8`; `main` and `origin/main` also resolve to `6e85e08b80f204f5320332232802a75048d820d8`. Worktree contains LA7 implementation artifacts and docs, not unrelated branch work.
3. Review new CLI surface: BLOCKED. `cargo run --features live-alpha-orders -- --help` lists no `live-alpha-taker-canary`; `cargo run --features live-alpha-orders -- live-alpha-taker-canary --help` exits with unrecognized subcommand; the full dry-run shape also exits with unrecognized subcommand before any config or network action.
4. Review approval artifact parser: BLOCKED. No LA7 taker-canary parser exists for the final fields `approval_id`, `baseline_id`, `baseline_capture_run_id`, `baseline_hash`, wallet/funder, exact market/token/outcome/BUY side, price/size/fee/slippage caps, no-near-close cutoff, one-order cap, and forbidden retry/batch/cancel-all fields. Existing approval parsers are LA3/LA5/LA6 only.
5. Review baseline binding: BLOCKED for the absent canary CLI; PASS for the underlying baseline/startup recovery code. `baseline-003` binds wallet/funder/signature type, baseline ID, capture run ID, hash, zero open orders, zero reserved pUSD, zero positions, and 23 explicitly baselined trade IDs. The future canary command still cannot load or prove this because it does not exist.
6. Review market binding: BLOCKED. The code has BUY-only shadow evaluation, but no `live-alpha-taker-canary --dry-run` command binds exactly one approval market/condition/token/outcome. The prior approval-candidate snapshot is read-only, non-approval, and expired at `2026-05-08T18:50:00Z`.
7. Review price and depth checks: BLOCKED for the absent canary dry run; PASS for the shadow gate implementation. `src/live_taker_gate.rs` computes best bid, best ask, visible ask depth, requested size, average executable price, worst executable price, worst-price limit, and slippage bps for BUY candidates.
8. Review fee and EV checks: BLOCKED for the absent canary dry run; PASS for the shadow gate implementation. The shadow gate computes taker fee, spread cost, slippage cost, latency buffer, adverse-selection buffer, minimum-profit buffer, and EV after all costs; ambiguous or missing fee/depth inputs fail closed through reason codes.
9. Review live safety gates: BLOCKED for the absent canary dry run; PASS for the shadow gate reason-code coverage. The shadow gate evaluates geoblock, heartbeat, reconciliation, inventory, baseline readiness, live risk readiness, cap remaining, no-near-close, stale book/reference, max notional, max fee, and max orders/day as blockers. Local validation still reports live gates blocked, as expected for non-live review.
10. Review non-execution guarantee: PASS. Scan hits are expected historical LA3/LA5/LA6/live-beta submit/cancel/signing code, runbook text, and LA7 shadow/baseline fields. No LA7 taker submit, signing, cancel, batch, FOK/FAK, or cancel-all path exists.
11. Required tests: PASS for requested local/offline checks. Commands run: `cargo test --offline live_taker_gate`, `cargo test --offline live_account_baseline`, `cargo test --offline startup_recovery`, `cargo test --offline live_alpha_config`, `cargo run --offline -- --config config/local.toml validate --local-only`, `cargo fmt --check`, `cargo clippy --offline -- -D warnings`, and `git diff --check`.
12. Review result: BLOCKED for dry-run canary approval. The branch remains suitable for human review of shadow taker and baseline binding, but not for approving a `live-alpha-taker-canary --dry-run` path.

Required status fields:

```text
dry_run_cli_review_status=blocked
approval_artifact_parser=blocked
baseline_binding=blocked
market_binding=blocked
depth_check=blocked
fee_ev_check=blocked
non_execution_scan=passed
live_submit_path_opened=false
live_canary_occurred=false
```

Additional reviewed sub-status:

```text
baseline_artifact_binding_code=passed
shadow_market_depth_fee_ev_checks=passed
shadow_live_safety_reason_codes=passed
```

## Dry-Run Canary Command Follow-Up

Follow-up date: 2026-05-08.

Scope: implement only the missing dry-run command. No fresh approval artifact was created. No live taker canary, submit, sign, cancel, API-key creation, batch order, FOK/FAK order, or retry-after-ambiguous-submit action was run.

Implemented surfaces:

- `live-alpha-taker-canary --dry-run`
- Required CLI args: `--dry-run`, `--approval-artifact`, `--approval-id`
- Approval parser fields: `approval_id`, `baseline_id`, `baseline_capture_run_id`, `baseline_hash`, `wallet`, `funder`, `market_slug`, `condition_id`, `token_id`, `outcome`, `side=BUY`, `max_size`, `max_notional`, `worst_price`, `max_fee`, `max_slippage_bps`, `no_near_close_cutoff_seconds`, `max_orders_per_day=1`, `retry_after_ambiguous_submit=forbidden`, `batch_orders=forbidden`, `cancel_all=forbidden`
- Dry-run evidence: `live_alpha_taker_canary_dry_run_report.json` plus `live_alpha_taker_canary_dry_run_decision.json` when a decision snapshot exists
- Non-execution guarantee: the command has no submit, sign, cancel, batch, FOK/FAK, or retry branch

Command checks:

```text
cargo run --features live-alpha-orders -- live-alpha-taker-canary --help
```

Result: PASS. Help now lists the dry-run command and required approval binding fields.

```text
cargo run --features live-alpha-orders -- live-alpha-taker-canary --dry-run
```

Result: EXPECTED FAIL-CLOSED. Clap exits before runtime with missing required `--approval-id` and `--approval-artifact`.

```text
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-taker-canary --dry-run --approval-artifact verification/2026-05-08-live-alpha-la7-approval-candidate.md --approval-id LA7-2026-05-08-taker-candidate-001
```

Result: EXPECTED FAIL-CLOSED. The parser rejects the expired candidate before live readback or market probing:

```text
approval_artifact_not_approved_or_consumed
approval_field_missing:approval_id
approval_field_missing:max_fee
approval_field_missing:max_notional
approval_field_missing:max_size
approval_field_missing:worst_price
approval_status_missing
```

Focused tests:

```text
cargo test --offline live_taker_gate
cargo test --offline live_alpha_taker_canary
cargo test --offline live_account_baseline
cargo test --offline startup_recovery
cargo test --offline live_alpha_config
cargo test --offline fee_model
cargo test --offline depth_check
```

Results:

- `live_taker_gate`: PASS, 13 library tests.
- `live_alpha_taker_canary`: PASS, 3 main tests.
- `live_account_baseline`: PASS, 12 library tests.
- `startup_recovery`: PASS, 9 library tests and 11 main tests.
- `live_alpha_config`: PASS, 7 library tests.
- `fee_model`: PASS, 1 library test.
- `depth_check`: PASS, 2 library tests.

Shadow command rerun:

```text
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker
```

Result: EXPECTED ENVIRONMENT/MARKET FAIL-CLOSED in this follow-up run. The command exited with:

```text
paper runtime requires one eligible in-window active BTC 15m market at now_wall_ts=1778269558863; next eligible start_ts=none
```

This does not open a live path. Earlier successful shadow reports in this note remain the shadow evidence for the implemented gate.

Current approval status:

```text
fresh_bounded_approval_artifact_created=false
expired_candidate_reused=false
dry_run_only_command_present=true
live_submit_path_opened=false
live_canary_occurred=false
la7_live_taker_status=NO-GO
```

## Fresh Bounded Dry-Run Approval

Approval artifact:

```text
path=verification/2026-05-08-live-alpha-la7-approval.md
approval_id=LA7-2026-05-08-taker-dry-run-001
approval_artifact_sha256=sha256:8dd716037530e870ce81ea92a7f8642f2e6796838899090e63733c9b12c54787
status=LA7 TAKER DRY RUN APPROVED
live_execution_authorized=false
```

Bounded market:

```text
market_slug=btc-updown-15m-1778273100
condition_id=0xa58b8cfde3f7aa75b19d95e891f0133507f4caf71df647c7792277a5acaf62f8
token_id=31397586596402482044445491161773882475705477303446864072433092447405604929366
outcome=Down
side=BUY
max_size=5.0
max_notional=2.70
worst_price=0.48
max_fee=0.10
max_slippage_bps=100
no_near_close_cutoff_seconds=600
max_orders_per_day=1
retry_after_ambiguous_submit=forbidden
batch_orders=forbidden
cancel_all=forbidden
```

Dry-run command:

```text
set -a
source .env
set +a
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-taker-canary --dry-run --approval-artifact verification/2026-05-08-live-alpha-la7-approval.md --approval-id LA7-2026-05-08-taker-dry-run-001
```

Result:

```text
run_id=18adb209348a61e0-b004-0
live_alpha_taker_canary_status=passed
live_alpha_taker_canary_block_reasons=
live_alpha_taker_canary_not_submitted=true
baseline_gate_status=passed
reconciliation_status=passed
position_evidence_complete=true
position_count=0
open_order_count=0
trade_count=23
reserved_pusd_units=0
available_pusd_units=6323882
heartbeat=not_started_no_open_orders
would_take=true
live_allowed=true
decision_reason_codes=
best_bid=0.33
best_ask=0.34
average_price=0.34
worst_price=0.34
worst_price_limit=0.35000000000000003
size=5
notional=1.7000000000000002
taker_fee=0.07854
slippage_bps=0
estimated_ev_after_costs_bps=1403.7742993931613
report_path=reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_report.json
decision_path=reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_decision.json
```

Interpretation: the dry-run-only LA7 canary gate passed for this exact market/account/baseline/cap binding. This proves the review path only. It does not authorize or imply that a live taker order was placed. The command reported `not_submitted=true` and `no_live_actions` all false for submitted, signed, canceled, batch orders, FOK/FAK, and retry-after-ambiguous-submit.

Post-dry-run status:

```text
fresh_bounded_approval_artifact_created=true
expired_candidate_reused=false
dry_run_only_command_present=true
dry_run_passed=true
live_submit_path_opened=false
live_canary_occurred=false
la7_live_taker_status=NO-GO
```

## Stop Point

Stop after LA7 implementation and verification for human review. Do not start LA8. Do not merge without PR review.
