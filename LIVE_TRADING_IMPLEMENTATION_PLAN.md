# Implementation Plan: Polymarket Final Live Trading Track

Date: 2026-05-12
Branch: `live-trading/prd`
Base: `main` after LA8 merge / PR #41
Related PRD: `LIVE_TRADING_PRD.md`
Status: Draft for approval
Scope type: Post-LA8 final live-trading implementation plan

## Purpose

This document decomposes `LIVE_TRADING_PRD.md` into milestone-gated implementation work for the final live-trading track and the original scale objective: a bot capable of reaching **1,000+ matched fills per day** when evidence, liquidity, rate limits, risk, and compliance support that scale.

This plan exists because a separate explicit request approved creating the plan. The PRD by itself did not authorize implementation-plan work, code changes, live orders, cap resets, or trading expansion.

This plan does not approve live trading. It is a draft sequence that must be reviewed and approved before any final-live source code, config, signing, order, cancel, or runtime behavior is added.

The final-live track starts after LA8 closed as:

```text
NO-GO: lifecycle unsafe
```

The plan treats that decision as an input requirement. It does not rewrite, delete, soften, or route around LA7/LA8 evidence. LA3 and LA7 already proved that controlled real orders can be submitted, matched, read back, and closed out. The remaining final-live problem is not whether fills are possible; it is whether the system can make fills repeatable, reconciled, profitable or risk-approved, and scalable to 1,000+ fills/day.

## Controlling Inputs

- `AGENTS.md`: permanent safety and project instructions.
- `PRD.md`: original replay-first and paper-first product scope.
- `LIVE_BETA_PRD.md` and `LIVE_BETA_IMPLEMENTATION_PLAN.md`: first live beta gates.
- `LIVE_ALPHA_PRD.md` and `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`: LA0-LA8 phase gates.
- `LIVE_TRADING_PRD.md`: final live-trading product boundary.
- `STATUS.md`: current branch, blockers, and handoff state.
- `verification/2026-05-09-live-alpha-la8-scale-decision.md`: current LA8 decision artifact.
- LA0-LA8 dated verification notes.

Official Polymarket docs rechecked while drafting this plan on 2026-05-12:

- Authentication: `https://docs.polymarket.com/api-reference/authentication`
- Order overview: `https://docs.polymarket.com/trading/orders/overview`
- Rate limits: `https://docs.polymarket.com/api-reference/rate-limits`
- Geographic restrictions: `https://docs.polymarket.com/api-reference/geoblock`

Doc-derived implementation assumptions to recheck before each live-capable phase:

- CLOB production host remains `https://clob.polymarket.com`.
- Public market-data reads do not require authentication, but CLOB trading, cancel, heartbeat, and user order/trade readback require the current authenticated path.
- L1 private-key signing and L2 API credentials are separate. L2 headers authenticate trading requests, but order payloads still require signing.
- The selected wallet signature type and funder/deposit/proxy address must match current docs and account readback exactly.
- All Polymarket orders are limit-order primitives. Market behavior is achieved by marketable limits.
- GTC and GTD are resting limit orders. FOK and FAK execute immediately against resting liquidity.
- Post-only is compatible only with GTC/GTD and must reject if marketable.
- GTD expiration security threshold, tick size, minimum size, fee fields, neg-risk flags, order statuses, trade statuses, heartbeat behavior, and current SDK behavior must be rechecked before implementation.
- `GET https://polymarket.com/api/geoblock` remains a mandatory deployment-host gate and is hosted on `polymarket.com`, not the CLOB API host.
- Blocked, close-only, malformed, stale, or unreachable geoblock results fail closed. No bypass, proxy, VPN workaround, or warning-only path is allowed.
- Current docs list the United States as blocked for placing orders. Any final-live deployment host must be separately approved and not in a blocked or close-only jurisdiction.
- Rate-limit throttling is a freshness and safety risk, even when the API queues or delays requests instead of immediately returning an error.

## Global Rules

- Do not begin any phase until the previous phase exit gate is complete and recorded in a dated verification note.
- Do not skip mandatory hold points.
- Keep order placement disabled by default in all builds and configs.
- Use a final-live compile-time feature only if the phase explicitly allows it, and keep it off by default.
- Existing `live-alpha-orders` authority does not imply final-live authority.
- Do not reuse the consumed LA7 taker cap.
- Do not reset, rename, or overwrite historical LA7/LA8 cap, report, or verification artifacts to manufacture clean evidence.
- Do not run live orders, live cancels, cancel-all, signing for submission, or authenticated write endpoints from docs-only or read-only phases.
- Do not add taker execution until maker evidence, shadow taker evidence, and a separate taker approval gate pass.
- Do not add cancel-all except as a separately approved emergency path with its own dry-run, approval, and evidence requirements.
- Do not expand more than one dominant dimension in a single phase unless the approval artifact explicitly records the coupled dimensions and the verification note proves each dimension separately.
- Do not store secrets in repo files, config examples, logs, reports, approval notes, CI, shell history, or chat.
- Secret checks must validate handle names and presence only. They must not print values.
- Fail closed on ambiguous venue state, geoblock state, auth state, heartbeat state, account state, order state, trade state, balance state, position state, settlement state, or reconciliation state.
- Every live order must have a durable reason: proposed, approved, skipped, submitted, accepted, rejected, filled, partially filled, canceled, expired, halted, or incident-reviewed.
- Every phase must update `STATUS.md` when it changes the durable handoff state.
- Every phase must create or update a dated verification note under `verification/`.

## Phase Order

