# 2026-05-03 Live Alpha LA0 Approval Scope

## Scope

LA0 establishes Live Alpha as the post-LB7 release track. This is a documentation, scope, and approval gate only.

Files in scope:

- `LIVE_ALPHA_PRD.md`
- `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`
- `STATUS.md`
- `verification/2026-05-03-live-alpha-la0-approval-scope.md`

No source code, config, runbook runtime behavior, live order path, live cancel path, cancel-all path, strategy-to-live routing, secret handling, wallet/private-key handling, API-key handling, or live execution expansion is in scope for LA0.

## Base State

- Branch: `live-alpha/la0-approval-scope`.
- Base: updated `main` at PR #27 merge commit `26144dc`.
- LB0-LB7 are complete.
- LB7 handoff evidence: `verification/2026-05-03-live-beta-lb7-runbook-handoff.md`.
- LB6 execution evidence: `verification/2026-05-03-live-beta-lb6-one-order-canary-execution.md`.

## LB6/LB7 Handoff Facts Preserved

- LB6 submitted exactly one reviewed live canary order.
- The exact canary order was canceled.
- No fill occurred.
- Post-cancel open orders were `0`.
- Post-cancel reserved pUSD was `0`.
- The local LB6 one-order cap sentinel is consumed.
- LB6 proved one tiny post-only submit/cancel lifecycle only.
- LB6 did not prove fill accounting, fee accounting, position accounting, partial-fill behavior, settlement, autonomous strategy routing, or strategy profitability.
- LB7 was runbook, observability, rollback hardening, incident workflow, and STATUS handoff only.

## LA0 Approval Boundary

LA0 authorizes only the Live Alpha scope documents and this handoff update. It does not authorize:

- live order placement,
- live canceling,
- cancel-all,
- strategy-selected live trading,
- starting LA1, LA2, LA3, or later work,
- resetting or bypassing the consumed LB6 one-order cap,
- enabling `LIVE_ORDER_PLACEMENT_ENABLED=true` globally,
- adding private keys, API secrets, seed phrases, raw L2 credentials, or secret values to repo/config/docs/logs/tests,
- weakening geoblock, readback, heartbeat, risk, stale-data, or approval gates.

## Live Alpha Sequencing

- LA1 and LA2 must pass before any controlled fill canary.
- LA3 is the first possible controlled fill canary, and only after explicit human/operator approval.
- LA5 or later is the first possible maker-only micro autonomy, and only after prior evidence gates.
- Strategy-selected live trading remains deferred behind a separate robustness gate and explicit approval.

## Approval Fields

- Approved scope: LA0 docs/scope/approval gate only.
- Approved wallet: not changed by LA0; any Live Alpha wallet approval must be recorded before a later live-capable phase.
- Approved host/environment: not changed by LA0; approved-host evidence remains phase-specific and must be rechecked before later live-capable phases.
- Approved assets: BTC, ETH, and SOL only for future Live Alpha planning; no live action authorized by LA0.
- Approved maximum pUSD funding: not set by LA0.
- Approved maximum single-order notional: not set by LA0.
- Approved maximum daily loss: not set by LA0.
- Approved maximum open orders: `0` for LA0.
- Approved live order types: none for LA0.
- Prohibited phases from this branch: LA1, LA2, LA3, LA4, LA5, LA6, LA7, LA8.
- Rollback owner: human/operator to define before live-capable phases.
- Monitoring owner: human/operator to define before live-capable phases.
- Reviewer/approver identity: operator requested LA0 completion on 2026-05-03; PR review remains the merge gate.

## Verification

Local checks run from branch `live-alpha/la0-approval-scope`:

- `cargo fmt --check` PASS.
- `cargo test --offline` PASS: 220 lib tests + 8 main tests.
- `cargo clippy --offline -- -D warnings` PASS.
- `git diff --check` PASS.
- `git diff --cached --check` PASS after staging the LA0 files.
- `git diff HEAD --check` PASS after staging the LA0 files.

Safety/no-secret scans run:

```bash
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
rg -n -i --hidden -g '!.git' -g '!target' -g '!.env' -g '!config/local.toml' "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
```

Expected hits only:

- existing LB6 gated canary `post_order` and exact single-order cancel/readback paths,
- paper order/cancel simulation paths,
- disabled live-order gate strings,
- approved secret handle names and L2 header names, not values,
- public placeholder addresses, public canary/order/condition IDs, and public Pyth/Chainlink IDs,
- safety-scan command text and no-secret warnings in docs/verification notes.

No new live order, live cancel, cancel-all, secret value, API-key value, seed phrase, wallet/private-key material, geoblock bypass, strategy-to-live route, broader order type, multi-order path, cap reset, or market/asset expansion was added by LA0.

## Result

LA0 PASS for approval/scope documentation only. Expected next action after PR merge: stop and obtain explicit human/operator approval to start LA1 from fresh updated `main`.
