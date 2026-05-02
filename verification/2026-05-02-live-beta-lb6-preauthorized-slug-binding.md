# 2026-05-02 LB6 Pre-Authorized Slug Binding Fix

## Scope

Fix the LB6 pre-authorized canary binding check after PR #24 merged.

No live order was submitted. No live cancel was sent. No cancel-all, autonomous trading, strategy-to-live route, secret value, or wallet/private-key material was added.

## Runtime Blocker Found After PR #24 Merge

After local `main` fast-forwarded to PR #24 merge commit `c8d0bfc`, LB4 approved-host readback passed:

- Command: `cargo run --offline -- --config config/local.toml validate --live-readback-preflight`
- Run ID: `18abe3d240c43b40-15285-0`
- Geoblock: `passed`, `MX/CMX`
- Open orders: `0`
- Reserved pUSD units: `0`
- Available pUSD units: `1614478`
- Venue state: `trading_enabled`
- Heartbeat: `not_started_no_open_orders`

The first pre-authorized canary attempt did not submit. It failed closed before any order call:

- Run ID: `18abe4137d932ef0-15382-0`
- Market: `eth-updown-15m-1777764600`
- Best ask: `0.004`
- Reference age: `1858 ms`
- Block reasons: `best_ask_not_above_bid,reference_stale`
- `live_beta_canary_not_submitted=true`
- `live_beta_canary_one_order_cap_remaining=true`

The next ETH window existed through Gamma's direct slug endpoint, but PR #24's pre-authorized binding lookup used paged keyset discovery and did not find `eth-updown-15m-1777765500` within the configured five pages. That would make the reviewed pre-authorized envelope fail closed with a missing binding even when the exact slug exists.

## Change

- Added `MarketDiscoveryClient::discover_crypto_15m_market_by_slug`.
- Added Gamma slug URL construction from the configured keyset URL:
  - `https://gamma-api.polymarket.com/markets/keyset`
  - to `https://gamma-api.polymarket.com/markets/slug/<slug>`
- Updated LB6 pre-authorized binding to fetch the exact supplied slug, then still require:
  - ETH asset
  - active lifecycle
  - no ineligibility reason
  - CLOB condition ID
  - matching Up token ID from CLOB market info
- Left normal broad keyset discovery unchanged.

## Verification

- `cargo fmt --check` PASS.
- `cargo test --offline market_discovery` PASS.
- `cargo test --offline canary` PASS.
- `cargo run --offline -- --config config/default.toml validate --local-only` PASS, run ID `18abe4888d71bc18-157db-0`.
- `cargo test --offline` PASS: 216 lib tests + 8 main tests.
- `cargo clippy --offline -- -D warnings` PASS.
- `git diff --check` PASS.
- Safety/no-secret scans PASS with expected hits only: existing gated canary `post_order` path, exact single-order cancel/readback path, paper order/cancel simulation paths, disabled live-order gate strings, public condition/feed IDs, and secret handle names.
- Ignored-local guard PASS: `.env` and `config/local.toml` are gitignored.
- One-order cap sentinel remains absent.

## Current Outcome

LB6 remains not submitted. Next live attempt must wait until this fix is reviewed/merged and then pass fresh geoblock, LB4, zero-open-order, non-marketable book, fresh-reference, exact slug binding, secret-handle, cancel-readiness, and one-order-cap gates naturally.
