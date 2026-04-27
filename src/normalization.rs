use std::error::Error;
use std::fmt::{Display, Formatter};

use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::domain::{Asset, OrderBookLevel, OrderBookSnapshot, ReferencePrice, Side};
use crate::events::NormalizedEvent;

pub const MODULE: &str = "normalization";

pub const SOURCE_POLYMARKET_CLOB: &str = "polymarket_clob";
pub const SOURCE_BINANCE: &str = "binance";
pub const SOURCE_COINBASE: &str = "coinbase";
pub const SOURCE_RESOLUTION: &str = "resolution_source";

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedFeedBatch {
    pub source: String,
    pub raw_event_type: Option<String>,
    pub events: Vec<NormalizedEvent>,
    pub unknown_event_type: Option<String>,
}

pub fn normalize_feed_message(
    source: &str,
    payload: &str,
    recv_wall_ts: i64,
) -> NormalizationResult<NormalizedFeedBatch> {
    let value: Value =
        serde_json::from_str(payload).map_err(|source| NormalizationError::InvalidJson {
            message: source.to_string(),
        })?;

    match source {
        SOURCE_POLYMARKET_CLOB => normalize_polymarket_value(source, &value),
        SOURCE_BINANCE => normalize_binance_value(source, &value, recv_wall_ts),
        SOURCE_COINBASE => normalize_coinbase_value(source, &value, recv_wall_ts),
        SOURCE_RESOLUTION => normalize_resolution_value(source, &value, recv_wall_ts),
        _ => Ok(NormalizedFeedBatch {
            source: source.to_string(),
            raw_event_type: event_type(&value),
            events: Vec::new(),
            unknown_event_type: Some("unknown_source".to_string()),
        }),
    }
}

fn normalize_polymarket_value(
    source: &str,
    value: &Value,
) -> NormalizationResult<NormalizedFeedBatch> {
    if let Value::Array(items) = value {
        let mut events = Vec::new();
        let mut unknown_event_type = None;
        for item in items {
            let batch = normalize_polymarket_object(source, item)?;
            events.extend(batch.events);
            if unknown_event_type.is_none() {
                unknown_event_type = batch.unknown_event_type;
            }
        }

        return Ok(NormalizedFeedBatch {
            source: source.to_string(),
            raw_event_type: Some("batch".to_string()),
            events,
            unknown_event_type,
        });
    }

    normalize_polymarket_object(source, value)
}

fn normalize_polymarket_object(
    source: &str,
    value: &Value,
) -> NormalizationResult<NormalizedFeedBatch> {
    let raw_event_type = event_type(value).or_else(|| {
        if looks_like_book_snapshot(value) {
            Some("book".to_string())
        } else {
            None
        }
    });
    let mut events = Vec::new();
    let unknown_event_type = match raw_event_type.as_deref() {
        Some("book") => {
            events.push(normalize_book(value)?);
            None
        }
        Some("price_change") => {
            events.extend(normalize_price_change(value)?);
            None
        }
        Some("tick_size_change") => {
            events.push(normalize_tick_size_change(value)?);
            None
        }
        Some("last_trade_price") => {
            events.push(normalize_last_trade_price(value)?);
            None
        }
        Some("best_bid_ask") => {
            events.push(normalize_best_bid_ask(value)?);
            None
        }
        Some("new_market") => {
            events.push(normalize_new_market(value)?);
            None
        }
        Some("market_resolved") => {
            events.push(normalize_market_resolved(value)?);
            None
        }
        Some(event_type) => Some(event_type.to_string()),
        None => Some("missing_event_type".to_string()),
    };

    Ok(NormalizedFeedBatch {
        source: source.to_string(),
        raw_event_type,
        events,
        unknown_event_type,
    })
}

