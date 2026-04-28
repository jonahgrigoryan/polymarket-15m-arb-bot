use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use serde::Deserialize;

use crate::config::ReferenceFeedConfig;
use crate::domain::{Asset, ReferencePrice};
use crate::events::NormalizedEvent;

pub const MODULE: &str = "reference_feed";
pub const SOURCE_PYTH_PROXY: &str = "pyth_proxy";
pub const PROVIDER_PYTH: &str = "pyth";

#[derive(Debug, Clone)]
pub struct PythHermesClient {
    http: reqwest::Client,
    base_url: String,
}

impl PythHermesClient {
    pub fn new(base_url: impl Into<String>, timeout_ms: u64) -> ReferenceFeedResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| ReferenceFeedError::Network {
                operation: "pyth_client_build",
                message: source.to_string(),
            })?;

        Ok(Self {
            http,
            base_url: base_url.into(),
        })
    }

    pub async fn fetch_latest(
        &self,
        config: &ReferenceFeedConfig,
        recv_wall_ts: i64,
    ) -> ReferenceFeedResult<PythProxyBatch> {
        let url = format!(
            "{}/v2/updates/price/latest",
            self.base_url.trim_end_matches('/')
        );
        let ids = pyth_price_ids(config);
        let query = ids
            .iter()
            .map(|(_, id)| ("ids[]", id.as_str()))
            .collect::<Vec<_>>();
        let response = self
            .http
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|source| ReferenceFeedError::Network {
                operation: "pyth_latest_request",
                message: source.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ReferenceFeedError::Protocol(format!(
                "pyth latest request to {url} returned HTTP {status}"
            )));
        }
        let raw_payload = response
            .text()
            .await
            .map_err(|source| ReferenceFeedError::Network {
                operation: "pyth_latest_body",
                message: source.to_string(),
            })?;
        let events = parse_pyth_latest_price_response(
            &raw_payload,
            config,
            recv_wall_ts,
            config.max_staleness_ms,
        )?;

        Ok(PythProxyBatch {
            raw_payload,
            events,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PythProxyBatch {
    pub raw_payload: String,
    pub events: Vec<NormalizedEvent>,
}

pub fn parse_pyth_latest_price_response(
    payload: &str,
    config: &ReferenceFeedConfig,
    recv_wall_ts: i64,
    max_staleness_ms: u64,
) -> ReferenceFeedResult<Vec<NormalizedEvent>> {
    let response: HermesLatestResponse =
        serde_json::from_str(payload).map_err(|source| ReferenceFeedError::InvalidPayload {
            message: source.to_string(),
        })?;
    let asset_by_id = pyth_asset_by_id(config);
    let mut events = Vec::new();

    for item in response.parsed {
        let id = normalize_price_id(&item.id);
        let Some(asset) = asset_by_id.get(&id).copied() else {
            continue;
        };
        let source_ts = item.price.publish_time.checked_mul(1_000).ok_or_else(|| {
            ReferenceFeedError::InvalidPayload {
                message: format!("pyth publish_time overflow for feed {}", item.id),
            }
        })?;
        if recv_wall_ts.saturating_sub(source_ts) > max_staleness_ms as i64 {
            return Err(ReferenceFeedError::StalePrice {
                asset,
                age_ms: recv_wall_ts.saturating_sub(source_ts),
                max_staleness_ms,
            });
        }

        events.push(NormalizedEvent::ReferenceTick {
            price: ReferencePrice {
                asset,
                source: asset.chainlink_resolution_source().to_string(),
                price: fixed_point_to_f64(&item.price.price, item.price.expo)?,
                confidence: Some(fixed_point_to_f64(&item.price.conf, item.price.expo)?),
                provider: Some(PROVIDER_PYTH.to_string()),
                matches_market_resolution_source: Some(false),
                source_ts: Some(source_ts),
                recv_wall_ts,
            },
        });
    }

    if events.len() != 3 {
        return Err(ReferenceFeedError::Protocol(format!(
            "pyth latest response produced {} of 3 required reference ticks",
            events.len()
        )));
    }

    events.sort_by_key(|event| match event {
        NormalizedEvent::ReferenceTick { price } => price.asset.symbol(),
        _ => "",
    });
    Ok(events)
}

#[derive(Debug, Deserialize)]
struct HermesLatestResponse {
    parsed: Vec<HermesParsedPrice>,
}

#[derive(Debug, Deserialize)]
struct HermesParsedPrice {
    id: String,
    price: HermesPrice,
}

#[derive(Debug, Deserialize)]
struct HermesPrice {
    price: String,
    conf: String,
    expo: i32,
    publish_time: i64,
}

fn pyth_price_ids(config: &ReferenceFeedConfig) -> [(Asset, String); 3] {
    [
        (Asset::Btc, config.pyth_btc_usd_price_id.clone()),
        (Asset::Eth, config.pyth_eth_usd_price_id.clone()),
        (Asset::Sol, config.pyth_sol_usd_price_id.clone()),
    ]
}

fn pyth_asset_by_id(config: &ReferenceFeedConfig) -> HashMap<String, Asset> {
    pyth_price_ids(config)
        .into_iter()
        .map(|(asset, id)| (normalize_price_id(&id), asset))
        .collect()
}

fn normalize_price_id(id: &str) -> String {
    id.trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .to_ascii_lowercase()
}

fn fixed_point_to_f64(value: &str, expo: i32) -> ReferenceFeedResult<f64> {
    let mantissa = value
        .parse::<f64>()
        .map_err(|source| ReferenceFeedError::InvalidPayload {
            message: format!("invalid pyth fixed point value {value}: {source}"),
        })?;
    let value = mantissa * 10_f64.powi(expo);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ReferenceFeedError::InvalidPayload {
            message: format!("non-finite pyth fixed point value {value}"),
        })
    }
}

