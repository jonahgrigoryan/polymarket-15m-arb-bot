# 2026-05-12 Live Trading LT2 Gates, Journal, And Evidence

Branch: `live-trading/lt2-gates-journal-evidence`

Scope: LT2 final-live gate, journal, reducer, reconciliation, and evidence schemas only.

## Decision

LT2 is implementation-complete locally and ready for commit.

The implementation adds pure modeling and artifact-schema modules. It does not add or authorize live order signing, order submission, cancel submission, authenticated write clients, heartbeat POST, cap sentinel writes, taker expansion, or production sizing.

## Official Documentation Recheck

Checked on 2026-05-12:

- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/api-reference/geoblock
- https://docs.polymarket.com/api-reference/rate-limits
- https://docs.polymarket.com/api-reference/trade/get-user-orders
- https://docs.polymarket.com/api-reference/core/get-trades-for-a-user-or-markets
- https://docs.polymarket.com/api-reference/core/get-current-positions-for-a-user
- https://docs.polymarket.com/trading/orders/overview
- https://docs.polymarket.com/api-reference/trade/post-a-new-order
- https://docs.polymarket.com/api-reference/trade/cancel-single-order
- https://docs.polymarket.com/api-reference/trade/send-heartbeat

Relevant assumptions used for LT2:

- Gamma/Data API and public CLOB orderbook/price reads are read-only.
- CLOB account/order/trade readback and balance/allowance reads are authenticated readback inputs.
- CLOB order placement, cancellation, and heartbeat are authenticated trading or liveness operations and remain out of LT2 scope.
- LT2 may model intended maker orders, intended cancels, fills, fees, balances, positions, incidents, settlement follow-up, freshness, reconciliation state, and evidence hashes from fixtures.
- LT2 must not introduce a network write path, signing for submission, or any CLI mode that can submit or cancel.

## Modules Changed

- `src/live_trading_gate.rs`
- `src/live_trading_journal.rs`
- `src/live_trading_reconciliation.rs`
- `src/live_trading_evidence.rs`
- `src/lib.rs`
- `STATUS.md`

## Implementation Summary

- Added `live_trading_gate` with a final-live gate evaluator that blocks on disabled final-live config, failed or unknown preflight, failed or unknown journal replay, invalid evidence hashes, failed or unknown reconciliation, stale heartbeat, stale geoblock, unresolved prior order, write capability presence, or observed live writes.
- Added `live_trading_journal` with final-live event schemas and reducer state for intended maker orders, intended cancels, accepted order observations, fills, fees, balances, positions, incidents, incident review, settlement follow-up, heartbeat freshness, and geoblock freshness.
- Added `live_trading_reconciliation` with fixture-driven comparison of local reduced state to readback fixtures.
- Added `live_trading_evidence` with redacted evidence bundle schema, stable hash validation, journal summary reporting, reconciliation status, gate status, and no-write proof.
- Registered all LT2 modules in `src/lib.rs`.
- Updated `STATUS.md` with the LT2 branch state and next gate.

## Fail-Closed Fixtures

In-module LT2 fixtures cover:

- default gate blocked state,
- unresolved prior order blocking the next order,
- observed live write blocking,
- journal schema version mismatch,
- intended maker order, cancel, fill, fee, balance, position, incident, and settlement reduction,
- unknown venue order,
- unknown trade,
- missing accepted order,
- unexpected fill,
- balance drift,
- position drift,
- settlement mismatch,
- stale heartbeat,
- stale geoblock,
- evidence hash validation,
- recursive redaction of sensitive fields.

## No-Order / No-Cancel / No-Signing Proof

LT2 adds no code path for:

- network order submission,
- network cancel submission,
- order signing for submission,
- authenticated write clients,
- live heartbeat POST,
- final-live submit/cancel CLI modes,
- cap sentinel writes.

The new gate includes explicit `write_capability_present` and `no_write_proof` inputs, and blocks if any live write capability or observed live write appears in evidence.

## Verification

| Check | Status | Notes |
| --- | --- | --- |
| `cargo fmt --check` | PASS | Formatting clean after LT2 edits. |
| `cargo test --offline live_trading_gate` | PASS | 3 tests passed. |
| `cargo test --offline live_trading_journal` | PASS | 2 tests passed. |
| `cargo test --offline live_trading_reconciliation` | PASS | 2 tests passed. |
| `cargo test --offline live_trading_evidence` | PASS | 2 tests passed. |
| `cargo test --offline` | PASS | 435 lib tests, 98 main tests, and 0 doc tests passed. |
| `cargo clippy --offline -- -D warnings` | PASS | Passed after replacing a too-many-arguments evidence builder with an input struct. |
| `cargo run --offline -- --config config/default.toml validate --local-only` | PASS | Run ID `18af0147d21493e8-3f55-0`; `validation_status=ok`; module count is 49; live placement remains disabled. |
| `git diff --check` | PASS | No whitespace errors. |
| `scripts/verify-pr.sh` | PASS | Formatting, full tests, clippy, diff whitespace, built-in safety scope scan, no-secret scan, and ignored local secret guards passed. |

Additional LT2 safety scans were run and retained under `/tmp`:

- `/tmp/lt2_order_scan.txt`: 2419 lines.
- `/tmp/lt2_secret_scan.txt`: 1683 lines.
- `/tmp/lt2_gate_scan.txt`: 2303 lines.

Targeted review of new LT2 hits found only:

- `post_only` fixture/model fields,
- no-write proof fields such as `submitted_orders`, `signed_orders_for_submission`, `submitted_cancels`, and `heartbeat_posts`,
- redaction-key tests for `POLY_API_KEY` and `private_key`,
- gate/reconciliation/freshness names for heartbeat, geoblock, and reconciliation,
- documentation/status text describing disallowed live writes.

No new transport client, order submission, cancel submission, signing-for-submission, authenticated write client, heartbeat POST, secret value, private-key value, geoblock bypass, or safety-gate weakening was added.

## Safety Boundary

LT2 remains non-writing and fixture/model only. The implementation does not add:

- order submission,
- cancel submission,
- signing for submission,
- authenticated write clients,
- heartbeat POST,
- taker expansion,
- production sizing or rate increase.

## Exit Gate

LT2 exits with final-live state modeled, reduced, reconciled from fixtures, and reported through redacted evidence schemas without live write authority.

Next action after local completion: commit LT2, push, open PR to `main`, review, merge, refresh `main`, then wait for explicit approval before starting LT3.
