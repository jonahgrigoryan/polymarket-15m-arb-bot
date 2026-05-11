# Live Alpha LA8 Scale Decision Report

Date: 2026-05-09
Branch: `live-alpha/la8-scale-decision`
Scope: LA8 scale decision report only.

## Starting State

LA8 was started only after PR #40 was confirmed merged and local `main` was refreshed.

- `gh pr view 40 --json ...`: PR #40 `MERGED`, merged at `2026-05-09T17:15:13Z`, merge commit `fd355223d6fc8938b82b219de8f8d27127160227`.
- `git fetch origin main`: completed.
- `git rev-parse HEAD`: `966b2c0f7add4dff37aecc33c2a2b50fc7f2110d`.
- `git rev-parse origin/main`: `966b2c0f7add4dff37aecc33c2a2b50fc7f2110d`.
- `git merge-base --is-ancestor fd355223d6fc8938b82b219de8f8d27127160227 HEAD`: passed.
- LA8 branch was created from that fresh local main.

## Scope Boundary

Allowed:

- Add a narrow `live-alpha-scale-report` reporting path.
- Aggregate LA0-LA7 evidence.
- Record P&L, fees, slippage, maker/taker split, paper/live divergence, adverse selection, mismatches, halts, bugs, and reconciliation history.
- Update `STATUS.md` with the LA8 decision and next hold point.

Not allowed and not done:

- No live orders.
- No `--human-approved` command.
- No LA7 cap reset.
- No increase in size, order count, assets, taker usage, duration, runtime behavior, production behavior, or wallet scope.
- No broader live-trading implementation.

## Sources Read

- `STATUS.md`
- `LIVE_ALPHA_PRD.md`
- `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`
- `runbooks/live-alpha-runbook.md`
- `verification/2026-05-03-live-alpha-la0-approval-scope.md`
- `verification/2026-05-03-live-alpha-la1-journal-reconciliation.md`
- `verification/2026-05-04-live-alpha-la2-heartbeat-crash-safety.md`
- `verification/2026-05-04-live-alpha-la3-controlled-fill-canary.md`
- `verification/2026-05-04-live-alpha-la4-shadow-live-executor.md`
- `verification/2026-05-05-live-alpha-la5-maker-micro-autonomy.md`
- `verification/2026-05-06-live-alpha-la6-quote-manager.md`
- `verification/2026-05-08-live-alpha-la7-wallet-baseline.md`
- `verification/2026-05-08-live-alpha-la7-taker-gate.md`
- `verification/2026-05-09-live-alpha-la7-live-taker-canary-path.md`

## LA8 Report Command

Implemented a narrow offline command:

```text
cargo run --offline -- live-alpha-scale-report --from 2026-05-03 --to 2026-05-09
```

This command reads local report artifacts only. It does not use credentials, submit orders, cancel orders, reset caps, or open any live path.

Observed command result:

```text
live_alpha_scale_report_status=ok
live_alpha_scale_report_from=2026-05-03
live_alpha_scale_report_to=2026-05-09
live_alpha_scale_report_decision=NO-GO: lifecycle unsafe
live_alpha_scale_report_evidence_count=11
live_alpha_scale_report_missing_evidence_count=0
```

Machine-readable summary from the command:

- Paper evidence: `6` paper orders, `6` paper fills, all taker fills, filled notional `3.468000`, fees `0.226100`, total pre-settlement P&L `-0.472100`, post-settlement P&L `-3.694100`.
- Paper P&L by asset: BTC `-0.181216`, ETH `-0.149084`, SOL `-0.141800`.
- Live machine-readable evidence: `5` orders, `2` matched taker fills, `3` maker orders, `3` maker final canceled statuses, `0` maker fills.
- LA7 machine-readable post-submit mismatches: `3`.
- LA7/local report halt or blocked statuses: `2`.
- Shadow taker aggregate from local reports: `1204` evaluations, `0` would-take, `0` live-allowed.
- LA7 one-order cap: consumed.

The command intentionally treats paper/live comparison as `not_comparable` because M9 paper evidence and Live Alpha live canaries/quote probes were different sessions with different execution authority and market conditions.

## Evidence Aggregation

| Phase | Evidence | Scale relevance |
| --- | --- | --- |
| LA0 | Scope and approval documentation only. No source, config, live order, cancel, cancel-all, secret, cap reset, or strategy route added. | Establishes that Live Alpha is not production rollout. |
| LA1 | Inert live-alpha config, gate evaluator, durable journal, balance/position reducers, reconciliation engine, redaction, and mismatch fixtures. | Good safety foundation, but no profitability evidence. |
| LA2 | Heartbeat state, user-event parser, startup recovery, halt events, and read-only preflight behavior. No live order placement. | Good safety foundation, but no scale evidence. |
| LA3 | Exactly one BTC `BUY` `FAK` controlled fill canary submitted and reconciled. Venue status `MATCHED`, trade ID observed, open orders `0`, reserved pUSD `0`, settlement complete. | Proves a tiny taker-style fill lifecycle can reconcile. Also exposed a fee-estimation bug. |
| LA4 | `paper --shadow-live-alpha` emitted shadow evidence without live actions. Latest evidence had `0` paper orders, `0` fills, `0` shadow decisions, `0` paper/live divergence. | No profitability or fill-quality evidence. |
| LA5 | Exactly three sequential maker-only post-only GTD micro orders. All ended final `CANCELED`, no fills, fees `0`, P&L `0`, open orders `0`, reserved pUSD `0`. | Maker order lifecycle exercised, but maker fill quality and maker expectancy remain unmeasured. |
| LA6 | Exactly one maker-only post-only GTD quote placed, exact-order-ID cancel sent and confirmed, replacements `0`, fills `0`, open orders `0`, reserved pUSD `0`, P&L `0`. | Quote manager lifecycle works at tiny scale, but no maker fill/P&L sample. |
| LA7 | Shadow taker stayed blocked by default; approved dry-run passed; one separately approved live taker canary matched; immediate post-submit report remained `submitted_post_check_blocked`; later flat baseline passed; cap remains consumed. | Taker must stay disabled. The accepted closeout does not authorize scale or another taker canary. |

## P&L And Fees

Live Alpha known and bounded results:

- LA3 controlled fill canary:
  - Filled shares: `5.12`.
  - Total trade cost from public activity: `2.652160 pUSD`.
  - Implied fee/extra cost: `0.092160 pUSD`.
  - Settlement value: `5.120000 pUSD`.
  - Realized P&L versus final pre-submit balance: `+2.467840 pUSD`.
  - Important bug: approval artifact max fee estimate was `0.06 pUSD`, below the official share-based fee estimate. The code was fixed to use shares traded as `C`.
- LA5 maker micro:
  - Maker orders: `3`.
  - Maker fills: `0`.
  - Fees: `0`.
  - Realized/unrealized P&L: `0`.
- LA6 quote manager:
  - Maker quote placed: `1`.
  - Exact-order-ID cancel confirmed: `1`.
  - Replacements: `0`.
  - Fills: `0`.
  - P&L: `0`.
- LA7 taker canary:
  - Submitted order count: `1`.
  - Venue status: `MATCHED`.
  - Making amount: `1.35`.
  - Taking amount: `5`.
  - Final pre-submit estimate from the live report: taker fee `0.067340`, slippage `0 bps`, adverse-selection buffer `25 bps`, estimated EV after costs `2246.819509 bps`.
  - Actual realized P&L and actual fee are not fully machine-attributable from committed report artifacts. Later account baseline resolved flat, but the historical live report remains `submitted_post_check_blocked`.

Paper/replay benchmark:

- M9 RTDS current-window paper run produced `6` taker paper fills, filled notional `3.468000`, fees `0.226100`, total pre-settlement P&L `-0.472100`.
- Post-settlement read-only Gamma reconciliation showed all held Up positions lost; final post-settlement P&L was `-3.694100`.
- This is negative expectancy evidence for the sampled paper strategy, not live profitability evidence.

## Slippage, Maker/Taker Split, And Adverse Selection

- LA3 slippage stayed within the approved worst-price envelope, but the fee bound was wrong until fixed.
- LA5 and LA6 had no fills, so slippage and adverse selection are not measurable for maker execution.
- LA7 final pre-submit report recorded `0 bps` slippage, `25 bps` adverse-selection buffer, gross edge `2561.499509 bps`, and estimated EV after costs `2246.819509 bps`, but immediate post-submit checks still failed closed.
- Paper M9 evidence had all `6` fills as taker fills and settled negatively. This is adverse-selection/settlement-outcome negative evidence for that paper sample.
- There is no live maker fill sample. Maker-only adverse selection remains unknown.

## Paper/Live Divergence

- LA4 shadow executor recorded `0` paper/live intent divergence in the latest successful run.
- LA7 shadow taker aggregate from local reports recorded `1204` evaluations with `0` would-take and `0` live-allowed under default/local gates.
- The paper/live P&L comparison is not directly comparable because paper sessions and live canaries were not the same market windows, authority, or execution mode.
- The absence of comparable maker live fills is a scale blocker.

