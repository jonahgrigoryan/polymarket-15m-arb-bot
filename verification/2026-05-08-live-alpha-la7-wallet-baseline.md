# Live Alpha LA7 Wallet Baseline

Date: 2026-05-08

## Scope

First LA7 prerequisite only: account-history-aware baseline capture for a history-bearing wallet. This note does not authorize live taker canary execution.

## Operator Decision

- Use the current wallet only if LA7 first adds account-history-aware baseline handling.
- Do not start live taker-canary work from summary-only readback.
- Wallet/funder/account preflight are acceptable for read-only LA7 shadow work.
- This is not a zero-history wallet: `baseline_trade_count=23`.
- Before any live-capable LA7 canary path, LA7 must capture a read-only authenticated baseline artifact and bind recovery/reconciliation to that artifact.

## Operator-Provided Readback Summary

Command source: operator terminal output pasted into the implementation thread.

```text
live_beta_readback_preflight_wallet_address=0x280ca8b14386Fe4203670538CCdE636C295d74E9
live_beta_readback_preflight_funder_address=0xB06867f742290D25B7430fD35D7A8cE7bc3a1159
live_beta_readback_preflight_signature_type=poly_proxy
live_beta_readback_preflight_status=passed
live_beta_readback_preflight_live_network_enabled=true
live_beta_readback_preflight_block_reasons=
live_beta_readback_preflight_open_order_count=0
live_beta_readback_preflight_trade_count=23
live_beta_readback_preflight_reserved_pusd_units=0
live_beta_readback_preflight_available_pusd_units=6314318
live_beta_readback_preflight_funder_allowance_units=18446744073709551615
live_beta_readback_preflight_venue_state=trading_enabled
live_beta_readback_preflight_heartbeat=not_started_no_open_orders
```

Conclusion: account state is clean for read-only shadow work, but not zero-history. Summary-only readback is not sufficient for live taker canary attribution.

## Official Docs Rechecked

- `https://docs.polymarket.com/api-reference/authentication`
  - Conclusion: authenticated CLOB operations use L2 API credentials and HMAC headers; private keys remain outside request logging. Baseline artifacts must not persist auth headers, API credential values, private keys, or signed payloads.
- `https://docs.polymarket.com/api-reference/trade/get-user-orders`
  - Conclusion: authenticated `GET /data/orders` returns open orders with order ID, status, maker address, market, asset ID, side, original/matched size, price, outcome, expiration, order type, associated trades, and creation time. These are appropriate baseline fields.
- `https://docs.polymarket.com/api-reference/trade/get-trades`
  - Conclusion: authenticated `GET /trades` returns trade IDs, order linkage fields, market, asset ID, status, transaction hash, maker address, and side/context fields. Baseline must store trade IDs and order IDs when present.
- `https://docs.polymarket.com/api-reference/rate-limits`
  - Conclusion: readback endpoints are rate-limited but suitable for a single baseline capture. The writer performs a bounded read-only capture, not polling or trading.

## Implementation

Added:

- `src/live_account_baseline.rs`
- CLI command: `live-alpha-account-baseline --read-only`

The command writes:

```text
artifacts/live_alpha/<baseline_id>/account_baseline.redacted.json
artifacts/live_alpha/<baseline_id>/orders.redacted.json
artifacts/live_alpha/<baseline_id>/trades.redacted.json
artifacts/live_alpha/<baseline_id>/balances.redacted.json
artifacts/live_alpha/<baseline_id>/positions.redacted.json
```

Artifact guarantees:

- includes `baseline_id`, `run_id`, capture timestamp, wallet/funder public addresses, signature type, readback report, collateral balance/allowance, open order IDs, trade IDs, order IDs when present, status, market/token identifiers, and `baseline_hash`;
- stores no auth headers, API credential values, signed payloads, private keys, seed phrases, or passphrases;
- runs geoblock validation before authenticated readback and blocks capture if the host is restricted;
- validates `report.trade_count == trades.len()`;
- validates `report.open_order_count == open_orders.len()`;
- requires `status=passed`, `open_order_count=0`, and `reserved_pusd_units=0` for accepted capture;
- records `position_evidence_complete=false` because current authenticated readback does not reconstruct conditional-token positions.

LA7 live-capable gate behavior:

- blocks when `trade_count > 0` and no baseline artifact is provided;
- requires enabled LA7 taker config to pin `baseline_id`, `baseline_capture_run_id`, and `baseline_artifact_path`;
- validates the configured artifact hash, baseline ID, capture run ID, wallet, funder, signature type, current readback count consistency, zero open orders, zero reserved pUSD, and baseline trade presence in current readback;
- ignores only explicitly baselined trade IDs during baseline-aware startup recovery/reconciliation;
- still halts on any new unbaselined venue trade;
- blocks live taker canary while `position_evidence_complete=false`.

## Code-Prerequisite Follow-Up

Added after the initial baseline writer:

- `live_alpha.taker.baseline_id`, `baseline_capture_run_id`, and `baseline_artifact_path` default to empty and are required only when taker mode is enabled.
- Startup recovery loads the configured redacted baseline artifact for enabled LA7 taker mode, fails closed on missing/mismatched artifacts, and reconciles history-bearing account readback through the baseline.
- Position evidence remains incomplete in the current artifact shape, so the live taker gate still blocks until real position evidence is complete or the canary remains explicitly blocked.

## Capture Command

Operator command to capture the real baseline:

```bash
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-2026-05-08-wallet-baseline-001
```

Status: `ATTEMPTED IN CODEX AND BLOCKED BEFORE AUTHENTICATED READBACK`.

Attempted command:

```bash
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-2026-05-08-wallet-baseline-001
```

Observed output:

```text
run_id=18ada5fd98b274b8-ef9f-0
geoblock_blocked=false
geoblock_country=BR
geoblock_region=SP
command_status=error
error=LB4 clob_l2_access handle is not present
```

No baseline artifact was written. No authenticated user orders/trades/balances/positions readback occurred. No live order or cancel path was exercised. The configured L2 handle names are:

```text
P15M_LIVE_BETA_CLOB_L2_ACCESS
P15M_LIVE_BETA_CLOB_L2_CREDENTIAL
P15M_LIVE_BETA_CLOB_L2_PASSPHRASE
```

All three were missing from this Codex process. The operator should rerun the same read-only command from an approved host shell where those environment handles are present. The command is read-only and writes redacted artifacts.

Follow-up credential-loaded attempt:

```bash
set -a
source .env
set +a
cargo run --offline -- --config config/local.toml live-alpha-account-baseline --read-only --baseline-id LA7-2026-05-08-wallet-baseline-001
```

First credential-loaded attempt reached authenticated readback but failed transiently on `/sampling-markets` body decoding. A retry succeeded.

Successful capture output:

```text
run_id=18ada6596491f080-10a41-0
geoblock_blocked=false
geoblock_country=BR
geoblock_region=SP
live_alpha_account_baseline_id=LA7-2026-05-08-wallet-baseline-001
live_alpha_account_baseline_wallet_address=0x280ca8b14386Fe4203670538CCdE636C295d74E9
live_alpha_account_baseline_funder_address=0xB06867f742290D25B7430fD35D7A8cE7bc3a1159
live_alpha_account_baseline_signature_type=poly_proxy
live_alpha_account_baseline_status=passed
live_alpha_account_baseline_open_order_count=0
live_alpha_account_baseline_trade_count=23
live_alpha_account_baseline_reserved_pusd_units=0
live_alpha_account_baseline_available_pusd_units=6314318
live_alpha_account_baseline_allowance_units=18446744073709551615
live_alpha_account_baseline_position_evidence_complete=true
live_alpha_account_baseline_position_count=5
live_alpha_account_baseline_hash=sha256:a79a3e55957795bb286fe119a9328acf6c62e6fb022210340bbf248ebec5dd43
live_alpha_account_baseline_output_dir=artifacts/live_alpha/LA7-2026-05-08-wallet-baseline-001
live_alpha_account_baseline_no_secrets_guarantee=auth_headers:false,l2_api_credentials:false,signed_payloads:false,private_keys:false
live_alpha_account_baseline_la7_live_gate_status=blocked
live_alpha_account_baseline_la7_live_gate_block_reasons=baseline_positions_nonzero
```

Position evidence source: official public Polymarket Data API `/positions` for the proxy/funder wallet. Current signer wallet position count was 0, but the proxy/funder wallet had 5 current positions:

```text
position=slug=btc-updown-5m-1777718700,outcome=Down,size=85.1307,currentValue=0,redeemable=true,mergeable=false
position=slug=btc-updown-5m-1777663800,outcome=Up,size=41.6666,currentValue=0,redeemable=true,mergeable=false
position=slug=btc-updown-5m-1777719000,outcome=Up,size=12.8205,currentValue=0,redeemable=true,mergeable=false
position=slug=btc-updown-15m-1777927500,outcome=Down,size=12.4829,currentValue=0,redeemable=true,mergeable=false
position=slug=btc-updown-15m-1777923000,outcome=Down,size=10.2301,currentValue=0,redeemable=true,mergeable=false
```

Decision for `baseline-001`: baseline capture was durable and evidence-complete, but LA7 live taker approval remained `NO-GO` because the proxy/funder wallet was not flat. The code now blocks `baseline_positions_nonzero` instead of treating complete but non-empty position evidence as approval-safe.

After the operator rechecked/cleared positions, independent Data API checks showed:

```text
funder_position_count=0
wallet_position_count=0
```

The first `baseline-002` recapture failed on `/sampling-markets` response-body decoding. The read-only CLOB client was updated to request `Accept-Encoding: identity`, and `cargo test --offline live_beta_readback` passed.

Successful flat-wallet recapture:

```text
run_id=18ada759309e3ad0-13d88-0
geoblock_blocked=false
geoblock_country=BR
geoblock_region=SP
live_alpha_account_baseline_id=LA7-2026-05-08-wallet-baseline-002
live_alpha_account_baseline_wallet_address=0x280ca8b14386Fe4203670538CCdE636C295d74E9
live_alpha_account_baseline_funder_address=0xB06867f742290D25B7430fD35D7A8cE7bc3a1159
live_alpha_account_baseline_signature_type=poly_proxy
live_alpha_account_baseline_status=passed
live_alpha_account_baseline_open_order_count=0
live_alpha_account_baseline_trade_count=23
live_alpha_account_baseline_reserved_pusd_units=0
live_alpha_account_baseline_available_pusd_units=6323882
live_alpha_account_baseline_allowance_units=18446744073709551615
live_alpha_account_baseline_position_evidence_complete=true
live_alpha_account_baseline_position_count=0
live_alpha_account_baseline_hash=sha256:22ab15276a4d8fe6418b20c2fefa27325fbb753bc5ced0acd9e9d9718c760737
live_alpha_account_baseline_output_dir=artifacts/live_alpha/LA7-2026-05-08-wallet-baseline-002
live_alpha_account_baseline_no_secrets_guarantee=auth_headers:false,l2_api_credentials:false,signed_payloads:false,private_keys:false
live_alpha_account_baseline_la7_live_gate_status=passed
live_alpha_account_baseline_la7_live_gate_block_reasons=
```

Decision for `baseline-002`: account-history and flat-position gates now pass. This does not by itself authorize a live taker canary; a bounded LA7 approval artifact with exact market/condition/token/side/outcome/price/size/notional/fee/slippage/no-near-close fields is still required.

Fresh recapture after browser-backed account inspection:

```text
run_id=18adab7ed4f41d38-170f4-0
geoblock_blocked=false
geoblock_country=BR
geoblock_region=SP
live_alpha_account_baseline_id=LA7-2026-05-08-wallet-baseline-003
live_alpha_account_baseline_wallet_address=0x280ca8b14386Fe4203670538CCdE636C295d74E9
live_alpha_account_baseline_funder_address=0xB06867f742290D25B7430fD35D7A8cE7bc3a1159
live_alpha_account_baseline_signature_type=poly_proxy
live_alpha_account_baseline_status=passed
live_alpha_account_baseline_open_order_count=0
live_alpha_account_baseline_trade_count=23
live_alpha_account_baseline_reserved_pusd_units=0
live_alpha_account_baseline_available_pusd_units=6323882
live_alpha_account_baseline_allowance_units=18446744073709551615
live_alpha_account_baseline_position_evidence_complete=true
live_alpha_account_baseline_position_count=0
live_alpha_account_baseline_hash=sha256:fff55e06dc3983e30fea11ceff7bfa63f45e50f9d3d42bd85d2e8060cb9e3d5e
live_alpha_account_baseline_output_dir=artifacts/live_alpha/LA7-2026-05-08-wallet-baseline-003
live_alpha_account_baseline_no_secrets_guarantee=auth_headers:false,l2_api_credentials:false,signed_payloads:false,private_keys:false
live_alpha_account_baseline_la7_live_gate_status=passed
live_alpha_account_baseline_la7_live_gate_block_reasons=
```

Decision for `baseline-003`: account-history and flat-position gates pass. `config/local.toml` was updated locally to pin `baseline-003`, but that file is intentionally gitignored. This still does not authorize a live taker canary.

## Browser/Screen Account Context

Browser account visibility is supporting context only and is not sufficient LA7 evidence. It cannot replace the durable `live-alpha-account-baseline --read-only` capture because LA7 needs paginated authenticated orders/trades, balance/allowance, explicit count checks, `baseline_hash`, and startup recovery/reconciliation binding to the exact captured artifact.

This agent run used Chrome only for non-mutating portfolio observation. The logged-in Polymarket portfolio page showed `$6.32` cash/available, `No positions found.`, and `No open orders found.` Safe observation remains limited to non-mutating context such as page availability, a redacted match to the already-approved wallet/funder identity, summarized balance/position status, and absence/presence summaries. Do not click trade, deposit, withdraw, reveal/export secrets, create/copy API credentials, sign messages, approve wallet transactions, or otherwise mutate account state.

Before LA7 can move from `NO-GO` toward a live taker canary, the operator must capture and review the approved CLI artifact, then record only sanitized outputs:

```text
baseline_id
run_id
baseline_hash
geoblock result
account preflight status
open_order_count
reserved_pusd_units
available_pusd_units
trade_count and captured trades length
orders/trades/balances/positions artifact paths
position_evidence_complete
```

The required pass conditions remain: geoblock PASS, readback status `passed`, `open_order_count=0`, `reserved_pusd_units=0`, `trade_count == trades.len()`, `open_order_count == open_orders.len()`, no secrets in artifacts, and baseline-aware recovery/reconciliation wired to the exact artifact. If `position_evidence_complete=false`, live taker canary remains blocked even if the baseline capture otherwise succeeds.

## Verification Commands

```bash
git status --short --branch
git log --oneline -5
rg -n "LA7|baseline|account-history|history-aware|unexpected_fill|authenticated_readback_preflight_evidence|LiveAlphaPreflight|Commands::LiveAlpha|live-alpha-preflight|live-alpha-quote-manager" AGENTS.md STATUS.md LIVE_ALPHA_PRD.md LIVE_ALPHA_IMPLEMENTATION_PLAN.md src/main.rs src/live_beta_readback.rs src/live_reconciliation.rs src/live_startup_recovery.rs Cargo.toml
cargo fmt
cargo fmt --check
cargo test --offline live_account_baseline
cargo test --offline startup_recovery
cargo test --offline live_reconciliation
cargo run --offline -- live-alpha-account-baseline --help
cargo test --offline
cargo clippy --offline -- -D warnings
git diff --check
rg -n -i "(cancel.?all|batch|FOK|FAK|marketable|taker)" src config runbooks verification
rg -n -i "(wallet|private.*key|secret|passphrase|mnemonic|seed|0x[0-9a-fA-F]{64})" src config runbooks verification
test ! -e .env || git check-ignore .env
test ! -e config/local.toml || git check-ignore config/local.toml
```

Current results:

- `cargo fmt --check`: PASS.
- `cargo test --offline live_account_baseline`: PASS, 11 filtered library tests.
- `cargo test --offline startup_recovery`: PASS, 9 filtered library tests and 11 filtered main tests.
- `cargo test --offline live_alpha_config`: PASS, 7 filtered library tests.
- `cargo test --offline live_taker_gate`: PASS, 9 filtered library tests.
- `cargo test --offline`: PASS, 407 library tests, 75 main tests, 0 doc tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- `.env` ignore guard: PASS, `.env` is ignored.
- `config/local.toml` ignore guard: PASS, `config/local.toml` is ignored.

Safety scan conclusions:

- Order/taker/cancel-all scan completed with expected historical hits from prior Live Alpha/Live Beta code, docs, runbooks, config flags, and this note's explicit LA7 taker-canary block language. No new taker strategy, live taker canary command, FOK/FAK strategy path, marketable strategy path, batch order path, or cancel-all runtime path was added by this patch.
- Wallet/secret scan completed with expected public wallet/funder/order/feed IDs, secret-handle names, field names, docs/runbooks/verification guardrail text, and the new baseline no-secrets guarantee labels. No private key, API secret value, passphrase value, mnemonic, seed phrase, raw L2 credential, signed payload, or auth header value was added.
- Diff-only scan for this patch found only status/verification guardrail text, public wallet/funder labels, and no-secrets guarantee field names.

## LA7 Decision

Read-only LA7 shadow work may proceed against this wallet. Account-history and flat-position baseline gates pass for `baseline-003`. Live taker canary remains blocked until:

- LA7 config is pinned to `baseline_id=LA7-2026-05-08-wallet-baseline-003`, `baseline_capture_run_id=18adab7ed4f41d38-170f4-0`, and `artifacts/live_alpha/LA7-2026-05-08-wallet-baseline-003/account_baseline.redacted.json`;
- startup recovery/readback validation passes against that exact artifact;
- a separate LA7 taker-gate approval exists.