| Phase | Name | Live order allowed | Taker allowed | Mandatory hold |
| --- | --- | --- | --- | --- |
| LT0 | Approval and scope lock | No | No | Yes |
| LT1 | Read-only final-live supervision | No | No | No |
| LT2 | Final-live gates, journal, and evidence schema | No | No | No |
| LT3 | Auth, secret handles, and signing dry-run | No | No | Yes |
| LT4 | Maker shadow and approval-envelope dry-run | No | No | Yes |
| LT5 | One post-only maker canary | One approved maker order | No | Yes |
| LT6 | Maker-only evidence window | Bounded maker orders | No | Yes |
| LT7 | Supervised maker autonomy | Bounded maker orders | No | Yes |
| LT8 | Throughput, rate-limit, and backpressure readiness | No new order authority | No | Yes |
| LT9 | Selective taker shadow evidence | No | Shadow only | Yes |
| LT10 | Optional one selective taker canary | Separately approved only | One FAK/no-resting-remainder canary only | Yes |
| LT11 | First volume ramp, 50-100 fills/day candidate | Approved scale window | Approved only if prior phase allows it | Yes |
| LT12 | Intermediate volume ramp, 250-500 fills/day candidate | Approved scale window | Approved only if prior phase allows it | Yes |
| LT13 | Thousand-fill candidate, 1,000+ fills/day | Approved scale window | Approved only if prior phase allows it | Yes |
| LT14 | Final production-scale decision report | No new order authority | No new taker authority | Yes |

LT10 is intentionally included as a boundary marker. It must not be implemented unless LT9 exits cleanly and a separate approval artifact authorizes one taker canary under a fresh cap.

## Required Verification Baseline

Every phase must run the checks that apply to its changed files plus:

```text
git status --short --branch
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

Every phase must also run a focused safety scan over source, manifests, config, docs, runbooks, and verification notes touched by the phase:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|cancel-all|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config docs runbooks verification LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|funder|allowance|POLY_API_KEY|POLY_SIGNATURE|POLY_PASSPHRASE)" src Cargo.toml config docs runbooks verification LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
rg -n -i "(geoblock|geo|restricted|jurisdiction|vpn|proxy|bypass|close-only|blocked)" src config docs runbooks verification LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
```

Expected scan results must be explained in the dated verification note. New source hits are blockers unless the current phase explicitly allows that class of behavior and the code is behind the required gates.

For docs-only phases, run:

```text
git status --short --branch
git diff --check
rg -n "LIVE_TRADING_IMPLEMENTATION_PLAN|LT0|LT1|LT2|LT3|LT4|LT5|LT6|LT7|LT8|LT9|LT10|LT11|LT12|LT13|LT14" LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
```

For live-capable phases, recheck the official docs listed in this plan and record the checked URLs, date, and relevant assumptions in the verification note before coding or running approved-host commands.

## Artifact And Directory Conventions

Use final-live-specific artifact names. Do not reuse LA7/LA8 artifact names for new evidence.

```text
verification/YYYY-MM-DD-live-trading-ltN-<slug>.md
artifacts/live_trading/<approval_id>/
reports/live-trading/<run_id>/
reports/live-trading-cap.json
reports/live-trading-scale-report-<from>-to-<to>.json
```

Every live-capable run must write:

```text
approval artifact path and sha256
run id
deployment host and geoblock result
wallet/funder/signature-type summary
secret handle inventory without values
market slug, condition id, token id, outcome, side
order type, post-only flag, price, size, notional, tick size, expiry
book/reference/predictive freshness
risk decision
signed order hash or sanitized signing hash
venue order id if accepted
trade ids if matched or read back
open order count
available and reserved pUSD
position count
fee estimate and actual fee when available
reconciliation result
settlement follow-up requirement
halt or incident state
```

## LT0: Approval And Scope Lock

### Objective

Lock the final-live PRD and implementation-plan scope as planning artifacts only.

### Allowed Changes

- Documentation updates to `LIVE_TRADING_PRD.md`, `LIVE_TRADING_IMPLEMENTATION_PLAN.md`, `STATUS.md`, and dated verification notes.
- Approval record updates after human review.
- Issue or checklist creation that does not add code, config, secrets, or live runtime behavior.

### Explicitly Disallowed Changes

- Source code changes.
- Config changes that introduce final-live credentials, wallet values, API keys, signing fields, or order endpoints.
- Any authenticated client, signing path, wallet path, order path, cancel path, or trading-capable runtime path.
- Any statement that this plan authorizes live trading.

### Required Implementation Notes

- Record approved final-live scope:
  - maker-first,
  - BTC, ETH, and SOL only,
  - dedicated wallet/funder pair,
  - deployment host and jurisdiction approval required,
  - initial funding cap required,
  - first maker order cap required,
  - taker disabled by default,
  - cancel-all disabled unless separately approved,
  - no multi-wallet deployment,
  - no production rollout.
- Record unresolved open decisions from `LIVE_TRADING_PRD.md`.
- Record that source implementation may begin only after LT0 exits.

### Verification Commands/Checks

```text
git status --short --branch
git diff --check
rg -n "Status: Draft for approval|LT0|does not approve live trading|NO-GO: lifecycle unsafe|taker disabled" LIVE_TRADING_PRD.md LIVE_TRADING_IMPLEMENTATION_PLAN.md STATUS.md
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt0-scope-lock.md
```

The note must include:

- approved planning scope,
- explicit non-authorization of live trading,
- legal/access status,
- deployment host status,
- wallet/signature-type status,
- funding cap status,
- taker status,
- cancel-all status,
- required next phase,
- approver identity or review record.

### Exit Gate

LT0 exits only when the PRD and implementation plan are explicitly approved for phased implementation and `STATUS.md` points to LT1 as the next action. LT0 approval does not approve any order placement.

### Hold Point

Mandatory. Stop after LT0 until the human/operator approves starting LT1.

## LT1: Read-Only Final-Live Supervision

### Objective

Add final-live read-only supervision and approved-host evidence without order signing, order submission, cancel submission, or cap mutation.

### Allowed Changes

- Final-live config section that defaults disabled.
- Final-live read-only gate report.
- Deployment-host geoblock readback.
- Read-only market discovery/book/reference/predictive checks.
- Account baseline/readback command or extension that reads only.
- Redacted artifact writing under `artifacts/live_trading/`.
- Metrics/report labels for final-live read-only posture.

### Explicitly Disallowed Changes

