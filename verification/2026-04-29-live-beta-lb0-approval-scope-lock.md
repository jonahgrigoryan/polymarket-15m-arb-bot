# 2026-04-29 LB0 Approval And Scope Lock

Date: 2026-04-29
Branch: `live-beta/lb0-scope-lock`
Run by: operator review request

## Documents Approved

- `LIVE_BETA_PRD.md`
- `LIVE_BETA_IMPLEMENTATION_PLAN.md`

## Finalized LB0 Scope

- Release-gate scope stays planning-only.
- First beta remains an order-lifecycle probe.
- Assets: BTC, ETH, SOL only.
- Dedicated beta wallet required.
- One open order maximum.
- Post-only GTD maker-only canary only.
- Human approval required for live-canary action.
- Funding policy:
  - tiny funding cap retained (initial <= 25 pUSD),
  - first-order cap <= 1 pUSD.
- Strategy-selected live trades are disallowed at LB0 and require separate strategy robustness approval.

## Explicit Non-Authorization

- LB0 does **not** authorize live order placement.
- LB0 does **not** authorize wallet/key/API-key onboarding, signing code, authenticated CLOB clients, order posting, cancel logic, or autonomous trading.
- `LIVE_ORDER_PLACEMENT_ENABLED=false` remains the default.

## Legal/Access

- Legal/access owner: operator.
- Legal/access status: pending formal confirmation (required before LB4+ and before any live runtime checks that depend on access/compliance).

## Required Next Phase

- LB1: Live-mode kill gates only.
- Precondition for LB1 entry: LB0 verification note filed and `STATUS.md` points to LB1 as the next action.

## Reviewer / Approver

- Approver identity: operator (manual confirmation).
- Approval type: scope-lock only; no implementation authorization.
