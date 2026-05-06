# Live Alpha LA5 Maker Micro Autonomy Verification

## Status

LA5 IMPLEMENTED AND LIVE RUN COMPLETED. Run `18ad0a3c18e5fc58-2b73-0` executed exactly three sequential maker-only post-only GTD micro orders through the production CLI path after the approval artifact, authenticated REST readback, browser comparison, feature/runtime gates, risk, heartbeat, journal, and reconciliation gates passed.

LA5 remains the only active scope in this branch. LA6 is on hold until human review and a separate phase approval.

## Scope

- Branch: `live-alpha/la5-maker-micro`.
- Base: `main` after PR #33 merge commit `cfec8dd`.
- Allowed: live risk engine, inventory-aware side mapping, post-only GTD maker planning, exact single-order cancel primitives, dry-run CLI, production CLI LA5 maker-micro execution, LA5 journal events, LA5 metrics, approval/readback evidence.
- Not allowed: taker strategy, FAK/FOK strategy orders, marketable strategy orders, batch orders, cancel-all, LA6 quote manager, scaling, cap reset, or committed secrets.

## Plan Amendment

Approved amendment: `[live_alpha.maker].ttl_seconds = 30` is the effective quote lifetime and stale-quote cancel threshold. Venue GTD expiration includes Polymarket's documented one-minute security threshold:

```text
expiration_unix = now_unix + 60 + approved_ttl_seconds
```

Implementation consequence: a 30-second effective quote TTL produces a venue GTD expiration at least `now + 90`, while cancel eligibility begins at `now + 30`.

## Official Polymarket Docs Checked

- https://docs.polymarket.com/trading/orders/overview
  - Conclusion: GTD orders use UTC-second expiration and the docs describe a one-minute security threshold. Post-only orders can only be used with GTC/GTD and are rejected if marketable.
- https://docs.polymarket.com/trading/orders/create
  - Conclusion: `postOrder` supports `postOnly`; GTD examples document using `now + 60 + N` for an effective lifetime of `N` seconds.
- https://docs.polymarket.com/trading/clients/l2
  - Conclusion: `postOrder` posts one signed order and `cancelOrder` cancels a single open order. `postOrders`, `cancelOrders`, `cancelAll`, and `cancelMarketOrders` are separate broader APIs that LA5 did not use.
- https://docs.polymarket.com/trading/orders/cancel
  - Conclusion: single-order cancel requires L2 authentication and returns exact canceled/not-canceled evidence; cancel-all and cancel-by-market remain disallowed for LA5.
- https://docs.polymarket.com/trading/fees
  - Conclusion: makers are not charged fees; fee accounting still records zero spend when there are no fills.
- https://docs.polymarket.com/market-data/websocket/user-channel
  - Conclusion: user-channel order/trade events exist, but LA5 used authenticated REST readback and journal reconciliation as the closeout evidence.

## Config Snapshot

- `live_alpha.mode = "maker_micro"`
- `live_alpha.maker.enabled = true`
- `live_alpha.maker.post_only = true`
- `live_alpha.maker.order_type = "GTD"`
- `live_alpha.maker.ttl_seconds = 30`
- `live_alpha.risk.max_open_orders = 1`
- `live_alpha.risk.max_reserved_pusd = 1.0`
- `live_alpha.risk.max_total_live_notional = 2.56`
- `live_alpha.risk.max_submit_rate_per_min = 1`
- `live_alpha.risk.max_cancel_rate_per_min = 1`

No secret values were printed, logged, written, or committed.

## Environment And Approval Evidence

The repo root `.env` was sourced without printing values before live readback and live submit. Handle-name presence check returned:

```text
P15M_LIVE_BETA_CLOB_L2_ACCESS=present
P15M_LIVE_BETA_CLOB_L2_CREDENTIAL=present
P15M_LIVE_BETA_CLOB_L2_PASSPHRASE=present
P15M_LIVE_BETA_CANARY_PRIVATE_KEY=present
```

Approval artifact: `verification/2026-05-05-live-alpha-la5-approval.md`.

- Approved by: Jonah / operator.
- Approval status: `LA5 APPROVED FOR THIS RUN ONLY`.
- Approval ID: `LA5-2026-05-06-001`.
- Scope: exactly three maker-only post-only GTD micro orders, no taker, no FAK/FOK, no marketable orders, no batch, no cancel-all, no LA6.
- Human action after completion: PR merge only.

