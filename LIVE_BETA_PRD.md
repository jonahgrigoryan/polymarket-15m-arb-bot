# PRD: Polymarket Live Beta Release Gate

Date: 2026-04-29
Branch: `live-beta/prd`
Base: M9 paper/replay validation merged in PR #13

## Summary

This document defines the release gate for the first controlled Polymarket live beta after M9. It is a planning and approval artifact only. It does not authorize implementation, order placement, signing, wallet/key handling, API-key handling, authenticated CLOB clients, or live trading.

M9 proved current-window paper/replay mechanics, RTDS Chainlink reference ingestion, deterministic replay, natural risk-reviewed paper fills, and post-market settlement reconciliation. M9 did not prove the strategy is profitable live. The settlement-reconciled sample was negative: filled notional `3.468000`, fees `0.226100`, settlement value `0.000000`, final P&L `-3.694100`.

The first live beta must therefore be treated as a controlled engineering/access test, not a performance rollout.

## Goals

- Define the approvals, evidence, and controls required before any real order path is implemented.
- Keep the M9 paper/replay pass separate from live-trading readiness.
- Verify legal/access, deployment geoblock behavior, key custody, signing/auth, CLOB order/cancel/readback semantics, live risk limits, observability, and rollback.
- Constrain the first live beta to tiny, human-approved orders from a dedicated beta wallet with a hard funding cap.
- Ensure every live action can be audited against captured market data, risk decisions, order acknowledgements, cancels, fills, balances, and settlement artifacts.

## Non-Goals

- No live order implementation in this PRD branch.
- No wallet, signing, private-key, API-key, or authenticated CLOB order-client code in this PRD branch.
- No attempt to bypass Polymarket geographic restrictions.
- No strategy-profitability claim from M9.
- No autonomous live trading before the release gate is explicitly approved.
- No scaling beyond BTC, ETH, and SOL 15-minute markets during the first beta.

## Source Evidence

Repository evidence:

- `STATUS.md`: M9 is PASS for paper/replay scope only; live trading remains blocked.
- `PRD.md`: MVP is replay and paper trading only; live order placement is out of scope.
- `IMPLEMENTATION_PLAN.md`: future live beta requires a separate PRD, legal/access review, key management plan, and signing audit.
- `API_VERIFICATION.md`: future live beta requires every API verification section plus signing/auth/order/cancel/rate-limit checks.
- `verification/2026-04-27-m9-live-readiness-findings.md`: live-readiness blockers before real orders.
- `verification/2026-04-29-m9-rtds-settlement-reconciliation.md`: M9 settlement reconciliation and negative paper result.

Official docs checked on 2026-04-29:

- Polymarket API introduction: `https://docs.polymarket.com/api-reference/introduction`
- Authentication: `https://docs.polymarket.com/api-reference/authentication`
- Rate limits: `https://docs.polymarket.com/api-reference/rate-limits`
- Geographic restrictions: `https://docs.polymarket.com/api-reference/geoblock`
- Market keyset pagination: `https://docs.polymarket.com/api-reference/markets/list-markets-keyset-pagination`
- Post order: `https://docs.polymarket.com/api-reference/trade/post-a-new-order`
- Cancel order: `https://docs.polymarket.com/api-reference/trade/cancel-single-order`
- Get order by ID: `https://docs.polymarket.com/api-reference/trade/get-single-order-by-id`
- Clients and SDKs: `https://docs.polymarket.com/developers/CLOB/clients`
- V2 migration: `https://docs.polymarket.com/v2-migration`
- CLOB trading overview: `https://docs.polymarket.com/developers/CLOB/trades/trades-data-api`

Current doc-derived assumptions that must be rechecked before implementation:

- Gamma and Data APIs are public; CLOB read endpoints are public; CLOB trading endpoints require authentication.
- Production CLOB base is `https://clob.polymarket.com`.
- CLOB trading endpoints require L2 authentication headers.
- Posting an order still requires a signed order payload, even with L2 authentication headers.
- Cancel and order readback endpoints exist under the CLOB API and require authenticated headers.
- United States access is blocked according to the geographic restrictions page.
- V2 collateral is pUSD; beta funding and balances must be checked in pUSD terms.
- Official docs currently identify open-source SDK paths including Rust `polymarket_client_sdk_v2` and repository `rs-clob-client-v2`; any direct REST client must own EIP-712 signing and HMAC auth correctness.

