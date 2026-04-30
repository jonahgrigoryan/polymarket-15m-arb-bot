# Live Beta LB3 Signing Dry Run Verification

Date: 2026-04-30
Branch: `live-beta/lb3-signing-dry-run`
Base short commit: `9a5c783`
Phase: LB3 - Signing Dry Run, No Network Post

## Scope

LB3 adds a local signing dry-run artifact builder only:

- V2 EIP-712 domain draft,
- GTD maker-only/post-only wire-shape draft,
- signature type validation,
- explicit funder/proxy field plus funder/proxy address consistency validation,
- pUSD-style six-decimal amount conversion,
- deterministic sanitized artifact fingerprint,
- validate-mode CLI flag for local dry-run output.

LB3 does not add live order placement, network order submission, live cancel, authenticated readback, authenticated CLOB clients, production signing, wallet key material, API-key values, secret values, geoblock bypass, or strategy/risk/freshness changes.

`LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.

## SDK And Signing Decision

Decision: do not import `polymarket_client_sdk_v2` or `rs-clob-client-v2` in LB3.

Those official Rust paths remain the SDKs to audit before any real signing or authenticated client path. They are not added in LB3 because importing an SDK with order, cancel, readback, or authenticated client surfaces would exceed the LB3 dry-run-only boundary.

LB3 uses `src/live_beta_signing.rs` as a minimal custom V2 payload draft builder. It does not create a cryptographic signature. The signature field is a redacted dry-run placeholder, and the owner field is redacted. Real signing remains blocked until a later reviewed implementation can use the approved secret backend without exposing credential values.

References checked:

- `https://docs.polymarket.com/api-reference/introduction`
- `https://docs.polymarket.com/developers/CLOB/clients`
- `https://docs.polymarket.com/v2-migration`
- `https://docs.polymarket.com/developers/CLOB/orders/orders`

Read-only endpoint recheck:

- `curl -sS -D - https://clob.polymarket.com/ok -o /tmp/p15m-lb3-clob-ok.txt`: `HTTP/2 200`, body `"OK"`.
- `curl -sS -D - https://clob-v2.polymarket.com/ok -o /tmp/p15m-lb3-clob-v2-ok.txt`: `HTTP/2 301`, `location: https://clob.polymarket.com/ok`.

Config cleanup:

- `config/default.toml`, `config/example.local.toml`, and `config/pyth-proxy.example.toml` now use `https://clob.polymarket.com`, matching the already-recorded post-cutover endpoint evidence.
- `config/polymarket-rtds-chainlink.example.toml` already used `https://clob.polymarket.com`.

## Dry-Run Evidence

Command:

```text
cargo run --offline -- --config config/default.toml validate --local-only --live-beta-signing-dry-run
```

Result: PASS.

Key output:

```text
live_order_placement_enabled=false
live_beta_gate_status=blocked
live_beta_gate_block_reasons=live_order_placement_disabled,missing_config_intent,missing_cli_intent,kill_switch_active,geoblock_unknown,later_phase_approvals_missing
live_beta_signing_dry_run_status=ok
live_beta_signing_dry_run_not_submitted=true
live_beta_signing_dry_run_network_post_enabled=false
live_beta_signing_dry_run_fingerprint=sha256:649e44a4913f5e58ad60147932c253eab0cf35e93f12c44631d2ec9ec2744d3c
```

Sanitized artifact summary:

- `sdk_decision=minimal_custom_v2_payload_builder_no_sdk_import`
- `clob_host=https://clob.polymarket.com`
- `order_type=GTD`
- `post_only=true`
- `not_submitted=true`
- `network_post_enabled=false`
- `dry_run_only=true`
- explicit funder/proxy fixture address
- domain name `Polymarket CTF Exchange`
- domain version `2`
- chain ID `137`
- verifying contract `0xE111180000d2663C0091e4f400237545B87B996B`
- owner redacted
- signature redacted
- metadata and builder set to zero bytes32 placeholders

## Verification

- `cargo test --offline safety`: PASS, 4 focused tests.
- `cargo test --offline compliance`: PASS, 3 focused tests.
- `cargo test --offline secret`: PASS, 8 focused tests.
- `cargo test --offline redaction`: PASS, 2 focused tests.
- `cargo test --offline signing`: PASS, 5 focused tests.
- `cargo test --offline dry_run`: PASS, 5 focused tests.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
- `cargo run --offline -- --config config/default.toml validate --local-only --live-beta-signing-dry-run`: PASS.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 147 lib tests and 5 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- `.env` guard: PASS via `test ! -e .env || git check-ignore .env`.

## Safety And No-Secret Scan

Independent safety review:

- A separate read-only reviewer found no LB3 safety/scope blocker in the executable/config diff.
- The reviewer confirmed the dry-run path is validate-only, sets `not_submitted=true`, sets `network_post_enabled=false`, and uses redacted owner/signature placeholders.
- Self-review found the dry-run artifact needed an explicit funder/proxy field to match the LB3 implementation plan, and `STATUS.md` needed to avoid describing the branch as being at the base commit. Both were corrected before final verification.

Required source/config order-path scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|order client|clob.*order|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config
```

Result: expected existing hits only:

- live-order disabled validation output,
- paper-order and paper-cancel event/lifecycle/reporting paths,
- config comments warning against credentials.

No live order placement, order submission, live cancel, authenticated readback, authenticated CLOB client, or live-trading path was added.

Required source/config wallet/credential/signing scan:

```text
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Result: expected hits:

- existing LB2 secret-handle and redaction code,
- config warning comments,
- new LB3 `live_beta_signing` dry-run module and CLI flag.

No wallet implementation, key material, API-key value, SDK import, authenticated client, or real signer was added.

Whole-repo no-secret scan:

```text
rg -n -i "(POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|private[_ -]?key|seed phrase|mnemonic|0x[0-9a-fA-F]{64})" .
```

Result: expected existing documentation/verification scan-command text, public Pyth/Chainlink/feed IDs, and prior verification condition IDs. No secret values were found. The LB3 dry-run output contains zero bytes32 placeholders only; no repository file stores a live secret or key value.

LB3 network-post scan:

```text
rg -n -i "(https?://|Client::new|reqwest|post\(|\.post\(|/order|/orders|/cancel)" src
```

Result: expected existing read-only clients and local metrics smoke hits. New LB3 hits are HTTPS validation and fixture CLOB host strings only. The LB3 module imports no HTTP client and includes a regression test proving no network-capable submit tokens exist in the dry-run source.

## Exit Gate

LB3 exit gate: PASS for dry-run payload construction.

The mandatory LB3 hold is active. Stop before LB4 until the human/operator explicitly approves starting LB4 and records the required legal/access and deployment geoblock prerequisites.