fn looks_like_book_snapshot(value: &Value) -> bool {
    value.get("market").is_some()
        && value.get("asset_id").is_some()
        && value.get("bids").is_some()
        && value.get("asks").is_some()
        && value.get("timestamp").is_some()
        && value.get("hash").is_some()
}

fn normalize_book(value: &Value) -> NormalizationResult<NormalizedEvent> {
    let market_id = required_string(value, "market")?;
    let token_id = required_string(value, "asset_id")?;
    let source_ts = optional_ts_ms(value.get("timestamp"))?;
    let book = OrderBookSnapshot {
        market_id,
        token_id,
        bids: parse_levels(value.get("bids"))?,
        asks: parse_levels(value.get("asks"))?,
        hash: optional_string(value, "hash"),
        source_ts,
    };

    Ok(NormalizedEvent::BookSnapshot { book })
}

fn normalize_price_change(value: &Value) -> NormalizationResult<Vec<NormalizedEvent>> {
    let market_id = required_string(value, "market")?;
    let source_ts = optional_ts_ms(value.get("timestamp"))?;
    let changes = value
        .get("price_changes")
        .and_then(Value::as_array)
        .ok_or_else(|| NormalizationError::MissingField("price_changes".to_string()))?;
    let mut events = Vec::new();

    for change in changes {
        let token_id = required_string(change, "asset_id")?;
        let price = required_f64(change, "price")?;
        let size = required_f64(change, "size")?;
        let level = OrderBookLevel { price, size };
        let side = parse_side(&required_string(change, "side")?)?;
        let (bids, asks) = match side {
            Side::Buy => (vec![level], Vec::new()),
            Side::Sell => (Vec::new(), vec![level]),
        };

        events.push(NormalizedEvent::BookDelta {
            market_id: market_id.clone(),
            token_id: token_id.clone(),
            bids,
            asks,
            hash: optional_string(change, "hash"),
            source_ts,
        });

        if change.get("best_bid").is_some() || change.get("best_ask").is_some() {
            events.push(NormalizedEvent::BestBidAsk {
                market_id: market_id.clone(),
                token_id,
                best_bid: optional_f64(change, "best_bid")?,
                best_ask: optional_f64(change, "best_ask")?,
                spread: None,
                source_ts,
            });
        }
    }

    Ok(events)
}

fn normalize_tick_size_change(value: &Value) -> NormalizationResult<NormalizedEvent> {
    Ok(NormalizedEvent::TickSizeChange {
        market_id: required_string(value, "market")?,
        token_id: required_string(value, "asset_id")?,
        old_tick_size: required_f64(value, "old_tick_size")?,
        new_tick_size: required_f64(value, "new_tick_size")?,
        source_ts: optional_ts_ms(value.get("timestamp"))?,
    })
}

fn normalize_last_trade_price(value: &Value) -> NormalizationResult<NormalizedEvent> {
    Ok(NormalizedEvent::LastTrade {
        market_id: required_string(value, "market")?,
        token_id: required_string(value, "asset_id")?,
        side: parse_side(&required_string(value, "side")?)?,
        price: required_f64(value, "price")?,
        size: required_f64(value, "size")?,
        fee_rate_bps: optional_f64(value, "fee_rate_bps")?,
        source_ts: optional_ts_ms(value.get("timestamp"))?,
    })
}

fn normalize_best_bid_ask(value: &Value) -> NormalizationResult<NormalizedEvent> {
    Ok(NormalizedEvent::BestBidAsk {
        market_id: required_string(value, "market")?,
        token_id: required_string(value, "asset_id")?,
        best_bid: optional_f64(value, "best_bid")?,
        best_ask: optional_f64(value, "best_ask")?,
        spread: optional_f64(value, "spread")?,
        source_ts: optional_ts_ms(value.get("timestamp"))?,
    })
}

