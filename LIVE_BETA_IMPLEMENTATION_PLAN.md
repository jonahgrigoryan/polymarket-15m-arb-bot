# Implementation Plan: Polymarket Live Beta Release Gate

Date: 2026-04-29
Branch: `live-beta/prd`
Related PRD: `LIVE_BETA_PRD.md`

## Purpose

This document decomposes the first Polymarket live beta into phase-gated implementation work after M9. It is separate from the replay/paper MVP plan in `IMPLEMENTATION_PLAN.md`.

This plan does not approve live trading. It is an implementation sequence that must be reviewed and approved before any live-beta code starts.

The first live beta remains an engineering and order-lifecycle probe. M9 proves paper/replay mechanics, RTDS Chainlink reference ingestion, natural paper fills under unchanged gates, deterministic replay, and one negative settlement-reconciled sample. M9 does not prove live profitability.

## Global Rules

- Do not begin any phase until the previous phase exit gate is complete and recorded in a dated verification note.
- Do not skip hold points. Work must stop at LB0, LB3, LB5, and LB6 for human review.
- Keep `LIVE_ORDER_PLACEMENT_ENABLED=false` by default in every phase.
- Treat `LIVE_BETA_PRD.md` as the controlling release-gate document.
- Preserve the existing paper/replay gates, EV thresholds, freshness gates, and risk controls.
- Do not bypass geoblock or compliance checks.
- Do not store secrets in repo files, config examples, logs, reports, CI, shell history, or chat.
- Do not treat any paper or beta result as profitability evidence unless a separate strategy robustness gate says so.
- Fail closed on ambiguous venue state, geoblock state, auth state, heartbeat state, balance state, or reconciliation state.
- Recheck official Polymarket docs and live read-only behavior before any phase that depends on external API behavior.

## Phase Order

| Phase | Name | Live order allowed | Mandatory hold |
| --- | --- | --- | --- |
| LB0 | Approval and scope lock | No | Yes |
| LB1 | Live-mode kill gates only | No | No |
| LB2 | Auth and secret handling | No | No |
| LB3 | Signing dry run | No | Yes |
| LB4 | Authenticated readback and account preflight | No | No |
| LB5 | Cancel readiness and rollback/runbook minimum | No new order; no live cancel proof | Yes |
| LB6 | One human-approved post-only tiny GTD canary | One order only | Yes |
| LB7 | Beta runbook, observability, rollback hardening, and handoff | No expansion by default | No |

## Required Verification Baseline

Every phase must run the checks that apply to its changed files plus:

```text
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Every phase must also run a focused safety scan over source, Cargo manifests, and config:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Expected scan results must be explained in the dated verification note. New source hits are blockers unless the current phase explicitly allows that class of code and the code is behind the required gates.

## LB0: Approval And Scope Lock

### Objective

Lock the release scope before implementation starts. Confirm that this plan, `LIVE_BETA_PRD.md`, and the M9 evidence are approved as planning artifacts only.

### Allowed Changes

- Documentation updates to `LIVE_BETA_PRD.md`, `LIVE_BETA_IMPLEMENTATION_PLAN.md`, `STATUS.md`, and dated verification notes.
- Approval record updates after human review.
- Issue or checklist creation that does not add code, secrets, or live runtime behavior.

### Explicitly Disallowed Changes

- Source code changes.
- Config changes that introduce live credentials, wallet values, API keys, signing fields, or order endpoints.
- Any authenticated client, signing path, wallet path, order path, cancel path, or trading-capable runtime path.
- Any statement that M9 or this plan authorizes live trading.

### Required Implementation Notes

- Record the final approved beta scope:
  - order-lifecycle probe by default,
  - BTC, ETH, and SOL only,
  - dedicated beta wallet,
  - tiny funding cap,
  - first-order cap,
  - post-only GTD maker-only canary,
  - one open order maximum,
  - human approval required.
- Record legal/access owner and approval status.
- Record whether strategy-selected live orders are disallowed or require additional RTDS robustness evidence.
- Record that no implementation may begin until LB0 exits.

### Verification Commands/Checks

```text
git status --short --branch
git diff --check
rg -n "LIVE_ORDER_PLACEMENT_ENABLED=false|live order|signing|wallet|API-key|authenticated" LIVE_BETA_PRD.md LIVE_BETA_IMPLEMENTATION_PLAN.md STATUS.md
test ! -e .env || git check-ignore .env
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb0-approval-scope-lock.md`

The note must include:

- approved scope,
- explicit non-authorization of live trading,
- legal/access status,
- strategy boundary,
- funding cap and first-order cap if known,
- required next phase,
- reviewer/approver identity.

### Exit Gate

LB0 exits only when the PRD and implementation plan are explicitly approved for phased implementation and `STATUS.md` points to LB1 as the next action. Approval of LB0 does not approve any order placement.

### Hold Point

Mandatory. Stop after LB0 until the human/operator approves starting LB1.

## LB1: Live-Mode Kill Gates Only, No Secrets/Auth/Order Client

### Objective

Add only fail-closed live-mode scaffolding and kill gates. No secrets, auth, signing, wallet, order client, cancel client, or authenticated CLOB client may be introduced.

### Allowed Changes

- Compile-time or runtime live-mode gate plumbing that defaults off.
- Config validation for non-secret live-mode intent fields.
- CLI guardrails that refuse live-capable modes unless all high-level gates are satisfied.
- Geoblock-required startup checks for any future trading-capable mode.
- Kill-switch state and local-only validation output.
- Tests proving live placement stays disabled.

### Explicitly Disallowed Changes

- Secret loading.
- API-key fields.
- Wallet, private-key, seed, signer, or signature-type values.
- Authenticated CLOB clients.
- Order post, cancel, or readback clients.
- Network calls to authenticated endpoints.
- Live order submission, forced paper orders, or weakened paper/risk gates.

### Required Implementation Notes

- `LIVE_ORDER_PLACEMENT_ENABLED=false` must remain the default and must be visible in validation output.
- The gate must require explicit config, explicit CLI intent, geoblock PASS, and later phase approvals before any future order placement can become reachable.
- Missing, malformed, blocked, stale, or unreachable geoblock state must fail closed.
- The code must not know how to authenticate or submit/cancel/read back orders in LB1.
- Existing paper/replay behavior must be unchanged except for read-only validation output where necessary.

### Verification Commands/Checks

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline safety
cargo test --offline compliance
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb1-kill-gates.md`

The note must include:

- exact gate defaults,
- validation output,
- geoblock fail-closed behavior,
- safety scan results,
- confirmation that no secrets/auth/signing/order/cancel/readback code exists.

### Exit Gate

LB1 exits only when live-mode scaffolding fails closed by default, all checks pass, and no authenticated or order-capable source path exists.

### Hold Point

None, unless any safety scan hit is ambiguous.

## LB2: Auth And Secret Handling, No Order Submission

### Objective

Design and implement approved secret handling and authentication preparation without adding order submission, cancel submission, or signed order posting.

### Allowed Changes

- Secret-provider abstraction after LB0/LB1 approval.
- Redaction and scrubber tests.
- Local validation that required secret names are present in the approved secret store without printing values.
- L2 credential handling only if approved by the PRD gate and only for non-order authenticated checks in later phases.
- Documentation for rotation, revocation, access control, audit logging, and deployment setup.

### Explicitly Disallowed Changes

- Secrets in repository files or config examples.
- Private key literals, API-key literals, or wallet seed material in code, docs, tests, fixtures, reports, or logs.
- Order creation, order posting, canceling, or order-client code.
- Signing order payloads.
- Authenticated network calls to order/cancel endpoints.
- Any source path that can place or cancel a live order.

### Required Implementation Notes

- Secret names may be documented; secret values must not be documented.
- Validation must prove presence and permissions without exposing values.
- Logging must redact by default and tests must cover accidental formatting paths.
- Secret access must be disabled in paper/replay unless a later approved phase explicitly requires it.
- If L2 credential derivation needs L1 material, that derivation must be separated from normal live runtime operation and audited before use.

### Verification Commands/Checks

```text
cargo test --offline secret
cargo test --offline redaction
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|/order|/orders|/cancel)" src Cargo.toml config
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb2-auth-secret-handling.md`

The note must include:

- approved secret backend,
- exact non-secret variable names or handles,
- redaction test results,
- rotation/revocation procedure,
- no-order safety scan results.

### Exit Gate

LB2 exits only when secret handling is approved, redaction is tested, no secret values exist in repo artifacts, and there is still no source path to post or cancel orders.

### Hold Point

None, unless key custody, secret storage, or redaction behavior is not approved.

## LB3: Signing Dry Run, No Network Post

### Objective

Implement and audit signed order payload construction in dry-run mode only. The dry run must never submit a network request.

### Allowed Changes

- SDK/signing decision record for `polymarket_client_sdk_v2` / `rs-clob-client-v2` or a justified minimal custom client.
- EIP-712/order payload construction behind dry-run-only gates.
- Signature-type, funder/proxy, pUSD collateral, and CLOB domain validation.
- Deterministic signing fixtures using test keys only if they cannot be confused with production secrets.
- Sanitized dry-run artifacts that prove payload shape without exposing private key material.

### Explicitly Disallowed Changes

- Network post to any order endpoint.
- Live order placement.
- Live cancel request.
- Authenticated order client.
- Production private-key material or API-key material.
- Any route from strategy output to live order submission.
- Any order type beyond the future first-order post-only GTD maker-only canary.

### Required Implementation Notes

- The selected SDK/client path must be documented with exact package/repository and version or commit.
- Signing code must be reviewed against current official Polymarket docs before use.
- Dry-run payloads must include token ID, side, price, size, order type, time-in-force, expiry, signature type, funder/proxy, and domain details.
- Dry-run output must be sanitized and clearly marked `not_submitted=true`.
- Tests must prove that dry-run signing cannot call the network.

### Verification Commands/Checks

```text
cargo test --offline signing
cargo test --offline dry_run
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(https?://|Client::new|reqwest|post\\(|\\.post\\(|/order|/orders|/cancel)" src
rg -n -i "(private[_ -]?key|seed|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" .
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb3-signing-dry-run.md`

The note must include:

- SDK/signing decision,
- audited docs and versions,
- sanitized dry-run payload evidence,
- proof that no network post occurred,
- no-secret scan results,
- reviewer signoff.

### Exit Gate

LB3 exits only when signing dry run is reviewed, deterministic, sanitized, and proven unable to submit orders.

### Hold Point

Mandatory. Stop after LB3 until the human/operator approves moving to authenticated readback and account preflight in LB4.

## LB4: Authenticated Readback And Account Preflight

### Objective

Add authenticated readback and account preflight for balances, allowances, open orders, trades, and heartbeat state without order post or cancel.

### Allowed Changes

- Authenticated read-only clients for account, balance, allowance, open-order, user-order, trade, and venue-state readback.
- Heartbeat verification and monitoring only after docs confirm the behavior is safe when there are no open orders.
- pUSD balance, available balance, reserved balance, allowance, funder/proxy, signature type, chain ID, and CLOB host preflight.
- Parser tests for documented venue states, trade statuses, transaction hash fields, and error responses.
- Metrics and logs for readback health and mismatch detection.

### Explicitly Disallowed Changes

- Order posting.
- Live cancel request.
- Cancel-all request.
- Any marketable or taker order path.
- Any order-placement route from signal/risk outputs.
- Any weakening of geoblock, freshness, EV, or risk gates.
- Any heartbeat behavior that could keep an unapproved open order alive.

### Required Implementation Notes

- Readback must fail closed on missing, malformed, stale, delayed, unmatched, failed, unknown, or inconsistent states.
- Balance calculations must include pUSD reserved by open orders.
- Allowance preflight must identify exact approved targets and amounts without changing allowances.
- Heartbeat ambiguity is a blocker for maker orders.
- If open orders already exist before the beta, the phase must stop and produce an incident/preflight note.
- Trade lifecycle readback must distinguish order status from matched trade status and transaction status.

### Verification Commands/Checks

```text
cargo test --offline readback
cargo test --offline balance
cargo test --offline allowance
cargo test --offline heartbeat
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(\\.post\\(|/order|/orders|/cancel|cancel.*order|post.*order|place.*order|create.*order)" src Cargo.toml config
rg -n -i "(private[_ -]?key|seed|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" .
```

Live checks, only after LB3 hold approval:

```text
cargo run -- --config <approved-live-beta-config> validate --live-readback-preflight
```

The approved config must reference secret handles only, not secret values.

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb4-readback-account-preflight.md`

The note must include:

- geoblock result from the deployment host,
- wallet and funder/proxy addresses,
- signature type,
- pUSD balance,
- available and reserved balance,
- open orders,
- allowances,
- trade readback status,
- heartbeat status,
- fail-closed test results,
- confirmation that no order post/cancel occurred.

### Exit Gate

LB4 exits only when authenticated readback and account preflight pass from the approved host, no unexpected open orders exist, heartbeat state is understood, and order post/cancel paths remain unreachable.

### Hold Point

None, unless any account, venue, heartbeat, geoblock, balance, allowance, or trade state is ambiguous.

## LB5: Cancel Path Readiness And Rollback/Runbook Minimum

### Objective

Build and audit cancel readiness behind gates, and complete the minimum rollback/runbook requirements before any live order is allowed. Actual live cancel proof waits for LB6 after the one tiny order exists.

### Allowed Changes

- Cancel request construction and response parsing behind disabled compile/runtime/human gates.
- Tests and fixtures for cancel success, already filled, partially filled, already canceled, missing order, auth error, rate limit, and unknown/error responses.
- Open-order readback procedure.
- Kill switch procedure.
- Service stop command.
- Cancel plan and cancel eligibility rules.
- Incident note template.
- Rollback runbook with artifact checklist.
- Metrics/logs for cancel attempts and cancel-readback mismatch.

### Explicitly Disallowed Changes

- Live order posting.
- Any live cancel request to the venue before LB6.
- Live cancel proof before LB6.
- Cancel-all against live venue unless separately approved after a live open order exists.
- Any autonomous cancel loop that can act without the approved rollback conditions.
- Any order path from strategy output.
- Any expansion beyond one future post-only GTD maker canary.

### Required Implementation Notes

- The cancel method must be unreachable unless all LB6 gates are satisfied and an approved canary order exists.
- Cancel response handling must fail closed on unknown status, partial fill ambiguity, rate limit, auth error, missing order, or readback mismatch.
- The rollback/runbook minimum must exist before LB6:
  - kill switch,
  - cancel plan,
  - service stop command,
  - open-order readback procedure,
  - incident note template,
  - balance/trade/settlement artifact checklist.
- The runbook must specify what to do if cancel fails or heartbeat becomes unhealthy.

### Verification Commands/Checks

```text
cargo test --offline cancel
cargo test --offline rollback
cargo test --offline runbook
cargo test --offline readback
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(post.*order|place.*order|create.*order|submit.*order|\\.post\\(|/orders)" src Cargo.toml config
rg -n -i "(cancel.*order|/cancel)" src Cargo.toml config
```

Any cancel-path hits must be documented as LB5-gated code and proven unreachable before LB6.

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb5-cancel-readiness-rollback.md`

The note must include:

- cancel response fixture coverage,
- proof that no live cancel proof occurred,
- kill switch command,
- service stop command,
- open-order readback procedure,
- cancel plan,
- incident note template path,
- rollback artifact checklist,
- safety scan results.

### Exit Gate

LB5 exits only when cancel readiness is tested behind gates, rollback minimums exist, and a human approves proceeding to one live canary order.

### Hold Point

Mandatory. Stop after LB5 until the human/operator approves LB6. This is the last hold before any live order can exist.

## LB6: One Human-Approved Post-Only Tiny GTD Canary Order

### Objective

Place exactly one human-approved post-only tiny GTD maker-only canary order, then immediately perform readback, heartbeat monitoring, cancel if open, trade/balance/fee reconciliation, and settlement follow-up.

### Allowed Changes

- One order-post path for the approved post-only GTD maker canary only.
- Human approval prompt and approval log.
- Immediate order readback.
- Heartbeat monitoring for the canary order.
- Cancel if the order remains open.
- Trade, balance, fee, transaction-status, and settlement reconciliation.
- Incident handling if any step fails.

### Explicitly Disallowed Changes

- More than one live order.
- Strategy-selected live order.
- Autonomous live trading.
- FOK, FAK, taker, marketable limit, or crossing orders.
- GTC first order unless GTD is unavailable and separately approved.
- Any order without explicit expiry before market end.
- Any order without geoblock PASS, account preflight PASS, heartbeat PASS, kill switch readiness, cancel readiness, and human approval.
- Any second order before the first order is fully reconciled and LB6 hold is approved.

### Required Implementation Notes

- The approval prompt must include run ID, host, geoblock result, wallet/funder, signature type, pUSD balance, reserved balance, market slug, condition ID, token ID, outcome, side, price, size, notional, order type, time-in-force, expiry, fee estimate, current book age, reference age, heartbeat state, cancel plan, and rollback command.
- The order must be non-marketable according to a fresh book snapshot.
- Size must be within the approved first-order cap and venue minimum-size rules.
- Expiry must leave enough time before market end for cancel/readback/reconciliation.
- If the order fills before cancel, reconcile trade status, transaction hash, balance delta, fees, and settlement.
- If the order remains open, cancel it and verify canceled state plus reserved-balance release.
- Any delayed, unmatched, failed, unknown, partial, inconsistent, or rate-limited state halts further orders.

### Verification Commands/Checks

Before order:

```text
cargo run -- --config <approved-live-beta-config> validate --live-readback-preflight
cargo run -- --config <approved-live-beta-config> live-canary --dry-run
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

During order:

```text
cargo run -- --config <approved-live-beta-config> live-canary --human-approved --one-order
cargo run -- --config <approved-live-beta-config> live-readback --order-id <venue_order_id>
cargo run -- --config <approved-live-beta-config> live-heartbeat-check
cargo run -- --config <approved-live-beta-config> live-cancel --order-id <venue_order_id> # only if order is still open
cargo run -- --config <approved-live-beta-config> live-readback --order-id <venue_order_id>
cargo run -- --config <approved-live-beta-config> live-trade-readback --since-run <run_id>
cargo run -- --config <approved-live-beta-config> live-balance-readback
```

Command names are placeholders until implementation. Final command names must be documented in the LB6 verification note before use.

After order:

```text
cargo run --offline -- replay --run-id <run_id>
git diff --check
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb6-one-order-canary.md`

The note must include:

- human approval log,
- exact command sequence,
- geoblock result,
- account preflight,
- market and order intent,
- venue order ID,
- order status transitions,
- heartbeat result,
- cancel request and cancel readback if open,
- trade records if matched,
- transaction hash and status if present,
- balance and fee reconciliation,
- reserved-balance release,
- settlement follow-up plan and final settlement artifact,
- incident note if any state is unreconciled.

### Exit Gate

LB6 exits only when the one canary order is fully reconciled or an incident note is approved. No second live order is allowed from LB6 evidence alone.

### Hold Point

Mandatory. Stop after LB6 for human review of the canary evidence, whether the order filled, canceled, failed, or required incident handling.

## LB7: Beta Runbook, Observability, Rollback Hardening, Incident Workflow, STATUS Handoff

### Objective

Harden the beta runbook and handoff after the canary. This phase prepares for a future beta decision but does not expand live order scope by default.

### Allowed Changes

- Runbook hardening from LB6 lessons.
- Observability dashboards, alerts, and artifact checks.
- Incident workflow refinements.
- STATUS handoff updates.
- Additional docs for go/no-go review.
- Tests for logs, metrics, halt behavior, readback mismatch handling, and rollback procedures.

### Explicitly Disallowed Changes

- Additional live orders by default.
- Strategy-selected trading unless a separate strategy robustness gate is approved.
- Taker, FOK, FAK, marketable limit, or multi-order paths.
- Raising caps or broadening assets/markets without a new approval record.
- Treating the canary as profitability evidence.

### Required Implementation Notes

- `STATUS.md` must separate:
  - M9 paper/replay PASS,
  - live beta canary evidence,
  - unresolved blockers,
  - next approval gate.
- Observability must include live mode enabled/disabled, geoblock status, kill switch state, heartbeat age/failures, order attempts, accepts/rejects, cancels, fills, readback mismatches, balance mismatches, open notional, realized P&L, and settlement P&L.
- Incident workflow must define who approves restart, what evidence is required, and what states permanently stop the beta.
- Any future expansion beyond one canary requires a new explicit go/no-go review.

### Verification Commands/Checks

```text
cargo test --offline metrics
cargo test --offline reporting
cargo test --offline rollback
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

### Required Dated Verification Note

`verification/YYYY-MM-DD-live-beta-lb7-runbook-handoff.md`

The note must include:

- final LB6 reconciliation status,
- observability coverage,
- rollback hardening,
- incident workflow,
- remaining blockers,
- explicit next gate,
- STATUS update summary.

### Exit Gate

LB7 exits only when the canary evidence is handed off cleanly, runbook/observability gaps are recorded, and the next approval gate is explicit. It does not approve expanded beta trading.

### Hold Point

None by default. Any expansion after LB7 requires a new approval record.

## Approval Record

This section is intentionally blank until review.

- LB0 approval:
  - 2026-04-29: Approved by operator for LB0 scope lock only.  The approved scope is an order-lifecycle probe on BTC/ETH/SOL with a dedicated beta wallet, tiny funding cap, first-order cap, post-only GTD maker-only canary, one open-order max, human approval required, and no autonomous trading.
  - Legal/access owner recorded as operator; legal/access compliance status remains pending and not required for LB0 completion, but required before LB4.
  - Explicitly confirmed: LB0 does NOT authorize live order placement, does not remove `LIVE_ORDER_PLACEMENT_ENABLED=false`, and does not introduce signing/wallet/API-key/authenticated CLOB behavior.
- LB0 verification note: `verification/2026-04-29-live-beta-lb0-approval-scope-lock.md`
- LB1 approval:
- LB2 approval:
- LB3 approval:
- LB4 approval:
- LB5 approval:
- LB6 approval:
- LB7 approval:
