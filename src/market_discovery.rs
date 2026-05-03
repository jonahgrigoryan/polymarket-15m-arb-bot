use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{Duration as TimeDuration, OffsetDateTime};

use crate::domain::{
    is_asset_matched_chainlink_resolution_source, Asset, FeeParameters, Market,
    MarketLifecycleState, OutcomeToken,
};
use crate::events::{EventEnvelope, NormalizedEvent};
use crate::storage::{StorageBackend, StorageResult};

pub const MODULE: &str = "market_discovery";

const FIFTEEN_MINUTES_MS: i64 = 15 * 60 * 1_000;
const DURATION_TOLERANCE_MS: i64 = 60 * 1_000;
const DISCOVERY_END_WINDOW_HOURS: i64 = 2;

#[derive(Debug, Clone)]
pub struct MarketDiscoveryClient {
    http: reqwest::Client,
    gamma_markets_url: String,
    clob_rest_url: String,
    page_limit: u16,
    max_pages: u16,
}

impl MarketDiscoveryClient {
    pub fn new(
        gamma_markets_url: impl Into<String>,
        clob_rest_url: impl Into<String>,
        page_limit: u16,
        max_pages: u16,
        timeout_ms: u64,
    ) -> DiscoveryResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| DiscoveryError::ClientBuild(source.to_string()))?;

        Ok(Self {
            http,
            gamma_markets_url: gamma_markets_url.into(),
            clob_rest_url: clob_rest_url.into(),
            page_limit,
            max_pages,
        })
    }

    pub async fn discover_crypto_15m_markets(&self) -> DiscoveryResult<DiscoveryRun> {
        let mut markets = Vec::new();
        let mut after_cursor = None;
        let mut pages_fetched = 0;
        let window = DiscoveryWindow::current()?;

        while pages_fetched < self.max_pages {
            let page = self
                .fetch_gamma_page(after_cursor.as_deref(), &window)
                .await?;
            pages_fetched += 1;

            for gamma_market in page.markets {
                if !is_candidate_crypto_15m_market(&gamma_market) {
                    continue;
                }

                let (clob_info, clob_error) = match gamma_market.condition_id.as_deref() {
                    Some(condition_id) if !condition_id.trim().is_empty() => {
                        match self.fetch_clob_market_info(condition_id).await {
                            Ok(info) => (Some(info), None),
                            Err(error) => (None, Some(error.to_string())),
                        }
                    }
                    _ => (
                        None,
                        Some("missing conditionId for CLOB lookup".to_string()),
                    ),
                };
                markets.push(map_gamma_market(
                    gamma_market,
                    clob_info.as_ref(),
                    clob_error.as_deref(),
                ));
            }

            after_cursor = page.next_cursor;
            if after_cursor.as_deref().unwrap_or_default().is_empty() {
                break;
            }
        }

        Ok(DiscoveryRun {
            markets,
            pages_fetched,
            next_cursor: after_cursor,
        })
    }

    pub async fn discover_crypto_15m_market_by_slug(
        &self,
        slug: &str,
    ) -> DiscoveryResult<Option<Market>> {
        let Some(gamma_market) = self.fetch_gamma_market_by_slug(slug).await? else {
            return Ok(None);
        };
        if !is_candidate_crypto_15m_market(&gamma_market) {
            return Ok(None);
        }

        let (clob_info, clob_error) = match gamma_market.condition_id.as_deref() {
            Some(condition_id) if !condition_id.trim().is_empty() => {
                match self.fetch_clob_market_info(condition_id).await {
                    Ok(info) => (Some(info), None),
                    Err(error) => (None, Some(error.to_string())),
                }
            }
            _ => (
                None,
                Some("missing conditionId for CLOB lookup".to_string()),
            ),
        };

        Ok(Some(map_gamma_market(
            gamma_market,
            clob_info.as_ref(),
            clob_error.as_deref(),
        )))
    }

    async fn fetch_gamma_page(
        &self,
        after_cursor: Option<&str>,
        window: &DiscoveryWindow,
    ) -> DiscoveryResult<GammaPage> {
        let mut request = self.http.get(&self.gamma_markets_url).query(&[
            ("limit", self.page_limit.to_string()),
            ("active", "true".to_string()),
            ("closed", "false".to_string()),
            ("order", "endDate".to_string()),
            ("ascending", "true".to_string()),
            ("end_date_min", window.end_date_min.clone()),
            ("end_date_max", window.end_date_max.clone()),
        ]);

        if let Some(after_cursor) = after_cursor {
            request = request.query(&[("after_cursor", after_cursor)]);
        }

        let response = request
            .send()
            .await
            .map_err(|source| DiscoveryError::Request {
                url: self.gamma_markets_url.clone(),
                message: source.to_string(),
            })?;

        decode_response(response, "gamma_markets").await
    }

    async fn fetch_gamma_market_by_slug(&self, slug: &str) -> DiscoveryResult<Option<GammaMarket>> {
        let url = self.gamma_market_slug_url(slug);
        let response =
            self.http
                .get(&url)
                .send()
                .await
                .map_err(|source| DiscoveryError::Request {
                    url: url.clone(),
                    message: source.to_string(),
                })?;
        if Self::is_missing_gamma_slug_response(response.status()) {
            return Ok(None);
        }

        decode_response(response, "gamma_market_by_slug")
            .await
            .map(Some)
    }

    fn gamma_market_slug_url(&self, slug: &str) -> String {
        let base = self.gamma_markets_url.trim_end_matches('/');
        if let Some(markets_base) = base.strip_suffix("/keyset") {
            format!("{}/slug/{}", markets_base.trim_end_matches('/'), slug)
        } else {
            format!("{}/slug/{}", base, slug)
        }
    }

    fn is_missing_gamma_slug_response(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::NOT_FOUND
    }

    async fn fetch_clob_market_info(&self, condition_id: &str) -> DiscoveryResult<ClobMarketInfo> {
        let url = format!(
            "{}/clob-markets/{}",
            self.clob_rest_url.trim_end_matches('/'),
            condition_id
        );
        let response =
            self.http
                .get(&url)
                .send()
                .await
                .map_err(|source| DiscoveryError::Request {
                    url: url.clone(),
                    message: source.to_string(),
                })?;

        decode_response(response, "clob_market_info").await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveryWindow {
    end_date_min: String,
    end_date_max: String,
}

impl DiscoveryWindow {
    fn current() -> DiscoveryResult<Self> {
        let now = OffsetDateTime::now_utc();
        Self::from_bounds(now, now + TimeDuration::hours(DISCOVERY_END_WINDOW_HOURS))
    }

    fn from_bounds(start: OffsetDateTime, end: OffsetDateTime) -> DiscoveryResult<Self> {
        Ok(Self {
            end_date_min: format_rfc3339(start)?,
            end_date_max: format_rfc3339(end)?,
        })
    }
}

fn format_rfc3339(timestamp: OffsetDateTime) -> DiscoveryResult<String> {
    timestamp
        .format(&Rfc3339)
        .map_err(|source| DiscoveryError::ResponseDecode {
            operation: "discovery_window",
            message: source.to_string(),
        })
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    operation: &'static str,
) -> DiscoveryResult<T> {
    let status = response.status();
    let url = response.url().to_string();
    if status.as_u16() == 429 {
        return Err(DiscoveryError::RateLimited { url });
    }
    if !status.is_success() {
        return Err(DiscoveryError::HttpStatus {
            url,
            status: status.as_u16(),
        });
    }

    response
        .json::<T>()
        .await
        .map_err(|source| DiscoveryError::ResponseDecode {
            operation,
            message: source.to_string(),
        })
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryRun {
    pub markets: Vec<Market>,
    pub pages_fetched: u16,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GammaPage {
    #[serde(default)]
    markets: Vec<GammaMarket>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GammaMarket {
    id: Option<Value>,
    slug: Option<String>,
    question: Option<String>,
    title: Option<String>,
    description: Option<String>,
    condition_id: Option<String>,
    resolution_source: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    active: Option<bool>,
    closed: Option<bool>,
    enable_order_book: Option<bool>,
    accepting_orders: Option<bool>,
    order_price_min_tick_size: Option<Value>,
    order_min_size: Option<Value>,
    clob_token_ids: Option<Value>,
    outcomes: Option<Value>,
    fees_enabled: Option<bool>,
    #[serde(alias = "fee_schedule")]
    fee_schedule: Option<Value>,
    #[serde(default)]
    events: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClobMarketInfo {
    #[serde(default)]
    t: Vec<ClobToken>,
    mos: Option<Value>,
    mts: Option<Value>,
    mbf: Option<Value>,
    tbf: Option<Value>,
    fd: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClobToken {
    t: String,
    o: String,
}

pub fn persist_discovered_markets(
    storage: &impl StorageBackend,
    run_id: &str,
    recv_wall_ts: i64,
    recv_mono_ns: u64,
    markets: &[Market],
) -> StorageResult<usize> {
    for (index, market) in markets.iter().enumerate() {
        storage.upsert_market(market.clone())?;
        append_market_lifecycle_event(storage, run_id, recv_wall_ts, recv_mono_ns, index, market)?;
    }

    Ok(markets.len())
}

pub fn emit_market_lifecycle_events(
    storage: &impl StorageBackend,
    run_id: &str,
    recv_wall_ts: i64,
    recv_mono_ns: u64,
    markets: &[Market],
) -> StorageResult<usize> {
    for (index, market) in markets.iter().enumerate() {
        append_market_lifecycle_event(storage, run_id, recv_wall_ts, recv_mono_ns, index, market)?;
    }

    Ok(markets.len())
}

fn append_market_lifecycle_event(
    storage: &impl StorageBackend,
    run_id: &str,
    recv_wall_ts: i64,
    recv_mono_ns: u64,
    index: usize,
    market: &Market,
) -> StorageResult<()> {
    let payload = NormalizedEvent::MarketDiscovered {
        market: market.clone(),
    };
    let event = EventEnvelope::new(
        run_id,
        format!("market-discovered-{}-{}", market.market_id, index),
        "market_discovery",
        recv_wall_ts,
        recv_mono_ns + index as u64,
        index as u64,
        payload,
    );
    storage.append_normalized_event(event)?;

    Ok(())
}

fn map_gamma_market(
    gamma: GammaMarket,
    clob_info: Option<&ClobMarketInfo>,
    clob_error: Option<&str>,
) -> Market {
    let slug_interval = gamma.slug.as_deref().and_then(parse_updown_slug_interval);
    let asset = slug_interval
        .as_ref()
        .map(|interval| interval.asset)
        .or_else(|| detect_asset(&gamma))
        .unwrap_or(Asset::Btc);
    let mut ineligibility_reasons = Vec::new();
    let market_id = gamma
        .id
        .as_ref()
        .and_then(value_to_string)
        .or_else(|| gamma.condition_id.clone())
        .unwrap_or_else(|| "unknown-market".to_string());
    let slug = required_string(gamma.slug.clone(), "slug", &mut ineligibility_reasons);
    let title = required_string(
        gamma.title.clone().or(gamma.question.clone()),
        "question/title",
        &mut ineligibility_reasons,
    );
    let condition_id = required_string(
        gamma.condition_id.clone(),
        "conditionId",
        &mut ineligibility_reasons,
    );
    let fallback_start_ts = parse_ts_ms(gamma.start_date.as_deref()).unwrap_or_default();
    let parsed_end_ts = parse_ts_ms(gamma.end_date.as_deref());
    let start_ts = slug_interval
        .as_ref()
        .map(|interval| interval.start_ts)
        .unwrap_or(fallback_start_ts);
    let end_ts = parsed_end_ts
        .or_else(|| {
            slug_interval
                .as_ref()
                .map(|interval| interval.start_ts + interval.duration_ms)
        })
        .unwrap_or_else(|| {
            ineligibility_reasons.push("missing or invalid endDate".to_string());
            0
        });
    let mut tick_size = value_to_f64(gamma.order_price_min_tick_size.as_ref()).unwrap_or(0.0);
    let mut min_order_size = value_to_f64(gamma.order_min_size.as_ref()).unwrap_or(0.0);

    if let Some(error) = clob_error {
        ineligibility_reasons.push(format!("CLOB market info unavailable: {error}"));
    }

    if let Some(clob_info) = clob_info {
        if let Some(value) = value_to_f64(clob_info.mts.as_ref()) {
            tick_size = value;
        }
        if let Some(value) = value_to_f64(clob_info.mos.as_ref()) {
            min_order_size = value;
        }
    }

    if tick_size <= 0.0 {
        ineligibility_reasons.push("missing tick size".to_string());
    }
    if min_order_size <= 0.0 {
        ineligibility_reasons.push("missing minimum order size".to_string());
    }
    if clob_info
        .and_then(|info| value_to_f64(info.mbf.as_ref()))
        .is_none()
    {
        ineligibility_reasons.push("missing maker fee setting".to_string());
    }
    if clob_info
        .and_then(|info| value_to_f64(info.tbf.as_ref()))
        .is_none()
    {
        ineligibility_reasons.push("missing taker fee setting".to_string());
    }
    if gamma.fees_enabled.is_none() {
        ineligibility_reasons.push("missing fees enabled setting".to_string());
    }
    if gamma.fee_schedule.is_none() && clob_info.and_then(|info| info.fd.as_ref()).is_none() {
        ineligibility_reasons.push("missing fee schedule".to_string());
    }

    let outcomes = outcome_tokens(&gamma, clob_info);
    if outcomes.len() != 2 {
        ineligibility_reasons.push("expected exactly two outcome tokens".to_string());
    }
    if let Some(interval) = slug_interval.as_ref() {
        let expected_end_ts = interval.start_ts + interval.duration_ms;
        if (end_ts - expected_end_ts).abs() > DURATION_TOLERANCE_MS {
            ineligibility_reasons.push("endDate does not match slug interval".to_string());
        }
    } else {
        ineligibility_reasons.push("missing 15-minute up/down slug interval".to_string());
    }

    let resolution_source = gamma
        .resolution_source
        .as_deref()
        .unwrap_or_default()
        .trim();
    if resolution_source.is_empty() {
        ineligibility_reasons.push("missing resolution source".to_string());
    } else if !is_asset_matched_chainlink_resolution_source(asset, resolution_source) {
        ineligibility_reasons
            .push("resolution source is not the asset-matched Chainlink stream".to_string());
    }
    if !resolution_rules_match_asset(asset, &gamma) {
        ineligibility_reasons.push(
            "resolution rules do not identify the asset-matched Chainlink stream".to_string(),
        );
    }
    if gamma.enable_order_book == Some(false) {
        ineligibility_reasons.push("order book disabled".to_string());
    }
    if gamma.accepting_orders == Some(false) {
        ineligibility_reasons.push("not accepting orders".to_string());
    }

    let lifecycle_state = if gamma.closed == Some(true) {
        MarketLifecycleState::Closed
    } else if !ineligibility_reasons.is_empty() {
        MarketLifecycleState::Ineligible
    } else if gamma.active.unwrap_or(false) {
        MarketLifecycleState::Active
    } else {
        MarketLifecycleState::Discovered
    };

    Market {
        market_id,
        slug,
        title,
        asset,
        condition_id,
        outcomes,
        start_ts,
        end_ts,
        resolution_source: gamma.resolution_source.clone(),
        tick_size,
        min_order_size,
        fee_parameters: fee_parameters(&gamma, clob_info),
        lifecycle_state,
        ineligibility_reason: if ineligibility_reasons.is_empty() {
            None
        } else {
            Some(ineligibility_reasons.join("; "))
        },
    }
}

fn is_candidate_crypto_15m_market(gamma: &GammaMarket) -> bool {
    gamma
        .slug
        .as_deref()
        .and_then(parse_updown_slug_interval)
        .is_some_and(|interval| interval.duration_ms == FIFTEEN_MINUTES_MS)
        && has_up_down_outcomes(gamma)
        && gamma.closed != Some(true)
}

fn detect_asset(gamma: &GammaMarket) -> Option<Asset> {
    let haystack = market_haystack(gamma);
    if contains_any(&haystack, &["btc", "bitcoin"]) {
        Some(Asset::Btc)
    } else if contains_any(&haystack, &["eth", "ethereum"]) {
        Some(Asset::Eth)
    } else if contains_any(&haystack, &["sol", "solana"]) {
        Some(Asset::Sol)
    } else {
        None
    }
}

fn has_up_down_outcomes(gamma: &GammaMarket) -> bool {
    let outcomes = parse_string_vec(gamma.outcomes.as_ref());
    let outcome_text = outcomes.join(" ").to_lowercase();
    (outcome_text.contains("up") && outcome_text.contains("down"))
        || market_haystack(gamma).contains("up or down")
        || market_haystack(gamma).contains("up/down")
}

fn outcome_tokens(gamma: &GammaMarket, clob_info: Option<&ClobMarketInfo>) -> Vec<OutcomeToken> {
    if let Some(clob_info) = clob_info {
        let tokens = clob_info
            .t
            .iter()
            .filter(|token| !token.t.trim().is_empty() && !token.o.trim().is_empty())
            .map(|token| OutcomeToken {
                token_id: token.t.clone(),
                outcome: token.o.clone(),
            })
            .collect::<Vec<_>>();
        if !tokens.is_empty() {
            return tokens;
        }
    }

    let token_ids = parse_string_vec(gamma.clob_token_ids.as_ref());
    let outcomes = parse_string_vec(gamma.outcomes.as_ref());
    token_ids
        .into_iter()
        .zip(outcomes)
        .filter(|(token_id, outcome)| !token_id.trim().is_empty() && !outcome.trim().is_empty())
        .map(|(token_id, outcome)| OutcomeToken { token_id, outcome })
        .collect()
}

fn fee_parameters(gamma: &GammaMarket, clob_info: Option<&ClobMarketInfo>) -> FeeParameters {
    let mut raw_fee_config = gamma.fee_schedule.clone();

    if raw_fee_config.is_none() {
        raw_fee_config = clob_info.and_then(|info| info.fd.clone());
    }

    let taker_fee_bps = raw_fee_config
        .as_ref()
        .and_then(|raw| fee_rate(raw).map(|rate| rate * 0.25 * 10_000.0))
        .or_else(|| clob_info.and_then(|info| value_to_f64(info.tbf.as_ref())))
        .unwrap_or(0.0);

    FeeParameters {
        fees_enabled: gamma.fees_enabled.unwrap_or(raw_fee_config.is_some()),
        maker_fee_bps: 0.0,
        taker_fee_bps,
        raw_fee_config,
    }
}

fn resolution_rules_match_asset(asset: Asset, gamma: &GammaMarket) -> bool {
    let haystack = market_haystack(gamma);
    haystack.contains("chainlink")
        && haystack.contains("data stream")
        && (haystack.contains(asset.chainlink_resolution_source())
            || haystack.contains(asset.chainlink_symbol()))
        && (haystack.contains(&asset.symbol().to_ascii_lowercase())
            || haystack.contains(asset.display_name()))
}

fn fee_rate(raw_fee_config: &Value) -> Option<f64> {
    let rate = raw_fee_config
        .get("r")
        .or_else(|| raw_fee_config.get("rate"))
        .and_then(|value| value_to_f64(Some(value)))?;
    if rate.is_finite() && rate >= 0.0 {
        Some(rate)
    } else {
        None
    }
}

fn required_string(
    value: Option<String>,
    field_name: &str,
    ineligibility_reasons: &mut Vec<String>,
) -> String {
    match value {
        Some(value) if !value.trim().is_empty() => value,
        _ => {
            ineligibility_reasons.push(format!("missing {field_name}"));
            String::new()
        }
    }
}

fn parse_ts_ms(value: Option<&str>) -> Option<i64> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|timestamp| timestamp.unix_timestamp_nanos() / 1_000_000)
        .and_then(|timestamp| i64::try_from(timestamp).ok())
}

fn market_haystack(gamma: &GammaMarket) -> String {
    let mut values = [
        gamma.slug.as_deref().unwrap_or_default(),
        gamma.question.as_deref().unwrap_or_default(),
        gamma.title.as_deref().unwrap_or_default(),
        gamma.description.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase();

    for event in &gamma.events {
        values.push(' ');
        values.push_str(&event.to_string().to_lowercase());
    }

    values
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UpDownSlugInterval {
    asset: Asset,
    duration_ms: i64,
    start_ts: i64,
}

fn parse_updown_slug_interval(slug: &str) -> Option<UpDownSlugInterval> {
    let parts = slug.split('-').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    if parts[1] != "updown" {
        return None;
    }
    let asset = match parts[0] {
        "btc" => Asset::Btc,
        "eth" => Asset::Eth,
        "sol" => Asset::Sol,
        _ => return None,
    };
    let duration_ms = match parts[2] {
        "15m" => FIFTEEN_MINUTES_MS,
        "5m" => 5 * 60 * 1_000,
        "4h" => 4 * 60 * 60 * 1_000,
        _ => return None,
    };
    let start_ts = parts[3].parse::<i64>().ok()?.checked_mul(1_000)?;

    Some(UpDownSlugInterval {
        asset,
        duration_ms,
        start_ts,
    })
}

fn parse_string_vec(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(value_to_string).collect(),
        Some(Value::String(value)) => {
            serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| vec![value.clone()])
        }
        Some(value) => value_to_string(value).into_iter().collect(),
        None => Vec::new(),
    }
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(value) => value.as_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

pub type DiscoveryResult<T> = Result<T, DiscoveryError>;

#[derive(Debug)]
pub enum DiscoveryError {
    ClientBuild(String),
    Request {
        url: String,
        message: String,
    },
    HttpStatus {
        url: String,
        status: u16,
    },
    RateLimited {
        url: String,
    },
    ResponseDecode {
        operation: &'static str,
        message: String,
    },
}

impl Display for DiscoveryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryError::ClientBuild(message) => {
                write!(
                    formatter,
                    "failed to build discovery HTTP client: {message}"
                )
            }
            DiscoveryError::Request { url, message } => {
                write!(
                    formatter,
                    "market discovery request failed for {url}: {message}"
                )
            }
            DiscoveryError::HttpStatus { url, status } => {
                write!(
                    formatter,
                    "market discovery request to {url} returned HTTP {status}"
                )
            }
            DiscoveryError::RateLimited { url } => {
                write!(
                    formatter,
                    "market discovery request to {url} was rate limited"
                )
            }
            DiscoveryError::ResponseDecode { operation, message } => {
                write!(
                    formatter,
                    "market discovery {operation} decode failed: {message}"
                )
            }
        }
    }
}

impl Error for DiscoveryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{InMemoryStorage, StorageBackend};

    #[test]
    fn discovery_window_formats_current_end_date_bounds() {
        let start = OffsetDateTime::parse("2026-04-29T03:00:00Z", &Rfc3339).expect("start parses");
        let end = OffsetDateTime::parse("2026-04-29T05:00:00Z", &Rfc3339).expect("end parses");

        let window = DiscoveryWindow::from_bounds(start, end).expect("window formats");

        assert_eq!(window.end_date_min, "2026-04-29T03:00:00Z");
        assert_eq!(window.end_date_max, "2026-04-29T05:00:00Z");
    }

    #[test]
    fn slug_lookup_url_uses_gamma_market_slug_endpoint() {
        let client = MarketDiscoveryClient::new(
            "https://gamma-api.polymarket.com/markets/keyset",
            "https://clob.polymarket.com",
            100,
            5,
            1_000,
        )
        .expect("client builds");

        assert_eq!(
            client.gamma_market_slug_url("eth-updown-15m-1777765500"),
            "https://gamma-api.polymarket.com/markets/slug/eth-updown-15m-1777765500"
        );
    }

    #[test]
    fn slug_lookup_treats_404_as_missing_binding_only() {
        assert!(MarketDiscoveryClient::is_missing_gamma_slug_response(
            reqwest::StatusCode::NOT_FOUND
        ));
        assert!(!MarketDiscoveryClient::is_missing_gamma_slug_response(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR
        ));
        assert!(!MarketDiscoveryClient::is_missing_gamma_slug_response(
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
    }

    #[test]
    fn maps_gamma_market_with_json_string_tokens() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "1",
            "slug": "btc-updown-15m-1777248000",
            "question": "Bitcoin Up or Down - April 26, 8:00PM-8:15PM ET",
            "description": btc_description(),
            "conditionId": "condition-btc",
            "resolutionSource": Asset::Btc.chainlink_resolution_source(),
            "startDate": "2026-04-26T00:08:47.17342Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "enableOrderBook": true,
            "acceptingOrders": true,
            "orderPriceMinTickSize": "0.01",
            "orderMinSize": "5",
            "clobTokenIds": "[\"token-up\",\"token-down\"]",
            "outcomes": "[\"Up\",\"Down\"]",
            "feesEnabled": true,
            "feeSchedule": {"rate": 0.072, "exponent": 1, "takerOnly": true}
        }))
        .expect("gamma fixture parses");

        assert!(is_candidate_crypto_15m_market(&gamma));

        let clob_info = clob_fee_info();
        let market = map_gamma_market(gamma, Some(&clob_info), None);

        assert_eq!(market.asset, Asset::Btc);
        assert_eq!(market.lifecycle_state, MarketLifecycleState::Active);
        assert_eq!(market.start_ts, 1_777_248_000_000);
        assert_eq!(market.end_ts, 1_777_248_900_000);
        assert_eq!(market.outcomes.len(), 2);
        assert_eq!(market.outcomes[0].token_id, "token-up");
        assert_eq!(market.fee_parameters.maker_fee_bps, 0.0);
        assert_eq!(market.fee_parameters.taker_fee_bps, 180.0);
        assert!(market.ineligibility_reason.is_none());
    }

    #[test]
    fn clob_token_mapping_overrides_gamma_position_mapping() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "2",
            "slug": "eth-updown-15m-1777248000",
            "question": "ETH Up or Down",
            "description": eth_description(),
            "conditionId": "condition-eth",
            "resolutionSource": Asset::Eth.chainlink_resolution_source(),
            "startDate": "2026-04-26T00:08:31.193838Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "enableOrderBook": true,
            "acceptingOrders": true,
            "orderPriceMinTickSize": 0.01,
            "orderMinSize": 5,
            "clobTokenIds": ["gamma-a", "gamma-b"],
            "outcomes": ["Down", "Up"]
        }))
        .expect("gamma fixture parses");
        let clob_info: ClobMarketInfo = serde_json::from_value(serde_json::json!({
            "t": [{"t": "clob-up", "o": "Up"}, {"t": "clob-down", "o": "Down"}],
            "mos": 5,
            "mts": 0.01,
            "mbf": 0,
            "tbf": 0,
            "fd": {"r": 0.072, "e": 1, "to": true}
        }))
        .expect("clob fixture parses");

        let market = map_gamma_market(gamma, Some(&clob_info), None);

        assert_eq!(market.outcomes[0].token_id, "clob-up");
        assert_eq!(market.outcomes[0].outcome, "Up");
    }

    #[test]
    fn matching_market_with_missing_metadata_is_ineligible() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "3",
            "slug": "sol-updown-15m-1777248000",
            "question": "SOL Up or Down",
            "conditionId": "condition-sol",
            "startDate": "2026-04-26T00:08:49.522459Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "clobTokenIds": ["token-up", "token-down"],
            "outcomes": ["Up", "Down"]
        }))
        .expect("gamma fixture parses");

        let market = map_gamma_market(gamma, None, Some("timeout"));

        assert_eq!(market.lifecycle_state, MarketLifecycleState::Ineligible);
        assert!(market
            .ineligibility_reason
            .as_deref()
            .unwrap_or_default()
            .contains("missing resolution source"));
    }

    #[test]
    fn non_15m_market_is_not_candidate() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "slug": "btc-updown-4h-1777248000",
            "question": "BTC Up or Down",
            "startDate": "2026-04-26T17:00:00Z",
            "endDate": "2026-04-27T04:00:00Z",
            "closed": false,
            "outcomes": ["Up", "Down"]
        }))
        .expect("gamma fixture parses");

        assert!(!is_candidate_crypto_15m_market(&gamma));
    }

    #[test]
    fn missing_clob_fee_metadata_is_ineligible() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "5",
            "slug": "btc-updown-15m-1777248000",
            "question": "BTC Up or Down",
            "description": btc_description(),
            "conditionId": "condition-btc",
            "resolutionSource": Asset::Btc.chainlink_resolution_source(),
            "startDate": "2026-04-26T00:08:47.17342Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "enableOrderBook": true,
            "acceptingOrders": true,
            "clobTokenIds": ["token-up", "token-down"],
            "outcomes": ["Up", "Down"]
        }))
        .expect("gamma fixture parses");
        let clob_info: ClobMarketInfo = serde_json::from_value(serde_json::json!({
            "t": [{"t": "token-up", "o": "Up"}, {"t": "token-down", "o": "Down"}],
            "mos": 5,
            "mts": 0.01
        }))
        .expect("clob fixture parses");

        let market = map_gamma_market(gamma, Some(&clob_info), None);

        assert_eq!(market.lifecycle_state, MarketLifecycleState::Ineligible);
        assert!(market
            .ineligibility_reason
            .as_deref()
            .unwrap_or_default()
            .contains("missing maker fee setting"));
    }

    #[test]
    fn asset_mismatched_resolution_source_is_ineligible() {
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "6",
            "slug": "btc-updown-15m-1777248000",
            "question": "BTC Up or Down",
            "description": eth_description(),
            "conditionId": "condition-btc",
            "resolutionSource": Asset::Eth.chainlink_resolution_source(),
            "startDate": "2026-04-26T00:08:47.17342Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "enableOrderBook": true,
            "acceptingOrders": true,
            "orderPriceMinTickSize": 0.01,
            "orderMinSize": 5,
            "clobTokenIds": ["token-up", "token-down"],
            "outcomes": ["Up", "Down"],
            "feesEnabled": true,
            "feeSchedule": {"rate": 0.072, "exponent": 1, "takerOnly": true}
        }))
        .expect("gamma fixture parses");

        let market = map_gamma_market(gamma, Some(&clob_fee_info()), None);

        assert_eq!(market.lifecycle_state, MarketLifecycleState::Ineligible);
        let reason = market.ineligibility_reason.as_deref().unwrap_or_default();
        assert!(reason.contains("resolution source is not the asset-matched Chainlink stream"));
        assert!(
            reason.contains("resolution rules do not identify the asset-matched Chainlink stream")
        );
    }

    #[test]
    fn persist_discovered_markets_writes_market_and_event() {
        let storage = InMemoryStorage::default();
        let gamma: GammaMarket = serde_json::from_value(serde_json::json!({
            "id": "4",
            "slug": "btc-updown-15m-1777248000",
            "question": "BTC Up or Down",
            "description": btc_description(),
            "conditionId": "condition-btc",
            "resolutionSource": Asset::Btc.chainlink_resolution_source(),
            "startDate": "2026-04-26T00:08:47.17342Z",
            "endDate": "2026-04-27T00:15:00Z",
            "active": true,
            "closed": false,
            "enableOrderBook": true,
            "acceptingOrders": true,
            "orderPriceMinTickSize": 0.01,
            "orderMinSize": 5,
            "clobTokenIds": ["token-up", "token-down"],
            "outcomes": ["Up", "Down"]
        }))
        .expect("gamma fixture parses");
        let clob_info = clob_fee_info();
        let market = map_gamma_market(gamma, Some(&clob_info), None);

        let count = persist_discovered_markets(&storage, "run-m2", 1_777_000_000_000, 1, &[market])
            .expect("persist succeeds");
        let events = storage.read_run_events("run-m2").expect("events read");

        assert_eq!(count, 1);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type.as_str(), "market_discovered");
    }

    fn clob_fee_info() -> ClobMarketInfo {
        serde_json::from_value(serde_json::json!({
            "t": [],
            "mos": 5,
            "mts": 0.01,
            "mbf": 0,
            "tbf": 0,
            "fd": {"r": 0.072, "e": 1, "to": true}
        }))
        .expect("clob fixture parses")
    }

    fn btc_description() -> &'static str {
        "This market resolves using Chainlink BTC/USD data stream available at https://data.chain.link/streams/btc-usd. It is about Bitcoin price according to Chainlink, not other sources or spot markets."
    }

    fn eth_description() -> &'static str {
        "This market resolves using Chainlink ETH/USD data stream available at https://data.chain.link/streams/eth-usd. It is about Ethereum price according to Chainlink, not other sources or spot markets."
    }
}