## Pre-Submit Readback

Authenticated REST readback passed immediately before the live session:

```text
cargo run --features live-alpha-orders -- --config config/local.toml validate --live-readback-preflight
run_id=18ad0a27b36aa900-289f-0
live_beta_geoblock_gate=passed
geoblock_country=BR
geoblock_region=SP
live_beta_readback_preflight_status=passed
live_beta_readback_preflight_live_network_enabled=true
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=23
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=6314318
live_beta_readback_preflight_funder_allowance_units=18446744073709551615
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

Computer Use read-only comparison in the already logged-in Polymarket Chrome session matched the REST state before submit:

- Polymarket region indicator: Brazil flag visible.
- Portfolio: `$6.31`.
- Cash: `$6.31`.
- Available to trade: `$6.31`.
- Positions tab: `No positions found.`
- Open orders tab: `No open orders found.`

## Live Run Command

```text
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-maker-micro --human-approved --approval-id LA5-2026-05-06-001 --approval-artifact verification/2026-05-05-live-alpha-la5-approval.md --max-orders 3 --max-duration-sec 300
```

Run result:

```text
run_id=18ad0a3c18e5fc58-2b73-0
live_alpha_maker_micro_status=completed
live_alpha_maker_micro_orders_submitted=3
live_alpha_maker_micro_cumulative_notional=2.550000
live_alpha_maker_micro_journal_replay_status=passed
```

Selected live market at runtime:

- Market slug: `btc-updown-15m-1778088600`.
- Market window: 2026-05-06 10:30:00 PDT to 2026-05-06 10:45:00 PDT.
- Outcome: `Up`.
- Token ID: `74451698162476616172003054641119426793738093636060462884903359916716560498247`.
- Side: `BUY`.
- Order type: `GTD`.
- Post-only: `true`.
- Price: `0.17`.
- Size: `5.0`.
- Notional per order: `0.85`.

## Order Evidence

| Seq | Intent ID | Order ID | Accepted | Final | GTD Expiration | Cancel After | Heartbeat ID |
| --- | --- | --- | --- | --- | ---: | ---: | --- |
| 1 | `la5-1-btc-1778088690072` | `0x2e00d21ad4c0f3242847359acb90bbaca2672a3ba96ac652a80dcd73d0c5627a` | `LIVE` | `CANCELED` | `1778088805` | `1778088745` | `6c31fd99-0493-4481-8ac1-3d275b40b084` |
| 2 | `la5-2-btc-1778088779465` | `0x23d1ebf47455ccaa39474680c38b80c72ab5ea926ca4552a523da4efe73fa62d` | `LIVE` | `CANCELED` | `1778088893` | `1778088833` | `6c8010de-9491-42ca-820c-f83ad117b29e` |
| 3 | `la5-3-btc-1778088868175` | `0x3b48a71c62b08b14e66c81ec509baa8f2b56004deaf39b17a94f4414b0dcc51e` | `LIVE` | `CANCELED` | `1778088983` | `1778088923` | `2de8a87a-dae2-4131-b92e-eb50fcfedc80` |

All three orders used:

- Market: `btc-updown-15m-1778088600`.
- Outcome/side: `BUY Up`.
- Price: `0.17`.
- Size: `5.0`.
- Notional: `0.85`.
- Effective quote TTL: `30` seconds.
- Venue GTD buffer: `60` seconds.
- Venue GTD expiration delta: `90` seconds.

Order-book/reference evidence from journal at submit:

| Seq | Best Bid | Best Ask | Book Age ms | Reference Age ms | Reference Snapshot |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 | `0.50` | `0.51` | `0` | `4424` | `https://data.chain.link/streams/btc-usd:polymarket_rtds_chainlink:1778088711000` |
| 2 | `0.53` | `0.55` | `0` | `4254` | `https://data.chain.link/streams/btc-usd:polymarket_rtds_chainlink:1778088799000` |
| 3 | `0.50` | `0.51` | `0` | `4238` | `https://data.chain.link/streams/btc-usd:polymarket_rtds_chainlink:1778088889000` |

