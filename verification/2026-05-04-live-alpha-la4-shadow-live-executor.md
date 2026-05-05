# 2026-05-04 Live Alpha LA4 Shadow Live Executor

## Scope Decision

- Branch: `live-alpha/la4-shadow-executor`
- Base: `main` at LA3 merge `7b7f952` / PR #32
- Phase: LA4 shadow live executor only
- Live order placement: NOT AUTHORIZED
- Live cancel/cancel-all/cancel-replace automation: NOT AUTHORIZED
- LA3 second canary / LA5 maker micro / taker strategy: NOT STARTED

LA4 records what the live executor would have decided for strategy intents, but paper remains the only execution path.

## Planning Sources Re-read

- `AGENTS.md`
- `STATUS.md`
- `LIVE_ALPHA_PRD.md` LA3 and LA4 sections
- `LIVE_ALPHA_IMPLEMENTATION_PLAN.md` LA3 hold point and LA4 sections
- `verification/2026-05-04-live-alpha-la3-controlled-fill-canary.md`
- `verification/2026-05-04-live-alpha-la3-approval.md`

## External Documentation Rechecked

Official Polymarket documentation was rechecked before coding LA4 shadow order semantics:

- Create orders: `https://docs.polymarket.com/trading/orders/create`
- L2 client methods: `https://docs.polymarket.com/trading/clients/l2`
- Fees: `https://docs.polymarket.com/trading/fees`
- Geoblock: `https://docs.polymarket.com/api-reference/geoblock`
- User channel: `https://docs.polymarket.com/market-data/websocket/user-channel`

Implementation consequence: LA4 does not call order, cancel, batch, or heartbeat/user WebSocket write paths. Post-only crossing is modeled as a shadow reject reason. Fee exposure is estimated only for reporting.

## Implemented Behavior

- Added `src/live_executor.rs`.
  - Defines `ExecutionSink`, `DisabledExecution`, `PaperExecution`, `ShadowLiveExecution`, and inert future `LiveMakerExecution` / `LiveTakerExecution` names.
  - `ShadowLiveExecution` consumes `ExecutionIntent` and emits `ShadowLiveDecision`.
  - `would_cancel` and `would_replace` are always false in LA4.
  - Future live maker/taker execution structs return inert decisions and do not submit.
- Wired `paper --shadow-live-alpha`.
  - Runtime still uses signal engine -> risk engine -> existing paper executor.
  - Shadow decisions are recorded after the paper path is evaluated.
  - Existing paper orders/fills/events are unchanged when shadow is enabled.
- Persisted shadow artifacts when the flag is enabled.
  - `shadow_live_decisions.jsonl`
  - `shadow_live_report.json`
  - `shadow_live_journal.jsonl`
  - optional `LiveShadowDecisionRecorded` live journal event when `live_alpha.journal_path` is configured.
- Required reason codes covered:
  - `edge_too_small`
  - `book_stale`
  - `reference_stale`
  - `market_too_close_to_close`
  - `post_only_would_cross`
  - `insufficient_pusd`
  - `insufficient_inventory_for_sell`
  - `max_open_orders_reached`
  - `max_market_notional_reached`
  - `max_asset_notional_reached`
  - `heartbeat_not_healthy`
  - `reconciliation_not_clean`
  - `geoblock_not_passed`
  - `mode_not_approved`

## Runtime Command

Command:

```bash
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha
```

Result in this Codex session:

- Status: PASS.
- Run ID: `18ac840bae3411d0-4b98-0`
- Duration: 40.47 seconds wall time.
- Session path: `reports/sessions/18ac840bae3411d0-4b98-0`
- Markets observed: 3.
  - BTC `btc-updown-15m-1777941000`
  - ETH `eth-updown-15m-1777941000`
  - SOL `sol-updown-15m-1777941000`
- Live market evidence: true.
- Normalized events: 130.
- Raw messages: 66.
- Signal evaluations: 130.
- Signal intents emitted: 0.
- Signal skip reason: `missing_reference_price` 130.
- Paper orders: 0.
- Paper fills: 0.
- Shadow decision count: 0.
- Shadow would-submit count: 0.
- Shadow would-cancel count: 0.
- Shadow would-replace count: 0.
- Shadow rejected count by reason: none.
- Paper/live intent divergence count: 0.
- Estimated fee exposure: 0.000000.
- Estimated reserved pUSD exposure: 0.000000.
- Live order placed: no.

Earlier run `18ac8234fb5c45f0-3bf3-0` failed closed before market capture under `US/CA` geoblock. The successful rerun above replaces that as the current LA4 runtime evidence.

## Test Evidence

Focused checks already run during implementation:

```bash
cargo test --offline live_executor
cargo test --offline shadow_live
cargo test --offline execution_intent
cargo test --offline live_risk_engine
cargo test --offline shadow_live_replay_records_decisions_without_changing_paper_outputs
cargo test --offline paper_shadow_live_alpha_flag_parses_without_live_order_enablement
cargo run --offline -- --config config/default.toml validate --local-only
```

Observed results:

- `live_executor`: PASS, 14 lib tests.
- `shadow_live`: PASS, 14 lib tests and 2 main tests.
- `execution_intent`: PASS, 4 lib tests.
- `live_risk_engine`: PASS, 1 lib test.
- Runtime integration regression: PASS; enabling shadow preserves generated paper orders, fills, and paper events.
- CLI flag regression: PASS; `paper --shadow-live-alpha` parses while `LIVE_ORDER_PLACEMENT_ENABLED=false`.
- `validate --local-only`: PASS, run ID `18ac8208df5aaff8-3134-0`; `live_order_placement_enabled=false`, `live_alpha_enabled=false`, `live_alpha_mode=disabled`, `live_alpha_shadow_executor_enabled=false`, `live_alpha_gate_status=blocked`.

## Final Closeout Verification

Commands run:

```bash
cargo test --offline live_executor
cargo test --offline shadow_live
cargo test --offline shadow_reason_codes_preserve_notional_risk_halt_specificity
cargo test --offline execution_intent
cargo test --offline live_risk_engine
cargo run --offline -- --config config/default.toml validate --local-only
cargo run --offline -- --config config/default.toml paper --shadow-live-alpha
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(postOrder|postOrders|createAndPostOrder|createAndPostMarketOrder|submit.*order|place.*order|cancelAll|cancel.*order|FOK|FAK|GTC|GTD|SELL)" src Cargo.toml config
rg -n -i "(private[_ -]?key|secret|api[_ -]?key|passphrase|mnemonic|seed|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE)" src Cargo.toml config runbooks verification *.md
```

Results:

- `cargo test --offline live_executor`: PASS, 14 lib tests.
- `cargo test --offline shadow_live`: PASS, 14 lib tests and 2 main tests.
- `cargo test --offline shadow_reason_codes_preserve_notional_risk_halt_specificity`: PASS, 1 lib test.
- `cargo test --offline execution_intent`: PASS, 4 lib tests.
- `cargo test --offline live_risk_engine`: PASS, 1 lib test.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS, run ID `18ac8208df5aaff8-3134-0`.
- `cargo run --offline -- --config config/default.toml paper --shadow-live-alpha`: PASS, run ID `18ac840bae3411d0-4b98-0`; 3 markets observed, 0 paper fills, 0 shadow decisions, 0 would-submit, 0 would-cancel, 0 rejected, no live order.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 323 lib tests, 24 main tests, 0 doc tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- Order/cancel scan: completed with expected historical hits from config, paper simulation, LA3 fill canary code, prior live-beta canary/cancel modules, docs/status text, and LA4 inert shadow `GTD`/`SELL` modeling. No new LA4 live submit, live cancel, cancel-all, cancel/replace, FOK/FAK, or strategy-to-live order route was added.
- Sensitive/no-secret scan: completed with expected docs/status/runbook/verification hits and non-secret handle-name text from earlier phases. No private key, seed phrase, raw L2 credential, API-key value, passphrase value, signed payload, or secret value was added.

## Live Risk Decision Examples

Unit-level examples:

- Clean approved context -> `would_submit=true`, no reason codes, `would_cancel=false`, `would_replace=false`.
- Stale book/reference -> `book_stale`, `reference_stale`, `would_submit=false`.
- Post-only crossing -> `post_only_would_cross`, `would_submit=false`.
- Heartbeat/reconciliation/geoblock/mode failures -> `heartbeat_not_healthy`, `reconciliation_not_clean`, `geoblock_not_passed`, `mode_not_approved`.
- Risk engine mapping -> stale book and max market notional risk rejections map into shadow reason codes.
- Notional risk halt mapping -> per-asset, total-live, and correlated-notional halts map to distinct shadow reason codes.

## Review Fixes Applied

- Paper/live divergence now compares paper order count to shadow would-submit count, not paper fills. This avoids falsely flagging normal unfilled maker orders as divergence.
- Shadow balance/risk context now carries reserved pUSD, max available pUSD usage, max reserved pUSD, single-order notional, and total live notional limits.
- Runtime shadow replay has explicit readiness input instead of hard-coding geoblock/heartbeat/reconciliation inside replay. The default paper command remains fail-closed for heartbeat and reconciliation because no live heartbeat/reconciliation evidence is collected in LA4.
- A regression test proves replay can produce `would_submit=true` when shadow mode, live readiness, risk, book, reference, balance, and notional context are all approved.
- Every persisted shadow run writes a session-local `shadow_live_journal.jsonl`, even when the optional global live journal path is not configured.
- PR #33 CodeRabbit P2 fix: `RiskHaltReason::MaxTotalNotional` now maps to `max_total_live_notional_reached` and `RiskHaltReason::MaxCorrelatedNotional` maps to `max_correlated_notional_reached` instead of collapsing into `max_asset_notional_reached`. Regression coverage: `shadow_reason_codes_preserve_notional_risk_halt_specificity`.

## Safety Notes

- No live order was placed.
- No live cancel was sent.
- No cancel-all path was added.
- No heartbeat network POST was enabled.
- No user-channel network write/subscription was enabled.
- LA3/LB6 consumed caps were not reset or bypassed.
- LA5 was not started.
