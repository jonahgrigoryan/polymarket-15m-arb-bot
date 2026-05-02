# Live Beta LB4 Approved-Host Readback Evidence

Date: 2026-04-30
Branch: `live-beta/lb4-approved-host-readback`
Base commit: `a4a54d2d1a3876435be73cd7935f80d1d0928549`
Phase: LB4 - Approved-host authenticated readback/account preflight

## Scope

This continuation attempts the LB4 approved-host evidence step only. It does not authorize or add order posting, canceling, cancel-all, strategy-to-live routing, live trading, wallet private-key material, API-key values, or secret values.

`LIVE_ORDER_PLACEMENT_ENABLED=false` remains unchanged.

## Operator Approval

The operator approved LB4 approved-host authenticated readback/account preflight from the current Mexico host/session only.

The same approval explicitly does not approve order posting, canceling, cancel-all, live trading, LB5, or LB6.

## Official Docs Rechecked

- `https://docs.polymarket.com/api-reference/introduction`: CLOB API base is `https://clob.polymarket.com`; authenticated CLOB endpoints require authentication.
- `https://docs.polymarket.com/api-reference/authentication`: L2 headers are `POLY_ADDRESS`, `POLY_SIGNATURE`, `POLY_TIMESTAMP`, `POLY_API_KEY`, and `POLY_PASSPHRASE`; `POLY_SIGNATURE` is an HMAC-SHA256 signature using the L2 secret value.
- `https://docs.polymarket.com/api-reference/trade/get-user-orders`: open-order readback is `GET /data/orders`.
- `https://docs.polymarket.com/api-reference/trade/get-trades`: trade readback is `GET /trades`.
- `https://docs.polymarket.com/trading/orders/overview`: balances, allowances, order reservations, trade lifecycle statuses, transaction hashes, and heartbeat behavior remain required LB4 preflight evidence.
- `https://docs.polymarket.com/resources/error-codes`: auth failure, rate limit, trading-disabled, cancel-only, malformed, delayed, unmatched, and other venue errors must fail closed. The current docs state `GET /balance-allowance` `signature_type` must be `EOA`, `POLY_PROXY`, or `GNOSIS_SAFE`, but the official `py_clob_client_v2` sends numeric values `0`, `1`, and `2`; the readback client follows the official v2 client after approved-host SDK comparison.

The implementation also cross-checked the official V2 Python client HMAC fixture from `Polymarket/py-clob-client-v2` and added a local regression for the documented L2 HMAC shape.

## Code Changes

- Added a read-only LB4 CLOB readback path that uses authenticated `GET` requests only.
- Added HMAC L2 header construction for readback requests without printing credential values.
- Added config metadata for LB4 readback account settings with empty fail-closed defaults.
- Changed `validate --live-readback-preflight` so the online LB4 path runs geoblock plus readback preflight without invoking the older M2 Postgres market-discovery persistence path.
- Changed `GET /balance-allowance` to match the official v2 Python client numeric signature-type query values (`0`, `1`, `2`) and added a regression test for that mapping.
- Updated `GET /balance-allowance` parsing to accept both documented singular `allowance` and live plural `allowances` map responses. Plural maps use the lowest returned allowance for the readiness threshold so the gate remains fail-closed when any returned spender allowance is low or malformed.
- Kept `validate --local-only --live-readback-preflight` on the existing synthetic fixture path.

No order post method, cancel method, cancel-all method, generalized trading-capable client, wallet private-key handling, or strategy-to-live routing was added.

## Approved-Host Geoblock Evidence

Command:

```text
cargo run --offline -- --config config/default.toml validate
```

Result: geoblock PASS from this host/session before the command later hit unrelated M2 Postgres validation.

Key output:

```text
geoblock_blocked=false
geoblock_country=MX
geoblock_region=CHP
```

Approved-host LB4 command:

```text
cargo run --offline -- --config config/local.toml validate --live-readback-preflight
```

`config/local.toml` is ignored by `.gitignore` and contains only non-secret approval/config flags plus secret handle names. It does not contain credential values.

Key output:

```text
geoblock_blocked=false
geoblock_country=MX
geoblock_region=CHP
live_beta_geoblock_gate=passed
live_beta_readback_preflight_lb3_hold_released=true
live_beta_readback_preflight_legal_access_approved=true
live_beta_readback_preflight_deployment_geoblock_passed=true
live_beta_secret_presence_status=missing
live_beta_secret_handle=label=clob_l2_access,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_ACCESS,present=false
live_beta_secret_handle=label=clob_l2_credential,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_CREDENTIAL,present=false
live_beta_secret_handle=label=clob_l2_passphrase,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_PASSPHRASE,present=false
```

Result: BLOCKED before authenticated readback. No authenticated account endpoint was called because the approved secret handles were absent from this shell.

Follow-up handle check after `.env` appeared:

```text
set -a && source .env >/dev/null 2>/dev/null && set +a && cargo run --offline -- --config config/local.toml validate --local-only --validate-secret-handles
```

Result: still BLOCKED before authenticated readback. The command printed handle presence booleans only, and all three approved handles remained `present=false`.

Follow-up approved-host operator-shell run after L2 env handles were populated:

```text
set -a && source .env && set +a
cargo run --offline -- --config config/local.toml validate --live-readback-preflight
```

Key non-secret output reported by the operator:

```text
geoblock_blocked=false
geoblock_country=MX
geoblock_region=CMX
live_beta_geoblock_gate=passed
live_beta_readback_preflight_lb3_hold_released=true
live_beta_readback_preflight_legal_access_approved=true
live_beta_readback_preflight_deployment_geoblock_passed=true
live_beta_secret_presence_status=ok
live_beta_secret_handle=label=clob_l2_access,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_ACCESS,present=true
live_beta_secret_handle=label=clob_l2_credential,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_CREDENTIAL,present=true
live_beta_secret_handle=label=clob_l2_passphrase,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_PASSPHRASE,present=true
live_beta_readback_preflight_signature_type=poly_proxy
```

Result: authenticated readback reached the balance/allowance response path and then failed closed with:

```text
error: failed to parse LB4 readback JSON: missing field `allowance`
```

Interpretation: the earlier credential/geoblock blocker was cleared in the operator shell, but the live `GET /balance-allowance` success response used a plural `allowances` map instead of the singular `allowance` shape assumed by the parser. LB4 remained BLOCKED. No raw response body or credential value was recorded.

Parser correction: `parse_balance_allowance` now accepts both singular `allowance` and plural `allowances` map responses. The plural path uses the lowest returned allowance as the single readiness value and rejects empty/malformed allowance maps.

Follow-up approved-host operator-shell run after the parser correction:

```text
cargo run --offline -- --config config/local.toml validate --live-readback-preflight
```

Key non-secret output reported by the operator:

```text
live_beta_readback_preflight_lb3_hold_released=true
live_beta_readback_preflight_legal_access_approved=true
live_beta_readback_preflight_deployment_geoblock_passed=true
live_beta_secret_presence_status=ok
live_beta_secret_handle=label=clob_l2_access,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_ACCESS,present=true
live_beta_secret_handle=label=clob_l2_credential,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_CREDENTIAL,present=true
live_beta_secret_handle=label=clob_l2_passphrase,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_PASSPHRASE,present=true
live_beta_readback_preflight_signature_type=poly_proxy
live_beta_readback_preflight_status=blocked
live_beta_readback_preflight_live_network_enabled=true
live_beta_readback_preflight_block_reasons=allowance_below_required,balance_below_required
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=0
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=0
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

Result: authenticated read-only account preflight reached the account-state gate and failed closed naturally because both available pUSD balance and allowance were below `required_collateral_allowance_units=1_000_000`.

Follow-up official v2 Python client check for the same signer/funder:

```text
signer_address= 0x280ca8b14386Fe4203670538CCdE636C295d74E9
{'balance': '45091977', 'allowances': {'0xE111180000d2663C0091e4f400237545B87B996B': '115792089237316195423570985008687907853269984665640564039457584007913129639935', '0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296': '115792089237316195423570985008687907853269984665640564039457584007913129639935', '0xe2222d279d744050d28e00520010520000310F59': '115792089237316195423570985008687907853269984665640564039457584007913129639935'}}
```

Interpretation: signer/funder, funded balance, and allowances are valid in the official v2 client. The Rust LB4 `balance-allowance` request was using the named `signature_type=POLY_PROXY` query shape, while the official v2 client uses numeric `signature_type=1`; Rust readback now matches the official v2 client numeric mapping.

Follow-up approved-host Rust preflight rerun after matching the official v2 client numeric `signature_type` query shape:

```text
cargo run --offline -- --config config/local.toml validate --live-readback-preflight
```

Key non-secret output reported by the operator:

```text
live_beta_geoblock_gate=passed
live_beta_gate_status=blocked
live_beta_gate_block_reasons=live_order_placement_disabled,missing_config_intent,missing_cli_intent,kill_switch_active,later_phase_approvals_missing
live_beta_readback_preflight_lb3_hold_released=true
live_beta_readback_preflight_legal_access_approved=true
live_beta_readback_preflight_deployment_geoblock_passed=true
live_beta_secret_presence_status=ok
live_beta_secret_handle=label=clob_l2_access,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_ACCESS,present=true
live_beta_secret_handle=label=clob_l2_credential,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_CREDENTIAL,present=true
live_beta_secret_handle=label=clob_l2_passphrase,backend=env,handle=P15M_LIVE_BETA_CLOB_L2_PASSPHRASE,present=true
live_beta_readback_preflight_signature_type=poly_proxy
live_beta_readback_preflight_status=passed
live_beta_readback_preflight_live_network_enabled=true
live_beta_readback_preflight_block_reasons=
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=3
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=45091977
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

Result: approved-host authenticated read-only account preflight PASS. The broader live-trading gate remained blocked by design because live order placement, config/CLI live intent, kill-switch release, and later-phase approvals remain disabled or missing.

## Pending Account Evidence

None for LB4. The run reported `trade_count=3` and passed, so trade status and confirmed/matched transaction-hash handling did not block the gate.

## Verification

- `cargo fmt --check`: PASS.
- `cargo test --offline readback`: PASS, 22 lib tests and 1 main test after the plural `allowances` parser correction.
- `cargo test --offline balance`: PASS, 4 lib tests.
- `cargo test --offline allowance`: PASS, 5 lib tests after the plural `allowances` parser correction.
- `cargo test --offline heartbeat`: PASS, 2 lib tests.
- `cargo test --offline secret`: PASS, 8 lib tests.
- `cargo test --offline redaction`: PASS, 2 lib tests.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS.
- Historical pre-population `cargo run --offline -- --config config/local.toml validate --local-only --validate-secret-handles`: expected BLOCKED result because approved env handles were absent in that shell.
- Historical pre-population `cargo run --offline -- --config config/local.toml validate --live-readback-preflight`: expected BLOCKED result after geoblock PASS because approved env handles were absent in that shell.
- `cargo test --offline`: PASS, 169 lib tests and 6 main tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- trailing whitespace scan on edited markdown/source: PASS, no hits.
- `.env` guard: PASS; `.env` exists and is gitignored.

## Safety And No-Secret Scan

Order/cancel/live-trading scan:

```text
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|cancel.*order|cancel-all|order client|clob.*order|/cancel|live[_ -]?order|live[_ -]?trading)" src Cargo.toml config --glob '!config/local.toml'
```

Result: expected hits only:

- `LIVE_ORDER_PLACEMENT_ENABLED=false` and live-mode disabled validation output,
- existing paper-order and paper-cancel simulation code,
- existing config/documentation warnings.

No order post, cancel, cancel-all, live-trading, or trading-capable order client path was added.

Credential/no-secret scan:

```text
rg -n -i "(private[_ -]?key|seed phrase|mnemonic|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" . --glob '!.env' --glob '!config/local.toml' --glob '!target'
```

Result: expected hits only:

- literal header names `POLY_API_KEY` and `POLY_PASSPHRASE`,
- public Pyth/Chainlink/reference IDs and historical condition IDs,
- documentation and prior verification warning text,
- the scan-command text itself.

No private key, seed phrase, mnemonic, API-key value, passphrase value, L2 secret value, or wallet key material was found.

Additional source/config credential scan:

```text
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|signing|signature|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SIGNATURE)" src Cargo.toml config
```

Result: expected hits only:

- existing LB2 secret-handle and redaction code,
- existing LB3 dry-run signing fixture code,
- new LB4 read-only L2 header construction,
- non-secret account metadata fields and fixture addresses,
- config warning comments and secret-handle names.

No secret values were printed or committed.

## Exit Gate

LB4 approved-host geoblock: PASS.

LB4 approved-host authenticated readback/account preflight: PASS.

Mandatory hold: this LB4 PASS does not authorize LB5, LB6, order posting, canceling, cancel-all, or live trading.

Do not start LB5 or LB6.
