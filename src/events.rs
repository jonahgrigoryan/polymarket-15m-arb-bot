use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    Asset, Market, OrderBookLevel, OrderBookSnapshot, PaperFill, PaperOrder, ReferencePrice,
    RiskState, Side, SignalDecision,
};

pub const MODULE: &str = "events";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    MarketDiscovered,
    MarketCreated,
    MarketUpdated,
    MarketResolved,
    BookSnapshot,
    BookDelta,
    TickSizeChange,
    BestBidAsk,
    LastTrade,
    ReferenceTick,
    PredictiveTick,
    SignalUpdate,
    PaperOrderPlaced,
    PaperOrderCanceled,
    PaperFill,
    RiskHalt,
    ReplayCheckpoint,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::MarketDiscovered => "market_discovered",
            EventType::MarketCreated => "market_created",
            EventType::MarketUpdated => "market_updated",
            EventType::MarketResolved => "market_resolved",
            EventType::BookSnapshot => "book_snapshot",
            EventType::BookDelta => "book_delta",
            EventType::TickSizeChange => "tick_size_change",
            EventType::BestBidAsk => "best_bid_ask",
            EventType::LastTrade => "last_trade",
            EventType::ReferenceTick => "reference_tick",
            EventType::PredictiveTick => "predictive_tick",
            EventType::SignalUpdate => "signal_update",
            EventType::PaperOrderPlaced => "paper_order_placed",
            EventType::PaperOrderCanceled => "paper_order_canceled",
            EventType::PaperFill => "paper_fill",
            EventType::RiskHalt => "risk_halt",
            EventType::ReplayCheckpoint => "replay_checkpoint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct EventEnvelope {
    pub run_id: String,
    pub event_id: String,
    pub event_type: EventType,
    pub source: String,
    pub source_ts: Option<i64>,
    pub recv_wall_ts: i64,
    pub recv_mono_ns: u64,
    pub ingest_seq: u64,
    pub market_id: Option<String>,
    pub asset: Option<Asset>,
    pub payload: NormalizedEvent,
}

impl EventEnvelope {
    pub fn new(
        run_id: impl Into<String>,
        event_id: impl Into<String>,
        source: impl Into<String>,
        recv_wall_ts: i64,
        recv_mono_ns: u64,
        ingest_seq: u64,
        payload: NormalizedEvent,
    ) -> Self {
        let event_type = payload.event_type();
        let market_id = payload.market_id();
        let asset = payload.asset();
        let source_ts = payload.source_ts();

        Self {
            run_id: run_id.into(),
            event_id: event_id.into(),
            event_type,
            source: source.into(),
            source_ts,
            recv_wall_ts,
            recv_mono_ns,
            ingest_seq,
            market_id,
            asset,
            payload,
        }
    }