## Release Gate Overview

Live beta is blocked until all gates below are complete and signed off.


| Gate                                           | Owner                | Required evidence                                                                                                   | Status  |
| ---------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------- | ------- |
| Legal/access approval                          | Human/operator       | Written approval that operator, jurisdiction, host, and market access are permitted                                 | BLOCKED |
| Deployment geoblock check                      | Engineering/operator | Deployment-host geoblock response showing eligible access; fail-closed behavior verified                            | BLOCKED |
| Strategy robustness approval                   | Human/operator       | Explicit approval that beta is order-lifecycle-only, or additional RTDS paper windows pass the robustness gate      | BLOCKED |
| Dedicated beta wallet and collateral preflight | Human/operator       | Wallet/funder address, pUSD funding cap, allowances, signature type, balance reservation, and custody plan approved | BLOCKED |
| Key management                                 | Engineering/security | Secret storage, access control, rotation, audit logging, and no-secrets-in-repo review                              | BLOCKED |
| SDK/signing decision                           | Engineering/security | Audit official SDK path or justify minimal custom client before any signing path lands                              | BLOCKED |
| Signing audit                                  | Engineering/security | Current CLOB signing/auth flow verified against official docs and reviewed implementation design                    | BLOCKED |
| CLOB order/cancel/readback verification        | Engineering          | Staged authenticated tests for create/sign locally, post tiny order, read back, cancel, read back canceled state    | BLOCKED |
| Heartbeat plan                                 | Engineering/operator | Maker-order heartbeat behavior verified and monitored before any maker order can stay open                          | BLOCKED |
| Live risk controls                             | Engineering/operator | Hard max notional, max loss, position, rate, freshness, and kill-switch behavior proven                             | BLOCKED |
| Human approval                                 | Human/operator       | First-order approval flow and approval log tested                                                                   | BLOCKED |
| Observability and audit                        | Engineering          | Metrics/logs/artifacts capture every live decision and venue response                                               | BLOCKED |
| Rollback                                       | Engineering/operator | Kill switch, cancel-all plan, service stop, and post-incident checklist tested                                      | BLOCKED |


No gate may be marked PASS from paper-only evidence alone.

## Beta Scope

Default first beta:

- Assets: BTC, ETH, SOL only.
- Markets: current 15-minute up/down markets only.
- Duration: two trading days maximum, or shorter if any stop condition triggers.
- Wallet: dedicated beta wallet only.
- Funding cap: tiny fixed cap approved before implementation. Suggested initial cap: `<= 25 pUSD`, with a stricter first-order cap.
- First-order cap: `<= 1 pUSD` notional per order until manual review passes.
- Order style: first order must be post-only GTC/GTD maker-only with explicit expiry before market end. FOK, FAK, marketable limit, taker, or any intentionally crossing path requires separate written approval.
- First orders: human approval required per order.
- Autonomous mode: disabled for first beta. Any later autonomy requires a second release gate.
- Strategy objective: verify access, signing, risk controls, order lifecycle, cancel/readback, fill capture, and settlement reconciliation. Do not judge profitability from this tiny beta.

## Strategy Boundary

The first live beta is an engineering and order-lifecycle probe only.

Default rule:

- The runtime may not place strategy-selected live trades based solely on M9 signals.
- The first order must be manually selected and approved as a venue lifecycle test.
- Any strategy-selected live order requires a separate strategy robustness gate and written approval.

Strategy robustness gate:

- Multiple additional RTDS Chainlink-backed paper windows must pass current-window selection, unchanged signal/risk gates, deterministic replay, final settlement reconciliation, and no unexplained data-quality defects.
- The robustness review must summarize order count, fill count, fees, pre-settlement P&L, post-settlement P&L, skip reasons, market phases, maker/taker split, and failure cases.
- Negative or unstable paper settlement results must keep live beta in order-lifecycle-only mode.
- A strategy go/no-go decision must be explicit; absence of a decision means no strategy-selected live orders.

## Legal And Access Gate

Before implementation:

- Confirm the operator is legally permitted to access Polymarket and place orders from the intended jurisdiction.
- Confirm the deployment host jurisdiction is permitted.
- Confirm the funding source and wallet ownership/custody are permitted.
- Confirm the beta does not rely on VPN/proxy/geoblock bypassing.
- Record approval in a dated verification note.