- Order signing.
- Order submission.
- Cancel submission.
- Heartbeat POST unless separately approved as read-only-equivalent for the current API behavior.
- Final-live order client or cancel client.
- Live order feature enablement.
- Cap sentinel writes.

### Required Implementation Notes

- The command surface should be explicit, for example:

```text
live-trading-preflight --read-only --baseline-id <id>
```

- The read-only output must prove:
  - geoblock PASS from the deployment host,
  - host/jurisdiction match to approval,
  - current wallet/funder readback if credentials are approved for readback,
  - zero or explained open orders,
  - zero or explained reserved pUSD,
  - current positions,
  - market/book/reference freshness,
  - no order/cancel/signing/cap writes.

### Verification Commands/Checks

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline live_account_baseline
cargo test --offline live_alpha_preflight
cargo run --offline -- --config config/default.toml live-trading-preflight --read-only --baseline-id LT1-LOCAL-DRY-RUN
git diff --check
```

Approved-host command, only after LT1 approval and with secret values masked:

```text
cargo run -- --config <approved-final-live-config> live-trading-preflight --read-only --baseline-id LT1-YYYY-MM-DD-READONLY-001
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt1-read-only-supervision.md
```

Must include:

- official-doc recheck date and URLs,
- command outputs summarized as statuses and counts,
- geoblock result,
- baseline ID and baseline hash,
- account/open-order/position/balance status,
- final-live gate status,
- proof that no order, cancel, signing, or cap write occurred.

### Exit Gate

LT1 exits only when read-only supervision produces a redacted baseline artifact and all read-only gates are either PASS or explicitly fail-closed with no live action.

### Hold Point

None by default, but any blocked/ambiguous geoblock, account, or market state stops the track until reviewed.

## LT2: Final-Live Gates, Journal, And Evidence Schema

### Objective

Create final-live gate, journal, reducer, reconciliation, and evidence schemas without authenticated write behavior.

### Allowed Changes

- Final-live gate evaluator.
- Final-live run ID and evidence bundle model.
- Final-live journal event types.
- Reducers for intended maker orders, cancels, fills, fees, balances, positions, incidents, and settlement follow-up.
- Reconciliation state machine that can compare local state to readback fixtures.
- Tests and fixtures for fail-closed states.
- Redaction and stable hash helpers for evidence bundles.

### Explicitly Disallowed Changes

- Network order submission.
- Network cancel submission.
- Signing for submission.
- Authenticated write client.
- Live heartbeat POST.
- Any final-live CLI mode that can submit or cancel.

### Required Implementation Notes

- Preserve Live Alpha safety invariants instead of loosening them.
- Keep final-live evidence separate from LA7/LA8 historical artifacts.
- Reconciliation must block the next order until the previous order is reconciled or incident-reviewed.
- Unknown venue order, unknown trade, missing accepted order, unexpected fill, balance drift, position drift, settlement mismatch, stale heartbeat, and stale geoblock must all halt.

### Verification Commands/Checks

```text
cargo test --offline live_trading_gate
cargo test --offline live_trading_journal
cargo test --offline live_trading_reconciliation
cargo test --offline live_trading_evidence
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt2-gates-journal-evidence.md
```

Must include:

- modules changed,
- fail-closed fixtures added,
- no-order/no-cancel/no-signing proof,
- safety-scan results,
- updated `STATUS.md` next action.

### Exit Gate

LT2 exits only when final-live state can be modeled, replayed, reconciled from fixtures, and reported without any live write authority.

### Hold Point

None by default.

## LT3: Auth, Secret Handles, And Signing Dry-Run

### Objective

Add final-live auth and signing dry-run support that validates secret handles and signing payload shape without submitting or canceling any order.

### Allowed Changes

- Secret handle inventory and validation.
- Approved secret backend integration by handle name only.
- L1/L2 credential flow validation in dry-run form.
- Sanitized signing-payload hash generation.
- Wallet/funder/signature-type binding validation.
- Authenticated readback only when approved-host scope allows it.

### Explicitly Disallowed Changes

- Posting orders.
- Posting cancels.
- Generating an order intended for immediate submission.
- Logging private keys, API secrets, passphrases, raw signatures, or auth headers.
- Reusing Live Beta or Live Alpha approval artifacts as final-live approvals.

### Required Implementation Notes

- The dry-run must output `not_submitted=true`.
- The dry-run must output `network_post_enabled=false` for order submission.
- If any secret handle is missing, duplicated, value-like, or printed, the phase fails.
- Signing artifacts may include sanitized hashes and field names, not secret values.

### Verification Commands/Checks

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/default.toml validate --local-only --validate-secret-handles
cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-DRY-RUN
cargo test --offline secret_handling
cargo test --offline live_trading_signing
git diff --check
```

Approved-host/readback command, only after LT3 approval:

```text
cargo run -- --config <approved-final-live-config> live-trading-signing-dry-run --approval-id LT3-YYYY-MM-DD-SIGNING-001
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt3-auth-signing-dry-run.md
```

Must include:

- official-doc recheck date and URLs,
- secret backend and handle names without values,
- wallet/funder/signature-type summary,
- sanitized signing hash,
- `not_submitted=true`,
- `network_post_enabled=false`,
- readback status if approved,
- safety-scan results.

### Exit Gate

LT3 exits only when signing and auth dry-runs can be verified without exposing secrets and without any order/cancel submission path.

### Hold Point

Mandatory. Stop after LT3 until security/signing and operator review approve LT4.

## LT4: Maker Shadow And Approval-Envelope Dry-Run

### Objective

Generate final-live maker quote decisions and a reviewed approval envelope without placing orders.

### Allowed Changes

- Final-live maker quote evaluator.
- Final-live maker dry-run command.
- Approval envelope generator for one post-only maker order.
- Edge-at-submit, fee, tick-size, min-size, book-age, reference-age, no-trade-window, and no-marketability checks.
- Shadow comparison against paper decisions for the same market window when feasible.

### Explicitly Disallowed Changes

