# PRD: Polymarket Final Live Trading Scope

Date: 2026-05-12
Branch: `live-trading/prd`
Base: `main` after LA8 merge / PR #41
Status: Draft for approval
Owner: Jonah
Scope type: Post-LA8 final live-trading product scope

## Summary

This document defines the final live-trading product scope for the Polymarket 15-minute arbitrage bot.

The project has now built and exercised the core architecture needed for live operation:

- market discovery for BTC, ETH, and SOL 15-minute markets,
- CLOB market-data ingestion and book normalization,
- RTDS Chainlink reference ingestion,
- paper execution and deterministic replay,
- fee-aware signal and risk logic,
- live order journaling,
- authenticated account/readback preflights,
- live heartbeat and crash-safety gates,
- controlled live fill and maker-only canary paths,
- quote-manager cancel/replace behavior,
- selective taker gate scaffolding,
- LA8 scale-decision reporting.

The final live-trading scope is **not** a tiny canary track. It is the product boundary for moving from milestone canaries into a supervised real-money trading bot that is capable of reaching the original objective: **1,000+ matched fills per day** when evidence, liquidity, rate limits, risk, and compliance all support that scale.

The scale target is explicit, but it is still gated. The product must reach high volume by proving each expansion with machine-readable live evidence, not by skipping safety checks.

Current LA8 evidence closed as:

```text
NO-GO: lifecycle unsafe
```

That decision does not mean the architecture is unusable. LA3 and LA7 already proved that real orders can be submitted, matched, read back, and closed out under controlled approval. It means current historical evidence is not enough to justify high-volume operation. This PRD therefore defines the scope for a new final-live track whose job is to turn that proven fill capability into repeatable, reconciled, volume-capable trading evidence under a fresh approval scope.

## Product Objective

Operate the bot in real Polymarket markets with real capital, starting from controlled maker-first operation and scaling toward 1,000+ matched fills/day only when live evidence proves:

1. lifecycle safety,
2. reconciliation correctness,
3. positive or acceptable risk-adjusted expectancy,
4. operational reliability,
5. compliance eligibility from the deployment host,
6. no unresolved wallet, signing, heartbeat, or venue-state ambiguity.

The target product is a supervised autonomous trading service for BTC, ETH, and SOL 15-minute up/down markets that can run continuously during approved windows, manage its own maker quotes, use selective taker execution when separately justified, reconcile fills and balances, halt on unsafe state, and produce decision-grade reports on the path to 1,000+ fills/day.

## Non-Authorization Statement

This PRD is a scope document only.

It does not authorize:

- treating the implementation-plan draft as approval to code,
- placing new live orders,
- resetting the consumed LA7 taker cap,
- bypassing any LA8 blocker by changing report logic,
- enabling production sizing before the required scale gates pass,
- expanding beyond BTC, ETH, and SOL,
- bypassing geoblock or legal/access constraints,
- weakening feature gates, approval gates, kill switches, reconciliation, or secret handling.

Source implementation must wait until `LIVE_TRADING_PRD.md` and `LIVE_TRADING_IMPLEMENTATION_PLAN.md` are both reviewed and explicitly approved for phased work.

## Source Evidence

Repository evidence:

- `PRD.md`: original replay/paper-first product scope.
- `LIVE_BETA_PRD.md`: first live beta release gate.
- `LIVE_ALPHA_PRD.md`: controlled live fill, reconciliation, maker micro-autonomy, quote manager, selective taker, and scale-decision scope.
- `LIVE_ALPHA_IMPLEMENTATION_PLAN.md`: phase sequence through LA8.
- `STATUS.md`: current handoff state after LA8.
- `verification/2026-05-09-live-alpha-la8-scale-decision.md`: LA8 decision artifact.
- LA0-LA8 dated verification notes.

Official Polymarket docs checked on 2026-05-12:

- API overview: `https://docs.polymarket.com/api-reference/introduction`
- Trading overview: `https://docs.polymarket.com/trading/overview`
- Authentication: `https://docs.polymarket.com/api-reference/authentication`
- Order overview: `https://docs.polymarket.com/trading/orders/overview`
- Rate limits: `https://docs.polymarket.com/api-reference/rate-limits`
- Geographic restrictions: `https://docs.polymarket.com/api-reference/geoblock`

Current doc-derived assumptions to recheck before implementation:

- CLOB production host is `https://clob.polymarket.com`.
- Gamma API, Data API, and CLOB market-data endpoints are public; trading/order-management endpoints require authentication.
- CLOB auth uses L1 private-key/EIP-712 signing to create or derive L2 API credentials.
- L2 credentials use HMAC headers for trading operations, but order payloads still require EIP-712 signing.
- Current wallet signature types include EOA, proxy wallet, Gnosis Safe, and deposit-wallet / ERC-1271 flows; the selected wallet type must bind signature type and funder address exactly.
- All orders are limit-order primitives; market behavior is achieved by marketable limits.
- GTC and GTD are resting limit order types; FOK and FAK execute immediately against resting liquidity.
- Post-only orders can only be used with GTC/GTD and are rejected if marketable.
- GTD expiration has a documented security threshold.
- Tick size, minimum size, fee fields, neg-risk flags, order statuses, trade readback, and heartbeat behavior must be rechecked live before implementation.
- Orders from blocked jurisdictions are rejected; `GET https://polymarket.com/api/geoblock` must remain a startup and runtime gate.
- Rate limits are Cloudflare-throttled and must be treated as latency and safety risk, not only HTTP-error risk.

## Starting State

### Completed Foundations

The system has enough architecture to support a final live-trading track:

- read-only market discovery and feed capture,
- stateful order books,
- settlement-source-aware reference feeds,
- paper/replay reports,
- live alpha order journal and reducers,
- account baseline/readback,
- heartbeat and crash recovery,
- live canary cap sentinels,
- controlled real fills proving order/fill mechanics,
- maker micro and quote-manager policy,
- selective taker gate policy,
- scale-report aggregation.

### Current Blockers From LA8

The final live-trading track starts with these known blockers:

- paper/post-settlement sample used by LA8 is negative,
- high-volume live maker fill/P&L sample is absent,
- LA7 historical post-submit reconciliation had fail-closed mismatch/halt evidence,
- paper/live P&L is not comparable from matched market-window evidence,
- LA7 one-order taker cap remains consumed and cannot be reused,
- current `GO` cannot be obtained by rerunning the same LA8 report.

This PRD treats those as input requirements, not reasons to abandon the product.

## Goals

1. Define the final product boundary for real-money live trading.
2. Preserve every existing live safety invariant from Live Beta and Live Alpha.
3. Start with maker-first live trading, not broad taker execution.
4. Generate new clean machine-readable evidence that is separate from the historical LA7 blocked record.
5. Prove live maker fills, fee accounting, adverse selection, edge decay, balance movement, and settlement P&L.
6. Support supervised continuous operation and progressive volume ramp only after clean bounded windows pass.
7. Add selective taker execution only after separate evidence and approval.
8. Keep each future scale increase measurable, reversible, and limited to the approved dimension.
9. Make every live order, cancel, fill, trade, position, balance delta, halt, and operator approval auditable.
10. Reach a final scale decision on whether the bot can sustain 1,000+ fills/day.

## Non-Goals

This final-live PRD does not include:

- unbounded production trading without caps,
- immediate 1,000+ fills/day operation before scale gates pass,
- immediate taker expansion,
- any new asset beyond BTC, ETH, and SOL,
- multi-wallet deployment before single-wallet high-volume evidence proves it is needed and separately approved,
- cross-venue execution,
- portfolio margin, borrow, leverage, or short selling beyond normal Polymarket outcome-token inventory,
- public UI or customer-facing service,
- mobile control plane,
- any geoblock bypass,
- trading from the United States or any restricted region,
- automatic funding or withdrawal flows,
- deleting or rewriting historical LA7/LA8 evidence,
- changing recommendation logic merely to produce a favorable `GO`.

## Product Definition

The final live-trading product is a supervised Rust/Tokio service that:

1. discovers active BTC/ETH/SOL 15-minute Polymarket markets,
2. maintains fresh CLOB books and reference/predictive feeds,
3. computes fair probability and edge,
4. proposes maker-first quotes,
5. applies live-specific risk limits,
6. posts approved post-only maker orders only when all gates are green,
7. manages quote TTL, cancel, and replacement behavior,
8. reconciles venue order/trade/account state continuously,
9. halts before ambiguity becomes loss,
10. ramps order rate, market coverage, and, if justified, selective taker usage toward 1,000+ fills/day,
11. produces reports that can justify or reject each scale step.

The service must behave like a capital-controlled trading system, not a script that submits orders.

## Live Trading Modes

### Mode 1: Read-Only Live Supervision

Purpose: confirm deployment host, market data, account state, geoblock status, and observability before any order authority is enabled.

Allowed:

- market discovery,
- CLOB book reads,
- RTDS Chainlink reads,
- account/readback preflight,
- heartbeat dry-run or readback where supported,
- report generation.

Not allowed:

- order submission,
- cancel submission,
- signing an order intended for submission,
- changing cap sentinels.

### Mode 2: Maker-Only Evidence Window

Purpose: generate clean maker-first evidence under fresh approval.

Allowed:

- post-only GTC/GTD maker orders,
- one approved active order at a time at first,
- exact-order-ID cancel,
- short TTL,
- strict no-trade cutoff before market close,
- full readback after every state transition.

Not allowed:

- FAK/FOK,
- marketable taker orders,
- batch orders,
- cancel-all unless separately approved as an emergency path,
- increasing order count, size, duration, or assets beyond the approved window.

### Mode 3: Supervised Maker Autonomy

Purpose: run bounded maker-first autonomy when Mode 2 evidence is clean.

Allowed:

- strategy-selected maker quotes within approved caps,
- cancel/replace for stale or degraded quotes,
- per-market and per-asset exposure management,
- continuous reporting.

Not allowed:

- taker execution,
- multiple simultaneous markets unless approved,
- expanding dimensions together,
- continuing after any unresolved mismatch.

### Mode 4: Selective Taker Evidence

Purpose: allow tightly bounded taker execution only after maker evidence and shadow taker evidence justify it.

Allowed only with separate approval:

- FAK / no-resting-remainder taker orders,
- one-order taker cap or stricter equivalent,
- depth and worst-price bound,
- explicit paper/live and shadow/live evidence binding,
- immediate post-submit readback/reconciliation.

Not allowed:

- reusing the consumed LA7 cap,
- GTC taker orders that can rest unexpectedly,
- blind retry after submit ambiguity,
- broad taker enablement.

### Mode 5: Volume Ramp

Purpose: scale from proven maker-first operation toward high-volume trading.

Allowed:

- higher order count,
- shorter quote TTL,
- more market windows,
- multiple approved concurrent maker quotes,
- broader BTC/ETH/SOL coverage,
- selective taker only if already approved,
- continuous evidence review.

Not allowed:

- automatic scale-up,
- simultaneous uncontrolled size/rate/asset/duration/taker expansion,
- ignoring venue liquidity or rate-limit backpressure,
- continuing after unresolved lifecycle ambiguity.

### Mode 6: Thousand-Fill Candidate

Purpose: prove whether the bot can safely sustain 1,000+ matched fills/day.

Allowed:

- approved high-frequency maker quote management,
- approved multi-market concurrency across BTC, ETH, and SOL,
- selective taker execution only inside approved bounds,
- live rate-limit and backpressure adaptation,
- continuous account/readback reconciliation,
- daily scale decision reporting.

Not allowed:

- uncapped capital,
- unmanaged order churn,
- bypassing rate limits,
- batch/cancel-all expansion without separate approval,
- treating fill count as success when P&L, reconciliation, or incident evidence fails.

## Strategy Scope

The final-live strategy remains maker-first fair-value quoting on BTC/ETH/SOL 15-minute markets.

The signal engine must continue to account for:

- settlement-source reference,
- predictive feed context,
- spread,
- taker fees when taker is considered,
- maker fee assumptions,
- latency buffer,
- adverse-selection buffer,
- market phase,
- stale book/reference/predictive data,
- resolution-source mismatch,
- venue market status.

Maker quotes must be:

- post-only,
- non-marketable,
- tick-size conformant,
- min-size conformant,
- bounded by TTL,
- canceled before the no-trade window,
- reconciled before new quote expansion.

Selective taker must remain off until a later approved scope proves:

- maker evidence is positive or maker-only opportunity is insufficient but safe,
- shadow taker decisions are non-pathological,
- live lifecycle evidence has no unresolved mismatch,
- expected EV clears all costs and buffers,
- no historical cap is being reused.

High-volume operation must prove:

- venue liquidity can support the intended quote and fill rate,
- API rate limits and throttling do not break freshness or reconciliation,
- quote churn remains inside approved cancel/replacement limits,
- maker rewards, fees, adverse selection, and edge decay are measured,
- fills/day increases without unexplained balance, position, or settlement drift,
- the system can halt, cancel approved open orders, and recover without leaving ambiguous exposure.

## Compliance And Access Requirements

Before any final-live order:

- operator eligibility must be approved,
- deployment host jurisdiction must be approved,
- startup geoblock check must pass from the deployment host,
- interval geoblock check must continue passing,
- no VPN/proxy/bypass behavior may be used,
- blocked, close-only, malformed, stale, or unreachable geoblock results must fail closed,
- approval artifact must record the jurisdiction, host, and access result.

The runtime must never convert a blocked or ambiguous geoblock result into a warning-only condition.

## Wallet, Capital, And Funding Requirements

Final-live trading must use a dedicated wallet/funder pair.

Required before orders:

- approved wallet address,
- approved funder/deposit/proxy address,
- selected signature type,
- pUSD balance,
- available pUSD after reservations,
- reserved pUSD,
- open orders,
- current positions,
- required allowances,
- chain ID and CLOB host,
- maximum funded balance,
- maximum live loss,
- maximum per-order notional,
- maximum open notional,
- maximum daily notional.

Stop if:

- wallet/funder/signature type mismatches,
- balance exceeds approved cap,
- an unknown funding or withdrawal transaction appears,
- open order reservation cannot be explained,
- allowance state changes unexpectedly,
- account readback is incomplete.

## Secret Management And Signing Requirements

Final-live implementation must keep secret values out of:

- git,
- config examples,
- logs,
- reports,
- approval notes,
- shell history,
- CI output,
- chat transcripts.

Required product behavior:

- private key material must live in an approved secret backend or signing service,
- L2 credentials must be stored, rotated, and audited,
- signing payloads must be logged only in sanitized hash form,
- no hot-path unaudited third-party signing code may be introduced,
- SDK and dependency selection must be reviewed before live use,
- order signing must be testable without submitting an order,
- order submission must require both compile-time and runtime gates plus approval binding.

The preferred starting point is the official SDK path, audited for current V2 Rust support, wallet signature type, funder/deposit wallet behavior, order posting, cancel, readback, and dependency risk. A custom Rust client is allowed only if the SDK path is insufficient and the signing/auth design is reviewed.

## Runtime Gates

Every trading-capable mode must require all of:

- compile-time live-order feature,
- runtime live-trading mode,
- config scope matching the current approved mode,
- deployment geoblock PASS,
- account preflight PASS,
- heartbeat PASS when maker orders are possible,
- startup recovery PASS,
- journal replay PASS,
- risk state PASS,
- no kill switch,
- fresh market/book/reference/predictive state,
- current approval artifact hash,
- cap sentinel available,
- secret handles present without revealing values.

If any gate is missing, stale, malformed, or internally inconsistent, no new order may be submitted.

## Risk Requirements

Initial final-live defaults:

- order placement disabled by default,
- taker disabled by default,
- one active maker order at a time initially, then approved concurrent maker quotes as scale evidence expands the cap,
- BTC/ETH/SOL only,
- no final-seconds entries,
- no market with mismatched resolution source,
- no market with unknown fee/tick/min-size fields,
- no batch order path,
- no cancel-all except separately approved emergency path.

Risk limits must include:

- max notional per order,
- max open notional per market,
- max open notional per asset,
- max total open notional,
- max daily loss,
- max settlement loss,
- max order rate,
- max cancel rate,
- max replacement rate,
- max stale quote age,
- max book age,
- max reference age,
- max heartbeat age,
- max unresolved trade age,
- max reconciliation mismatch count of zero,
- max fills per day,
- max order-to-fill ratio,
- max cancel-to-fill ratio,
- max concurrent markets,
- max concurrent open orders,
- max rate-limit queueing or throttling delay.

Any breach halts new orders and writes an incident artifact.

## Reconciliation Requirements

Every live order must reconcile:

- local intent,
- risk approval,
- approval artifact,
- signed order hash,
- venue order ID,
- CLOB order status,
- user order readback,
- user trade readback,
- trade status,
- transaction hash when present,
- matched size,
- remaining size,
- fee / fee rate,
- pUSD available and reserved balance,
- outcome-token position,
- local position book,
- cancel state when canceled,
- expiry state when expired,
- final settlement state when resolved.

No next order is allowed if the previous order is not reconciled or explicitly incident-reviewed.

## Evidence Requirements For A Real `GO`

A future `GO` cannot come from rerunning the historical LA8 range.

To produce a real final-live `GO`, the system needs a new clean evidence range with:

- fresh approval scope,
- zero missing machine-readable paths,
- zero unresolved lifecycle mismatches,
- zero fail-closed post-submit halts in the evaluated range,
- maker live fills greater than zero,
- maker fill quality and adverse-selection data,
- edge at submit and edge after fill,
- edge decay after fill,
- fees and realized/settlement P&L,
- post-resolution settlement follow-up,
- comparable paper/live market-window evidence,
- no consumed-cap reuse,
- no unexplained balance, position, or reservation drift,
- no secret, geoblock, heartbeat, or signing incident,
- scale evidence showing the next requested daily fill target is feasible.

Minimum first evidence target:

```text
lifecycle_unsafe=false
maker_fill_count>0
paper_live_comparable=true
post_settlement_pnl>=0 or explicitly risk-approved small loss with positive expected evidence
missing_evidence_count=0
unresolved_incident_count=0
```

If evidence is safe but too thin, the correct decision is `HOLD`, not forced `GO`.

To claim the original high-volume objective is met, the final evidence range must also show:

```text
target_daily_fills>=1000
observed_daily_fills>=1000
fill_count_source=venue_readback
maker_taker_split_recorded=true
rate_limit_safety_passed=true
daily_reconciliation_mismatch_count=0
unresolved_incident_count=0
net_pnl_after_fees_and_settlement>=0 or explicitly approved risk-adjusted exception
```

## Scale Policy

Any future scaling must be planned and reversible. The default scale ladder should raise one dominant dimension at a time:

- order size,
- order count,
- asset count,
- runtime duration,
- taker usage,
- wallet count,
- market window count.

No scale step may combine dimensions.

An exception is allowed only when an approval artifact explicitly records the coupled dimensions and explains why they cannot be separated safely. The verification note must then prove each coupled dimension separately.

Target ladder:

```text
LT5: 1 approved maker canary
LT6: first maker fill/P&L evidence
LT7: supervised maker autonomy
LT8: rate/backpressure and high-throughput dry-run readiness
LT9: selective taker shadow evidence
LT10: optional one selective taker canary
LT11: 50-100 matched fills/day candidate
LT12: 250-500 matched fills/day candidate
LT13: 1,000+ matched fills/day candidate
LT14: production-scale decision
```

No scale step may proceed from:

- stale evidence,
- report-only evidence,
- paper-only evidence,
- unresolved live incident,
- historical cap reuse,
- manually edited report conclusions.

## Observability Requirements

The final live product must expose:

- live mode,
- live order feature status,
- kill switch status,
- geoblock status,
- account preflight status,
- heartbeat age and failures,
- order attempts,
- accepted orders,
- rejected orders,
- cancels,
- replacements,
- fills,
- maker/taker split,
- fees,
- slippage,
- edge at submit,
- edge at fill,
- adverse selection,
- realized P&L,
- settlement P&L,
- readback mismatches,
- balance mismatches,
- position mismatches,
- unknown venue states,
- incident count,
- cap state,
- fills per day,
- orders per day,
- cancels per day,
- order-to-fill ratio,
- cancel-to-fill ratio,
- rate-limit delay,
- concurrent markets,
- concurrent open orders.

Every live run must emit an immutable report bundle under `reports/`.

## Stop Conditions

The runtime must halt new orders on:

- geoblock blocked, close-only, unknown, stale, or unreachable,
- auth/signing error,
- private key or API-secret exposure risk,
- heartbeat unhealthy or ambiguous,
- market closed, delayed, cancel-only, or trading disabled,
- stale book/reference/predictive feed,
- resolution-source mismatch,
- fee/tick/min-size unknown,
- post-only rejection ambiguity,
- accepted order missing in readback,
- unexpected open order,
- unexpected fill,
- partial fill with ambiguous remainder,
- cancel failure,
- trade failed, delayed beyond threshold, or missing required status,
- balance or position drift,
- settlement discrepancy,
- rate-limit throttling that threatens freshness,
- operator kill switch,
- max loss or exposure breach,
- any parser fallback on critical venue state.

## Acceptance Criteria

The PRD is accepted when reviewers agree that it:

- names the product boundary for final live trading,
- preserves LA8 no-go evidence instead of papering it over,
- defines a maker-first path for generating clean live evidence,
- defines a gated path to 1,000+ fills/day,
- keeps selective taker behind separate evidence and approval,
- keeps all live gates fail-closed,
- keeps geoblock/legal/access checks mandatory,
- keeps wallet/secret/signing requirements explicit,
- defines evidence needed for a future real `GO`,
- makes clear that the objective is high-volume capability, not permanent tiny canaries,
- does not include source-code implementation.

## Future Implementation Plan Boundary

After this PRD is approved, use the separate implementation plan to implement the gated path toward 1,000+ fills/day.

That plan must:

- start from synced `main`,
- preserve this PRD's non-authorization boundaries,
- keep implementation phases milestone-gated,
- include scale phases that can reach 1,000+ fills/day when evidence supports it,
- include exact verification commands,
- require dated verification notes,
- include approval artifacts for live-capable steps,
- stop at every live hold point.

This PRD intentionally defines the target and gates, while the implementation plan defines the exact steps.

## Open Questions

- Final approved deployment host and jurisdiction.
- Final wallet type and signature type.
- Final secret backend or signing service.
- Initial funding cap.
- Initial max order notional.
- Initial runtime duration and first daily fill target.
- Whether first clean evidence window is maker-only BTC first or BTC/ETH/SOL with one active market at a time.
- Exact scale ladder thresholds between first maker evidence and 1,000+ fills/day.
- Maximum approved order-to-fill and cancel-to-fill ratios at high volume.
- Whether high-volume operation can be achieved with one wallet or needs a later separately approved multi-wallet design.
- Whether any taker evidence is allowed in the first final-live plan or deferred until after maker evidence.
- Whether cancel-all should remain emergency-only and disabled until separately verified.
- Whether `GO` requires strictly positive settlement P&L or permits a small bounded loss if lifecycle quality and forward expectancy are strong.

## Approval Record

This section is intentionally blank until reviewed.

Required approvals:

- Product/operator:
- Legal/access:
- Security/signing:
- Risk:
- Engineering:
