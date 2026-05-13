# 2026-05-13 Live Trading LT3 Auth, Secret Handles, And Signing Dry-Run

## Scope

LT3 adds final live-trading auth and signing dry-run support only. It validates final-live secret handle metadata, wallet/funder/signature-type binding, and a sanitized signing payload shape without generating a raw signature, auth headers, submit-ready order, order submission, cancel submission, heartbeat POST, cap write, or authenticated write client.

## Official Documentation Recheck

Rechecked on 2026-05-13:

- Polymarket Authentication: https://docs.polymarket.com/api-reference/authentication
  - CLOB auth is still split between L1 private-key signing and L2 API-key/HMAC headers.
  - Trading endpoints require `POLY_*` L2 headers.
  - L2 authentication is used for balance/allowance checks and user open-order reads.
  - Orders still require a signed order payload even when L2 headers are present.
  - Signature types now include `0` EOA, `1` POLY_PROXY, `2` GNOSIS_SAFE, and `3` POLY_1271 deposit wallet.
- Polymarket Create Order: https://docs.polymarket.com/trading/orders/create
  - Order flow still separates local create/sign from CLOB submission.
  - Post-only applies only to GTC/GTD and must reject crossing orders.
  - GTD keeps the one-minute security threshold.
- Polymarket Post a new order API: https://docs.polymarket.com/api-reference/trade/post-a-new-order
  - The live write endpoint remains `POST /order`.
  - The LT3 implementation does not call this endpoint and does not contain a network submit path.

## Implementation Summary

- Added `src/live_trading_signing.rs`.
- Added final-live secret handle config under `[live_trading.secret_handles]`:
  - backend: `env`
  - `P15M_LIVE_TRADING_CLOB_L2_ACCESS`
  - `P15M_LIVE_TRADING_CLOB_L2_CREDENTIAL`
  - `P15M_LIVE_TRADING_CLOB_L2_PASSPHRASE`
  - `P15M_LIVE_TRADING_SIGNER_PRIVATE_KEY`
- Added final-live gate/account binding config fields under `[live_trading]`:
  - `legal_access_approved`
  - `wallet_address`
  - `funder_address`
  - `signature_type`
- Added CLI:
  - `live-trading-signing-dry-run --approval-id <LT3-id>`
- Added approved-host authenticated readback wiring for enabled final-live configs. The command checks exact approved host/country/region scope, explicit final-live legal/access approval, and local account-binding validity before using final-live L2 handle values for read-only account readback. The artifact can pass with `final_live_config_enabled=true` only when that readback returns `authenticated_readback_status=passed`.
- Added redacted artifacts:
  - `artifacts/live_trading/LT3-LOCAL-DRY-RUN/signing_dry_run.redacted.json`
  - `artifacts/live_trading/LT3-LOCAL-DRY-RUN/signing_payload_shape.redacted.json`

## LT3 Local Dry-Run Result

Command:

```text
set -a; source .env; set +a; LIVE_TRADING_ENABLED=false P15M_LIVE_TRADING_ENABLED=false cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-DRY-RUN
```

Result:

- Command status: PASS
- Artifact status: `passed`
- Block reasons: none
- `final_live_config_enabled=false`
- `not_submitted=true`
- `network_post_enabled=false`
- `network_cancel_enabled=false`
- `raw_signature_generated=false`
- `order_submit_auth_headers_generated=false`
- `readback_auth_headers_generated=false`
- `authenticated_readback_status=not_run_local_dry_run`
- Sanitized signing payload hash: `sha256:b1248bb921d2fa352ca775a8f69b715667702a263ece9eacbd6ec823790f278c`
- Artifact hash: `sha256:2b020782bd6b93ed919c37dfe258a64bee4d6ceff4828e938a012c815ef946ab`

Interpretation: with the local `.env` sourced and live trading explicitly disabled for the local `LT3-LOCAL-*` dry-run, the LT3 signing/auth dry-run gate passes while the tracked `config/default.toml` remains fail-closed. The dry-run still proves no submission, cancel, raw signature generation, order-submit auth header generation, or readback auth header generation in local no-readback mode.