Runtime requirements:

- Trading-capable mode must fail closed when geoblock check is blocked, malformed, unreachable, stale, or inconsistent.
- The deployment host must run a geoblock check at startup and on a fixed interval.
- A blocked or close-only jurisdiction must prevent opening new positions.
- Close-only behavior, if ever supported, must be explicitly designed and separately approved.

## Wallet And Funding Gate

Dedicated beta wallet requirements:

- One dedicated beta wallet, separate from any main wallet.
- Funding cap approved before implementation.
- Wallet address and funder/proxy wallet address recorded in a local, non-secret deployment note.
- Signature type selected and justified before implementation.
- pUSD collateral balance verified before any order.
- pUSD allowances/approvals verified for the exact exchange/contracts required by current CLOB docs.
- Balance reserved by open orders is read back and included in available-budget calculations.
- POL gas balance is required only if the selected wallet path needs EOA onchain transactions.
- Funding transactions recorded for audit.
- No production funds or unrelated assets in the beta wallet.
- No wallet seed/private key stored in repo, config examples, logs, reports, shell history, or CI.

Funding stop conditions:

- Stop if wallet balance exceeds the approved cap.
- Stop if any transaction appears that was not initiated by the approved beta process.
- Stop if balance/readback cannot be reconciled against venue and chain/accounting records.
- Stop if pUSD balance, reserved balance, allowance, or proxy/funder address does not match the preflight record.

Collateral and account preflight must record:

- Wallet address.
- Funder/proxy wallet address, if different.
- Signature type.
- pUSD wallet balance.
- pUSD available balance after open-order reservations.
- Open orders consuming reserved balance.
- Required pUSD allowance targets and amounts.
- POL balance only when needed for EOA transactions.
- Chain ID, CLOB host, and exchange/contract identifiers used for signing.

## Key Management And Signing Gate

Implementation may not begin until a key-management design is approved.

Required design points:

- Where private key material lives.
- Who can access it.
- How the runtime obtains signing capability without printing or persisting secrets.
- How L1 credential derivation is separated from normal live runtime operation.
- How L2 credentials are stored, rotated, revoked, and audited.
- How logs and reports are scrubbed.
- How local development avoids using production keys.
- How signing code is reviewed before it can post any order.

Minimum acceptable posture:

- No private keys or API secrets in git.
- No private keys or API secrets in `.env` committed files.
- No client-side or browser-exposed secret path.
- No unaudited third-party signing library in the hot path.
- Signing payloads and venue requests must be logged only in sanitized form.
- A dry-run signing test must prove exact payload construction without submitting an order.

## SDK And Signing Decision

Before any signing path lands, the team must choose and document one of these approaches:

1. Use the official Rust client path after auditing `polymarket_client_sdk_v2` / `rs-clob-client-v2` for current V2 support, signature type support, funder/proxy wallet support, order/cancel/readback coverage, dependency risk, and examples.
2. Use the official TypeScript/Python V2 clients only as reference implementations while building a minimal Rust client.
3. Build a minimal custom Rust REST client only after documenting why the official SDK path is insufficient.

Decision evidence must include:

- Exact package/repository and version or commit audited.
- Whether the audited path supports CLOB V2, pUSD, EIP-712 domain version, signature type, funder/proxy wallet, order posting, canceling, and readback.
- Dependency and transitive dependency review for signing and HTTP.
- Example payloads for dry-run signing.
- Reason for rejecting any official SDK path.
- Security review signoff before private key material can ever be introduced.

No signing code may be merged until this decision is complete.

## CLOB Auth, Order, Cancel, And Readback Verification

Before implementation:

- Recheck official CLOB docs and live endpoint behavior.
- Verify L1 and L2 auth requirements.
- Verify required authenticated headers.
- Verify signature type and funder address requirements for the selected wallet type.
- Verify order fields for the current CLOB version.
- Verify order tick size, minimum size, fee, neg-risk, and token ID requirements.
- Verify permitted order types and time-in-force values.
- Verify post-only maker behavior and how the venue rejects or handles marketable post-only orders.
- Verify GTD/GTC expiry semantics and minimum/maximum expiration.
- Verify order statuses and readback response shape.
- Verify cancel response shape and behavior for already-filled, partially-filled, already-canceled, and missing orders.
- Verify user-order and trade/fill readback endpoints.
- Verify trade-status response shape, transaction hash field, settlement/fill status values, and failed/unmatched states.
- Verify rate limits for order posting, canceling, and readback.

Staged verification sequence:

1. Read-only endpoint recheck: `/ok`, market discovery, CLOB book, server time.
2. Auth-only check: derive or load L2 credentials without placing orders.
3. Dry-run signing: build and sign an order payload locally, but do not post it.
4. Tiny live post: one human-approved post-only maker order with explicit expiry before market end.
5. Immediate readback: confirm venue order ID, market, token, side, price, size, and status.
6. Cancel test: cancel the order if still open.
7. Cancel readback: verify canceled state and no remaining open order.
8. Trade readback: if matched, verify trade status, transaction hash, mined/confirmed/failed state, fees, and balance deltas.
9. Settlement follow-up: reconcile final market outcome and realized P&L.

No step may skip the previous step.

## Order Type And Expiry Rules

First live order rules:

- Post-only maker order only.
- GTC or GTD only.
- If GTD is available, expiry must be before the market end timestamp with enough time to cancel and reconcile before settlement.
- If only GTC is used, the runtime must cancel before the configured cutoff and verify cancel readback.
- Price must be non-marketable at submission according to the latest book snapshot and configured stale-book gate.
- Size must be within the first-order notional cap and minimum-size rules.

Disallowed without separate approval:

- FOK.
- FAK.
- Taker order.
- Marketable limit order.
- Any order submitted during final seconds.
- Any order submitted without a heartbeat plan.
- Any order when cancel/readback or trade/readback is degraded.

## Heartbeat Plan

Maker orders require an explicit heartbeat plan because missing heartbeat behavior can cancel open orders.

Before maker orders:

- Verify current official heartbeat requirements and endpoint behavior.
- Verify whether heartbeat is required per API key, market, account, or open order.
- Verify heartbeat interval, timeout, failure response, and rate limit.
- Verify what happens to open orders when heartbeat is delayed, missing, duplicated, or rejected.
- Add metrics and alerts for heartbeat age, failures, venue acknowledgement, and cancel-on-missed-heartbeat events.
- Define whether heartbeat is sent by the trading process or a supervised sidecar.

Fail closed:

- Do not open maker orders if heartbeat state is unknown, stale, failing, or not monitored.
- Cancel or stop according to the approved rollback plan if heartbeat becomes unhealthy.
- Treat heartbeat ambiguity as a no-go for maker beta.

## Trade Lifecycle Reconciliation

Order status alone is insufficient for live beta completion.

Every live order must reconcile:

- Submitted order intent.
- Signed order hash or sanitized signing evidence.
- Venue order ID.
- Order status transitions.
- Cancel request and cancel response, if canceled.
- Matched trade records, if any.
- Trade status.
- Transaction hash, if present.
- Mined, confirmed, failed, delayed, or unmatched state.
- Fill price, size, fee, and balance delta.
- pUSD available/reserved balance before and after.
- Position after fill/cancel.
- Final market settlement outcome.
- Final realized P&L.

Unreconciled states:

- `delayed`, `unmatched`, missing transaction hash, failed transaction, inconsistent balance, or unknown trade status must halt new orders.
- The beta cannot proceed to another order until the previous order is reconciled or an incident note is approved.

## Venue State And Error Handling

Trading-capable mode must fail closed on venue states and responses that are not explicitly safe.

Hard stop states:

- Trading disabled.
- Cancel-only.
- Closed-only.
- Market closed.
- Market not started unless the approved order type explicitly allows pre-start maker orders and legal/risk approval exists.
- Delayed market.
- Unmatched or indeterminate trade state.
- Order error response.
- Authentication error.
- Signature error.
- Rate-limit response on order, cancel, or readback.
- Balance, allowance, or funder/proxy mismatch.
- Unexpected tick size, min size, fee, neg-risk, or token mapping.

Rules:

- Unknown venue status means no new orders.
- Readback mismatch means no new orders.
- Cancel failure means no new orders until manually reconciled.
- Partial fill with unclear remaining quantity means no new orders.
- Any response parser fallback or unknown enum value must be treated as a halt, not ignored.

## Live Risk Policy

Live risk controls must be implemented before any post-order code path is enabled.

Default beta limits:

- `LIVE_ORDER_PLACEMENT_ENABLED=false` remains the default.
- Live mode requires explicit config, CLI flag, and human confirmation.
- Max first-order notional: `<= 1 pUSD`.
- Max notional per market: tiny, approved before implementation.
- Max notional per asset: tiny, approved before implementation.
- Max total open notional: bounded by beta funding cap and lower runtime caps.
- Max daily realized loss: tiny, approved before implementation.
- Max order rate: conservative; default one human-approved order at a time.
- Max open orders: one until cancel/readback is proven.
- Max stale reference age: no weaker than paper gate.
- Max stale book age: no weaker than paper gate.
- Max heartbeat age: must be defined before maker orders.
- Stop on any geoblock warning, auth error, signing error, order mismatch, readback mismatch, cancel failure, missing fill reconciliation, or unexpected balance delta.

Kill switch requirements:

- Operator can disable order placement immediately without code changes.
- Runtime stops opening new orders when kill switch is active.
- Runtime attempts cancel-all only if cancel-all behavior has been verified and approved.
- Runtime records a shutdown artifact with reason, open orders, positions, balances, and last venue readback.

## Human Approval Flow

First live orders require explicit human approval.

Approval prompt must include:

- Run ID.
- Deployment host and geoblock result.
- Wallet/funder address.
- Signature type.
- pUSD balance, pUSD available balance, and any reserved balance from open orders.
- Market slug and condition ID.
- Token ID and outcome.
- Side, price, size, notional, tick size, order type, time-in-force, expiry, and fee estimate.
- Signal reason and risk approval reason.
- Worst-case loss and remaining beta budget.
- Current book snapshot age, reference tick age, and heartbeat state.
- Whether the order is maker or taker.
- Cancel plan and rollback command.

Approval log must include:

- Approver identity.
- Timestamp.
- Full sanitized order intent.
- Resulting venue order ID or explicit non-submission.

## Observability And Audit

Live beta must persist:

- Raw market/reference/predictive feed messages.
- Normalized events.
- Market metadata and current-window selection logs.
- Signal decisions and skip reasons.
- Risk decisions and gate values.
- Human approvals.
- Signed-order hash or sanitized signing evidence.
- Venue post/cancel/readback responses.
- Fills/trades.
- Trade status and transaction hash when present.
- Balances before and after each live action.
- Reserved balance from open orders.
- Open orders and positions.
- Settlement artifacts.
- P&L reconciliation.

Metrics must expose:

- Live mode enabled/disabled.
- Geoblock status.
- Kill switch state.
- Order placement attempts, accepted orders, rejected orders, cancels, fills.
- Heartbeat age/failures/acknowledgements.
- Risk rejects by reason.
- Readback mismatches.
- Balance mismatches.
- Open notional by market/asset/total.
- Realized and settlement P&L.

Alerts must fire on:

- Any live order attempt.
- Any auth/signing failure.
- Any geoblock failure.
- Any readback/cancel/fill mismatch.
- Any heartbeat miss or heartbeat uncertainty.
- Any delayed/unmatched/failed trade state.
- Any stale feed gate.
- Any loss or notional cap breach.
- Any kill-switch activation.

## Deployment Plan

Before live beta:

1. Provision deployment host in an approved jurisdiction.
2. Run local-only validation.
3. Run read-only online validation from the host.
4. Run bounded RTDS Chainlink paper session from the host.
5. Replay the host session deterministically.
6. Confirm geoblock startup and interval checks.
7. Configure secrets through approved secret storage only.
8. Confirm `LIVE_ORDER_PLACEMENT_ENABLED=false` by default.
9. Enable live beta only for the approved run window.

The deployment host must be able to run paper-only mode without any secrets before live-mode configuration is introduced.

## Rollback Plan

Immediate rollback:

- Activate kill switch.
- Stop live order placement.
- Read back open orders.
- Cancel verified open orders if cancel path has been approved.
- Stop service.
- Snapshot balances, reserved balances, open orders, positions, trade statuses, transaction hashes, heartbeat state, and latest market data.
- Preserve logs and reports.

Post-rollback:

- Reconcile wallet balance and venue order/trade history.
- Reconcile trade status, transaction hashes, fees, and reserved balance release.
- Reconcile market settlement when final.
- Produce incident note.
- Do not restart live beta until the incident note has explicit approval.

## Go/No-Go Criteria

Go requires all of:

- Legal/access approval recorded.
- Deployment geoblock PASS from the live host.
- Dedicated beta wallet funded within cap.
- pUSD collateral, funder/proxy wallet, signature type, allowances, reserved balance, and POL-if-needed preflight recorded.
- Key-management design approved.
- SDK/signing decision completed.
- Signing/auth implementation reviewed and tested without posting.
- CLOB order/cancel/readback behavior verified in the staged sequence.
- Heartbeat plan verified for maker orders.
- Trade lifecycle reconciliation verified.
- Venue state and error handling fail-closed tests pass.
- Live risk controls tested.
- Human approval flow tested.
- Observability and rollback tested.
- `STATUS.md` updated with exact approved live-beta scope.

No-go if any of:

- Operator or host jurisdiction is blocked or ambiguous.
- Any secret appears in git, logs, reports, CI, shell history, or chat.
- Signing/auth docs or live behavior are ambiguous.
- Readback/cancel behavior is not verified.
- SDK/signing path is unaudited or undecided.
- pUSD collateral, allowances, signature type, or funder/proxy wallet state is ambiguous.
- Heartbeat behavior is unknown or unhealthy.
- First order is not post-only maker with explicit expiry/cancel plan.
- Trade lifecycle cannot reconcile trade status, transaction hash, balances, fees, and settlement.
- Venue reports trading-disabled, cancel-only, closed-only, delayed, unmatched, or an unknown/error state.
- Risk controls can be bypassed.
- Kill switch is untested.
- M9 paper evidence is used as a profitability claim.
- Additional paper sessions show material unexplained losses or replay divergence.

## Implementation Phases After Approval

Approval of this PRD is approval of the release-gate requirements only. It is not authorization to begin live-beta code.

Before implementation begins, create and approve a separate `LIVE_BETA_IMPLEMENTATION_PLAN.md` with explicit LB0-LB7 phases, owners, verification commands, rollback points, and stop/go criteria. The implementation plan must preserve every gate in this PRD and must be reviewed before any live order placement, signing, wallet/key handling, API-key handling, authenticated CLOB client, or trading-capable runtime path is added.

The implementation plan should decompose work along this sequence:

1. Config-only live mode gate with `LIVE_ORDER_PLACEMENT_ENABLED=false` default and no order code.
2. Secret-management integration and sanitized config validation.
3. Auth-only CLOB verification with no orders.
4. Dry-run signed-order construction with no submission.
5. Collateral/account preflight for pUSD, funder/proxy, signature type, allowances, balances, and reservations.
6. Readback client for authenticated user orders, trades, balances, and venue state.
7. Heartbeat client or supervised heartbeat sidecar with monitoring.
8. Cancel client, tested only against approved staged scenarios.
9. Post-only maker order client behind compile/runtime/human approval gates.
10. One-order live beta with full readback/cancel/fill/trade/settlement reconciliation.

Each phase needs its own verification note before the next phase starts.

## Open Questions

- Final approved beta host and jurisdiction.
- Final funding cap and first-order notional cap.
- Selected wallet type and signature type.
- Whether a Gnosis Safe/proxy wallet or EOA will be used.
- Secret-management backend.
- Whether close-only behavior should be supported in the first beta or treated as stop-only.
- Exact order type allowed for the first live order.
- Whether any taker order is allowed in beta or maker-only is mandatory.
- Official SDK path versus minimal custom client.
- Heartbeat mechanism and owner.
- Whether strategy-selected live orders are allowed after additional paper robustness evidence or the beta remains lifecycle-only.

## Approval Record

- LB0 scope-lock approval: 2026-04-29 — APPROVED (planning artifacts only).
- Legal/access owner: Operator.
- Legal/access status: not yet formally approved; required before LB4+.
- Strategy boundary for this lock: no strategy-selected live orders until separate robustness gate; first live test remains a human-approved one-order lifecycle canary.
- Funding cap: tiny cap policy retained (`<= 25 pUSD`), first-order cap retained (`<= 1 pUSD`) pending operational review.
- Explicit non-authorization statement: LB0 does not authorize live order placement, signing, wallet/API-key handling, authenticated CLOB implementation, or autonomous trading.
- Next phase: LB1 live-mode kill gates only.
- Reviewer/approver identity: operator (manual confirmation).

This section is intentionally blank until reviewed.

Required approvals:

- Legal/access:
- Operator:
- Engineering:
- Security/signing:
- Risk:
