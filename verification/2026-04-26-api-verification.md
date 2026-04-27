# API Verification: 2026-04-26

## Scope

M2 requires `API_VERIFICATION.md` sections 1, 2, 3, 8, and 9.

This note records the current gate evidence used for M2 implementation and signoff.

## Environment

- Date: 2026-04-26
- Branch: `m2/market-discovery-compliance`
- Shell network status: Polymarket hosts reachable; geoblock endpoint currently reports this shell as blocked from `US/CA`.
- Browser/docs access: official Polymarket documentation was reachable.

## Section 1: V2 Endpoint And Cutover Behavior

Result: PASS for M2.

Official docs confirm:

- CLOB V2 go-live is April 28, 2026 around 11:00 UTC.
- Before cutover, clients should test against `https://clob-v2.polymarket.com`.
- After cutover, V2 takes over `https://clob.polymarket.com`.
- Cutover includes about 1 hour of downtime and all open orders are wiped.
- V2 uses new exchange contracts and pUSD collateral.

Live shell evidence:

```text
cargo run -- validate --config config/default.toml
...
geoblock_blocked=true
geoblock_country=US
geoblock_region=CA
market_discovery_status=ok
market_discovery_pages=5
market_discovery_count=30
market_discovery_ineligible_count=0
market_discovery_postgres_persisted_count=30
market_discovery_postgres_readback_count=30
market_lifecycle_event_count=30
```

Implementation decision:

- Keep one configurable `polymarket.clob_rest_url`.
- Default remains `https://clob-v2.polymarket.com` because the current date is before the April 28, 2026 cutover.

## Section 2: Market Discovery Endpoint

Result: PASS for M2.

Official docs confirm:

- Endpoint: `GET https://gamma-api.polymarket.com/markets/keyset`.
- Response shape includes `markets` and optional `next_cursor`.
- `limit` supports `1..=1000`.
- `after_cursor` is the pagination field.
- `offset` is rejected.
- Response includes nested market fields including `clobTokenIds`, `outcomes`, `feesEnabled`, and `feeSchedule`.

Live shell evidence:

```text
cargo run -- validate --config config/default.toml
...
market_discovery_market=asset=ETH,slug=eth-updown-15m-1777340700,state=active,start_ts=1777340700000,end_ts=1777341600000,outcomes=Up|Down
market_discovery_market=asset=BTC,slug=btc-updown-15m-1777340700,state=active,start_ts=1777340700000,end_ts=1777341600000,outcomes=Up|Down
market_discovery_market=asset=SOL,slug=sol-updown-15m-1777340700,state=active,start_ts=1777340700000,end_ts=1777341600000,outcomes=Up|Down
```

Observed implementation detail:

- The Gamma `startDate` on current crypto interval markets is market creation/deployment time, not the 15-minute interval start.
- The actual interval is encoded in slugs shaped like `btc-updown-15m-1777340700`, `eth-updown-15m-1777340700`, and `sol-updown-15m-1777340700`.
- M2 discovery therefore identifies BTC/ETH/SOL 15-minute up/down markets from the slug interval and validates `endDate` against that interval.

Implementation decision:

- Use keyset pagination only.
- Parse `clobTokenIds` and `outcomes` as either JSON arrays or JSON-encoded strings.
- Mark matching markets ineligible when required metadata is missing.

## Section 3: Token IDs And Outcome Mapping

Result: PASS for M2.

Official docs confirm:

- CLOB market info endpoint accepts `condition_id`.
- CLOB market info response includes `t`, an array of token objects.
- Each token object includes:
  - `t`: token ID
  - `o`: outcome label
- Response also includes CLOB-level market parameters such as `mos`, `mts`, `mbf`, `tbf`, and `fd`.

Live shell evidence:

```text
curl -fsS --max-time 15 'https://gamma-api.polymarket.com/markets/keyset?limit=20&closed=false&order=startDate&ascending=false' | jq -r '.markets[] | select(.slug|test("^(btc|eth|sol)-updown-15m-")) | [.slug, .conditionId] | @tsv'
sol-updown-15m-1777341600  0xf3844ea779dfa7653ed91c93483ca4d26a6cd90eeffb4c520cca127b376599d3
btc-updown-15m-1777341600  0x2cbbaa5fd03cc05a6a4c6c60a2d117a8519dcde01679b7a598e7c098ea194808
eth-updown-15m-1777341600  0x93223bfae87450e90a0ef224e19c8d9ad2552c83aa06a412692ed6cdcc101815

curl -fsS --max-time 15 'https://clob-v2.polymarket.com/clob-markets/0x2cbbaa5fd03cc05a6a4c6c60a2d117a8519dcde01679b7a598e7c098ea194808' | jq '{tokens: .t, min_order_size: .mos, min_tick_size: .mts, maker_fee_bps: .mbf, taker_fee_bps: .tbf}'
{
  "tokens": [
    {
      "t": "46723911067983210140844203476194941969066255330244026086202646572976196111931",
      "o": "Up"
    },
    {
      "t": "57050326501966890300149417632726385238677362668728784707188203479960770419925",
      "o": "Down"
    }
  ],
  "min_order_size": 5,
  "min_tick_size": 0.01,
  "maker_fee_bps": 1000,
  "taker_fee_bps": 1000
}
```

Implementation decision:

- Prefer CLOB market info token/outcome mapping when available.
- Fall back to Gamma `clobTokenIds` + `outcomes` pairing.
- Store explicit token/outcome pairs; never rely on unlabeled token order after parsing.

## Section 8: Geoblock Endpoint

Result: PASS for M2.

Official docs confirm:

- Endpoint: `GET https://polymarket.com/api/geoblock`.
- Response fields: `blocked`, `ip`, `country`, `region`.
- The endpoint is on `polymarket.com`.
- The US is listed as blocked.

Browser evidence:

```text
Response shape observed with fields: blocked, ip, country, region.
IP is intentionally not stored in this repo note.
Observed country: US
Observed blocked: true
```

Live shell evidence:

```text
cargo run -- validate --config config/default.toml
...
geoblock_blocked=true
geoblock_country=US
geoblock_region=CA
```

Implementation decision:

- `paper` mode must fail closed on blocked or unreachable geoblock checks.
- `validate` mode reports geoblock status and discovery status without running strategy logic.
- No bypass behavior is implemented.

## Section 9: Rate Limits

Result: PASS for M2.

Official docs confirm:

- Rate limits are enforced through Cloudflare throttling.
- Gamma base URL: `https://gamma-api.polymarket.com`.
- Gamma general: 4,000 req / 10s.
- Gamma `/markets`: 300 req / 10s.
- Gamma `/markets` + `/events` listing: 900 req / 10s.
- CLOB base URL: `https://clob.polymarket.com`.
- CLOB general: 9,000 req / 10s.
- CLOB market data examples:
  - `/book`: 1,500 req / 10s
  - `/books`: 500 req / 10s
  - `/price`: 1,500 req / 10s
  - `/prices`: 500 req / 10s
  - `/midpoint`: 1,500 req / 10s
  - `/midpoints`: 500 req / 10s

Implementation decision:

- Add conservative config budgets for market discovery.
- Use REST only for startup, recovery, and metadata.
- Keep WebSocket as the live market data path for later milestones.
- Treat 429/Cloudflare throttling as a degraded API state.

## Gate Status

M2 implementation passed the required read-only live gate.

- `validate` mode reached the geoblock endpoint and reported `blocked=true` for `US/CA`.
- `validate` mode reached Gamma keyset market discovery and listed active BTC/ETH/SOL 15-minute up/down markets.
- 30 matching markets were discovered across 5 pages with 0 ineligible records in the final gate run.
- 30 discovered markets were persisted to Postgres and read back from Postgres in the final gate run.
- 30 market lifecycle normalized events were emitted.
- Token/outcome labels were extracted as explicit `Up|Down` pairs for listed markets.
- `paper` mode failed closed from the blocked geoblock response.
- No live order placement or signing path was added.

M2 is ready to close with the local verification commands passing.
