# Live Beta LB3 Signing Dry Run

Date: 2026-04-30
Phase: LB3 - Signing Dry Run, No Network Post

## SDK And Signing Decision

LB3 does not import `polymarket_client_sdk_v2` or `rs-clob-client-v2`.

Those official Rust paths remain the SDKs to audit before any real signing or authenticated CLOB client work. They are not added in LB3 because importing an SDK with order, cancel, readback, or authenticated client surfaces would exceed this phase.

LB3 uses a minimal custom V2 payload draft builder only. It constructs a deterministic, sanitized dry-run artifact that can be reviewed for V2 order shape and local validation without loading credentials, creating a signer, authenticating, posting, canceling, or reading back venue state.

Official references rechecked for LB3:

- `https://docs.polymarket.com/api-reference/introduction`
- `https://docs.polymarket.com/developers/CLOB/clients`
- `https://docs.polymarket.com/v2-migration`
- `https://docs.polymarket.com/developers/CLOB/orders/orders`

Live read-only endpoint check on 2026-04-30:

- `https://clob.polymarket.com/ok` returned `HTTP/2 200` with body `"OK"`.
- `https://clob-v2.polymarket.com/ok` returned `HTTP/2 301` to `https://clob.polymarket.com/ok`.

## Dry-Run Artifact

The LB3 dry-run artifact includes:

- CLOB host.
- EIP-712 domain draft.
- Token ID.
- Side and side code.
- Price and size converted to six-decimal maker/taker amount fields.
- GTD expiry before market end.
- Signature type.
- Maker, funder/proxy, and signer fixture addresses.
- Zero bytes32 placeholders for metadata and builder.
- Redacted owner and redacted signature placeholders.
- `post_only=true`.
- `not_submitted=true`.
- `network_post_enabled=false`.
- `dry_run_only=true`.

The dry run does not produce a cryptographic signature. That remains blocked until a later reviewed implementation can use an approved signer and approved secret backend without exposing credential values.

## Local Command

```text
cargo run --offline -- --config config/default.toml validate --local-only --live-beta-signing-dry-run
```

Expected output includes:

```text
live_order_placement_enabled=false
live_beta_gate_status=blocked
live_beta_signing_dry_run_status=ok
live_beta_signing_dry_run_not_submitted=true
live_beta_signing_dry_run_network_post_enabled=false
```

The artifact JSON is sanitized and must not contain credential values.

## Boundary

LB3 adds no live order placement, no network order post, no live cancel, no authenticated readback, no wallet material, no API-key values, no secret values, no geoblock bypass, and no strategy/risk/freshness changes.

Stop after LB3. LB4 may not begin until the mandatory human hold approval is recorded.
