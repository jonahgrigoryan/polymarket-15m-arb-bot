# 2026-05-04 Live Alpha LA3 Controlled Fill Canary

## Scope Decision

LA3 was opened from fresh `main` after PR #31 merged. The local approval artifact is `verification/2026-05-04-live-alpha-la3-approval.md`.

- Branch: `live-alpha/la3-controlled-fill-canary`
- Base commit: `f493c78`
- Phase: LA3 controlled live fill canary
- Result: PASS; ONE LIVE FAK FILL CANARY SUBMITTED AND RECONCILED
- Stop reason: LA3 executed successfully; stop after LA3 for human review and settlement follow-up

## Planning Sources Re-read

- `LA3 Kickoff Prompt.md`
- `LIVE_ALPHA_PRD.md` section 12, LA3 controlled live fill canary
- `LIVE_ALPHA_PRD.md` section 25, Live Alpha acceptance criteria
- `LIVE_ALPHA_IMPLEMENTATION_PLAN.md` section LA3
- `STATUS.md` latest Live Alpha handoff
- `AGENTS.md`

## External Documentation

Official Polymarket documentation and the pinned local official SDK source were checked before implementing the live-submit path:

- Polymarket CLOB L2 client methods and authenticated readback endpoints.
- Polymarket CLOB authentication and L2 header requirements.
- Polymarket on-chain order information and order construction constraints.
- Local `polymarket_client_sdk_v2` `0.6.0-canary.1` source/examples for `market_order`, `OrderType::FAK`, `Side::Buy`, `Amount::usdc`, signing, and single `post_order` submission.

The implemented path uses exactly one official-SDK `post_order` call in the final human-approved path and has no retry loop, no `post_orders` batch path, no FOK, no GTC/GTD marketable-limit path, no SELL path, and no cancel-all path.

## Implemented Scope

- Added `src/live_alpha_preflight.rs`.
  - Builds a fail-closed LA3 preflight report for read-only, dry-run, and final-submit modes.
  - Checks approved host/account/signature/geoblock/config/order/market/book/reference/journal/cap state.
  - Refuses final submit on stale, mismatched, missing, or under-proven evidence.
- Added `src/live_fill_canary.rs`.
  - Parses the Markdown approval artifact strictly.
  - Builds a canonical approval envelope and prompt hash.
  - Validates secret-handle presence without printing secret values.
  - Implements the one-attempt official-SDK FAK `BUY` submit function for the final human-approved path.
  - Implements immediate post-submit reconciliation helpers.
- Updated `src/main.rs`.
  - Added `live-alpha-preflight --read-only --approval-artifact <path>`.
  - Added `live-alpha-fill-canary --dry-run --approval-artifact <path>`.
  - Added `live-alpha-fill-canary --human-approved --approval-id <id> --approval-artifact <path>`.
  - Uses the approval artifact's asset for reference freshness instead of hardcoding BTC.
  - Accepts Polymarket RTDS Chainlink read-only reference ticks for LA3 reference freshness.
- Updated `src/feed_ingestion.rs`.
  - Installs the ring rustls provider before WSS connections so RTDS reference checks do not panic.
- Updated `src/live_alpha_config.rs`.
  - Allows `fill_canary.allow_fak=true` only when `mode=fill_canary` and `fill_canary.enabled=true`.
  - Keeps FOK, marketable-limit, and taker mode rejected for LA3.
- Updated `src/live_alpha_preflight.rs` during closeout.
  - Estimates official crypto taker fees from shares traded using `fee = C * 0.072 * p * (1 - p)`.
  - Blocks future LA3-style preflight with `approved_fee_estimate_below_official_taker_fee` when the approval artifact's max fee estimate is below that official estimate.
- Updated `src/live_order_journal.rs`.
  - Added LA3 journal event types and reducer behavior for fill attempt/success/failure/reconciliation evidence.
- Updated `src/lib.rs`.
  - Exposed the LA3 modules.

## Approval Artifact Check

- Approval ID: `LA3-2026-05-04-004`
- Approved host: `Jonahs-MacBook-Pro.local`
- Approved wallet: `0x280ca8b14386Fe4203670538CCdE636C295d74E9`
- Approved funder: `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159`
- Approved side/order type: `BUY` / `FAK`
- Approved market/token: `btc-updown-15m-1777925700` / `27694987584135655174383601846064222249431031901378233816925163016282018353279`
- Approved outcome: `Down`
- Approved max notional: `2.56 pUSD`
- Approved max fee: `0.06 pUSD`
- Approved worst price: `0.51`
- Approved retry count: `0`
- Explicitly not approved: second attempt, FOK, SELL, GTC/GTD marketable limit, cancel-all, maker autonomy, taker strategy, scaling, later-phase work
- Funding note: authenticated final preflight proved available pUSD units `10070772`, allowance sufficient, zero open orders, and 16 recent trades before submit. Post-run read-only preflight reported available pUSD units `7418612`, reserved pUSD units `0`, zero open orders, and 17 recent trades.

## Fresh Preflight Evidence

Read-only preflight command:

```bash
cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
```

Result:

- PASS command exit: the command completed and refused live submit.
- Run ID: `18ac6908e49cd6d0-5bdb-0`
- Geoblock: `blocked=false`, country `MX`, region `CMX`
- Preflight status: `blocked`
- Mode: `read_only`
- Approval ID: `LA3-2026-05-04-002`
- Intent: approved BTC 15m `BUY` `FAK`, outcome `Down`, amount `2.56`, worst price `0.51`
- Compile-time live-alpha-orders feature: `false`
- Book evidence: snapshot `33c02cabd1f63d4e3d516b29dff2d25012a6dab7`, age `103339 ms`
- Reference evidence: Polymarket RTDS Chainlink snapshot `https://data.chain.link/streams/btc-usd:polymarket_rtds_chainlink:1777911445000`, age `4224 ms`
- Block reasons: `account_preflight_not_passed`, `account_preflight_not_live_network`, `missing_cli_intent`, `l2_secret_handles_missing`, `allowance_below_notional_plus_fee`

Dry-run command with compile-time live order feature enabled:

```bash
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
```

Result:

- PASS command exit: the command completed and did not submit.
- Run ID: `18ac69098393a200-5bdc-0`
- Geoblock: `blocked=false`, country `MX`, region `CMX`
- Fill-canary status: `blocked`
- `live_alpha_fill_canary_not_submitted=true`
- Compile-time live-alpha-orders feature: `true`
- Book evidence: snapshot `33c02cabd1f63d4e3d516b29dff2d25012a6dab7`, age `105986 ms`
- Reference evidence: Polymarket RTDS Chainlink snapshot `https://data.chain.link/streams/btc-usd:polymarket_rtds_chainlink:1777911448000`, age `4302 ms`
- Block reasons: `account_preflight_not_passed`, `account_preflight_not_live_network`, `l2_secret_handles_missing`, `allowance_below_notional_plus_fee`

## LA3 Evidence Fields