pub type ReferenceFeedResult<T> = Result<T, ReferenceFeedError>;

#[derive(Debug)]
pub enum ReferenceFeedError {
    Network {
        operation: &'static str,
        message: String,
    },
    Protocol(String),
    InvalidPayload {
        message: String,
    },
    StalePrice {
        asset: Asset,
        age_ms: i64,
        max_staleness_ms: u64,
    },
}

impl Display for ReferenceFeedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReferenceFeedError::Network { operation, message } => {
                write!(formatter, "{operation} failed: {message}")
            }
            ReferenceFeedError::Protocol(message) => write!(formatter, "{message}"),
            ReferenceFeedError::InvalidPayload { message } => {
                write!(formatter, "invalid pyth payload: {message}")
            }
            ReferenceFeedError::StalePrice {
                asset,
                age_ms,
                max_staleness_ms,
            } => write!(
                formatter,
                "stale pyth {} price age_ms={} max_staleness_ms={}",
                asset.symbol(),
                age_ms,
                max_staleness_ms
            ),
        }
    }
}

impl Error for ReferenceFeedError {}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: i64 = 1_777_357_010_000;

    #[test]
    fn pyth_parser_decodes_hermes_fixture_and_maps_default_assets() {
        let events = parse_pyth_latest_price_response(
            fixture_payload(1_777_357_010).as_str(),
            &config(),
            NOW_MS,
            5_000,
        )
        .expect("fixture parses");

        assert_eq!(events.len(), 3);
        assert_reference(&events[0], Asset::Btc, 76_990.5, 20.0);
        assert_reference(&events[1], Asset::Eth, 2_288.70499999, 0.821072);
        assert_reference(&events[2], Asset::Sol, 84.21763878, 0.04267754);
    }

    #[test]
    fn pyth_parser_rejects_stale_publish_time() {
        let error = parse_pyth_latest_price_response(
            fixture_payload(1_777_357_000).as_str(),
            &config(),
            NOW_MS,
            5_000,
        )
        .expect_err("stale fixture fails closed");

        assert!(error.to_string().contains("stale pyth BTC price"));
    }

    fn assert_reference(
        event: &NormalizedEvent,
        expected_asset: Asset,
        expected_price: f64,
        expected_confidence: f64,
    ) {
        let NormalizedEvent::ReferenceTick { price } = event else {
            panic!("expected reference tick");
        };
        assert_eq!(price.asset, expected_asset);
        assert_eq!(price.source, expected_asset.chainlink_resolution_source());
        assert_eq!(price.provider.as_deref(), Some(PROVIDER_PYTH));
        assert_eq!(price.matches_market_resolution_source, Some(false));
        assert!((price.price - expected_price).abs() < 0.000001);
        assert!((price.confidence.expect("confidence") - expected_confidence).abs() < 0.000001);
        assert_eq!(price.source_ts, Some(NOW_MS));
        assert_eq!(price.recv_wall_ts, NOW_MS);
    }

    fn config() -> ReferenceFeedConfig {
        ReferenceFeedConfig {
            provider: "pyth_proxy".to_string(),
            pyth_enabled: true,
            pyth_hermes_url: "https://hermes.pyth.network".to_string(),
            pyth_btc_usd_price_id:
                "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43".to_string(),
            pyth_eth_usd_price_id:
                "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace".to_string(),
            pyth_sol_usd_price_id:
                "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d".to_string(),
            max_staleness_ms: 5_000,
        }
    }

    fn fixture_payload(publish_time: i64) -> String {
        format!(
            r#"{{
              "binary": {{"encoding": "hex", "data": ["00"]}},
              "parsed": [
                {{
                  "id": "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
                  "price": {{"price": "7699050000000", "conf": "2000000000", "expo": -8, "publish_time": {publish_time}}},
                  "metadata": {{"slot": 1, "proof_available_time": {publish_time}, "prev_publish_time": {publish_time}}}
                }},
                {{
                  "id": "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace",
                  "price": {{"price": "228870499999", "conf": "82107200", "expo": -8, "publish_time": {publish_time}}},
                  "metadata": {{"slot": 1, "proof_available_time": {publish_time}, "prev_publish_time": {publish_time}}}
                }},
                {{
                  "id": "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d",
                  "price": {{"price": "8421763878", "conf": "4267754", "expo": -8, "publish_time": {publish_time}}},
                  "metadata": {{"slot": 1, "proof_available_time": {publish_time}, "prev_publish_time": {publish_time}}}
                }}
              ]
            }}"#
        )
    }
}
