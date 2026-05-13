# 2026-05-13 Live Trading LT3 Auth, Secret Handles, And Signing Dry-Run

## Scope

LT3 adds final live-trading auth and signing dry-run support only. It validates final-live secret handle metadata, wallet/funder/signature-type binding, and a sanitized signing payload shape without generating a raw signature, auth headers, submit-ready order, order submission, cancel submission, heartbeat POST, cap write, or authenticated write client.

## Official Documentation Recheck

Rechecked on 2026-05-13:

- Polymarket Authentication: https://docs.polymarket.com/api-reference/authentication
  - CLOB auth is still split between L1 private-key signing and L2 API-key/HMAC headers.
  - Trading endpoints require `POLY_*` L2 headers.
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
- Added final-live account binding config fields under `[live_trading]`:
  - `wallet_address`
  - `funder_address`
  - `signature_type`
- Added CLI:
  - `live-trading-signing-dry-run --approval-id <LT3-id>`
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
- `auth_headers_generated=false`
- `authenticated_readback_status=not_run_local_dry_run`
- Sanitized signing payload hash: `sha256:b1248bb921d2fa352ca775a8f69b715667702a263ece9eacbd6ec823790f278c`
- Artifact hash: `sha256:46a61e1e642c97fff8bf5026b279e67fb6ad5732f83bba3faf463daf0ce0ca73`

Interpretation: with the local `.env` sourced and live trading explicitly disabled for the local `LT3-LOCAL-*` dry-run, the LT3 signing/auth dry-run gate passes while the tracked `config/default.toml` remains fail-closed. The dry-run still proves no submission, cancel, raw signature generation, or auth header generation.

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
- Supported final-live signature types: `eoa`, `poly_proxy`, `gnosis_safe`, `poly_1271`, or numeric `0` through `3`.
- Tests cover a pass-capable LT3 binding with explicit handles present and `poly_1271`, while still proving `not_submitted=true` and `network_post_enabled=false`.

## Verification Commands

| Command | Result | Notes |
| --- | --- | --- |
| `cargo run --offline -- --config config/default.toml validate --local-only` | PASS | Local-only validation passed with `live_order_placement_enabled=false`. |
| `set -a; source .env >/dev/null 2>&1; set +a; cargo run --offline -- --config config/default.toml validate --local-only --validate-secret-handles` | PASS | Existing presence-strict Live Beta handle check passed with local env loaded; values were not printed. |
| `set -a; source .env; set +a; LIVE_TRADING_ENABLED=false P15M_LIVE_TRADING_ENABLED=false cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-DRY-RUN` | PASS | Redacted LT3 local artifact generated with `status=passed`, `final_live_config_enabled=false`, `secret_handles_present=true`, `not_submitted=true`, and `network_post_enabled=false`. |
| `set -a; source .env; set +a; P15M_LIVE_TRADING_ENABLED=true cargo run --offline -- --config config/default.toml live-trading-signing-dry-run --approval-id LT3-LOCAL-READBACK-BLOCK-CHECK --output-root /tmp/lt3-readback-block-check` | PASS | Enabled final-live config generated a fail-closed artifact with `status=blocked` and `approved_authenticated_readback_not_passed`. |
| `cargo test --offline secret_handling` | PASS | 5 tests passed. |
| `set -a; source .env; set +a; cargo test --offline live_trading_signing` | PASS | 7 module tests and 3 CLI/id tests passed. |
| `cargo test --offline --quiet` | PASS | 450 lib tests, 101 bin tests, and 0 doc tests passed after this note was added. |
| `cargo clippy --offline -- -D warnings` | PASS | Passed through `scripts/verify-pr.sh` after this note was added. |
| `scripts/verify-pr.sh` | PASS | Formatting, full tests, clippy, diff whitespace, safety scan, no-secret scan, and ignored-local-secret-file checks passed. |

## Safety Scan

- No raw private keys, API secrets, passphrases, raw signatures, or auth headers are emitted by the LT3 artifact.
- `rg` scan over the LT3 artifact and new signing/config surfaces found only safe handle names and header field names.
- `src/live_trading_signing.rs` unit tests assert the module contains no request client construction, submit dispatch token, cancel dispatch token, or raw secret placeholders.
- No live order submit, cancel submit, heartbeat POST, cap sentinel write, taker expansion, production sizing, multi-wallet deployment, asset expansion, cancel-all behavior, or authenticated write client was added.

## Exit Gate

LT3 is locally implemented and verified for code review. The signing/auth local dry-run gate passes with sourced local final-live handles and env-supplied wallet/funder/signature-type binding while tracked defaults remain fail-closed. Enabled final-live configs now fail closed unless approved-host authenticated readback has passed. Stop here; do not start LT4 maker shadow work from this branch without security/signing review and operator approval.