## Enabled-Config Readback Block Check

Command:

```text
set -a; source .env; set +a; P15M_LIVE_TRADING_ENABLED=true cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-READBACK-BLOCK-CHECK --output-root /tmp/lt3-readback-block-check
```

Result:

- Command status: PASS
- Artifact status: `blocked`
- Block reasons: `approved_authenticated_readback_not_passed`
- `final_live_config_enabled=true`
- `not_submitted=true`
- `network_post_enabled=false`
- `authenticated_readback_status=not_run_no_approved_host_readback_in_lt3_local_dry_run`

Interpretation: an enabled final-live config can no longer emit a passing LT3 signing dry-run artifact unless approved-host authenticated readback has passed.

No approved-host live-network readback was run from this local branch context because the tracked default config has no approved host/country/region and `live_trading.legal_access_approved` defaults to `false`. The code path is wired for the post-approval command in `LIVE_TRADING_IMPLEMENTATION_PLAN.md` and remains fail-closed outside exact approved scope plus explicit final-live legal/access approval.

Approved-host readback audit note: LT3 schema `lt3.live_trading_signing_dry_run.v2` separates `order_submit_auth_headers_generated` from `readback_auth_headers_generated`. A post-approval readback may set `readback_auth_headers_generated=true` because read-only authenticated CLOB GETs require L2 headers; this does not imply order submission, raw signature generation, or order-submit auth header generation.

Approved-host identity note: LT3 approved-host checks use `libc::gethostname` for kernel-reported host identity and do not read `HOSTNAME`/`HOST` or invoke PATH-resolved `hostname`/`uname` binaries before unblocking authenticated readback.

## Secret Handling

- Secret backend: `env`
- The implementation records handle names and boolean presence only.
- The implementation never loads or serializes secret values for the LT3 artifact.
- Missing, duplicate, or value-like final-live secret handles fail before producing pass status.
- Existing Live Beta secret presence validation was run with the local secret environment loaded; output contained handle names and presence booleans only.
- Fresh `.env` sourced re-check on 2026-05-13 found the exact LT3 final-live handles present:
  - `P15M_LIVE_TRADING_CLOB_L2_ACCESS`
  - `P15M_LIVE_TRADING_CLOB_L2_CREDENTIAL`
  - `P15M_LIVE_TRADING_CLOB_L2_PASSPHRASE`
  - `P15M_LIVE_TRADING_SIGNER_PRIVATE_KEY`
- This means the LT3 final-live secret handle presence check passes after sourcing `.env`.

## Wallet/Funder/Signature-Type Summary

- Default config wallet address: empty/fail-closed.
- Default config funder address: empty/fail-closed.
- Default config signature type: empty/fail-closed.
- Local `.env` override wallet/funder/signature-type binding: present and consumed by the LT3 dry-run.
- Local LT3 preflight rejects invalid or zero wallet/funder EVM addresses before any authenticated readback endpoint calls. EOA mode also requires wallet and funder to match before readback.
- Supported final-live signature types: `eoa`, `poly_proxy`, `gnosis_safe`, `poly_1271`, or numeric `0` through `3`.
- `poly_1271` support is scoped to final-live LT3 readback only; the shared legacy Live Beta/Live Alpha `SignatureType::from_config` parser does not accept `poly_1271`, so existing submit helpers cannot receive it through the LB4 account path.
- Tests cover a pass-capable LT3 binding with explicit handles present and `poly_1271`, while still proving `not_submitted=true` and `network_post_enabled=false`.

## Verification Commands

| Command | Result | Notes |
| --- | --- | --- |
| `cargo run --offline -- --config config/default.toml validate --local-only` | PASS | Local-only validation passed with `live_order_placement_enabled=false`. |
| `set -a; source .env >/dev/null 2>&1; set +a; cargo run --offline -- --config config/default.toml validate --local-only --validate-secret-handles` | PASS | Existing presence-strict Live Beta handle check passed with local env loaded; values were not printed. |
| `set -a; source .env; set +a; LIVE_TRADING_ENABLED=false P15M_LIVE_TRADING_ENABLED=false cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-DRY-RUN` | PASS | Redacted LT3 local artifact generated with `status=passed`, `final_live_config_enabled=false`, `secret_handles_present=true`, `not_submitted=true`, `network_post_enabled=false`, `order_submit_auth_headers_generated=false`, and `readback_auth_headers_generated=false`. |
| `set -a; source .env; set +a; P15M_LIVE_TRADING_ENABLED=true cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-READBACK-BLOCK-CHECK --output-root /tmp/lt3-readback-block-check` | PASS | Enabled final-live config generated a fail-closed artifact with `status=blocked` and `approved_authenticated_readback_not_passed`. |
| `cargo test --offline secret_handling` | PASS | 5 tests passed. |
| `cargo test --offline live_trading_signing` | PASS | 8 module tests and 8 CLI/id/readback-gate tests passed, including final-live legal gate, invalid account-binding pre-readback blockers, and split order-submit vs readback auth-header audit fields. |
| `cargo test --offline live_trading_deployment_host_identity` | PASS | 2 main tests passed, including a PATH-spoof regression with fake `hostname`/`uname` binaries. |
| `cargo test --offline live_trading_env_overrides_bind_local_account_without_committing_defaults` | PASS | Confirms env overrides bind final-live account config and explicit final-live legal/access approval without committing local values. |
| `cargo test --offline live_trading_readback_prerequisites_use_final_live_legal_gate` | PASS | Confirms LT3 authenticated readback prerequisites source legal/access from `live_trading.legal_access_approved` instead of a hard-coded pass. |
| `cargo test --offline balance_allowance_signature_type_params_match_official_v2_client` | PASS | Confirms legacy `SignatureType::from_config("poly_1271")` is rejected while final-live readback can still use balance-allowance query param `3`. |
| `cargo test --offline --quiet` | PASS | 451 lib tests, 109 bin tests, and 0 doc tests passed after the legal/access, host-identity, and pre-readback account-binding fixes. |
| `cargo clippy --offline -- -D warnings` | PASS | Passed through `scripts/verify-pr.sh` after this note was added. |
| `scripts/verify-pr.sh` | PASS | Formatting, full tests, clippy, diff whitespace, safety scan, no-secret scan, and ignored-local-secret-file checks passed. |

## Safety Scan

- No raw private keys, API secrets, passphrases, raw signatures, or auth header values are emitted by the LT3 artifact.
- `rg` scan over the LT3 artifact and new signing/config surfaces found only safe handle names and header field names.
- `src/live_trading_signing.rs` unit tests assert the module contains no request client construction, submit dispatch token, cancel dispatch token, or raw secret placeholders.
- The artifact keeps `order_submit_auth_headers_generated=false` as a no-submit invariant while recording whether read-only authenticated readback generated L2 headers.
- Approved-host identity uses a syscall-backed hostname source rather than PATH-resolved executables; the spoofed-PATH regression test proves fake `hostname`/`uname` binaries do not affect the gate.
- Approved-host readback now still fails closed unless `live_trading.legal_access_approved=true`, and invalid wallet/funder strings are rejected before authenticated L2 GETs.
- No live order submit, cancel submit, heartbeat POST, cap sentinel write, taker expansion, production sizing, multi-wallet deployment, asset expansion, cancel-all behavior, or authenticated write client was added.

## Exit Gate

LT3 is locally implemented and verified for code review. The signing/auth local dry-run gate passes with sourced local final-live handles and env-supplied wallet/funder/signature-type binding while tracked defaults remain fail-closed. Enabled final-live configs now fail closed unless exact approved-host scope, explicit final-live legal/access approval, valid final-live account binding, and authenticated readback all pass. Stop here; do not start LT4 maker shadow work from this branch without security/signing review and operator approval.
