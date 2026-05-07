# Live Alpha LA6 Quote Manager Verification

## Status

LA6 IMPLEMENTED, LIVE-RUN VERIFIED, AND HELD FOR HUMAN REVIEW. On 2026-05-07 the final approved run `LA6-2026-05-07-005` placed exactly one maker-only post-only GTD quote, canceled it by exact order ID after the quote manager marked the quote stale, reconciled the cancel confirmation, and ended flat with `open_order_count=0`, `reserved_pusd_units=0`, and no fills.

LA6 remains the only active scope in this branch. LA7 is out of scope.

## Scope

- Branch: `live-alpha/la6-quote-manager`.
- Base: fresh `main` after PR #34 / LA5 merge; branch created from `04fec98`.
- Allowed: deterministic quote manager, exact-order-ID cancel/replace decisions, anti-churn rules, no-trade window policy, LA6 dry-run CLI, LA6 approval parsing, LA6 journal/metrics coverage, and verification artifacts.
- Not allowed: taker strategy, FAK/FOK strategy orders, marketable strategy orders, batch orders, cancel-all as normal runtime behavior, production sizing, multi-wallet deployment, asset expansion beyond BTC/ETH/SOL, cap reset, or committed secrets.

## Starting Command Evidence

```text
git branch --show-current
main

git status --short --branch
## main...origin/main
?? "LA6 Kickoff Prompt.md"

git switch main
Already on 'main'
Your branch is up to date with 'origin/main'.

git pull --ff-only origin main
From https://github.com/jonahgrigoryan/polymarket-15m-arb-bot
 * branch            main       -> FETCH_HEAD
Already up to date.

git switch -c live-alpha/la6-quote-manager
Switched to a new branch 'live-alpha/la6-quote-manager'

git status --short --branch
## live-alpha/la6-quote-manager
```

Note: the untracked kickoff prompt observed on `main` was not present in the final LA6 branch status output and was not edited.

## Official Polymarket Docs Checked

- https://docs.polymarket.com/developers/CLOB/orders/cancel-orders
  - Conclusion: single-order cancel, multiple-order cancel, cancel-all, and cancel-by-market are separate surfaces. LA6 uses exact-order-ID decisions only and treats ambiguous cancel evidence as halt/reconcile.
- https://docs.polymarket.com/api-reference/trade/post-a-new-order
  - Conclusion: order posting is a trading endpoint under the CLOB API and remains behind compile-time feature, runtime gates, explicit CLI intent, approval artifact, and authenticated readback.
- https://docs.polymarket.com/api-reference/trade/cancel-all-orders
  - Conclusion: cancel-all is an explicit broader endpoint and remains disallowed as normal LA6 runtime behavior.
- https://docs.polymarket.com/api-reference/authentication
  - Conclusion: user order management requires authenticated CLOB headers; public book data alone is not final reconciliation evidence.
- https://docs.polymarket.com/api-reference/trade/get-single-order-by-id
  - Conclusion: exact order readback exists and must be used for order-state reconciliation.
- https://docs.polymarket.com/api-reference/trade/get-trades
  - Conclusion: trade readback is required when fill/partial-fill evidence appears; ambiguous or nonterminal trade status must fail closed or reconcile explicitly.
- https://docs.polymarket.com/market-data/websocket/user-channel
  - Conclusion: user WebSocket events may be useful hints, but authenticated REST readback remains the final reconciliation source for LA6.

## Implementation Summary

- Added `src/live_quote_manager.rs` with typed deterministic inputs, quote state, policy, decision outputs, approval artifact parsing, and regression coverage.
- Wired `live-alpha-quote-manager` CLI with `--dry-run` and `--human-approved` modes.
- Dry-run emits a non-network quote lifecycle plan with place, leave, cancel, replace, expire, skip, no-trade exact-cancel, and halt decisions.
- Existing quotes entering the no-trade window default to exact-order-ID cancel or halt; leaving open is allowed only when the explicit policy flag and final approval artifact allow it.
- Human-approved path validates compile-time/runtime gates, config mode, approval artifact, geoblock, secret-handle presence, account config, authenticated readback, approval/readback value binding, and atomic approval-cap reservation before dispatching one live quote lifecycle.
- The final live path records quote started/planned/placed/cancel-requested/cancel-confirmed/reconciliation/stopped events, requires final authenticated readback to be flat, and reports the structured live outcome.
- Added LA6 journal events and metrics names for quote started/stopped, planned, placed, left, cancel requested/confirmed, replace requested/submitted/accepted/rejected, expired, halted, reconciliation result, and anti-churn triggers.

## Config Snapshot

Dry-run command used `config/default.toml` only:

- `live_alpha.enabled = false`
- `live_alpha.mode = "disabled"`
- `live_alpha.quote_manager.enabled = false`
- `live_alpha.maker.post_only = true`
- `live_alpha.maker.order_type = "GTD"`
- `live_beta.kill_switch_active = true`
- committed defaults remain inert

`config/local.toml` exists but was not printed, copied, or committed.

## Dry-Run Evidence

```text
cargo run --features live-alpha-orders -- --config config/default.toml live-alpha-quote-manager --dry-run
live_alpha_quote_manager_status=ok
live_alpha_quote_manager_not_submitted=true
live_alpha_quote_manager_not_canceled=true
live_alpha_quote_manager_max_orders=1
live_alpha_quote_manager_max_replacements=1
live_alpha_quote_manager_max_duration_sec=300
live_alpha_quote_manager_config_mode=disabled
```

Dry-run plan decisions:

- `place_quote`: planned one sample maker quote.
- `leave_quote`: left a healthy quote alone.
- `cancel_quote`: exact-order-ID cancel decision for stale book evidence.
- `replace_quote`: exact-order-ID replacement decision for fair-value movement.
- `expire_quote`: TTL expiry decision.
- `skip_market`: no-trade window blocked a new quote.
- `cancel_quote`: no-trade window default exact-order-ID cancel for an existing quote.
- `halt_quote`: reconciliation mismatch failed closed.

## Approval And Live Evidence

- Approval ID: `LA6-2026-05-07-005`.
- Approval artifact: `verification/2026-05-06-live-alpha-la6-approval.md`.
- Approval status: `APPROVED FOR THIS RUN ONLY; CONSUMED`.
- Approval artifact used at runtime: `/tmp/p15m-la6-approval-005.md`, SHA-256 `sha256:b6e4171ddc5c56a962aa8a3c37689e7622c663ba528a08d5d37f0dec22f1f52f`.
- Approval cap path: `/tmp/live-alpha-la6-approval-caps/LA6-2026-05-07-005.json`, SHA-256 `sha256:ca6c355f74c9b2da77087be91d41fcb368005497865bc16e9f9413bb0a14972b`.
- Command: `cargo run --features live-alpha-orders -- --config /tmp/p15m-la6-quote-manager.toml live-alpha-quote-manager --human-approved --approval-id LA6-2026-05-07-005 --approval-artifact /tmp/p15m-la6-approval-005.md --max-orders 1 --max-replacements 1 --max-duration-sec 300`.
- Run ID: `18ad38f9204f44e0-b76d-0`.
- Markets touched: `btc-updown-15m-1778139900`.
- Quote placed: `1`, order ID `0xea764a6d1846cef1602c37945c3734a35f99bb671ad38e9bc89236118a3e0ca9`, price `0.2`, size `5.0`, notional `1.0`.
- Quotes left alone: `0`.
- Quotes canceled: `1`; decision `cancel_quote`, reason `book_stale`, exact-order-ID cancel request sent and confirmed on attempt `1`.
- Quotes replaced: `0`; replacement cap was present but not exercised because the quote-manager stale-cancel gate fired first.
- Fills: `0`; trade IDs empty.
- Open orders after run: `0`.
- Reserved pUSD after run: `0`.
- Cancel rate: `1` cancel in the approved session; within configured `max_cancel_rate_per_min=1`.
- Replacement rate: `0`; within configured `max_replacements=1`.
- Anti-churn triggers: dry-run/unit tests cover cooldown/rate/min-lifetime gates; live run canceled due stale book before replacement.
- Risk halts: final live run none. Same-session non-final run `LA6-2026-05-07-003` correctly risk-rejected as `market_too_close_to_close`.
- Mismatches: none in final quote reconciliation. The post-run `validate --live-readback-preflight` still reports startup recovery `unexpected_fill` from pre-existing account trade history outside this run; this remains a future durable-journal seeding/recovery concern, not an LA6 quote mismatch.
- P&L: `0.000000` realized for LA6 final run; no fills.
- Paper/shadow/live divergence: no live fill occurred; final readback flat matched local quote lifecycle.