    pub fn replay_ordering_key(&self) -> (u64, u64, &str) {
        (self.recv_mono_ns, self.ingest_seq, self.event_id.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum NormalizedEvent {
    MarketDiscovered {
        market: Market,
    },
    MarketCreated {
        market_id: String,
        condition_id: Option<String>,
        slug: Option<String>,
        token_ids: Vec<String>,
        outcomes: Vec<String>,
        source_ts: Option<i64>,
        raw: Value,
    },
    MarketUpdated {
        market: Market,
        changes: Vec<String>,
    },
    MarketResolved {
        market_id: String,
        outcome_token_id: String,
        resolved_ts: i64,
    },
    BookSnapshot {
        book: OrderBookSnapshot,
    },
    BookDelta {
        market_id: String,
        token_id: String,
        bids: Vec<OrderBookLevel>,
        asks: Vec<OrderBookLevel>,
        hash: Option<String>,
        source_ts: Option<i64>,
    },
    TickSizeChange {
        market_id: String,
        token_id: String,
        old_tick_size: f64,
        new_tick_size: f64,
        source_ts: Option<i64>,
    },
    BestBidAsk {
        market_id: String,
        token_id: String,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        spread: Option<f64>,
        source_ts: Option<i64>,
    },
    LastTrade {
        market_id: String,
        token_id: String,
        side: Side,
        price: f64,
        size: f64,
        fee_rate_bps: Option<f64>,
        source_ts: Option<i64>,
    },
    ReferenceTick {
        price: ReferencePrice,
    },
    PredictiveTick {
        price: ReferencePrice,
    },
    SignalUpdate {
        decision: SignalDecision,
    },
    PaperOrderPlaced {
        order: PaperOrder,
    },
    PaperOrderCanceled {
        order_id: String,
        market_id: String,
        reason: String,
        canceled_ts: i64,
    },
    PaperFill {
        fill: PaperFill,
    },
    RiskHalt {
        market_id: Option<String>,
        asset: Option<Asset>,
        risk_state: RiskState,
    },
    ReplayCheckpoint {
        replay_run_id: String,
        event_count: u64,
        checkpoint_ts: i64,
    },
}

impl NormalizedEvent {
    pub fn event_type(&self) -> EventType {
        match self {
            NormalizedEvent::MarketDiscovered { .. } => EventType::MarketDiscovered,
            NormalizedEvent::MarketCreated { .. } => EventType::MarketCreated,
            NormalizedEvent::MarketUpdated { .. } => EventType::MarketUpdated,
            NormalizedEvent::MarketResolved { .. } => EventType::MarketResolved,
            NormalizedEvent::BookSnapshot { .. } => EventType::BookSnapshot,
            NormalizedEvent::BookDelta { .. } => EventType::BookDelta,
            NormalizedEvent::TickSizeChange { .. } => EventType::TickSizeChange,
            NormalizedEvent::BestBidAsk { .. } => EventType::BestBidAsk,
            NormalizedEvent::LastTrade { .. } => EventType::LastTrade,
            NormalizedEvent::ReferenceTick { .. } => EventType::ReferenceTick,
            NormalizedEvent::PredictiveTick { .. } => EventType::PredictiveTick,
            NormalizedEvent::SignalUpdate { .. } => EventType::SignalUpdate,
            NormalizedEvent::PaperOrderPlaced { .. } => EventType::PaperOrderPlaced,
            NormalizedEvent::PaperOrderCanceled { .. } => EventType::PaperOrderCanceled,
            NormalizedEvent::PaperFill { .. } => EventType::PaperFill,
            NormalizedEvent::RiskHalt { .. } => EventType::RiskHalt,
            NormalizedEvent::ReplayCheckpoint { .. } => EventType::ReplayCheckpoint,
        }
    }

    pub fn market_id(&self) -> Option<String> {
        match self {
            NormalizedEvent::MarketDiscovered { market }
            | NormalizedEvent::MarketUpdated { market, .. } => Some(market.market_id.clone()),
            NormalizedEvent::MarketCreated { market_id, .. }
            | NormalizedEvent::TickSizeChange { market_id, .. } => Some(market_id.clone()),
            NormalizedEvent::MarketResolved { market_id, .. }
            | NormalizedEvent::BookDelta { market_id, .. }
            | NormalizedEvent::BestBidAsk { market_id, .. }
            | NormalizedEvent::LastTrade { market_id, .. }
            | NormalizedEvent::PaperOrderCanceled { market_id, .. } => Some(market_id.clone()),
            NormalizedEvent::BookSnapshot { book } => Some(book.market_id.clone()),
            NormalizedEvent::SignalUpdate { decision } => Some(decision.market_id.clone()),
            NormalizedEvent::PaperOrderPlaced { order } => Some(order.market_id.clone()),
            NormalizedEvent::PaperFill { fill } => Some(fill.market_id.clone()),
            NormalizedEvent::RiskHalt { market_id, .. } => market_id.clone(),
            NormalizedEvent::ReferenceTick { .. }
            | NormalizedEvent::PredictiveTick { .. }
            | NormalizedEvent::ReplayCheckpoint { .. } => None,
        }
    }

    pub fn asset(&self) -> Option<Asset> {
        match self {
            NormalizedEvent::MarketDiscovered { market }
            | NormalizedEvent::MarketUpdated { market, .. } => Some(market.asset),
            NormalizedEvent::ReferenceTick { price }
            | NormalizedEvent::PredictiveTick { price } => Some(price.asset),
            NormalizedEvent::PaperOrderPlaced { order } => Some(order.asset),
            NormalizedEvent::PaperFill { fill } => Some(fill.asset),
            NormalizedEvent::RiskHalt { asset, .. } => *asset,
            _ => None,
        }
    }

    pub fn source_ts(&self) -> Option<i64> {
        match self {
            NormalizedEvent::BookSnapshot { book } => book.source_ts,
            NormalizedEvent::BookDelta { source_ts, .. }
            | NormalizedEvent::TickSizeChange { source_ts, .. }
            | NormalizedEvent::BestBidAsk { source_ts, .. }
            | NormalizedEvent::LastTrade { source_ts, .. }
            | NormalizedEvent::MarketCreated { source_ts, .. } => *source_ts,
            NormalizedEvent::ReferenceTick { price }
            | NormalizedEvent::PredictiveTick { price } => price.source_ts,
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FeeParameters, MarketLifecycleState, OrderKind, OutcomeToken, PaperOrderStatus,
        RiskHaltReason,
    };

    #[test]
    fn every_normalized_event_variant_round_trips() {
        for payload in sample_payloads() {
            let event_type = payload.event_type();
            let envelope = EventEnvelope::new(
                "run-1",
                format!("event-{}", event_type.as_str()),
                "unit-test",
                1_777_000_000_000,
                1_000,
                1,
                payload,
            );

            let encoded = serde_json::to_string(&envelope).expect("event serializes");
            let decoded: EventEnvelope =
                serde_json::from_str(&encoded).expect("event deserializes");

            assert_eq!(decoded, envelope);
            assert_eq!(decoded.event_type, decoded.payload.event_type());
            assert_eq!(decoded.run_id, "run-1");
            assert_eq!(decoded.recv_mono_ns, 1_000);
            assert_eq!(decoded.ingest_seq, 1);
        }
    }

    #[test]
    fn replay_ordering_key_uses_required_fields() {
        let envelope = EventEnvelope::new(
            "run-1",
            "event-1",
            "unit-test",
            1,
            10,
            20,
            NormalizedEvent::ReplayCheckpoint {
                replay_run_id: "replay-1".to_string(),
                event_count: 1,
                checkpoint_ts: 1,
            },
        );

        assert_eq!(envelope.replay_ordering_key(), (10, 20, "event-1"));
    }

    fn sample_payloads() -> Vec<NormalizedEvent> {
        let market = sample_market();
        let book = OrderBookSnapshot {
            market_id: market.market_id.clone(),
            token_id: "token-up".to_string(),
            bids: vec![OrderBookLevel {
                price: 0.49,
                size: 100.0,
            }],
            asks: vec![OrderBookLevel {
                price: 0.51,
                size: 100.0,
            }],
            hash: Some("book-hash".to_string()),
            source_ts: Some(1_777_000_000_001),
        };
        let reference_price = ReferencePrice {
            asset: Asset::Btc,
            source: "unit-reference".to_string(),
            price: 65_000.0,
            source_ts: Some(1_777_000_000_002),
            recv_wall_ts: 1_777_000_000_003,
        };
        let decision = SignalDecision {
            market_id: market.market_id.clone(),
            token_id: "token-up".to_string(),
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            price: 0.49,
            size: 10.0,
            fair_probability: 0.53,
            market_probability: 0.50,
            expected_value_bps: 100.0,
            reason: "unit sample".to_string(),
            created_ts: 1_777_000_000_004,
        };
        let order = PaperOrder {
            order_id: "paper-order-1".to_string(),
            market_id: market.market_id.clone(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            price: 0.49,
            size: 10.0,
            filled_size: 0.0,
            status: PaperOrderStatus::Open,
            reason: "unit sample".to_string(),
            created_ts: 1_777_000_000_005,
            updated_ts: 1_777_000_000_005,
        };
        let fill = PaperFill {
            fill_id: "paper-fill-1".to_string(),
            order_id: order.order_id.clone(),
            market_id: market.market_id.clone(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            price: 0.49,
            size: 5.0,
            fee_paid: 0.0,
            liquidity: OrderKind::Maker,
            filled_ts: 1_777_000_000_006,
        };
        let risk_state = RiskState {
            halted: true,
            active_halts: vec![RiskHaltReason::StorageUnavailable],
            reason: Some("unit sample".to_string()),
            updated_ts: 1_777_000_000_007,
        };

        vec![
            NormalizedEvent::MarketDiscovered {
                market: market.clone(),
            },
            NormalizedEvent::MarketCreated {
                market_id: market.market_id.clone(),
                condition_id: Some(market.condition_id.clone()),
                slug: Some(market.slug.clone()),
                token_ids: vec!["token-up".to_string(), "token-down".to_string()],
                outcomes: vec!["Up".to_string(), "Down".to_string()],
                source_ts: Some(1_777_000_000_009),
                raw: serde_json::json!({"event_type": "new_market"}),
            },
            NormalizedEvent::MarketUpdated {
                market: market.clone(),
                changes: vec!["fees".to_string()],
            },
            NormalizedEvent::MarketResolved {
                market_id: market.market_id.clone(),
                outcome_token_id: "token-up".to_string(),
                resolved_ts: 1_777_000_000_010,
            },
            NormalizedEvent::BookSnapshot { book },
            NormalizedEvent::BookDelta {
                market_id: market.market_id.clone(),
                token_id: "token-up".to_string(),
                bids: vec![OrderBookLevel {
                    price: 0.50,
                    size: 20.0,
                }],
                asks: Vec::new(),
                hash: Some("delta-hash".to_string()),
                source_ts: Some(1_777_000_000_011),
            },
            NormalizedEvent::TickSizeChange {
                market_id: market.market_id.clone(),
                token_id: "token-up".to_string(),
                old_tick_size: 0.01,
                new_tick_size: 0.001,
                source_ts: Some(1_777_000_000_011),
            },
            NormalizedEvent::BestBidAsk {
                market_id: market.market_id.clone(),
                token_id: "token-up".to_string(),
                best_bid: Some(0.49),
                best_ask: Some(0.51),
                spread: Some(0.02),
                source_ts: Some(1_777_000_000_012),
            },
            NormalizedEvent::LastTrade {
                market_id: market.market_id.clone(),
                token_id: "token-up".to_string(),
                side: Side::Buy,
                price: 0.50,
                size: 10.0,
                fee_rate_bps: Some(200.0),
                source_ts: Some(1_777_000_000_013),
            },
            NormalizedEvent::ReferenceTick {
                price: reference_price.clone(),
            },
            NormalizedEvent::PredictiveTick {
                price: reference_price,
            },
            NormalizedEvent::SignalUpdate { decision },
            NormalizedEvent::PaperOrderPlaced {
                order: order.clone(),
            },
            NormalizedEvent::PaperOrderCanceled {
                order_id: order.order_id,
                market_id: market.market_id.clone(),
                reason: "unit cancel".to_string(),
                canceled_ts: 1_777_000_000_014,
            },
            NormalizedEvent::PaperFill { fill },
            NormalizedEvent::RiskHalt {
                market_id: Some(market.market_id),
                asset: Some(Asset::Btc),
                risk_state,
            },
            NormalizedEvent::ReplayCheckpoint {
                replay_run_id: "replay-1".to_string(),
                event_count: 15,
                checkpoint_ts: 1_777_000_000_015,
            },
        ]
    }

    fn sample_market() -> Market {
        Market {
            market_id: "market-1".to_string(),
            slug: "btc-up-down-15m".to_string(),
            title: "BTC Up or Down".to_string(),
            asset: Asset::Btc,
            condition_id: "condition-1".to_string(),
            outcomes: vec![
                OutcomeToken {
                    token_id: "token-up".to_string(),
                    outcome: "Up".to_string(),
                },
                OutcomeToken {
                    token_id: "token-down".to_string(),
                    outcome: "Down".to_string(),
                },
            ],
            start_ts: 1_777_000_000_000,
            end_ts: 1_777_000_900_000,
            resolution_source: Some("unit-resolution-source".to_string()),
            tick_size: 0.01,
            min_order_size: 5.0,
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps: 200.0,
                raw_fee_config: None,
            },
            lifecycle_state: MarketLifecycleState::Active,
            ineligibility_reason: None,
        }
    }
}
