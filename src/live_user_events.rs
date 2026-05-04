use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::Deserialize;

use crate::live_order_journal::LiveJournalEventType;

pub const MODULE: &str = "live_user_events";
pub const USER_CHANNEL_ENDPOINT: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/user";
pub const USER_CHANNEL_NETWORK_ENABLED: bool = false;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveUserEvent {
    Order(LiveUserOrderEvent),
    Trade(LiveUserTradeEvent),
}

impl LiveUserEvent {
    pub fn journal_event_type(&self) -> LiveJournalEventType {
        match self {
            Self::Order(order) => order.event_type.journal_event_type(),
            Self::Trade(trade) => trade.status.journal_event_type(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveUserOrderEvent {
    pub order_id: String,
    pub event_type: LiveUserOrderEventType,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub price: String,
    pub original_size: String,
    pub size_matched: String,
    pub associate_trades: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUserOrderEventType {
    Placement,
    Update,
    Cancellation,
}

impl LiveUserOrderEventType {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "PLACEMENT" => Some(Self::Placement),
            "UPDATE" => Some(Self::Update),
            "CANCELLATION" => Some(Self::Cancellation),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Placement => "PLACEMENT",
            Self::Update => "UPDATE",
            Self::Cancellation => "CANCELLATION",
        }
    }

    fn journal_event_type(self) -> LiveJournalEventType {
        match self {
            Self::Placement => LiveJournalEventType::LiveOrderReadbackObserved,
            Self::Update => LiveJournalEventType::LiveOrderPartiallyFilled,
            Self::Cancellation => LiveJournalEventType::LiveOrderCanceled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveUserTradeEvent {
    pub trade_id: String,
    pub status: LiveUserTradeStatus,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub taker_order_id: Option<String>,
    pub maker_order_ids: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUserTradeStatus {
    Matched,
    Mined,
    Confirmed,
    Retrying,
    Failed,
}

impl LiveUserTradeStatus {
    pub fn from_wire(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "MATCHED" => Some(Self::Matched),
            "MINED" => Some(Self::Mined),
            "CONFIRMED" => Some(Self::Confirmed),
            "RETRYING" => Some(Self::Retrying),
            "FAILED" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "MATCHED",
            Self::Mined => "MINED",
            Self::Confirmed => "CONFIRMED",
            Self::Retrying => "RETRYING",
            Self::Failed => "FAILED",
        }
    }

    fn journal_event_type(self) -> LiveJournalEventType {
        match self {
            Self::Matched => LiveJournalEventType::LiveTradeMatched,
            Self::Mined => LiveJournalEventType::LiveTradeMined,
            Self::Confirmed => LiveJournalEventType::LiveTradeConfirmed,
            Self::Retrying => LiveJournalEventType::LiveTradeRetrying,
            Self::Failed => LiveJournalEventType::LiveTradeFailed,
        }
    }
}

pub fn parse_user_event(json: &str) -> LiveUserEventResult<LiveUserEvent> {
    let wire: LiveUserEventWire = serde_json::from_str(json).map_err(LiveUserEventError::Parse)?;
    let event_type = wire
        .event_type
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if event_type == "order" || LiveUserOrderEventType::from_wire(&wire.type_field).is_some() {
        return parse_order_event(wire);
    }
    if event_type == "trade" || wire.type_field.eq_ignore_ascii_case("TRADE") {
        return parse_trade_event(wire);
    }

    Err(LiveUserEventError::UnknownEventType(wire.type_field))
}

fn parse_order_event(wire: LiveUserEventWire) -> LiveUserEventResult<LiveUserEvent> {
    let event_type = LiveUserOrderEventType::from_wire(&wire.type_field)
        .ok_or_else(|| LiveUserEventError::UnknownOrderEvent(wire.type_field.clone()))?;
    Ok(LiveUserEvent::Order(LiveUserOrderEvent {
        order_id: wire.id,
        event_type,
        market: wire.market,
        asset_id: wire.asset_id,
        side: required(wire.side, "side")?,
        price: required(wire.price, "price")?,
        original_size: required(wire.original_size, "original_size")?,
        size_matched: required(wire.size_matched, "size_matched")?,
        associate_trades: wire.associate_trades.unwrap_or_default(),
        timestamp: required(wire.timestamp, "timestamp")?,
    }))
}

fn parse_trade_event(wire: LiveUserEventWire) -> LiveUserEventResult<LiveUserEvent> {
    let status_text = required(wire.status, "status")?;
    let status = LiveUserTradeStatus::from_wire(&status_text)
        .ok_or_else(|| LiveUserEventError::UnknownTradeStatus(status_text.clone()))?;
    Ok(LiveUserEvent::Trade(LiveUserTradeEvent {
        trade_id: wire.id,
        status,
        market: wire.market,
        asset_id: wire.asset_id,
        side: required(wire.side, "side")?,
        price: required(wire.price, "price")?,
        size: required(wire.size, "size")?,
        taker_order_id: wire.taker_order_id,
        maker_order_ids: wire
            .maker_orders
            .unwrap_or_default()
            .into_iter()
            .map(|order| order.order_id)
            .collect(),
        timestamp: required(wire.timestamp.or(wire.matchtime), "timestamp")?,
    }))
}

fn required(value: Option<String>, field: &'static str) -> LiveUserEventResult<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(LiveUserEventError::MissingField(field))
}

#[derive(Debug, Deserialize)]
struct LiveUserEventWire {
    #[serde(default)]
    event_type: Option<String>,
    #[serde(rename = "type")]
    type_field: String,
    id: String,
    market: String,
    asset_id: String,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    original_size: Option<String>,
    #[serde(default)]
    size_matched: Option<String>,
    #[serde(default)]
    associate_trades: Option<Vec<String>>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    taker_order_id: Option<String>,
    #[serde(default)]
    maker_orders: Option<Vec<LiveUserMakerOrderWire>>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    matchtime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LiveUserMakerOrderWire {
    order_id: String,
}

pub type LiveUserEventResult<T> = Result<T, LiveUserEventError>;

#[derive(Debug)]
pub enum LiveUserEventError {
    Parse(serde_json::Error),
    MissingField(&'static str),
    UnknownEventType(String),
    UnknownOrderEvent(String),
    UnknownTradeStatus(String),
}

impl Display for LiveUserEventError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(source) => write!(formatter, "user event parse failed: {source}"),
            Self::MissingField(field) => write!(formatter, "user event missing {field}"),
            Self::UnknownEventType(value) => write!(formatter, "unknown user event type {value}"),
            Self::UnknownOrderEvent(value) => write!(formatter, "unknown order event type {value}"),
            Self::UnknownTradeStatus(value) => write!(formatter, "unknown trade status {value}"),
        }
    }
}

impl Error for LiveUserEventError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(source) => Some(source),
            Self::MissingField(_)
            | Self::UnknownEventType(_)
            | Self::UnknownOrderEvent(_)
            | Self::UnknownTradeStatus(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_user_events_parse_order_lifecycle_fixtures() {
        for (wire_type, expected, journal_type) in [
            (
                "PLACEMENT",
                LiveUserOrderEventType::Placement,
                LiveJournalEventType::LiveOrderReadbackObserved,
            ),
            (
                "UPDATE",
                LiveUserOrderEventType::Update,
                LiveJournalEventType::LiveOrderPartiallyFilled,
            ),
            (
                "CANCELLATION",
                LiveUserOrderEventType::Cancellation,
                LiveJournalEventType::LiveOrderCanceled,
            ),
        ] {
            let event = parse_user_event(&order_fixture(wire_type)).expect("order fixture parses");
            let LiveUserEvent::Order(order) = &event else {
                panic!("expected order event");
            };

            assert_eq!(order.event_type, expected);
            assert_eq!(order.event_type.as_str(), wire_type);
            assert_eq!(order.order_id, "order-1");
            assert_eq!(event.journal_event_type(), journal_type);
        }
    }

    #[test]
    fn live_user_events_parse_trade_lifecycle_fixtures() {
        for (wire_status, expected, journal_type) in [
            (
                "MATCHED",
                LiveUserTradeStatus::Matched,
                LiveJournalEventType::LiveTradeMatched,
            ),
            (
                "MINED",
                LiveUserTradeStatus::Mined,
                LiveJournalEventType::LiveTradeMined,
            ),
            (
                "CONFIRMED",
                LiveUserTradeStatus::Confirmed,
                LiveJournalEventType::LiveTradeConfirmed,
            ),
            (
                "RETRYING",
                LiveUserTradeStatus::Retrying,
                LiveJournalEventType::LiveTradeRetrying,
            ),
            (
                "FAILED",
                LiveUserTradeStatus::Failed,
                LiveJournalEventType::LiveTradeFailed,
            ),
        ] {
            let event =
                parse_user_event(&trade_fixture(wire_status)).expect("trade fixture parses");
            let LiveUserEvent::Trade(trade) = &event else {
                panic!("expected trade event");
            };

            assert_eq!(trade.status, expected);
            assert_eq!(trade.status.as_str(), wire_status);
            assert_eq!(trade.taker_order_id.as_deref(), Some("order-taker"));
            assert_eq!(trade.maker_order_ids, vec!["order-maker".to_string()]);
            assert_eq!(event.journal_event_type(), journal_type);
        }
    }

    #[test]
    fn live_user_events_network_subscription_is_parser_only_in_la2() {
        assert_eq!(
            USER_CHANNEL_ENDPOINT,
            "wss://ws-subscriptions-clob.polymarket.com/ws/user"
        );
        assert!(!USER_CHANNEL_NETWORK_ENABLED);
    }

    fn order_fixture(wire_type: &str) -> String {
        format!(
            r#"{{
                "asset_id":"asset-1",
                "associate_trades":["trade-1"],
                "event_type":"order",
                "id":"order-1",
                "market":"market-1",
                "original_size":"10",
                "price":"0.57",
                "side":"SELL",
                "size_matched":"1",
                "timestamp":"1672290687",
                "type":"{wire_type}"
            }}"#
        )
    }

    fn trade_fixture(status: &str) -> String {
        format!(
            r#"{{
                "asset_id":"asset-1",
                "event_type":"trade",
                "id":"trade-1",
                "last_update":"1672290701",
                "maker_orders":[{{"matched_amount":"10","order_id":"order-maker","price":"0.57"}}],
                "market":"market-1",
                "matchtime":"1672290701",
                "price":"0.57",
                "side":"BUY",
                "size":"10",
                "status":"{status}",
                "taker_order_id":"order-taker",
                "type":"TRADE"
            }}"#
        )
    }
}