- Approval ID: `LA3-2026-05-04-004`
- Exact command sequence: implementation checks and LA3 read-only/dry-run commands listed below
- Geoblock result: PASS for this host/session, `BR/SP`
- Account preflight: PASS during final dry-run, final submit, and post-run read-only check. Final pre-submit readback reported available pUSD units `10070772`, allowance sufficient, zero open orders, and 16 recent trades.
- Heartbeat result: no live heartbeat POST; sample readback reported `not_started_no_open_orders`
- Market and order intent: BTC `btc-updown-15m-1777925700`, outcome `Down`, token `27694987584135655174383601846064222249431031901378233816925163016282018353279`, `BUY` `FAK`, worst price `0.51`, amount `2.56 pUSD`
- Venue order ID: `0xd16026c677ff8b5d0f8cc89a1c75bebc61fd047d71232d0b323a2c50acd5b6a0`
- Trade ID: `495feb52-5706-4660-9f52-fa0449fda520`
- Order status transitions: submitted via official SDK, venue status `MATCHED`, reconciliation status `filled_and_reconciled`
- Trade status transitions: matching trade observed during immediate reconciliation; public activity later showed `TRADE` followed by `REDEEM`
- Maker/taker status: FAK BUY fill canary; activity readback is consistent with a taker fill
- Fee / fee rate: official crypto taker fee rate `0.072`; activity readback implies `0.092160 pUSD` fee/extra cost, exceeding the artifact's `0.06 pUSD` max fee estimate
- Balance before/after: final pre-submit available pUSD units `10070772`; post-run/pre-redeem available pUSD units `7418612`; final post-redeem available pUSD units `12538612`
- Reserved balance before/after: final pre-submit reserved pUSD units `0`; post-run reserved pUSD units `0`
- Position before/after: post-run taking amount `5.12`; public positions endpoint returned `[]` after redemption, consistent with the winning position being redeemed
- Open orders after run: `0`
- Journal replay result: PASS for the configured local LA3 journal path because no corrupted prior journal exists at that path
- Reconciliation result: `filled_and_reconciled`, no reconciliation block reasons, zero open orders after run
- Settlement follow-up result: COMPLETE. Gamma outcome prices were `["0","1"]`, so `Down` won; public activity showed a `5.12 pUSD` redeem; authenticated readback confirmed final available pUSD units `12538612`, reserved pUSD units `0`, and zero open orders.
- Incident note: final LA3 order and settlement reconciled, but fee estimation was wrong in the approval artifact. The artifact used a `0.06 pUSD` max fee estimate; actual activity implies `0.092160 pUSD` fee/extra cost because Polymarket's fee formula uses shares traded as `C`, not notional.

## Funded/L2 Retry Evidence

The operator selected `https://polymarket.com/event/btc-updown-15m-1777922100` and reported the account was funded above `$12`. `.env` was sourced without printing secret values; handle-presence checks reported all required L2/signing handles present.

Public market readback at 2026-05-04T19:14:26Z:

- Slug: `btc-updown-15m-1777922100`
- Question: `Bitcoin Up or Down - May 4, 3:15PM-3:30PM ET`
- Active/closed/accepting orders: `true` / `false` / `true`
- End time: `2026-05-04T19:30:00Z`
- Condition ID: `0xfade9d16e789bf2ec842275399369c255bb0598f24b7e11183598c0220e8adc1`
- Outcomes/tokens: `Up` / `19515910227740200415361944125780085728888181205600574886106419452369671155165`, `Down` / `63465547936729291711647993679163422132029364382615358037354890581975462648867`

Commands and results:

- Read-only preflight run `18ac72d16a872788-a4b3-0`: command exit PASS, no submit, account preflight PASS, live network enabled, available pUSD units `12185950`, allowance sufficient, zero open orders, 14 recent trades, blocked with `missing_cli_intent,best_ask_exceeds_worst_price`.
- Dry-run `18ac72e5a0e2e970-a59d-0`: command exit PASS, `live_alpha_fill_canary_not_submitted=true`, blocked with `best_ask_exceeds_worst_price`.
- Dry-run `18ac72f1447f0478-a64d-0`: command exit PASS, `live_alpha_fill_canary_not_submitted=true`, blocked with `slippage_exceeds_approval`.
- Dry-run `18ac72f8f3b1e080-a6bd-0`: command exit PASS, `live_alpha_fill_canary_not_submitted=true`, blocked with `slippage_exceeds_approval`.
- Final timing check at 2026-05-04T19:20:06Z: market still showed `active=true`, `closed=false`, `acceptingOrders=true`, and end time `2026-05-04T19:30:00Z`, but the configured `600` second no-trade cutoff had been reached. No final submit was invoked.

## Final LA3 Fill Evidence

The operator then selected `btc-updown-15m-1777925700`, which had enough lead time before the configured no-trade cutoff. The approval artifact was refreshed to `LA3-2026-05-04-004`.

Public market readback at 2026-05-04T19:35:58Z:

- Slug: `btc-updown-15m-1777925700`
- Question: `Bitcoin Up or Down - May 4, 4:15PM-4:30PM ET`
- Active/closed/accepting orders: `true` / `false` / `true`
- End time: `2026-05-04T20:30:00Z`
- Condition ID: `0x00fac5682f5cf756e1602c88709d999b663d94b09c8c9da2eaa3e23444df6ea6`
- Outcomes/tokens: `Up` / `29038621437271669880763883649085945741087676428224655373060963560779543038644`, `Down` / `27694987584135655174383601846064222249431031901378233816925163016282018353279`

Book refresh before final dry-run:

- Down token book refreshed to hash `f65400ac441b95c59af2f1f9dd7100ef5740141f`
- Best bid `0.49` size `65`
- Best ask `0.50` size `10`
- Book age at final dry-run: `35026 ms`

Final dry-run:

- Command: `cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`
- Run ID: `18ac742809bd7b70-af72-0`
- Status: `passed`
- Block reasons: none
- Approval hash: `sha256:085355ea65337927fb8bb0ff33213b840ecc37a8627905e98a58ac02e4da3ccf`
- Account: PASS, live network enabled, available pUSD units `10070772`, reserved pUSD units `0`, open orders `0`, recent trades `16`

Final submit:

- Command: `cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --human-approved --approval-id LA3-2026-05-04-004 --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`
- Run ID: `18ac742fd4a83e40-b000-0`
- Preflight status: `passed`
- Fill-canary status: `passed`
- Venue order ID: `0xd16026c677ff8b5d0f8cc89a1c75bebc61fd047d71232d0b323a2c50acd5b6a0`
- Venue status: `MATCHED`
- Success: `true`
- Making amount: `2.56`
- Taking amount: `5.12`
- Transaction hash: `0x94fd00369403b3c6835c31956df2788d5e6d1a0c5e4b4c6647b0abf820be4077`
- Immediate reconciliation status: `filled_and_reconciled`
- Matching trade ID: `495feb52-5706-4660-9f52-fa0449fda520`
- Open orders after run: `0`
- Reconciliation block reasons: none

Post-run read-only check:

- Command: `cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`
- Run ID: `18ac7437fce59170-b065-0`
- Status: `blocked`, as expected after the one-attempt cap was consumed
- Block reasons: `missing_cli_intent`, `visible_best_ask_notional_below_amount`, `approval_attempt_already_consumed`
- Account: PASS, live network enabled, available pUSD units `7418612`, reserved pUSD units `0`, open orders `0`, recent trades `17`
- Prior attempt consumed: `true`

## Settlement Follow-up Evidence

Settlement check time: 2026-05-04T20:36:40Z.

Public Gamma market readback:

- Slug: `btc-updown-15m-1777925700`
- Closed: `true`
- Accepting orders: `false`
- End time: `2026-05-04T20:30:00Z`
- Outcomes: `["Up", "Down"]`
- Outcome prices: `["0", "1"]`
- Resolution source: `https://data.chain.link/streams/btc-usd`
- Settlement interpretation: `Down` won.

Public activity readback for the approved funder/market:

- `TRADE` at 2026-05-04T19:42:04Z: `BUY` `Down`, size `5.12`, price `0.5`, `usdcSize=2.65216`, transaction hash `0x94fd00369403b3c6835c31956df2788d5e6d1a0c5e4b4c6647b0abf820be4077`.
- `REDEEM` at 2026-05-04T20:32:30Z: size `5.12`, `usdcSize=5.12`, transaction hash `0xb8ac24cd4ab9b86bb6d48acf8e1ca673bb870eac84e55164cbf5651abbd2c330`.
- Public positions endpoint returned `[]` for the funder/market after redemption, consistent with no remaining open settled position.