fn normalize_new_market(value: &Value) -> NormalizationResult<NormalizedEvent> {
    let market_id = required_string(value, "market")?;
    Ok(NormalizedEvent::MarketCreated {
        market_id,
        condition_id: optional_string(value, "condition_id"),
        slug: optional_string(value, "slug"),
        token_ids: parse_string_vec(
            value
                .get("assets_ids")
                .or_else(|| value.get("clob_token_ids")),
        ),
        outcomes: parse_string_vec(value.get("outcomes")),
        source_ts: optional_ts_ms(value.get("timestamp"))?,
        raw: value.clone(),
    })
}

fn normalize_market_resolved(value: &Value) -> NormalizationResult<NormalizedEvent> {
    Ok(NormalizedEvent::MarketResolved {
        market_id: required_string(value, "market")?,
        outcome_token_id: required_string(value, "winning_asset_id")?,
        resolved_ts: optional_ts_ms(value.get("timestamp"))?.unwrap_or_default(),
    })
}

fn normalize_binance_value(
    source: &str,
    value: &Value,
    recv_wall_ts: i64,
) -> NormalizationResult<NormalizedFeedBatch> {
    let data = value.get("data").unwrap_or(value);
    let event = event_type(data).or_else(|| data.get("e").and_then(value_to_string));
    if !matches!(event.as_deref(), Some("trade" | "aggTrade")) {
        return Ok(NormalizedFeedBatch {
            source: source.to_string(),
            raw_event_type: event.clone(),
            events: Vec::new(),
            unknown_event_type: event.or_else(|| Some("missing_event_type".to_string())),
        });
    }
    let symbol = required_string(data, "s")?;
    let asset = asset_from_symbol(&symbol)?;
    let price = required_f64(data, "p")?;
    let source_ts = optional_ts_ms(data.get("E").or_else(|| data.get("T")))?;

    Ok(NormalizedFeedBatch {
        source: source.to_string(),
        raw_event_type: event,
        events: vec![NormalizedEvent::PredictiveTick {
            price: ReferencePrice {
                asset,
                source: source.to_string(),
                price,
                source_ts,
                recv_wall_ts,
            },
        }],
        unknown_event_type: None,
    })
}

fn normalize_coinbase_value(
    source: &str,
    value: &Value,
    recv_wall_ts: i64,
) -> NormalizationResult<NormalizedFeedBatch> {
    let raw_event_type = event_type(value).or_else(|| value.get("type").and_then(value_to_string));
    if raw_event_type.as_deref() != Some("ticker") {
        return Ok(NormalizedFeedBatch {
            source: source.to_string(),
            raw_event_type: raw_event_type.clone(),
            events: Vec::new(),
            unknown_event_type: raw_event_type.or_else(|| Some("missing_event_type".to_string())),
        });
    }
    let product_id = required_string(value, "product_id")?;
    let asset = asset_from_symbol(&product_id)?;
    let price = required_f64(value, "price")?;
    let source_ts = optional_ts_ms(value.get("time"))?;

    Ok(NormalizedFeedBatch {
        source: source.to_string(),
        raw_event_type,
        events: vec![NormalizedEvent::PredictiveTick {
            price: ReferencePrice {
                asset,
                source: source.to_string(),
                price,
                source_ts,
                recv_wall_ts,
            },
        }],
        unknown_event_type: None,
    })
}

fn normalize_resolution_value(
    source: &str,
    value: &Value,
    recv_wall_ts: i64,
) -> NormalizationResult<NormalizedFeedBatch> {
    let asset_text = required_string(value, "asset")
        .or_else(|_| required_string(value, "symbol"))
        .or_else(|_| required_string(value, "stream"))?;
    let asset = asset_from_symbol(&asset_text)?;
    let price = required_f64(value, "price")
        .or_else(|_| required_f64(value, "benchmarkPrice"))
        .or_else(|_| required_f64(value, "value"))?;
    let source_ts = optional_ts_ms(
        value
            .get("timestamp")
            .or_else(|| value.get("ts"))
            .or_else(|| value.get("time")),
    )?;

    Ok(NormalizedFeedBatch {
        source: source.to_string(),
        raw_event_type: event_type(value),
        events: vec![NormalizedEvent::ReferenceTick {
            price: ReferencePrice {
                asset,
                source: source.to_string(),
                price,
                source_ts,
                recv_wall_ts,
            },
        }],
        unknown_event_type: None,
    })
}

