# Live Beta LB4 Readback Account Preflight Verification

Date: 2026-04-30
Branch: `live-beta/lb4-readback-account-preflight`
Base short commit: `664c4cb`
Phase: LB4 - Authenticated Readback And Account Preflight

## Scope

LB4 adds local readback/account-preflight scaffolding only:

- readback response parsing for balance/allowance, open orders, trades, venue state, and endpoint error responses,
- fail-closed account preflight evaluation for runtime-derived LB4 prerequisites, CLOB host, chain ID, wallet/funder consistency, pUSD balance, reserved balance from open orders, allowance, order status, trade lifecycle state, transaction hash presence, venue state, and heartbeat readiness,
- validate-mode CLI flag for local LB4 preflight output.

LB4 does not add order posting, cancel submission, cancel-all, live trading, strategy-to-live routing, wallet/private-key material, API-key values, secret values, allowance-changing code, geoblock bypass, or strategy/risk/freshness changes.

`LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.

## Approval And Prerequisite State

Operator approval to release the mandatory LB3 hold and start LB4 was recorded on 2026-04-30 for branch `live-beta/lb4-readback-account-preflight`.

Approved-host prerequisites are not yet recorded:

- Legal/access: NOT RECORDED. LB0 still records legal/access as pending formal confirmation.
- Deployment geoblock PASS: NOT RECORDED.

Because those prerequisites are missing, live-host/authenticated readback checks were NOT RUN in this phase. The local preflight intentionally fails closed with `legal_access_not_recorded` and `deployment_geoblock_not_recorded`.

The LB4 prerequisite flags are runtime-derived:

- `lb3_hold_released` from `live_beta.lb3_hold_released`,
- `legal_access_approved` from `live_beta.legal_access_approved`,
- `deployment_geoblock_passed` from the runtime geoblock gate status.

## Local Preflight Evidence

Command:

```text
cargo run --offline -- --config config/default.toml validate --local-only --live-readback-preflight
```

Result: expected fail-closed behavior.

Key output:

```text
live_order_placement_enabled=false
live_beta_gate_status=blocked
live_beta_readback_preflight_lb3_hold_released=true
live_beta_readback_preflight_legal_access_approved=false
live_beta_readback_preflight_deployment_geoblock_passed=false
live_beta_readback_preflight_status=blocked
live_beta_readback_preflight_live_network_enabled=false
live_beta_readback_preflight_block_reasons=deployment_geoblock_not_recorded,legal_access_not_recorded
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=0
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=25000000
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

The command exited nonzero by design:

```text
LB4 readback/account preflight blocked: deployment_geoblock_not_recorded,legal_access_not_recorded
```

No live network readback was attempted.

## Required Live Evidence

The following LB4 verification fields remain pending approved-host evidence:

- geoblock result from the deployment host,
- wallet address and funder/proxy address,
- signature type,
- pUSD balance,
- available and reserved balance,
- open orders,
- allowances,
- trade readback status,
- heartbeat status.

Local tests and validation use non-secret fixture addresses and synthetic readback JSON only. They are not evidence of account readiness.

## Verification

- `cargo test --offline readback`: PASS, 17 focused tests.
- `cargo test --offline balance`: PASS, 2 focused tests.
- `cargo test --offline allowance`: PASS, 2 focused tests.
- `cargo test --offline heartbeat`: PASS, 2 focused tests.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
- `cargo run --offline -- --config config/default.toml validate --local-only --live-readback-preflight`: expected fail-closed result with no live network.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 164 lib tests and 6 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- `.env` guard: PASS via `test ! -e .env || git check-ignore .env`.

## Safety And No-Secret Scan

Required source/config order-path scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
```

Result: expected hits only:

- existing paper-order and paper-cancel simulation code,
- existing LB1/LB3 live-order disabled validation output,
- LB4 read-only path constants `/balance-allowance`, `/data/orders`, `/trades`, and `/order/`,
- LB4 parser enum values for venue/order states such as canceled or cancel-only.

No order post, live cancel, cancel-all, order write client, or live-trading path was added.

Required source/config wallet/credential/signing scan:

```text
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Result: expected hits only:

- existing LB2 secret-handle and redaction code,
- existing LB3 signing dry-run module and CLI flag,
- config warning comments,
- LB4 metadata fields for wallet/funder address and signature type, with fixture values only.

No wallet implementation, private-key material, API-key value, production signing path, secret value, SDK import, or authenticated order write client was added.

Whole-repo no-secret scan:

```text
rg -n -i "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
```

Result: expected documentation and scan-command hits only, plus public fixture identifiers already documented in prior phases. No secret values were found.

Self-review correction:

- Endpoint error parsing now classifies official `{"error":"..."}` CLOB error responses into stable codes and redacts the message rather than preserving raw operator/error text. Reference: `https://docs.polymarket.com/resources/error-codes`.
- Account preflight now rejects zero wallet or funder/proxy addresses so placeholder account configuration cannot satisfy the LB4 readiness check.
- Open-order `original_size` and `size_matched` now parse as fixed-math unit strings, matching the CLOB order API example for `original_size: "100000000"` rather than scaling the values as human decimals. Reference: `https://docs.polymarket.com/api-reference/trade/get-single-order-by-id`.
- Funder consistency checks now compare EVM addresses case-insensitively for both open orders and trade readback so checksum/mixed-case configuration does not block equivalent lowercase API responses.
- EOA signature preflight now requires wallet and funder/proxy addresses to match case-insensitively so an empty account cannot pass with a deterministic EOA funder configuration mismatch. Reference: `https://docs.polymarket.com/api-reference/authentication`.
- LB4 prerequisite flags now derive from runtime config and the computed geoblock gate status instead of being hardcoded false, so an approved config plus an online geoblock PASS can pass the local LB4 preflight without a code edit.

## Exit Gate

LB4 local scaffolding status: PASS.

Full LB4 exit gate: BLOCKED. Authenticated readback and account preflight have not passed from an approved host because legal/access approval and deployment geoblock PASS are not recorded.

Do not start LB5 unless the reviewer/human explicitly resolves this blocker and approves the next phase strategy.