## Mismatches, Halts, Bugs, And Reconciliation History

- LA1 mismatch fixtures correctly halt for unknown open orders, missing venue orders, unexpected fills, partial-fill mismatches, cancel mismatch, balance/reserved drift, position mismatch, failed trades, and trade/order mismatches.
- LA3 reconciled its controlled fill and settlement, but found the fee-estimation bug. The fee bug was fixed with a regression test.
- LA5 final run had no reconciliation mismatches and ended flat. Earlier validation surfaces could still report historical account trades outside the LA5 run.
- LA6 final run had no quote-run reconciliation mismatches and ended flat. A non-final same-session run correctly risk-rejected `market_too_close_to_close`. Post-run startup recovery still reported historical `unexpected_fill` outside the run, preserving a future durable-journal seeding concern.
- LA7 historical live canary report remains `submitted_post_check_blocked` with `post_submit_readback_not_passed`, `post_submit_reconciliation_not_passed`, and mismatches `unexpected_fill`, `nonterminal_venue_trade_status`, and `baseline:current_readback_not_passed`.
- LA7 follow-up fixed the false `unexpected_fill` classification for future reports, added bounded post-submit polling, preserved cap consumption on ambiguity, preserved report writing on post-submit evidence errors, added final approval-expiry recheck, required parsed approval ID binding, and changed future submit shape to BUY FAK/no-resting-remainder. No second live action was run.

## Recommendation Policy Fix

Follow-up review found that the initial recommendation function always returned `NO-GO`, even if a future evidence set had no blockers. That was a reporting-policy bug, not proof that the current evidence can scale.

The policy now supports all three LA8 decision families:

- `NO-GO:*` when lifecycle evidence is unsafe or expectancy is negative.
- `HOLD:*` when evidence is not unsafe but is still too thin, such as missing maker live fills.
- `GO: propose next PRD for broader scaling` only for clean positive evidence, and only as a next-planning recommendation.

Regression tests now cover the current no-go case, the hold-without-maker-fill case, and a clean synthetic GO case. The real local evidence still returns `NO-GO: lifecycle unsafe`.

## Decision Rationale

NO-GO: lifecycle unsafe for scaling because:

- The strongest paper/replay profitability sample is negative after settlement: `-3.694100 pUSD`.
- Live maker execution has no fill-quality, adverse-selection, or P&L sample.
- LA7 taker canary matched, but immediate post-submit readback/reconciliation failed closed and the one-order cap remains consumed.
- Paper/live P&L divergence is not comparable or explained by matched market-window evidence.
- The remaining next step would be new planning after review, not code or live execution from LA8.

## Verification Commands

Already run before this note was written:

```text
gh pr view 40 --json number,state,mergedAt,mergeCommit,headRefName,baseRefName,url,title
git fetch origin main
git rev-parse HEAD
git rev-parse origin/main
git merge-base --is-ancestor fd355223d6fc8938b82b219de8f8d27127160227 HEAD
git switch -c live-alpha/la8-scale-decision
cargo fmt --check
cargo test --offline live_alpha_report
cargo run --offline -- live-alpha-scale-report --from 2026-05-03 --to 2026-05-09
```

Final verification pass:

```text
cargo fmt --check
cargo test --offline live_alpha_report
cargo run --offline -- live-alpha-scale-report --from 2026-05-03 --to 2026-05-09
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

Result: passed.

Follow-up focused checks after the recommendation policy fix:

```text
cargo test --offline live_alpha_report
cargo run --offline -- live-alpha-scale-report --from 2026-05-03 --to 2026-05-09
```

Result: passed. The command still returned `live_alpha_scale_report_decision=NO-GO: lifecycle unsafe` for current evidence.

Additional LA8 scope scans were run against source, config, runbooks, docs, and verification text:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading|FOK|FAK|GTD|GTC|post[_ -]?only)" src Cargo.toml config runbooks *.md
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
```

The broad scans produced expected historical and documentation hits only. Targeted review of new LA8 hits found reporting path strings, documentation/status evidence, and no new live order/cancel implementation, no secret value material, no gate weakening, no cap reset, and no production-behavior expansion.

## Next Hold Point

Stop at LA8. Do not run another live canary, do not reset the LA7 cap, do not increase size/rate/assets/taker usage/duration, and do not start production behavior. Any future path requires a new PRD or implementation plan plus new approval scope.

Decision: NO-GO: lifecycle unsafe
