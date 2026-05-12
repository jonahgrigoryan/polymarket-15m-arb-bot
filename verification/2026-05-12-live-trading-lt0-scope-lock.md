# Live Trading LT0 Scope Lock

Date: 2026-05-12
Branch: `live-trading/lt0-scope-lock`
Base: `main` / `origin/main` at `f57979d` (`Merge pull request #42 from jonahgrigoryan/live-trading/prd`)
Scope: LT0 approval and scope lock only

## Objective

Lock the final live-trading PRD and implementation plan as planning artifacts for phased work.

LT0 does not authorize live order placement, live canceling, cancel-all, cap reset, taker expansion, production sizing, multi-wallet deployment, asset expansion, secret material changes, or live-trading source/config implementation.

## Source Documents

- `LIVE_TRADING_PRD.md`
- `LIVE_TRADING_IMPLEMENTATION_PLAN.md`
- `STATUS.md`
- `AGENTS.md`
- `verification/2026-05-09-live-alpha-la8-scale-decision.md`

## Review Record

- Product/operator review: the user requested execution of the LT0 task on 2026-05-12 in the current Codex thread.
- Interpretation: approval to perform LT0 scope lock and treat `LIVE_TRADING_PRD.md` plus `LIVE_TRADING_IMPLEMENTATION_PLAN.md` as the current planning basis for phased final-live work.
- Boundary: this approval is planning/scope-lock approval only. It does not approve LT1 execution, approved-host commands, live-capable code, live orders, live cancels, taker execution, cap reset, funding changes, or production rollout.

## Approved Planning Scope

The approved planning scope is:

- maker-first final-live track,
- BTC, ETH, and SOL only,
- explicit path toward the original objective of `1,000+` matched fills/day,
- staged LT0-LT14 phase sequence,
- fresh final-live evidence separated from historical LA7/LA8 evidence,
- dedicated wallet/funder pair required before orders,
- deployment host and jurisdiction approval required before approved-host/live phases,
- initial funding cap required before orders,
- first maker order cap required before orders,
- taker disabled by default,
- cancel-all disabled unless separately approved as an emergency path,
- no multi-wallet deployment without separate approval,
- no production rollout from LT0.

## Non-Authorization

LT0 explicitly does not authorize:

- source code changes,
- config changes that introduce live credentials, wallet values, API keys, signing fields, or order endpoints,
- authenticated clients,
- signing paths,
- wallet paths,
- order paths,
- cancel paths,
- trading-capable runtime paths,
- live orders,
- live cancels,
- cancel-all,
- taker expansion,
- LA7 cap reuse or reset,
- production sizing,
- multi-wallet deployment,
- any geoblock bypass.

## Required Statuses

| Item | LT0 status |
| --- | --- |
| Legal/access approval | Not yet approved for live or approved-host operation; required before later approved-host/live phases. |
| Deployment host | Not selected or approved. |
| Deployment jurisdiction | Not selected or approved. |
| Wallet type | Not selected. |
| Signature type | Not selected. |
| Funder/deposit/proxy address | Not selected. |
| Secret backend or signing service | Not selected. |
| Initial funding cap | Not selected. |
| Initial max order notional | Not selected. |
| Initial max live loss | Not selected. |
| Initial runtime duration | Not selected. |
| First daily fill target | Not selected. |
| Taker status | Disabled by default. |
| Cancel-all status | Disabled; emergency-only path would require separate approval. |
| Multi-wallet status | Not authorized. |
| Production rollout status | Not authorized. |

## Open Decisions Carried Forward

- Final deployment host and jurisdiction.
- Final wallet type, signature type, and funder/deposit/proxy address.
- Final secret backend or signing service.
- Initial funding cap.
- Initial max order notional.
- Initial max live loss.
- Initial runtime duration and first daily fill target.
- First LT6 evidence asset: BTC only, or BTC/ETH/SOL with one active market at a time.
- Exact LT11 and LT12 fill thresholds before attempting LT13.
- Maximum allowed order-to-fill and cancel-to-fill ratios for `1,000+` fills/day.
- Whether LT10 remains in this plan or is deferred to a later plan amendment after LT9.
- Whether cancel-all remains entirely out of scope or gets a separately approved emergency-only proof.
- Whether `GO` requires strictly non-negative settlement P&L or permits a small bounded risk-approved loss with strong lifecycle and forward-expectancy evidence.

## Required Next Phase

The next phase is LT1: Read-Only Final-Live Supervision.

LT1 may not start until the human/operator explicitly approves starting LT1. LT1 remains read-only and must not submit orders, submit cancels, sign an order intended for submission, or mutate cap sentinels.

## Verification

Commands/checks run for LT0:

```text
git status --short --branch
git diff --check
rg -n "Status: Draft for approval|LT0|does not approve live trading|NO-GO: lifecycle unsafe|taker disabled" LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
```

Additional scope checks:

```text
git diff --name-status
git diff --cached --name-status
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|cancel-all|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md verification/2026-05-12-live-trading-lt0-scope-lock.md
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|funder|allowance|POLY_API_KEY|POLY_SIGNATURE|POLY_PASSPHRASE)" LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md verification/2026-05-12-live-trading-lt0-scope-lock.md
```

Result:

- PASS: LT0 changed docs/status/verification only.
- PASS: no source, config, Cargo, signing, order, cancel, wallet, or secret-value implementation was added.
- PASS: live trading remains unauthorized.
- PASS: `STATUS.md` points to LT1 as the next phase while preserving the mandatory hold.

## Exit Gate

LT0 is complete when this note and `STATUS.md` are committed on `live-trading/lt0-scope-lock`.

Exit decision:

```text
LT0 COMPLETE - HOLD BEFORE LT1
```