Authenticated post-settlement readback:

- Command: `cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`
- Run ID: `18ac77308a4b2f70-c7b9-0`
- Account preflight: PASS
- Available pUSD units: `12538612`
- Reserved pUSD units: `0`
- Open orders: `0`
- Recent trades: `17`
- Expected block reasons: `missing_cli_intent`, closed/expired market blockers, missing book, and `approval_attempt_already_consumed`

Settlement P&L:

- Final pre-submit available pUSD: `10.070772`
- Post-fill/pre-redeem available pUSD: `7.418612`
- Final post-redeem available pUSD: `12.538612`
- Filled shares / settlement value: `5.12` Down shares / `5.120000 pUSD`
- Total trade cost from activity: `2.652160 pUSD`
- Realized P&L versus final pre-submit balance: `+2.467840 pUSD`

Fee discrepancy:

- Approval artifact max fee estimate: `0.06 pUSD`
- Official Polymarket fee docs define `C` as number of shares traded in `fee = C * feeRate * p * (1 - p)`.
- For this crypto trade, `C=5.12`, `feeRate=0.072`, and `p=0.5`, so implied fee is `0.092160 pUSD`.
- The public activity readback's `usdcSize=2.65216` matches order making amount `2.56` plus `0.092160`; future fee gates must use share count, not notional, for this estimate.
- Closeout fix: LA3 preflight now computes the official taker-fee estimate from shares traded and blocks with `approved_fee_estimate_below_official_taker_fee` when the approved max fee is too low. A regression test covers the exact `2.56 pUSD` at `0.50` case that implies `0.092160 pUSD`.

## Commands Run

```bash
git status --short --branch
cargo fmt
cargo test --offline
cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
curl -sS 'https://gamma-api.polymarket.com/markets/slug/btc-updown-15m-1777925700'
curl -sS 'https://data-api.polymarket.com/activity?user=<approved_funder>&market=0x00fac5682f5cf756e1602c88709d999b663d94b09c8c9da2eaa3e23444df6ea6&limit=20'
curl -sS 'https://data-api.polymarket.com/positions?user=<approved_funder>&market=0x00fac5682f5cf756e1602c88709d999b663d94b09c8c9da2eaa3e23444df6ea6'
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --human-approved --approval-id LA3-2026-05-04-004 --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md
cargo test --offline la3_reference_evidence_accepts_rtds_chainlink_for_requested_asset --bin polymarket-15m-arb-bot
cargo test --offline la3_approval_asset_symbol_is_not_hardcoded_to_btc --bin polymarket-15m-arb-bot
cargo test --offline live_alpha_preflight_blocks_underestimated_official_taker_fee
cargo run --offline -- --config config/default.toml validate --local-only
cargo fmt --check
cargo clippy --offline -- -D warnings
git diff --check
cargo test --offline
rg -n -i "(submit.*order|post.*order|place.*order|create.*order|createAndPostOrder|createAndPostMarketOrder|postOrder|postOrders|cancel.*order|cancelOrder|cancelOrders|cancelAll|/order|/orders|/cancel|live[_ -]?order|live[_ -]?trading|FOK|FAK|GTD|GTC|post[_ -]?only)" src Cargo.toml config runbooks *.md
rg -n -i "(wallet|private[_ -]?key|secret|api[_ -]?key|passphrase|signing|signature|mnemonic|seed|ethers|web3|alloy|secp256k1|k256|ecdsa|POLY_API_KEY|POLY_SECRET|POLY_PASSPHRASE|0x[0-9a-fA-F]{64})" src Cargo.toml config runbooks verification *.md
rg -n -i "(LIVE_ORDER_PLACEMENT_ENABLED|LIVE_ALPHA|live-alpha-orders|kill_switch|geoblock|heartbeat|reconciliation|risk_halt)" src Cargo.toml config
rg -n "HEARTBEAT_NETWORK_POST_ENABLED|USER_CHANNEL_NETWORK_ENABLED|cancel_all|cancel-all|post_orders|postOrders|OrderType::FOK|OrderType::GTC" src Cargo.toml config
```