No secret values were printed, logged, written, or committed.

## Verification Commands

Initial focused check:

```text
cargo test --offline live_quote_manager
PASS: 29 passed; 0 failed after 2026-05-07 remediation
```

Dry-run command:

```text
cargo run --features live-alpha-orders -- --config config/default.toml live-alpha-quote-manager --dry-run
PASS: non-live dry-run emitted place/leave/cancel/replace/expire/skip/no-trade-cancel/halt plan, run_id=18ad371718b927e8-890b-0
```

2026-05-07 remediation checks:

```text
cargo test --offline live_quote_manager
PASS: 29 passed; 0 failed

cargo test --offline la6_approval
PASS: main 2 passed; 0 failed

cargo run --features live-alpha-orders -- --config config/local.toml validate --live-readback-preflight
EXPECTED BLOCK: missing L2 handles; no live submit/cancel attempted

cargo run --features live-alpha-orders -- --config /tmp/p15m-la6-quote-manager.toml validate --live-readback-preflight
PASS: authenticated readback passed, run_id=18ad39088a8fbf60-b82d-0, open_order_count=0, reserved_pusd_units=0, available_pusd_units=6314318

cargo run --features live-alpha-orders -- --config /tmp/p15m-la6-quote-manager.toml live-alpha-quote-manager --human-approved --approval-id LA6-2026-05-07-005 --approval-artifact /tmp/p15m-la6-approval-005.md --max-orders 1 --max-replacements 1 --max-duration-sec 300
PASS: one quote placed, exact-order-ID cancel requested and confirmed, final reconciliation passed, final open_order_count=0, reserved_pusd_units=0
```

Required final verification:

```text
cargo test --offline cancel_replace
PASS: 5 passed; 0 failed

cargo test --offline anti_churn
PASS: 6 passed; 0 failed

cargo test --offline post_only_safety
PASS: 1 passed; 0 failed

cargo test --offline no_trade_window
PASS: library 5 passed; main 1 passed; 0 failed

cargo test --offline live_reconciliation
PASS: 24 passed; 0 failed

cargo fmt --check
PASS

cargo test --offline
PASS: library 373 passed; main 52 passed; doc 0 passed; 0 failed

cargo clippy --offline -- -D warnings
PASS

git diff --check
PASS
```

Safety/no-secret scans:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading|FOK|FAK|GTD|GTC|post[_ -]?only)" src Cargo.toml config runbooks *.md
PASS with expected historical and guardrail hits only. Count-only rerun after cleanup: 1123 for the plan order/cancel scan.

rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
PASS with expected public addresses/order IDs/feed IDs, secret-handle names, tests, and guardrail text only. Count-only rerun: 1301.

rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
PASS with expected live-gate/config/journal hits only. Count-only rerun: 1576.

test ! -e .env || git check-ignore .env
PASS: .env is ignored.

test ! -e config/local.toml || git check-ignore config/local.toml
PASS: config/local.toml is ignored.
```

Expected scan-hit classes: older LA3 FAK/taker approval code, historical docs and runbooks that explicitly forbid cancel-all/batch/taker paths, existing signal-engine paper taker modeling, existing secret-handle names, public wallet/funder/order/condition/feed IDs in prior evidence, and the new LA6 docs/tests that state cancel-all is disallowed. No new cancel-all runtime path, batch submit path, taker strategy path, FOK/FAK strategy path, secret value, raw L2 credential, private key, mnemonic, seed phrase, or API-key value was added.

## Safety Boundary

- No taker path was added.
- No FAK/FOK strategy path was added.
- No marketable strategy order path was added.
- No batch order path was added.
- No cancel-all runtime path was added.
- No production sizing, multi-wallet deployment, or asset expansion was added.
- Live order placement remains controlled by `--features live-alpha-orders`, `LIVE_ORDER_PLACEMENT_ENABLED`, runtime gates, approval artifact binding, and one-run approval-cap reservation.
- The human-approved LA6 path consumed approval `LA6-2026-05-07-005`; any further live run requires a new approval artifact and cap reservation.

## LA7 Decision

LA7 go/no-go: `NO-GO`. LA6 has final run evidence, but LA7 remains forbidden until a separate human review and approval.
