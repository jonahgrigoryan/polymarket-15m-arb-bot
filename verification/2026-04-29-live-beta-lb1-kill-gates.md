# 2026-04-29 LB1 Live-Mode Kill Gates

Date: 2026-04-29
Branch: `live-beta/lb1-kill-gates`
Base short commit: `eb62868`

## Scope

LB1 adds fail-closed live-mode scaffolding and kill gates only.

No secrets, auth, signing, wallet/key handling, API-key handling, authenticated CLOB clients, order posting, canceling, readback clients, or live trading paths were added.

## Implemented Gate Defaults

Compile-time default:

```text
LIVE_ORDER_PLACEMENT_ENABLED=false
```

Config defaults:

```text
[live_beta]
intent_enabled = false
kill_switch_active = true
```

Validate-time defaults from `cargo run --offline -- --config config/default.toml validate --local-only`:

```text
live_order_placement_enabled=false
live_beta_config_intent_enabled=false
live_beta_cli_intent_enabled=false
live_beta_kill_switch_active=true
online_validation_status=skipped
live_beta_geoblock_gate=unknown
live_beta_gate_status=blocked
live_beta_gate_block_reasons=live_order_placement_disabled,missing_config_intent,missing_cli_intent,kill_switch_active,geoblock_unknown,later_phase_approvals_missing
```

## Fail-Closed Behavior

The LB1 gate requires all of the following before any future live-capable mode could pass:

- compile-time placement enabled,
- explicit config intent,
- explicit CLI intent,
- kill switch inactive,
- geoblock PASS,
- later phase approvals complete.

LB1 intentionally cannot pass the gate because `LIVE_ORDER_PLACEMENT_ENABLED=false` and later phase approvals are incomplete.

Explicit future intent check:

```text
cargo run --offline -- --config config/default.toml validate --local-only --live-beta-intent
```

Result:

```text
exit_code=1
live_beta_cli_intent_enabled=true
live_beta_geoblock_gate=unknown
live_beta_gate_status=blocked
live_beta_gate_block_reasons=live_order_placement_disabled,missing_config_intent,kill_switch_active,geoblock_unknown,later_phase_approvals_missing
error: LB1 live-mode gate refused future live intent: live_order_placement_disabled,missing_config_intent,kill_switch_active,geoblock_unknown,later_phase_approvals_missing
```

Geoblock handling:

- `GeoblockGateStatus::Blocked` blocks the gate.
- `GeoblockGateStatus::Unknown` blocks the gate.
- `validate --local-only` maps geoblock to `unknown`, so future live intent fails closed without an online geoblock PASS.
- Online geoblock request, HTTP-status, and decode errors remain command errors in the existing compliance path, so malformed or unreachable geoblock state fails closed.

## Tests

Required checks:

```text
cargo run --offline -- --config config/default.toml validate --local-only
cargo test --offline safety
cargo test --offline compliance
cargo fmt --check
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
```

Results:

- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
- `cargo test --offline safety`: PASS, 4 tests.
- `cargo test --offline compliance`: PASS, 3 tests.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 134 lib tests and 5 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.

Focused LB1 tests prove:

- live order placement remains disabled by default,
- config and CLI intent are both required,
- blocked or unknown geoblock state blocks the future live gate,
- the LB1 gate remains closed even when visible inputs are favorable because compile-time placement remains disabled.

## Safety Scan

Commands:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Result: PASS.

Observed hits are expected and non-live:

- existing paper-only event and paper executor cancellation names,
- existing `LIVE_ORDER_PLACEMENT_ENABLED=false` output and LB1 blocked-reason text,
- existing config comments that warn not to add private keys, credentials, signing, wallet, API-key, or live-trading credentials.

No source path exists for live order placement, signing, wallet/key handling, API-key handling, authenticated CLOB clients, order posting, canceling, readback clients, or live trading.

## Exit Gate

LB1 exit gate: PASS.

LB2 is the next planned phase, but LB2 has not started. LB2 must remain limited to auth and secret-handling design/implementation with no order submission, no cancel path, no signed order post, and no live trading.
