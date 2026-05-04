# 2026-05-03 Live Alpha LA1 Journal Reconciliation

## Scope

LA1 builds the Live Alpha gates, inert config defaults, execution intent shape, append-only live journal, balance tracker, position book, reconciliation engine, and reconciliation-health metrics.

This phase does not authorize or add live order placement, live cancel expansion, cancel-all, taker/FOK/FAK/marketable-limit live calls, strategy-selected live orders, maker autonomy, controlled fill canaries, or resetting/bypassing the consumed LB6 one-order cap.

## Config Defaults

- `[live_alpha]` defaults to `enabled=false` and `mode="disabled"`.
- Fill canary, maker, taker, and scale output flags default to disabled.
- Live Alpha risk caps default to zero.
- Default validation prints `live_order_placement_enabled=false`.
- Default validation prints `live_alpha_compile_time_orders_enabled=false`.
- Default validation prints `live_alpha_gate_status=blocked`.
- Default validation block reasons:
  - `live_order_placement_disabled`
  - `compile_time_live_disabled`
  - `live_alpha_disabled`
  - `mode_disabled`
  - `missing_config_intent`
  - `missing_cli_intent`
  - `kill_switch_active`
  - `geoblock_unknown`
  - `account_preflight_unknown`
  - `heartbeat_unknown`
  - `reconciliation_unknown`
  - `approval_missing`
  - `phase_not_approved`

Validation run:

```text
cargo run --offline -- --config config/default.toml validate --local-only
```

Result: PASS. Latest run ID `18ac3462596503d8-ddb7-0`.

## Gate Decision Examples

Focused gate tests passed:

```text
cargo test --offline live_alpha_gate
```

Covered examples:

- default Live Alpha gate blocks;
- missing compile-time/default global placement gates block;
- modes that cannot place live orders, including `shadow`, remain blocked through `LiveAlphaMode::can_place_live_orders()`;
- live-order-capable modes require their matching enabled submode flag, including fill canary, maker micro, quote manager, and taker gate;
- reconciliation failure blocks.

## Execution Intent Shape

Focused execution-intent tests passed:

```text
cargo test --offline execution_intent
```

Coverage includes rejection when `notional` disagrees with `price * size` outside the small shape-validation tolerance and rejection when an intent price is above `1.0`.

## Journal Path And Replay

LA1 adds `src/live_order_journal.rs`.

Journal shape:

- append-only JSONL;
- `schema_version`;
- `run_id`;
- `event_id`;
- `event_type`;
- `created_at`;
- `payload`;
- `redaction_status`.

Durability behavior:

- write JSON event;
- write newline;
- flush;
- `sync_data`.

Replay/reducer behavior reconstructs:

- known intents;
- venue-known orders;
- successful known trades;
- exact trade ID to order ID mappings;
- partially filled orders;
- canceled orders;
- latest live balance snapshot;
- live positions;
- reconciliation mismatch count;
- risk halt state.

Regression coverage confirms `replay_state(run_id)` scopes reduced journal state to the requested run ID. Rejected submission events do not become venue-known orders and therefore do not create false `missing_venue_order` reconciliation state. Failed trade events also do not become venue-known order state, known-trade fill evidence, trade-order evidence, or exact trade/order mappings.

Focused journal tests passed:

```text
cargo test --offline live_order_journal
```

## Redaction Result

Journal payload redaction covers sensitive keys including private-key, secret, credential, passphrase, mnemonic, and seed-like fields.

Focused redaction checks passed:

```text
cargo test --offline redaction
```

## Mismatch Fixture Results

LA1 adds `src/live_reconciliation.rs`.

Focused reconciliation checks passed:

```text
cargo test --offline live_reconciliation
```

Mismatch fixtures halt fail-closed for:

- `unknown_open_order`;
- `missing_venue_order`;
- `unknown_venue_order_status`;
- `unexpected_fill`;
- `filled_order_without_matching_local_trade_order`;
- `unexpected_partial_fill`;
- `cancel_not_confirmed`;
- `reserved_balance_mismatch`;
- `balance_delta_mismatch`;
- `position_mismatch`;
- `missing_venue_trade`;
- `unknown_venue_trade_status`;
- `trade_status_failed`;
- `trade_order_mismatch`;
- `sdk_rust_disagreement`.

Regression coverage also confirms conditional-token balance drift is included in balance mismatch checks and Rust/SDK readback fingerprints are compared only within the same source snapshot. A local-only Rust fingerprint and venue-only SDK fingerprint do not create `sdk_rust_disagreement`.

## Focused LA1 Tests

All focused LA1 filters passed:

```text
cargo test --offline live_alpha_config
cargo test --offline live_alpha_gate
cargo test --offline --features live-alpha-orders live_alpha_gate
cargo test --offline execution_intent
cargo test --offline live_order_journal
cargo test --offline live_reconciliation
cargo test --offline live_position_book
cargo test --offline live_balance_tracker
cargo test --offline redaction
```

## Full Verification

Passed:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Full test count:

- `cargo test --offline`: 265 lib tests, 8 main tests, 0 doc tests.

## Safety And No-Secret Scans

Commands:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|/order|/orders|/cancel|FOK|FAK)" src Cargo.toml config
rg -n -i --hidden -g '!.git' -g '!target' -g '!.env' -g '!config/local.toml' "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
```

Expected hits only:

- existing LB6 gated canary `post_order` path;
- existing exact single-order cancel/readback paths;
- existing paper order/cancel simulation paths;
- existing readback/auth secret-handle names and L2 header names, not values;
- new LA1 inert config/gate/order-intent/journal/reconciliation definitions;
- new LA1 gate, reducer, and reconciliation tests for submode enablement, run-scoped journal replay, rejected submissions, failed trade evidence filtering, exact trade/order mapping, missing venue trade readback, and conditional-token balance drift;
- safety scan command text in docs and verification notes;
- public fixture IDs, public Pyth/Chainlink feed IDs, public condition/order IDs already recorded in prior evidence.

No new live order placement, live cancel expansion, cancel-all, taker/FOK/FAK/marketable-limit live call, strategy-to-live route, secret value, API-key value, seed phrase, raw L2 credential, private-key material, geoblock bypass, risk weakening, stale-data gate weakening, or approval bypass was added by LA1.

## Result

LA1 PASS for gates, journal, and reconciliation foundation only.

Expected next action after PR merge: stop and obtain explicit human/operator approval to start LA2 from fresh updated `main`.