- Order submission.
- Cancel submission.
- Taker submission.
- Batch order path.
- Cancel-all path.
- Approval envelope that omits host, wallet/funder, baseline, market, side, order type, price, size, expiry, fee, or cap fields.

### Required Implementation Notes

- The proposed command surface should be:

```text
live-trading-maker-canary --dry-run --approval-id <id> --approval-artifact <path>
```

- The dry-run must reject:
  - stale or blocked geoblock,
  - stale book/reference/predictive state,
  - marketable post-only order,
  - unknown tick size or min size,
  - near-close market,
  - missing baseline binding,
  - missing heartbeat requirement if heartbeat is needed before maker orders,
  - any existing unresolved live order,
  - any unreviewed incident.

### Verification Commands/Checks

```text
cargo test --offline live_trading_maker
cargo test --offline live_quote_manager
cargo run --offline -- --config config/default.toml live-trading-maker-canary --dry-run --approval-id LT4-LOCAL-DRY-RUN --approval-artifact verification/YYYY-MM-DD-live-trading-lt4-approval-candidate.md
git diff --check
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt4-maker-shadow-approval.md
```

Must include:

- market-window candidate,
- approval envelope path and hash,
- dry-run report path and hash,
- expected order shape,
- maker/taker status,
- no-submit proof,
- comparable paper/live plan,
- blocker list if no safe maker candidate exists.

### Exit Gate

LT4 exits only when a dry-run maker approval envelope is complete, reviewed, and still records no order submission, no cancel submission, no signing-for-submit, and no cap mutation.

### Hold Point

Mandatory. Stop after LT4 until the human/operator approves the exact LT5 maker canary artifact.

## LT5: One Post-Only Maker Canary

### Objective

Submit exactly one approved post-only maker order, reconcile its lifecycle, and stop.

### Allowed Changes

- One final-live maker canary submit path.
- One final-live cap sentinel with create-new semantics.
- Exact-order-ID readback.
- Exact-order-ID cancel if the order remains open past TTL or risk degrades.
- Settlement follow-up reporting when the market resolves.

### Explicitly Disallowed Changes

- More than one order.
- Any taker order.
- FOK or FAK.
- Marketable GTC/GTD order.
- Batch orders.
- Cancel-all.
- Retry after ambiguous submit.
- New order before the previous order is reconciled or incident-reviewed.
- Any cap reset or reuse of LA7/LA8 artifacts.

### Required Implementation Notes

- The live command shape should be:

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-canary --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt5-maker-approval.md --approval-sha256 sha256:<exact-artifact-hash>
```

- Before submission, the runtime must recheck:
  - approval hash,
  - approval expiry,
  - geoblock,
  - wallet/funder/signature type,
  - baseline hash,
  - heartbeat state if required,
  - startup recovery,
  - open orders and reservations,
  - market status,
  - book/reference/predictive freshness,
  - tick/min-size/fee fields,
  - post-only non-marketability,
  - no-trade cutoff,
  - risk limits,
  - cap availability.

- After submission, the runtime must immediately run authenticated readback and reconcile accepted, rejected, filled, partially filled, canceled, expired, or ambiguous state.

### Verification Commands/Checks

Dry-run immediately before live approval:

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-canary --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt5-maker-approval.md
```

Live command, only after exact approval:

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-canary --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt5-maker-approval.md --approval-sha256 sha256:<exact-artifact-hash>
```

Post-run readback:

```text
cargo run -- --config <approved-final-live-config> live-trading-preflight --read-only --baseline-id LT5-YYYY-MM-DD-POST-CANARY-001
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt5-maker-canary.md
```

Must include:

- approval ID and artifact hash,
- dry-run report hash,
- cap state before and after,
- order ID if accepted,
- final order status,
- cancel status if canceled,
- fill status if filled,
- trade IDs if matched/read back,
- fee estimate and actual fee if available,
- balance and position deltas,
- reconciliation result,
- incident state,
- settlement follow-up requirement,
- explicit stop decision.

### Exit Gate

LT5 exits only when the one maker canary is fully reconciled, canceled/expired cleanly, or incident-reviewed. A no-fill canary can prove lifecycle but does not satisfy maker fill/P&L evidence for `GO`.

### Hold Point

Mandatory. Stop after LT5 for human review whether the order filled, canceled, expired, rejected, or incidented.

## LT6: Maker-Only Evidence Window

### Objective

Run a bounded maker-only evidence window to produce clean machine-readable maker fill and P&L evidence.

### Entry Criteria

- LT5 completed and reviewed.
- No unresolved LT5 incident.
- Current geoblock, account baseline, heartbeat, startup recovery, and reconciliation gates pass.
- Fresh approval artifact exists for the exact window.

### Allowed Changes

- One active maker order at a time.
- Small approved order count cap.
- Short approved runtime duration.
- Exact-order-ID cancel/replace within approved bounds.
- Maker fill quality metrics.
- Edge at submit, edge at fill, edge decay, adverse-selection, fees, realized P&L, and settlement P&L reporting.

### Explicitly Disallowed Changes

- Taker orders.
- Multiple simultaneous open maker orders unless separately approved.
- Multiple assets in the same first evidence window unless the approval explicitly chooses that.
- Increasing size, duration, asset coverage, and order count together.
- Continuing after any unresolved mismatch.

### Required Implementation Notes

- The first LT6 window should prefer the smallest useful scope: one approved asset, one market window at a time, one active order at a time.
- If no maker fill occurs, the correct outcome is `HOLD: more maker-only data required`, not `GO`.
- Settlement follow-up is required for any held position.

### Verification Commands/Checks

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-window --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt6-maker-window-approval.md
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-window --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt6-maker-window-approval.md --approval-sha256 sha256:<exact-artifact-hash>
cargo run -- --config <approved-final-live-config> live-trading-preflight --read-only --baseline-id LT6-YYYY-MM-DD-POST-WINDOW-001
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt6-maker-window.md
```

Must include:

- approval scope,
- runtime duration,
- market windows,
- orders, cancels, replacements, fills,
- maker fill count,
- fees,
- edge at submit/fill,
- edge decay,
- adverse-selection result,
- realized and settlement P&L,
- open-order and reservation closeout,
- comparison to paper/shadow decisions,
- final `NO-GO`, `HOLD`, or next-phase recommendation.

### Exit Gate

LT6 exits only when the bounded maker evidence window is closed, reconciled, and reviewed. Maker fill evidence must be machine-readable before it can support expansion.

### Hold Point

Mandatory. Stop after LT6 for human review.

## LT7: Supervised Maker Autonomy

### Objective

Run supervised maker-first autonomy for approved windows after LT6 evidence is clean.

### Entry Criteria

- LT6 evidence has no unresolved lifecycle mismatch.
- Maker lifecycle is clean.
- Maker fill/P&L evidence is positive or explicitly risk-approved as a bounded small loss with strong forward evidence.
- The next expansion changes one dimension only.

### Allowed Changes

- Strategy-selected maker quotes within approved caps.
- Cancel/replace for stale, crossed, degraded, or risk-invalid quotes.
- Continuous readback and reconciliation.
- Longer runtime or slightly higher order count, but not both unless one remains unchanged from LT6.
- Maker-only observability and alerting.

### Explicitly Disallowed Changes

- Taker execution.
- Multi-wallet deployment.
- Production sizing.
- Multiple expansion dimensions at once.
- Ignoring unresolved settlement, balance, position, or venue-state ambiguity.

### Required Implementation Notes

- The service must behave like a supervised capital-controlled runtime.
- Operator kill switch and incident workflow must be tested before any LT7 live window.
- Restart/resume must prove journal replay, cap state, open orders, positions, and balances before any new order.

### Verification Commands/Checks

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-autonomy --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt7-maker-autonomy-approval.md
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-maker-autonomy --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt7-maker-autonomy-approval.md --approval-sha256 sha256:<exact-artifact-hash>
cargo run -- --config <approved-final-live-config> live-trading-scale-report --from <date> --to <date>
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt7-maker-autonomy.md
```

Must include:

- approved expansion dimension,
- runtime health,
- all order lifecycle counts,
- maker fill quality,
- fees and P&L,
- cancel/replace reasons,
- reconciliation and incident state,
- kill-switch evidence,
- restart/recovery evidence if exercised,
- next-phase recommendation.

### Exit Gate

LT7 exits only when supervised maker autonomy has clean lifecycle evidence and enough maker performance evidence to decide whether taker remains unnecessary, shadow-only, or worth a separate shadow phase.

### Hold Point

Mandatory. Stop after LT7 for human review.

## LT8: Throughput, Rate-Limit, And Backpressure Readiness

### Objective

Add the infrastructure needed for high-volume trading before increasing live volume.

### Entry Criteria

- LT7 complete and reviewed.
- Maker lifecycle and reconciliation evidence are clean.
- No unresolved incident, settlement drift, balance drift, or open-order ambiguity.

### Allowed Changes

- Order-rate and cancel-rate limiters.
- Queue/backpressure handling for market data, order decisions, submission, cancel, readback, and report writing.
- Throughput dry-run that simulates high order/fill/cancel rates without live order authority.
- Metrics for orders/day, fills/day, cancels/day, order-to-fill ratio, cancel-to-fill ratio, readback lag, rate-limit delay, and reconciliation lag.
- High-volume report fields and alerts.

### Explicitly Disallowed Changes

- New live order authority beyond the approved LT7 scope.
- Live taker submission.
- Batch order path.
- Cancel-all path.
- Raising live caps because a dry-run passes.

### Required Implementation Notes

- This phase exists because 1,000+ fills/day is an engineering throughput target as well as a trading target.
- The dry-run must prove that high event volume does not drop journal events, skip reconciliation, hide rate-limit pressure, or corrupt reports.
- Rate-limit delay must become a halt or throttle input, not just a warning.

### Verification Commands/Checks

```text
cargo test --offline live_trading_throughput
cargo test --offline live_trading_report
cargo run --offline -- --config config/default.toml live-trading-throughput-dry-run --target-daily-fills 1000
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt8-throughput-readiness.md
```

Must include:

- target daily fills,
- simulated order/fill/cancel event counts,
- max queue depth,
- max reconciliation lag,
- rate-limit and backpressure behavior,
- report-write durability,
- no-order-authority proof,
- blocker list before volume ramp.

### Exit Gate

LT8 exits only when the local high-throughput path is deterministic, auditable, and fail-closed without adding new live order authority.

### Hold Point

Mandatory. Stop after LT8 for human review.

## LT9: Selective Taker Shadow Evidence

### Objective

Evaluate selective taker behavior in shadow only, after maker evidence and throughput readiness exist.

### Entry Criteria

- LT8 complete and reviewed.
- Maker evidence is clean enough to justify comparing taker alternatives.
- Taker remains disabled in config and runtime.

### Allowed Changes

- Shadow taker evaluator.
- Fee/depth/worst-price/slippage/adverse-selection modeling.
- Paper/live and maker/taker comparison reports.
- Decision report recommending continued maker-only operation, more shadow data, or a separate one-taker-canary approval.

### Explicitly Disallowed Changes

- Live taker submission.
- Signing taker orders for submission.
- Reusing LA7 cap or artifacts.
- FOK/FAK live path.
- Marketable GTC/GTD path.
- Taker retry loop.

### Required Implementation Notes

- The default command should run shadow only:

```text
paper --shadow-live-alpha --shadow-taker
live-trading-taker-shadow --read-only
```

- Shadow decisions must separate maker opportunity, taker opportunity, fees, depth, market phase, and worst-price envelope.

### Verification Commands/Checks

```text
cargo test --offline live_taker_gate
cargo test --offline live_trading_taker_shadow
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha --shadow-taker --run-id LT9-LOCAL-SHADOW
cargo run --offline -- --config config/default.toml live-trading-taker-shadow --read-only --from <date> --to <date>
git diff --check
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt9-taker-shadow.md
```

Must include:

- shadow run IDs,
- would-take count,
- blocked reason counts,
- maker/taker comparison,
- EV after fees and buffers,
- depth and slippage analysis,
- expected contribution to daily fill count,
- explicit statement that no live taker order was signed or submitted,
- recommendation for maker-only, more shadow, or LT10 approval.

### Exit Gate

LT9 exits only when taker remains disabled and the shadow evidence supports a documented decision. If live taker is proposed, a fresh LT10 approval artifact is required.

### Hold Point

Mandatory. Stop after LT9 for human review.

## LT10: Optional One Selective Taker Canary

### Objective

Submit at most one separately approved selective taker canary only if LT9 evidence and human review justify it.

### Entry Criteria

- LT9 exits with a documented recommendation for one taker canary.
- Fresh approval artifact exists.
- Fresh cap exists and is not LA7.
- Current dry-run proves no resting remainder, no retry, exact depth, exact worst price, and immediate reconciliation.

### Allowed Changes

- One BUY FAK or equivalent no-resting-remainder canary if current official docs and SDK behavior support it.
- One fresh final-live taker cap.
- Immediate readback and reconciliation.
- Settlement follow-up.

### Explicitly Disallowed Changes

- Reusing LA7 taker cap.
- Broad taker routing.
- Marketable GTC/GTD that can rest unexpectedly.
- Batch taker orders.
- Retry after ambiguous submit.
- Taker order without fresh maker and shadow evidence binding.

### Required Implementation Notes

- LT10 must inherit the LA7 post-submit lessons:
  - cap reserved before submit,
  - approval expiry rechecked immediately before submit,
  - dry-run report and decision hashes bound in the live approval,
  - immediate authenticated readback,
  - no clean success unless post-submit reconciliation passes,
  - cap remains consumed even if post-submit evidence fails.

### Verification Commands/Checks

Dry-run:

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-taker-canary --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt10-taker-approval.md
```

Live command, only after exact approval:

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-taker-canary --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt10-taker-live-approval.md --approval-sha256 sha256:<exact-artifact-hash>
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt10-taker-canary.md
```

Must include:

- LT9 evidence binding,
- dry-run report and hash,
- live approval hash,
- cap state,
- order/trade IDs if submitted,
- post-submit reconciliation,
- fee/slippage/adverse-selection evidence,
- settlement follow-up,
- explicit stop decision.

### Exit Gate

LT10 exits only when the one taker canary is reconciled or incident-reviewed. LT10 does not approve a second taker order or broad taker usage.

### Hold Point

Mandatory. Stop after LT10 for human review.

## LT11: First Volume Ramp, 50-100 Fills/Day Candidate

### Objective

Run the first approved daily volume ramp and prove the bot can produce tens of reconciled fills/day without lifecycle ambiguity.

### Entry Criteria

- LT8 throughput readiness passed.
- LT7 maker autonomy is clean.
- LT10 taker canary is either not used or completed cleanly if it was approved.
- Approval artifact defines the daily target, max orders, max cancels, max open orders, assets, market windows, notional, loss limit, and runtime duration.

### Allowed Changes

- Approved maker quote concurrency.
- Approved order-rate and cancel-rate increase.
- Approved BTC/ETH/SOL market-window coverage.
- Selective taker only if LT10 was completed and the LT11 approval explicitly allows it.
- Daily live scale report.

### Explicitly Disallowed Changes

- Jumping directly to 1,000+ fills/day.
- Multi-wallet deployment.
- Unbounded order or cancel rates.
- Continuing after reconciliation mismatch, rate-limit freshness breach, or unexplained balance/position drift.

### Required Implementation Notes

- Target range: `50-100` matched fills/day.
- The approval may choose a smaller target if venue liquidity is thin.
- Fills must be counted from authenticated venue readback, not local intent alone.
- If the day is lifecycle-clean but below target because liquidity is insufficient, the correct decision is `HOLD: liquidity/market opportunity insufficient`, not a fake pass.

### Verification Commands/Checks

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt11-volume-approval.md
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt11-volume-approval.md --approval-sha256 sha256:<exact-artifact-hash>
cargo run -- --config <approved-final-live-config> live-trading-scale-report --from <date> --to <date>
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt11-volume-ramp.md
```

Must include:

- target fills/day,
- observed fills/day from venue readback,
- orders, cancels, replacements, and order-to-fill ratio,
- maker/taker split,
- max concurrent open orders and markets,
- rate-limit delay and backpressure,
- P&L, fees, slippage, adverse selection, and settlement follow-up,
- reconciliation mismatch count,
- incidents and halts,
- next-volume recommendation.

### Exit Gate

LT11 exits only when the 50-100 fills/day candidate is reconciled, reported, and reviewed.

### Hold Point

Mandatory. Stop after LT11 for human review.

## LT12: Intermediate Volume Ramp, 250-500 Fills/Day Candidate

### Objective

Scale the bot from tens of fills/day to hundreds of fills/day while preserving reconciliation, profitability, and halt behavior.

### Entry Criteria

- LT11 was lifecycle-clean.
- No unresolved settlement, balance, position, or rate-limit incident remains.
- The next approval artifact raises only the approved dimension or explicitly records coupled scale dimensions.

### Allowed Changes

- Higher approved daily fill target.
- Higher approved order/cancel caps.
- More concurrent maker quotes and market windows.
- Selective taker inside approved bounds if prior taker evidence is clean.
- More frequent readback/report snapshots if needed for high volume.

### Explicitly Disallowed Changes

- Skipping from failed LT11 evidence into higher volume.
- Multi-wallet deployment.
- Increasing capital and order rate together unless explicitly approved and separately measured.
- Continuing through stale readback or throttling that threatens freshness.

### Required Implementation Notes

- Target range: `250-500` matched fills/day.
- The run must prove that event volume does not hide individual order lifecycle failures.
- Daily reports must be machine-readable enough to answer which fills made or lost money after fees and settlement.

### Verification Commands/Checks

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt12-volume-approval.md
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt12-volume-approval.md --approval-sha256 sha256:<exact-artifact-hash>
cargo run -- --config <approved-final-live-config> live-trading-scale-report --from <date> --to <date>
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt12-volume-ramp.md
```

Must include the LT11 fields plus:

- comparison against LT11,
- per-asset and per-market-window fill density,
- rate-limit utilization,
- queue/backpressure maximums,
- cancel-to-fill ratio,
- realized and settlement P&L by asset,
- reason to continue, hold, or reduce scale.

### Exit Gate

LT12 exits only when the 250-500 fills/day candidate is reconciled, reported, and reviewed.

### Hold Point

Mandatory. Stop after LT12 for human review.

## LT13: Thousand-Fill Candidate, 1,000+ Fills/Day

### Objective

Run the explicit original-objective candidate: 1,000+ matched fills/day with clean reconciliation and risk evidence.

### Entry Criteria

- LT12 was lifecycle-clean and reviewed.
- Venue liquidity and rate-limit evidence support attempting the target.
- Capital, loss, exposure, market-window, order-rate, cancel-rate, and taker bounds are approved.
- Backpressure and readback lag are below approved thresholds.

### Allowed Changes

- Approved high-frequency maker quote management.
- Approved multi-market concurrency across BTC, ETH, and SOL.
- Approved selective taker usage if prior taker evidence supports it.
- Approved order/cancel caps sufficient for the 1,000+ fills/day target.
- Intraday scale report checkpoints.

### Explicitly Disallowed Changes

- Treating 1,000 fills as success if P&L, reconciliation, or incident evidence fails.
- Unbounded capital or open-order exposure.
- Ignoring cancel-to-fill or order-to-fill blowups.
- Bypassing rate-limit or geoblock restrictions.
- Multi-wallet deployment unless a separate approved design has already been completed.

### Required Implementation Notes

The LT13 target is not vague. The evidence must include:

```text
target_daily_fills>=1000
observed_daily_fills>=1000
fill_count_source=venue_readback
daily_reconciliation_mismatch_count=0
unresolved_incident_count=0
rate_limit_safety_passed=true
geoblock_safety_passed=true
secret_safety_passed=true
```

If the bot cannot safely hit 1,000+ fills/day because liquidity is insufficient, rate limits bind, or edge disappears, the correct result is a documented `NO-GO` or `HOLD`, not lowering the objective silently.

### Verification Commands/Checks

```text
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --dry-run --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt13-thousand-fill-approval.md
cargo run --features live-trading-orders -- --config <approved-final-live-config> live-trading-volume-ramp --human-approved --approval-id <approval-id> --approval-artifact verification/YYYY-MM-DD-live-trading-lt13-thousand-fill-approval.md --approval-sha256 sha256:<exact-artifact-hash>
cargo run -- --config <approved-final-live-config> live-trading-scale-report --from <date> --to <date>
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt13-thousand-fill-candidate.md
```

Must include:

- target and observed daily fills,
- venue-readback fill source,
- maker/taker split,
- order, cancel, replacement, and reject counts,
- order-to-fill and cancel-to-fill ratios,
- concurrent markets and open orders,
- capital used,
- exposure and loss-limit utilization,
- rate-limit and backpressure evidence,
- P&L after fees and settlement,
- reconciliation mismatch count,
- incidents and halt decisions,
- explicit decision whether the original 1,000+ fills/day objective is met.

### Exit Gate

LT13 exits only when the 1,000+ fills/day candidate is reconciled, reported, and reviewed.

### Hold Point

Mandatory. Stop after LT13 for human review.

## LT14: Final Production-Scale Decision Report

### Objective

Aggregate final-live evidence into a decision-grade report on whether the original 1,000+ fills/day objective has been achieved and whether a later production scope is justified.

### Allowed Changes

- Final-live scale report command.
- Evidence aggregation across LT1-LT13.
- Paper/live comparable report.
- Maker/taker split.
- Daily fill-rate report.
- P&L, fee, slippage, adverse-selection, lifecycle, rate-limit, and incident analysis.
- Recommendation for `NO-GO`, `HOLD`, or `GO`.

### Explicitly Disallowed Changes

- Any new live order authority.
- Any cap reset.
- Any taker expansion.
- Any production rollout.
- Any report logic that manually edits a decision into `GO`.

### Required Implementation Notes

Possible decisions:

```text
NO-GO: lifecycle unsafe
NO-GO: negative expectancy
NO-GO: paper/live divergence unexplained
NO-GO: 1,000 fills/day not feasible under current liquidity or rate limits
HOLD: more maker-only data required
HOLD: taker shadow only
HOLD: volume safe but edge insufficient
GO: 1,000+ fills/day objective met under approved caps
GO: propose next PRD for production operation
```

`GO: 1,000+ fills/day objective met` requires:

```text
lifecycle_unsafe=false
observed_daily_fills>=1000
fill_count_source=venue_readback
paper_live_comparable=true
post_settlement_pnl>=0 or explicitly risk-approved small loss with positive expected evidence
missing_evidence_count=0
unresolved_incident_count=0
daily_reconciliation_mismatch_count=0
rate_limit_safety_passed=true
```

### Verification Commands/Checks

```text
cargo test --offline live_trading_report
cargo run --offline -- --config config/default.toml live-trading-scale-report --from <date> --to <date>
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
scripts/verify-pr.sh
```

### Required Dated Verification Note

```text
verification/YYYY-MM-DD-live-trading-lt14-scale-decision.md
```

Must include:

- period covered,
- capital used,
- target and observed daily fills,
- orders/fills/cancels/replacements,
- maker/taker split,
- P&L and fees,
- slippage,
- adverse selection,
- paper/live comparison,
- rate-limit and backpressure evidence,
- mismatches,
- halts,
- incidents,
- missing evidence count,
- go/hold/no-go decision,
- next proposed PRD or hold decision.

### Exit Gate

LT14 exits only when a documented final production-scale decision exists.

### Hold Point

Mandatory. Any expansion after LT14 requires a new approval scope.

## Branch And PR Strategy

Recommended branch names:

```text
live-trading/lt0-scope-lock
live-trading/lt1-read-only-supervision
live-trading/lt2-gates-journal-evidence
live-trading/lt3-auth-signing-dry-run
live-trading/lt4-maker-shadow
live-trading/lt5-maker-canary
live-trading/lt6-maker-window
live-trading/lt7-maker-autonomy
live-trading/lt8-throughput-readiness
live-trading/lt9-taker-shadow
live-trading/lt10-taker-canary
live-trading/lt11-first-volume-ramp
live-trading/lt12-intermediate-volume-ramp
live-trading/lt13-thousand-fill-candidate
live-trading/lt14-scale-decision
```

Before each branch:

```text
git fetch origin main
git checkout main
git pull --ff-only origin main
git checkout -b live-trading/<phase-slug>
```

Each PR must include:

- phase name,
- exact scope,
- explicit disallowed work not done,
- verification commands and outputs,
- safety-scan summary,
- dated verification note path,
- hold-point status,
- next phase and stop point.

Do not mix docs-only approval changes with live-capable source changes in the same PR unless the phase explicitly says so.

## Minimum Interfaces

Final names can change during implementation, but the product surface must preserve these concepts.

### Final-live gate report

```text
mode
compile_time_feature_enabled
runtime_intent_enabled
approval_artifact_hash
deployment_host
geoblock_status
account_preflight_status
heartbeat_status
startup_recovery_status
journal_replay_status
reconciliation_status
risk_status
kill_switch_status
cap_status
block_reasons
```

### Final-live approval artifact

```text
approval_id
approval_expires_at_unix
approved_phase
approved_host
approved_jurisdiction
wallet_address
funder_address
signature_type
baseline_id
baseline_sha256
market_slug
condition_id
token_id
outcome
side
order_type
post_only
price
size
notional
tick_size
fee_bound
worst_price
no_trade_cutoff_unix
max_orders
max_replacements
max_duration_sec
target_daily_fills
max_daily_orders
max_daily_cancels
max_concurrent_markets
max_concurrent_open_orders
dry_run_report_path
dry_run_report_sha256
```

### Final-live lifecycle event

```text
run_id
event_id
event_type
approval_id
market_id
token_id
local_order_id
venue_order_id
trade_id
side
price
size
notional
fee
reason
timestamp
sanitized_payload_hash
```

### Final-live scale report

```text
decision
evidence_count
missing_evidence_count
unresolved_incident_count
maker_order_count
maker_fill_count
taker_order_count
taker_fill_count
target_daily_fills
observed_daily_fills
fill_count_source
cancel_count
replacement_count
order_to_fill_ratio
cancel_to_fill_ratio
max_concurrent_markets
max_concurrent_open_orders
rate_limit_safety_passed
max_rate_limit_delay_ms
fee_total
realized_pnl
settlement_pnl
paper_live_comparable
edge_at_submit
edge_at_fill
edge_decay
adverse_selection
reconciliation_mismatch_count
halt_count
recommendation_reasons
evidence_gaps
```

## Documentation References To Recheck Before Live Phases

Before LT3 and every later live-capable phase, recheck and record the current official sources for:

- CLOB host and order endpoint behavior.
- Authentication, L1/L2 signing, and required headers.
- Wallet signature types and funder/deposit-wallet requirements.
- Order types, post-only behavior, tick sizes, minimum sizes, neg-risk behavior, and GTD expiration threshold.
- Balance, allowance, open-order, trade, position, and heartbeat readback behavior.
- Fee-rate fields and fee calculation examples.
- Rate limits for market data, ledger/readback, trading, cancel, heartbeat, and related endpoints.
- Geoblock endpoint, blocked and close-only jurisdictions, and deployment-host requirements.
- Current official SDK support and dependency risk.

If docs and observed read-only behavior conflict, stop and document the conflict before coding or running a live-capable command.

## Final Acceptance Criteria

The final live-trading track is complete only when:

- every phase has a dated verification note,
- every hold point has an explicit review decision,
- all live evidence is machine-readable and separate from LA7/LA8 historical evidence,
- maker live fill evidence exists,
- lifecycle/reconciliation evidence is clean,
- paper/live comparison is tied to comparable market windows or explicitly marked not comparable,
- settlement follow-up exists for any held position,
- no unresolved incident remains,
- no consumed historical cap is reused,
- no secret value is exposed,
- geoblock/legal/access evidence is current and approved,
- LT13 either proves `observed_daily_fills>=1000` from venue readback or documents exactly why the objective remains blocked,
- the final scale report reaches `GO`, `HOLD`, or `NO-GO` from evidence rather than manual conclusion editing.

The original objective is met only if LT13 and LT14 show:

```text
observed_daily_fills>=1000
daily_reconciliation_mismatch_count=0
unresolved_incident_count=0
rate_limit_safety_passed=true
```

## Open Decisions Before LT0 Approval

- Final deployment host and jurisdiction.
- Final wallet type, signature type, and funder/deposit/proxy address.
- Final secret backend or signing service.
- Initial funding cap.
- Initial max order notional.
- Initial max live loss.
- Initial runtime duration and first daily fill target.
- First LT6 evidence asset: BTC only, or BTC/ETH/SOL with one active market at a time.
- Exact LT11 and LT12 fill thresholds before attempting LT13.
- Maximum allowed order-to-fill and cancel-to-fill ratios for 1,000+ fills/day.
- Whether LT10 remains in this plan or is deferred to a later plan amendment after LT9.
- Whether cancel-all remains entirely out of scope or gets a separately approved emergency-only proof.
- Whether `GO` requires strictly non-negative settlement P&L or permits a small bounded risk-approved loss with strong lifecycle and forward-expectancy evidence.

## Approval Record

This section is intentionally blank until reviewed.

Required approvals:

- Product/operator:
- Legal/access:
- Security/signing:
- Risk:
- Engineering:
