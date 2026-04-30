# Live Beta LB2 Auth And Secret Handling Verification

Date: 2026-04-29
Branch: `live-beta/lb2-auth-secrets`
Base short commit: `b839ffc`
Phase: LB2 - Auth And Secret Handling, No Order Submission

## Scope

LB2 adds secret-handling scaffolding only:

- environment-variable handle metadata,
- handle-name validation,
- presence checks that report names and booleans only,
- redaction utilities,
- rotation/revocation/access-control documentation.

LB2 does not add live order placement, signing, signed payload construction, wallet key material, API-key values, authenticated CLOB clients, order posting, canceling, readback clients, or a trading-capable network path.

`LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.

## Approved Secret Backend

Approved LB2 backend: environment-variable handles managed outside the repository.

Repository config stores handle names only:

- `P15M_LIVE_BETA_CLOB_L2_ACCESS`
- `P15M_LIVE_BETA_CLOB_L2_CREDENTIAL`
- `P15M_LIVE_BETA_CLOB_L2_PASSPHRASE`

The repository does not store credential values. Paper and replay modes remain usable without these handles.

Operational notes for rotation, revocation, access control, audit logging, and deployment setup are documented in `docs/live-beta-lb2-secret-handling.md`.

## Implementation Summary

- `src/secret_handling.rs` adds metadata-only secret inventory, env-handle validation, presence checks, and redaction helpers.
- `src/config.rs` adds `[live_beta.secret_handles]` config with `env` backend and handle-name validation. Invalid handles fail without echoing the invalid value.
- `src/main.rs` prints LB2 secret backend/count/`values_loaded=false` during local validation and adds an explicit `--validate-secret-handles` check for approved-host use.
- Config examples include handle names only, not values.

## Verification

- `cargo test --offline secret`: PASS, 8 focused tests.
- `cargo test --offline redaction`: PASS, 2 focused tests.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
  - `live_order_placement_enabled=false`
  - `live_beta_secret_backend=env`
  - `live_beta_secret_handle_count=3`
  - `live_beta_secret_values_loaded=false`
  - `live_beta_gate_status=blocked`
- Missing-handle presence check: PASS as expected fail-closed behavior.
  - Run with all three approved handles unset and `--validate-secret-handles`.
  - Exit code was non-zero.
  - Output printed only handle labels, backend, handle names, and `present=false`.
  - No values were printed.
- Secretless deterministic paper/replay smoke: PASS.
  - Run ID: `lb2-secretless-fixture-20260429a`
  - Paper command ran with all three handles unset.
  - Replay command ran with all three handles unset.
  - Paper orders/fills: `1 / 1`
  - Paper total P&L: `-0.250000`
  - Paper event fingerprint: `sha256:5100fdb817c179770ca91b5691cb36813c0333c7e712dc41b023ac7143a0cbfb`
  - Replay fingerprint: `sha256:317adb0ffa1fd61270e7e4b4eb22ed18c7718903360d34337af3fb478f1fe918`
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 142 lib tests and 5 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.

Post-review hardening:

- Redaction now covers quoted environment-assignment values with whitespace.
- Config validation now rejects duplicate LB2 secret-handle names without echoing the duplicated handle value.
- Additional secretless deterministic paper/replay check passed after hardening:
  - Run ID: `lb2-secretless-fixture-verify-20260429a`
  - Paper orders/fills: `1 / 1`
  - Paper total P&L: `-0.250000`
  - Replay generated/recorded paper events: `2 / 2`
  - Replay fingerprint: `sha256:9e8393789d582bb13144b03376d0bbfe97988a2437e0abc760895a69a0698b4c`

## Safety And No-Secret Scan

Required no-secret scan found only existing documented scan-command text in verification/plan files. No secret values were found.

Stronger no-secret scan also matched:

- existing public Pyth price IDs in source/config/docs,
- existing documentation warning text about forbidden key/API/wallet material,
- the new LB2 handle names, which are non-secret handles.

Required no-order scan over `src`, `Cargo.toml`, and `config` found only existing paper-order/paper-cancel lifecycle/reporting paths. No live order placement, order post, authenticated order client, cancel endpoint, readback client, or trading-capable path was added.

The broader safety scan over `src`, `Cargo.toml`, and `config` found:

- existing config comments warning against credentials,
- new LB2 secret-handle metadata and redaction code,
- no wallet implementation,
- no signing implementation,
- no API-key value,
- no authenticated CLOB client,
- no live trading path.

## Exit Gate

LB2 exit gate: PASS for auth/secret-handling scaffolding only.

Next phase is LB3 signing dry run, no network post. LB3 must not start without explicit operator request and must still avoid order posting, canceling, readback, or live trading.