## Verification Results

- `cargo run -- --config config/local.toml live-alpha-preflight --read-only --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`: PASS command exit, blocked before submit with run ID `18ac6908e49cd6d0-5bdb-0`.
- `cargo run --features live-alpha-orders -- --config config/local.toml live-alpha-fill-canary --dry-run --approval-artifact verification/2026-05-04-live-alpha-la3-approval.md`: PASS command exit, blocked before submit with run ID `18ac69098393a200-5bdc-0`.
- Funded/L2 retry commands: PASS command exit for read-only preflight run `18ac72d16a872788-a4b3-0` and dry-run runs `18ac72e5a0e2e970-a59d-0`, `18ac72f1447f0478-a64d-0`, and `18ac72f8f3b1e080-a6bd-0`; all stopped before submit.
- Final LA3 commands: PASS command exit for dry-run `18ac742809bd7b70-af72-0`, final submit `18ac742fd4a83e40-b000-0`, and post-run read-only check `18ac7437fce59170-b065-0`.
- Settlement follow-up commands: PASS for Gamma resolution readback, public activity/positions readback, and authenticated post-settlement readback `18ac77308a4b2f70-c7b9-0`.
- Targeted LA3 reference tests: PASS.
- Fee discrepancy closeout test: PASS after adding the fail-closed official taker-fee estimate guard.
- `cargo run --offline -- --config config/default.toml validate --local-only`: PASS on final closeout rerun, run ID `18ac798adafc5910-f6b7-0`; default validation stayed blocked with `live_order_placement_enabled=false`, `live_alpha_enabled=false`, and no live order placement.
- `cargo fmt --check`: PASS.
- `cargo test --offline`: PASS, 304 library tests, 20 main tests, 0 doc tests.
- `cargo clippy --offline -- -D warnings`: PASS.
- `git diff --check`: PASS.
- Live Alpha order/cancel scan: PASS with expected LA3 hits for the gated single `post_order` path and expected existing disabled cancel/readback/journal docs/tests. No `post_orders` batch submit path was added.
- Live Alpha no-secret scan: PASS with expected public wallet/funder/token/condition IDs in the approval/evidence text and tests, ignored local config public address/signature-type fields, and secret-handle names only. No secret values, API-key values, seed phrase, raw L2 credential, or private-key material were added.
- Live Alpha gate scan: PASS with expected LA3 gate/config/journal/reconciliation hits, including ignored local `config/local.toml` operator settings that are not staged for PR.
- Targeted forbidden-boundary scan: PASS. Expected hits included disabled `HEARTBEAT_NETWORK_POST_ENABLED=false`, disabled `USER_CHANNEL_NETWORK_ENABLED=false`, existing LB5/LB6 cancel-all-disabled code, and LA3 prompt text saying never cancel-all. No LA3 `OrderType::FOK`, `OrderType::GTC`, `post_orders`, heartbeat network enablement, or user-channel network enablement path was added.

## Next Action

Do not run another `live-alpha-fill-canary --human-approved`. LA3 has consumed the one-attempt cap. Settlement follow-up is complete; remaining action is human review plus final LA3-only verification/PR handling:

- preserve the settlement evidence and fee discrepancy note
- fix the fee-estimation formula before any future fee-gated live phase
- preserve the local cap state and journal evidence
- keep unapproved FOK, marketable-limit, SELL, retry, cancel-all, taker strategy, maker autonomy, LA4, LA5, and scaling disabled