## Reconciliation Evidence

- Journal path: `reports/live-alpha-la5-maker-micro-journal.jsonl` (gitignored runtime artifact).
- Run journal events: `maker_micro_started`, `maker_micro_approval_accepted`, three `maker_risk_approved`, three `maker_order_submit_attempted`, three `maker_order_accepted`, six `maker_reconciliation_passed`, and `maker_micro_stopped`.
- Journal replay for run `18ad0a3c18e5fc58-2b73-0`: `passed`.
- Reconciliation status for every order: `passed`.
- Reconciliation mismatches: none.
- Trade IDs for every order: empty.
- Filled orders: `0`.
- Partial fills: `0`.
- Fees paid: `0`.
- Realized P&L: `0`.
- Unrealized P&L: `0`.
- Positions before run: none.
- Positions after run: none.
- REST open orders after each final readback: `0`.
- REST reserved pUSD units after each final readback: `0`.

Balance/readback table:

| Seq | Pre-submit Available Units | Post-order Available Units | Final Available Units | Final Reserved Units |
| --- | ---: | ---: | ---: | ---: |
| 1 | `6314318` | `6314318` | `6314318` | `0` |
| 2 | `6314318` | `6314318` | `6314318` | `0` |
| 3 | `6314318` | `6314318` | `6314318` | `0` |

Post-run authenticated readback:

```text
cargo run --features live-alpha-orders -- --config config/local.toml validate --live-readback-preflight
run_id=18ad0a7c98a96638-31e9-0
live_beta_readback_preflight_status=passed
live_beta_readback_preflight_live_network_enabled=true
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=23
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=6314318
live_beta_readback_preflight_funder_allowance_units=18446744073709551615
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

Computer Use post-run browser comparison:

- Portfolio: `$6.31`.
- Cash: `$6.31`.
- Available to trade: `$6.31`.
- Positions tab: `No positions found.`
- Open orders tab: `No open orders found.`

## Incidents And Observations

- No safety gate failed during the live run.
- No order filled.
- No taker, FAK/FOK, marketable, batch, cancel-all, or LA6 action occurred.
- Observation for review before LA6: by the TTL readback point, each order already returned final venue status `CANCELED`, so the code did not send an extra exact cancel request. Final state was flat and reconciled, but LA6 quote-manager design should explicitly account for venue-side GTD/heartbeat cancellation timing versus agent-issued exact-cancel timing.
- The live-run outcome JSON emitted by the pre-closeout build used `canceled=false` to mean no explicit cancel request was sent. The code now distinguishes `cancel_request_sent`, `exact_cancel_confirmed`, and `venue_final_canceled` so future evidence does not blur explicit cancel attempts with final venue-canceled status.
- Observation for the broad validation command: the legacy startup-recovery portion of `validate --live-readback-preflight` can still flag historical account trades as reconciliation work outside the LA5 run. The LA5 live session uses a run-scoped baseline trade set and passed reconciliation for the three-order session.

## PR #34 P1 Review Fix

Review blocker fixed after the initial closeout commit: the human-approved LA5 path no longer accepts an approval artifact that merely has final-looking fields. It now parses `verification/2026-05-05-live-alpha-la5-approval.md` into typed fields and fails closed if those fields do not match:

- CLI args: `approval_id`, `max_orders`, and `max_duration_sec`.
- Config: wallet/funder, signature type, risk caps, maker TTL, GTD delta, order type, and post-only scope.
- Authenticated readback: available pUSD units, reserved pUSD units, open-order count, heartbeat status, and funder allowance units.
- Submitted plan/session: order count, single-order notional cap, total notional cap, effective TTL, GTD delta, order type, and post-only flag.

Remaining Cursor review blockers were fixed after that binding patch:

- Completed or consumed approval artifacts now fail closed before any future LA5 human-approved submit path. The current approval artifact intentionally says `Execution Gate Status: LA5 RUN COMPLETED`, so it is no longer reusable for a second live run.
- The human-approved LA5 command now atomically reserves a per-approval cap file before entering the live maker session. The reservation uses `create_new` under gitignored `reports/live-alpha-la5-approval-caps/` and binds `approval_id`, approval artifact SHA-256, artifact path, `max_orders`, `max_duration_sec`, and reservation time.
- A second command using the same approval ID fails closed on the existing cap before any submit.
- The human-approved LA5 live-submit path now explicitly requires the compile-time `live-alpha-orders` feature, `LIVE_ORDER_PLACEMENT_ENABLED=true`, and `kill_switch_active=false` before accepting the approval for live execution.

An additional P1 review blocker was fixed for accepted-order cleanup:

- Once the venue accepts an LA5 maker order, heartbeat, authenticated readback, exact order readback, reconciliation, reconciliation-journal, and cancel-rate-slot failures now route through a best-effort exact-cancel cleanup before the original error is propagated.
- Cleanup is limited to the accepted order ID. It records `cleanup_cancel_confirmed`, `cleanup_cancel_not_confirmed`, or `cleanup_cancel_failed` evidence in the LA5 journal without masking the original blocker.
- The normal cancel decision now treats unknown or otherwise nonterminal order status as needing exact cancel; only clearly `CANCELED` or fully `FILLED` statuses skip the cancel request.

Two follow-up review blockers were fixed after that cleanup patch:

- The primary exact-cancel path no longer uses a direct `?` on the cancel RPC. It retries transient cancel RPC errors or missing exact-order confirmations inside the approved duration window, records retry metadata on success, and records a final `cancel_failed_after_retries` reconciliation failure before aborting if exact cancel cannot be confirmed.
- The live maker path now rejects an order plan before submit when the effective quote TTL cannot reach its cancel point within the approved `max_duration_sec`, so a short approved run cannot keep an accepted order open beyond the approval duration cap.

A final P2 review blocker was fixed after the duration-cap patch:

- Final LA5 reconciliation now treats a terminal venue `FILLED` order with confirmed trade evidence as flat without inserting a local canceled-order marker. Filled terminal orders without matching trade evidence still fail closed as `unexpected_fill`, and canceled terminal orders still require venue cancel confirmation.

Regression coverage added:

```text
cargo test --offline la5_ --bin polymarket-15m-arb-bot: PASS, 20 focused tests.
cargo test --offline live_maker_micro --lib: PASS, 4 focused tests.
```

## Final Verification

Final closeout gates were rerun after the live run, document/code closeout, PR #34 approval-binding fix, approval-reuse/live-submit gate hardening, post-acceptance cleanup hardening, primary cancel retry hardening, duration-cap hardening, and filled-terminal reconciliation hardening:

```text
cargo fmt --check: PASS
cargo test --offline: PASS, 342 library tests, 46 binary tests, 0 doc tests
cargo clippy --offline -- -D warnings: PASS
git diff --check: PASS
cargo run --offline -- --config config/local.toml validate --local-only: PASS, run_id=18ad174b14fcd550-bc47-0
cargo run --features live-alpha-orders -- --config config/local.toml validate --local-only --validate-secret-handles: PASS, run_id=18ad174e4b831640-bcc4-0
four-handle presence check after sourcing .env: PASS
LA5 safety/no-secret scans: PASS with expected public order IDs, public wallet/funder IDs, secret handle names, feature-gated order/cancel code, approval-cap code, and Live Alpha/Live Beta documentation hits only. Count-only scan totals: order/cancel/live-order `1389`, secret/handle `1006`, gate/reconciliation `1421`.
```

## Live Run Fields

- Orders submitted: `3`.
- Orders accepted: `3`.
- Orders rejected: `0`.
- Orders filled: `0`.
- Orders canceled/final venue canceled: `3`.
- Open orders after session: `0`.
- Reserved pUSD before/after: `0` / `0`.
- Available pUSD units before/after: `6314318` / `6314318`.
- Browser portfolio/cash/available before/after: `$6.31` / `$6.31`.
- Positions before/after: none / none.
- P&L: `0`.
- Fees paid: `0`.
- Risk halts/mismatches: `0`.
- Paper/shadow/live comparison: live outcome matched the approved maker-only plan shape; no fill quality comparison exists because fills were `0`.
- LA6 go/no-go: `NO-GO`; review this LA5 evidence first, then require a separate LA6 approval.

## Scope Confirmation

This branch remains LA5 only. No taker, FAK/FOK strategy, batch, cancel-all, or LA6 behavior is authorized.
