# Live Alpha LA6 Quote Manager Verification

## Status

LA6 IMPLEMENTED FOR DRY-RUN AND FAIL-CLOSED LIVE GATING. No live submit or live cancel was authorized or run because `verification/2026-05-06-live-alpha-la6-approval.md` is incomplete and intentionally blocked.

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
- Dry-run emits a non-network quote lifecycle plan with place, leave, cancel, replace, expire, skip, and halt decisions.
- Human-approved path validates compile-time/runtime gates, config mode, approval artifact, geoblock, secret-handle presence, account config, and authenticated readback. It defines the atomic LA6 approval-cap reservation step for future submit/cancel dispatch, but live dispatch is blocked in this PR because the LA6 approval artifact is incomplete.
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
- `halt_quote`: reconciliation mismatch failed closed.

## Approval And Live Evidence

- Approval ID: `LA6-2026-05-06-001` is reserved as a placeholder only.
- Approval artifact: `verification/2026-05-06-live-alpha-la6-approval.md`.
- Approval status: `BLOCKED`.
- Markets touched: `NOT RUN`.
- Quotes placed: `NOT RUN`.
- Quotes left alone: `NOT RUN`.
- Quotes canceled: `NOT RUN`.
- Quotes replaced: `NOT RUN`.
- Fills: `NOT RUN`.
- Open orders after run: `NOT RUN`.
- Reserved pUSD after run: `NOT RUN`.
- Cancel rate: `NOT RUN`.
- Replacement rate: `NOT RUN`.
- Anti-churn triggers: dry-run tested; live `NOT RUN`.
- Risk halts: dry-run tested; live `NOT RUN`.
- Mismatches: dry-run tested; live `NOT RUN`.
- P&L: `NOT RUN`.
- Paper/shadow/live divergence: `NOT RUN`.

No secret values were printed, logged, written, or committed.

## Verification Commands

Initial focused check:

```text
cargo test --offline live_quote_manager
PASS: 27 passed; 0 failed
```

Dry-run command:

```text
cargo run --features live-alpha-orders -- --config config/default.toml live-alpha-quote-manager --dry-run
PASS: non-live dry-run emitted place/leave/cancel/replace/expire/skip/halt plan
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
PASS: library 4 passed; main 1 passed; 0 failed

cargo test --offline live_reconciliation
PASS: 24 passed; 0 failed

cargo fmt --check
PASS

cargo test --offline
PASS: library 371 passed; main 50 passed; doc 0 passed; 0 failed

cargo clippy --offline -- -D warnings
PASS

git diff --check
PASS
```

Safety/no-secret scans:

```text
rg -n -i "(cancel.?all|batch|FOK|FAK|marketable|taker)" src config runbooks verification
PASS with expected historical and guardrail hits only. Count-only rerun after cleanup: 485.

rg -n -i "(wallet|private.*key|secret|passphrase|mnemonic|seed|0x[0-9a-fA-F]{64})" src config runbooks verification
PASS with expected public addresses/order IDs/feed IDs, secret-handle names, tests, and guardrail text only. Count-only rerun: 776.

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
- Live order placement remains controlled by `--features live-alpha-orders` and `LIVE_ORDER_PLACEMENT_ENABLED`.
- The human-approved LA6 path is blocked by the incomplete LA6 approval artifact.

## LA7 Decision

LA7 go/no-go: `NO-GO`. LA6 needs review, final verification, PR handoff, and a separate human decision before any LA7 work.
