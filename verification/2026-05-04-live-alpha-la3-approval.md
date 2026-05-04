# 2026-05-04 Live Alpha LA3 Approval Artifact

## Approval Status

- Approval ID: `LA3-2026-05-04-004`
- Approval status: APPROVED FOR LA3 ONLY, within the exact limits below; CONSUMED by the one LA3 submit run `18ac742fd4a83e40-b000-0`.
- Approved by: Jonah Grigoryan / operator, via explicit Codex instruction on 2026-05-04.
- Prepared by: Codex, using repo evidence, current read-only Polymarket/browser evidence, and official Polymarket docs.
- Publication status: local approval artifact. This file records public wallet/funder identifiers; do not push or otherwise publish it without an explicit publication review.

This approval authorized one controlled LA3 fill canary only and is now consumed. It does not authorize LA4, LA5, LA6, LA7, LA8, autonomous live trading, strategy-selected live orders, maker micro mode, quote manager mode, scaling, cancel-all, repeated canaries, another LA3 submit, resetting/bypassing the LA3 one-attempt cap, or resetting/bypassing the consumed LB6 one-order cap.

## Approved Host And Account

- Approved host/session: current Brazil host/session from this machine.
- Local hostname evidence: `Jonahs-MacBook-Pro.local`; local host name `Jonahs-MacBook-Pro`.
- Current geoblock evidence: PASS, `blocked=false`, country `BR`, region `SP`, checked 2026-05-04T19:35:58Z.
- Browser/account funding evidence: operator reported the Polymarket account was freshly funded with more than `$12`; authenticated live-account readback must prove sufficient pUSD and allowance before submit.
- Funding note recorded at approval time: the approved canary required at least `2.62 pUSD` available and allowed before submit (`2.56 pUSD` max notional plus `0.06 pUSD` max fee). Closeout later found this fee estimate was incorrect; the official fee formula implies `0.092160 pUSD` at the observed `0.50` execution price, so the true notional plus fee was `2.652160 pUSD`.
- Approved wallet/signer address: `0x280ca8b14386Fe4203670538CCdE636C295d74E9`.
- Approved funder/proxy address: `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159`.
- Signature type: `1` / `POLY_PROXY`, matching the local approved readback config precedent.

Before any submit, LA3 preflight must recheck geoblock, authenticated balance/allowance, open orders, recent trades, heartbeat, journal health, market status, book freshness, reference freshness, and reconciliation health. If any check fails or differs materially from this artifact, no order may be submitted.

## Approved Market And Order Intent

- Approved asset: BTC only for this LA3 canary.
- Approved market slug: `btc-updown-15m-1777925700`.
- Market question: `Bitcoin Up or Down - May 4, 4:15PM-4:30PM ET`.
- Condition ID: `0x00fac5682f5cf756e1602c88709d999b663d94b09c8c9da2eaa3e23444df6ea6`.
- Approved outcome: `Down`.
- Approved token ID: `27694987584135655174383601846064222249431031901378233816925163016282018353279`.
- Approved side: `BUY`.
- Approved order type: `FAK` only.
- Approved amount_or_size: `2.56 pUSD` BUY spend amount.
- Approved max notional: `2.56 pUSD`.
- Approved max fee estimate: `0.06 pUSD`.
- Approved worst-price limit: `0.51`.
- Approved max slippage bound: `300 bps` from the observed `0.50` best ask, capped by the `0.51` worst-price limit.
- Approved max open orders after run: `0`.
- Approved retry count: `0`.

The market snapshot used to prepare this artifact showed `active=true`, `closed=false`, `acceptingOrders=true`, order min size `5`, tick size `0.01`, and end time `2026-05-04T20:30:00Z`. The approved Down token order book snapshot showed best bid `0.49` size `565`, best ask `0.50` size `10`, hash `5d6e335ddd9c92e87fa702bd1a717a3e029ca821`, and timestamp `1777923129857`.

Because 15-minute markets expire quickly, this market approval is valid only while this exact market remains active, not closed, accepting orders, and fresh under LA3 preflight. If this market is stale, closed, not accepting orders, below required liquidity, below the approved visible-depth/min-size constraints, or past the no-trade cutoff, no live submission is authorized from this artifact until the artifact is updated with a new exact market/token binding and approval ID.

## Fee And Order-Type Rationale

Official Polymarket docs state that all orders are expressed as limit orders, market orders are marketable limit orders, and FAK fills immediately what is available then cancels the remainder. Docs also state that FOK/FAK are market order types and that BUY FOK/FAK specifies the dollar amount to spend.

Official fee docs state crypto taker fee rate is `0.072` and use:

```text
fee = C * feeRate * p * (1 - p)
```

Approval-time discrepancy: this artifact incorrectly treated the `2.56 pUSD` BUY spend as `C`. Official docs define `C` as shares traded. At the observed `0.50` execution price, the order bought `5.12` shares, so the implied fee was:

```text
5.12 * 0.072 * 0.50 * (1 - 0.50) = 0.092160 pUSD
```

That exceeded the approved `0.06 pUSD` max fee estimate. This artifact is already consumed and does not authorize another submit. Closeout code now estimates crypto taker fee from shares traded and blocks future LA3-style preflight when the approved max fee is below the official taker-fee estimate.

## Rollback And Monitoring

- Rollback owner: Jonah Grigoryan / operator.
- Monitoring owner: Jonah Grigoryan / operator.
- Rollback command: run the LA3 readback/reconciliation path immediately; if an unexpected live open order exists, stop and use only the approved exact single-order cancel procedure for the exact LA3 order ID. Do not use cancel-all.
- Incident rule: if submit status, order status, trade status, balance, reserved balance, position, fee, or SDK/Rust readback is ambiguous, write an incident note and halt.

## Hard Boundaries

- Exactly one LA3 fill attempt is approved.
- No second attempt without a new approval artifact.
- No strategy-selected live order.
- No autonomous live trading.
- No maker micro mode.
- No quote manager mode.
- No batch orders.
- No retry loop after failed or ambiguous submit.
- No FOK for this artifact.
- No GTC/GTD marketable-limit path for this artifact.
- No SELL path for this artifact.
- No cancel-all.
- No HEARTBEAT network POST enablement.
- No USER_CHANNEL network enablement.
- No global default enablement of `LIVE_ORDER_PLACEMENT_ENABLED=true`.
- No private key, API secret, seed phrase, raw L2 credential, or secret value may be written to repo/config/docs/logs/tests.

## Evidence Sources

- Local ignored config evidence: `config/local.toml` contains the approved public wallet/funder and signature type; no secret values were copied.
- Current browser evidence: Polymarket Portfolio page in the logged-in Chrome session showed the account balance and history view.
- Current geoblock evidence: `https://polymarket.com/api/geoblock` returned PASS for `BR/SP` from this host/session.
- Current public market evidence: Gamma `/markets/slug/btc-updown-15m-1777925700` and CLOB `/book?token_id=27694987584135655174383601846064222249431031901378233816925163016282018353279`.
- Official docs checked: Polymarket CLOB order overview, Polymarket orderbook docs, Polymarket API introduction, Polymarket RTDS docs, and Polymarket fees docs.