fn event_type(value: &Value) -> Option<String> {
    value
        .get("event_type")
        .or_else(|| value.get("type"))
        .or_else(|| value.get("e"))
        .and_then(value_to_string)
}

fn parse_levels(value: Option<&Value>) -> NormalizationResult<Vec<OrderBookLevel>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value.as_array().ok_or_else(|| {
        NormalizationError::InvalidField("book levels must be an array".to_string())
    })?;
    items
        .iter()
        .map(|item| {
            Ok(OrderBookLevel {
                price: required_f64(item, "price")?,
                size: required_f64(item, "size")?,
            })
        })
        .collect()
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

fn parse_side(value: &str) -> NormalizationResult<Side> {
    match value.to_ascii_uppercase().as_str() {
        "BUY" | "BID" => Ok(Side::Buy),
        "SELL" | "ASK" => Ok(Side::Sell),
        _ => Err(NormalizationError::InvalidField(format!(
            "unsupported side {value}"
        ))),
    }
}

fn asset_from_symbol(value: &str) -> NormalizationResult<Asset> {
    let upper = value.to_ascii_uppercase();
    if upper.contains("BTC") || upper.contains("BITCOIN") {
        Ok(Asset::Btc)
    } else if upper.contains("ETH") || upper.contains("ETHEREUM") {
        Ok(Asset::Eth)
    } else if upper.contains("SOL") || upper.contains("SOLANA") {
        Ok(Asset::Sol)
    } else {
        Err(NormalizationError::InvalidField(format!(
            "unsupported asset symbol {value}"
        )))
    }
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(value_to_string)
}

fn required_string(value: &Value, field: &str) -> NormalizationResult<String> {
    value
        .get(field)
        .and_then(value_to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| NormalizationError::MissingField(field.to_string()))
}

fn optional_f64(value: &Value, field: &str) -> NormalizationResult<Option<f64>> {
    value.get(field).map(value_to_f64).transpose()
}

fn required_f64(value: &Value, field: &str) -> NormalizationResult<f64> {
    value
        .get(field)
        .map(value_to_f64)
        .transpose()?
        .ok_or_else(|| NormalizationError::MissingField(field.to_string()))
}

fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> NormalizationResult<f64> {
    match value {
        Value::Number(value) => value
            .as_f64()
            .ok_or_else(|| NormalizationError::InvalidField("invalid numeric value".to_string())),
        Value::String(value) => value.parse::<f64>().map_err(|_| {
            NormalizationError::InvalidField(format!("invalid numeric string {value}"))
        }),
        _ => Err(NormalizationError::InvalidField(
            "expected number or numeric string".to_string(),
        )),
    }
}

fn optional_ts_ms(value: Option<&Value>) -> NormalizationResult<Option<i64>> {
    value.map(value_to_ts_ms).transpose()
}

fn value_to_ts_ms(value: &Value) -> NormalizationResult<i64> {
    if let Some(number) = value.as_i64() {
        return Ok(number);
    }
    let Some(text) = value_to_string(value) else {
        return Err(NormalizationError::InvalidField(
            "timestamp must be a string or integer".to_string(),
        ));
    };
    if let Ok(timestamp) = text.parse::<i64>() {
        return Ok(timestamp);
    }

    OffsetDateTime::parse(&text, &Rfc3339)
        .map_err(|_| NormalizationError::InvalidField(format!("invalid timestamp {text}")))
        .and_then(|timestamp| {
            i64::try_from(timestamp.unix_timestamp_nanos() / 1_000_000).map_err(|_| {
                NormalizationError::InvalidField(format!("timestamp out of range {text}"))
            })
        })
}

pub type NormalizationResult<T> = Result<T, NormalizationError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationError {
    InvalidJson { message: String },
    MissingField(String),
    InvalidField(String),
}

impl Display for NormalizationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            NormalizationError::InvalidJson { message } => {
                write!(formatter, "invalid feed JSON: {message}")
            }
            NormalizationError::MissingField(field) => {
                write!(formatter, "feed message missing required field {field}")
            }
            NormalizationError::InvalidField(message) => {
                write!(formatter, "invalid feed field: {message}")
            }
        }
    }
}

impl Error for NormalizationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_polymarket_book_snapshot() {
        let batch = normalize_feed_message(
            SOURCE_POLYMARKET_CLOB,
            r#"{
              "event_type": "book",
              "asset_id": "token-up",
              "market": "condition-1",
              "bids": [{"price": ".48", "size": "30"}],
              "asks": [{"price": ".52", "size": "25"}],
              "timestamp": "1757908892351",
              "hash": "book-hash"
            }"#,
            1_777_000_000_000,
        )
        .expect("book normalizes");

        assert_eq!(batch.raw_event_type.as_deref(), Some("book"));
        assert_eq!(batch.events.len(), 1);
        match &batch.events[0] {
            NormalizedEvent::BookSnapshot { book } => {
                assert_eq!(book.market_id, "condition-1");
                assert_eq!(book.token_id, "token-up");
                assert_eq!(book.bids[0].price, 0.48);
                assert_eq!(book.asks[0].size, 25.0);
                assert_eq!(book.source_ts, Some(1_757_908_892_351));
            }
            event => panic!("unexpected event {event:?}"),
        }
    }

    #[test]
    fn parses_rest_book_snapshot_without_event_type() {
        let batch = normalize_feed_message(
            SOURCE_POLYMARKET_CLOB,
            r#"{
              "asset_id": "token-up",
              "market": "condition-1",
              "bids": [{"price": ".48", "size": "30"}],
              "asks": [{"price": ".52", "size": "25"}],
              "timestamp": "1757908892351",
              "hash": "book-hash",
              "tick_size": "0.01",
              "min_order_size": "5",
              "last_trade_price": "0.50"
            }"#,
            1_777_000_000_000,
        )
        .expect("REST book normalizes");

        assert_eq!(batch.raw_event_type.as_deref(), Some("book"));
        assert_eq!(batch.events.len(), 1);
        assert!(matches!(
            batch.events[0],
            NormalizedEvent::BookSnapshot { .. }
        ));
    }

    #[test]
    fn parses_polymarket_price_change_into_delta_and_best_bid_ask() {
        let batch = normalize_feed_message(
            SOURCE_POLYMARKET_CLOB,
            r#"{
              "market": "condition-1",
              "price_changes": [{
                "asset_id": "token-up",
                "price": "0.5",
                "size": "200",
                "side": "BUY",
                "hash": "delta-hash",
                "best_bid": "0.5",
                "best_ask": "1"
              }],
              "timestamp": "1757908892351",
              "event_type": "price_change"
            }"#,
            1_777_000_000_000,
        )
        .expect("price_change normalizes");

        assert_eq!(batch.events.len(), 2);
        assert!(matches!(
            batch.events[0],
            NormalizedEvent::BookDelta { ref bids, .. } if bids.len() == 1
        ));
        assert!(matches!(
            batch.events[1],
            NormalizedEvent::BestBidAsk {
                best_bid: Some(0.5),
                ..
            }
        ));
    }

    #[test]
    fn parses_polymarket_tick_trade_bba_and_resolution_events() {
        for (payload, expected_type) in [
            (
                r#"{"event_type":"tick_size_change","asset_id":"token-up","market":"condition-1","old_tick_size":"0.01","new_tick_size":"0.001","timestamp":"1757908892351"}"#,
                "tick_size_change",
            ),
            (
                r#"{"event_type":"last_trade_price","asset_id":"token-up","market":"condition-1","price":"0.456","side":"BUY","size":"219.217767","fee_rate_bps":"0","timestamp":"1757908892351"}"#,
                "last_trade_price",
            ),
            (
                r#"{"event_type":"best_bid_ask","asset_id":"token-up","market":"condition-1","best_bid":"0.73","best_ask":"0.77","spread":"0.04","timestamp":"1757908892351"}"#,
                "best_bid_ask",
            ),
            (
                r#"{"event_type":"market_resolved","market":"condition-1","winning_asset_id":"token-up","winning_outcome":"Up","timestamp":"1757908892351"}"#,
                "market_resolved",
            ),
        ] {
            let batch = normalize_feed_message(SOURCE_POLYMARKET_CLOB, payload, 1_777_000_000_000)
                .expect("message normalizes");
            assert_eq!(batch.raw_event_type.as_deref(), Some(expected_type));
            assert_eq!(batch.events.len(), 1);
        }
    }

    #[test]
    fn parses_polymarket_new_market_as_lifecycle_metadata() {
        let batch = normalize_feed_message(
            SOURCE_POLYMARKET_CLOB,
            r#"{
              "event_type":"new_market",
              "market":"condition-1",
              "condition_id":"condition-1",
              "slug":"btc-updown-15m-1777340700",
              "assets_ids":["token-up","token-down"],
              "outcomes":["Up","Down"],
              "timestamp":"1757908892351"
            }"#,
            1_777_000_000_000,
        )
        .expect("new_market normalizes");

        match &batch.events[0] {
            NormalizedEvent::MarketCreated {
                market_id,
                token_ids,
                outcomes,
                ..
            } => {
                assert_eq!(market_id, "condition-1");
                assert_eq!(token_ids.len(), 2);
                assert_eq!(outcomes, &vec!["Up".to_string(), "Down".to_string()]);
            }
            event => panic!("unexpected event {event:?}"),
        }
    }

    #[test]
    fn parses_predictive_and_reference_ticks() {
        let binance = normalize_feed_message(
            SOURCE_BINANCE,
            r#"{"e":"trade","E":1777000000000,"s":"BTCUSDT","p":"65000.5","q":"0.01"}"#,
            1_777_000_000_100,
        )
        .expect("binance normalizes");
        let coinbase = normalize_feed_message(
            SOURCE_COINBASE,
            r#"{"type":"ticker","product_id":"ETH-USD","price":"3300.5","time":"2026-04-27T00:00:00Z"}"#,
            1_777_000_000_100,
        )
        .expect("coinbase normalizes");
        let resolution = normalize_feed_message(
            SOURCE_RESOLUTION,
            r#"{"asset":"SOL","price":"150.25","timestamp":1777000000000}"#,
            1_777_000_000_100,
        )
        .expect("resolution normalizes");

        assert!(matches!(
            binance.events[0],
            NormalizedEvent::PredictiveTick { .. }
        ));
        assert!(matches!(
            coinbase.events[0],
            NormalizedEvent::PredictiveTick { .. }
        ));
        assert!(matches!(
            resolution.events[0],
            NormalizedEvent::ReferenceTick { .. }
        ));
    }

    #[test]
    fn unknown_polymarket_event_is_reported_without_normalized_events() {
        let batch = normalize_feed_message(
            SOURCE_POLYMARKET_CLOB,
            r#"{"event_type":"unexpected_new_type","market":"condition-1"}"#,
            1_777_000_000_000,
        )
        .expect("unknown event is still parseable");

        assert_eq!(batch.events.len(), 0);
        assert_eq!(
            batch.unknown_event_type.as_deref(),
            Some("unexpected_new_type")
        );
    }
}
