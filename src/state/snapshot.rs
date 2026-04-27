use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::order_book::{BookFreshness, BookUpdateError, OrderBookState, TokenBookSnapshot};
use crate::domain::{Asset, Market, MarketLifecycleState, ReferencePrice};
use crate::events::{EventEnvelope, NormalizedEvent};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct AssetPriceKey {
    pub asset: Asset,
    pub source: String,
}

impl AssetPriceKey {
    pub fn new(asset: Asset, source: impl Into<String>) -> Self {
        Self {
            asset,
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ReferenceFreshness {
    pub key: AssetPriceKey,
    pub last_recv_wall_ts: Option<i64>,
    pub age_ms: Option<i64>,
    pub stale_after_ms: u64,
    pub is_stale: bool,
}

impl ReferenceFreshness {
    pub fn missing(key: AssetPriceKey, stale_after_ms: u64) -> Self {
        Self {
            key,
            last_recv_wall_ts: None,
            age_ms: None,
            stale_after_ms,
            is_stale: true,
        }
    }

    pub fn from_last_recv(
        key: AssetPriceKey,
        last_recv_wall_ts: i64,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> Self {
        let age_ms = now_wall_ts.saturating_sub(last_recv_wall_ts).max(0);
        Self {
            key,
            last_recv_wall_ts: Some(last_recv_wall_ts),
            age_ms: Some(age_ms),
            stale_after_ms,
            is_stale: age_ms > stale_after_ms as i64,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketStateSnapshot {
    pub market_id: String,
    pub market: Option<Market>,
    pub lifecycle_state: MarketLifecycleState,
    pub token_books: Vec<TokenBookSnapshot>,
    pub book_freshness: Vec<BookFreshness>,
    pub updated_wall_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PositionSnapshot {
    pub market_id: String,
    pub token_id: String,
    pub asset: Asset,
    pub size: f64,
    pub average_price: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionSnapshot {
    pub market: Market,
    pub lifecycle_state: MarketLifecycleState,
    pub token_books: Vec<TokenBookSnapshot>,
    pub book_freshness: Vec<BookFreshness>,
    pub reference_prices: Vec<ReferencePrice>,
    pub predictive_prices: Vec<ReferencePrice>,
    pub positions: Vec<PositionSnapshot>,
    pub reference_freshness: Vec<ReferenceFreshness>,
    pub snapshot_wall_ts: i64,
}

#[derive(Debug, Clone, Default)]
pub struct StateStore {
    markets: HashMap<String, Market>,
    market_lifecycle: HashMap<String, MarketLifecycleState>,
    market_updated_wall_ts: HashMap<String, i64>,
    reference_prices: HashMap<AssetPriceKey, ReferencePrice>,
    predictive_prices: HashMap<AssetPriceKey, ReferencePrice>,
    positions: HashMap<(String, String), PositionSnapshot>,
    order_books: OrderBookState,
}

impl StateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn order_books(&self) -> &OrderBookState {
        &self.order_books
    }

    pub fn market(&self, market_id: &str) -> Option<&Market> {
        self.markets.get(market_id)
    }

    pub fn market_lifecycle(&self, market_id: &str) -> Option<&MarketLifecycleState> {
        self.market_lifecycle.get(market_id)
    }

    pub fn reference_price(&self, asset: Asset, source: &str) -> Option<&ReferencePrice> {
        self.reference_prices
            .get(&AssetPriceKey::new(asset, source))
    }

    pub fn predictive_price(&self, asset: Asset, source: &str) -> Option<&ReferencePrice> {
        self.predictive_prices
            .get(&AssetPriceKey::new(asset, source))
    }

    pub fn position_snapshots(&self, market_id: &str) -> Vec<PositionSnapshot> {
        sorted_positions_for_market(&self.positions, market_id)
    }

    pub fn apply_event(&mut self, envelope: &EventEnvelope) -> Result<(), BookUpdateError> {
        match &envelope.payload {
            NormalizedEvent::MarketDiscovered { market }
            | NormalizedEvent::MarketUpdated { market, .. } => {
                self.upsert_market(market.clone(), envelope.recv_wall_ts);
            }
            NormalizedEvent::MarketCreated { market_id, .. } => {
                self.market_lifecycle
                    .entry(market_id.clone())
                    .or_insert(MarketLifecycleState::Discovered);
                self.market_updated_wall_ts
                    .insert(market_id.clone(), envelope.recv_wall_ts);
            }
            NormalizedEvent::MarketResolved { market_id, .. } => {
                self.market_lifecycle
                    .insert(market_id.clone(), MarketLifecycleState::Resolved);
                if let Some(market) = self.markets.get_mut(market_id) {
                    market.lifecycle_state = MarketLifecycleState::Resolved;
                }
                self.market_updated_wall_ts
                    .insert(market_id.clone(), envelope.recv_wall_ts);
            }
            NormalizedEvent::BookSnapshot { .. }
            | NormalizedEvent::BookDelta { .. }
            | NormalizedEvent::BestBidAsk { .. }
            | NormalizedEvent::LastTrade { .. } => {
                self.order_books
                    .apply_event_with_recv_wall_ts(&envelope.payload, envelope.recv_wall_ts)?;
            }
            NormalizedEvent::ReferenceTick { price } => {
                self.reference_prices.insert(
                    AssetPriceKey::new(price.asset, price.source.clone()),
                    price.clone(),
                );
            }
            NormalizedEvent::PredictiveTick { price } => {
                self.predictive_prices.insert(
                    AssetPriceKey::new(price.asset, price.source.clone()),
                    price.clone(),
                );
            }
            NormalizedEvent::TickSizeChange { .. }
            | NormalizedEvent::SignalUpdate { .. }
            | NormalizedEvent::PaperOrderPlaced { .. }
            | NormalizedEvent::PaperOrderCanceled { .. }
            | NormalizedEvent::PaperFill { .. }
            | NormalizedEvent::RiskHalt { .. }
            | NormalizedEvent::ReplayCheckpoint { .. } => {}
        }

        Ok(())
    }

    pub fn reference_freshness(
        &self,
        asset: Asset,
        source: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> ReferenceFreshness {
        let key = AssetPriceKey::new(asset, source);
        self.reference_prices.get(&key).map_or_else(
            || ReferenceFreshness::missing(key.clone(), stale_after_ms),
            |price| {
                ReferenceFreshness::from_last_recv(
                    key.clone(),
                    price.recv_wall_ts,
                    now_wall_ts,
                    stale_after_ms,
                )
            },
        )
    }

    pub fn is_reference_stale(
        &self,
        asset: Asset,
        source: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> bool {
        self.reference_freshness(asset, source, now_wall_ts, stale_after_ms)
            .is_stale
    }

    pub fn book_freshness(
        &self,
        market_id: &str,
        token_id: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> BookFreshness {
        self.order_books
            .book_freshness(market_id, token_id, now_wall_ts, stale_after_ms)
    }

    pub fn is_book_stale(
        &self,
        market_id: &str,
        token_id: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> bool {
        self.book_freshness(market_id, token_id, now_wall_ts, stale_after_ms)
            .is_stale()
    }

    pub fn market_snapshot(
        &self,
        market_id: &str,
        now_wall_ts: i64,
        stale_book_ms: u64,
    ) -> Option<MarketStateSnapshot> {
        let market = self.markets.get(market_id).cloned();
        let lifecycle_state = self
            .market_lifecycle
            .get(market_id)
            .cloned()
            .or_else(|| market.as_ref().map(|market| market.lifecycle_state.clone()))?;
        let token_books = sorted_token_books(self.order_books.market_snapshots(market_id));
        let book_freshness = token_books
            .iter()
            .map(|book| {
                self.book_freshness(&book.market_id, &book.token_id, now_wall_ts, stale_book_ms)
            })
            .collect();

        Some(MarketStateSnapshot {
            market_id: market_id.to_string(),
            market,
            lifecycle_state,
            token_books,
            book_freshness,
            updated_wall_ts: self.market_updated_wall_ts.get(market_id).copied(),
        })
    }

    pub fn decision_snapshot(
        &self,
        market_id: &str,
        now_wall_ts: i64,
        stale_book_ms: u64,
        stale_reference_ms: u64,
    ) -> Option<DecisionSnapshot> {
        let market_snapshot = self.market_snapshot(market_id, now_wall_ts, stale_book_ms)?;
        let market = market_snapshot.market?;
        let reference_prices = sorted_prices_for_asset(&self.reference_prices, market.asset);
        let predictive_prices = sorted_prices_for_asset(&self.predictive_prices, market.asset);
        let positions = self.position_snapshots(&market.market_id);
        let reference_freshness = reference_prices
            .iter()
            .map(|price| {
                self.reference_freshness(
                    price.asset,
                    &price.source,
                    now_wall_ts,
                    stale_reference_ms,
                )
            })
            .collect();

        Some(DecisionSnapshot {
            market,
            lifecycle_state: market_snapshot.lifecycle_state,
            token_books: market_snapshot.token_books,
            book_freshness: market_snapshot.book_freshness,
            reference_prices,
            predictive_prices,
            positions,
            reference_freshness,
            snapshot_wall_ts: now_wall_ts,
        })
    }

    fn upsert_market(&mut self, market: Market, recv_wall_ts: i64) {
        self.market_lifecycle
            .insert(market.market_id.clone(), market.lifecycle_state.clone());
        self.market_updated_wall_ts
            .insert(market.market_id.clone(), recv_wall_ts);
        self.markets.insert(market.market_id.clone(), market);
    }
}

fn sorted_token_books(mut books: Vec<TokenBookSnapshot>) -> Vec<TokenBookSnapshot> {
    books.sort_by(|left, right| {
        left.market_id
            .cmp(&right.market_id)
            .then_with(|| left.token_id.cmp(&right.token_id))
    });
    books
}

fn sorted_prices_for_asset(
    prices: &HashMap<AssetPriceKey, ReferencePrice>,
    asset: Asset,
) -> Vec<ReferencePrice> {
    let mut prices = prices
        .values()
        .filter(|price| price.asset == asset)
        .cloned()
        .collect::<Vec<_>>();
    prices.sort_by(|left, right| left.source.cmp(&right.source));
    prices
}

fn sorted_positions_for_market(
    positions: &HashMap<(String, String), PositionSnapshot>,
    market_id: &str,
) -> Vec<PositionSnapshot> {
    let mut positions = positions
        .values()
        .filter(|position| position.market_id == market_id)
        .cloned()
        .collect::<Vec<_>>();
    positions.sort_by(|left, right| left.token_id.cmp(&right.token_id));
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FeeParameters, OrderBookLevel, OrderBookSnapshot, OutcomeToken, Side};

    #[test]
    fn reference_freshness_marks_missing_fresh_and_stale_states() {
        let mut store = StateStore::new();

        assert!(store.is_reference_stale(Asset::Btc, "resolution", 1_000, 500));

        store
            .apply_event(&envelope(
                1,
                1_000,
                NormalizedEvent::ReferenceTick {
                    price: ReferencePrice {
                        asset: Asset::Btc,
                        source: "resolution".to_string(),
                        price: 65_000.0,
                        source_ts: Some(995),
                        recv_wall_ts: 1_000,
                    },
                },
            ))
            .expect("reference tick applies");

        let fresh = store.reference_freshness(Asset::Btc, "resolution", 1_400, 500);
        assert_eq!(fresh.age_ms, Some(400));
        assert!(!fresh.is_stale);

        let stale = store.reference_freshness(Asset::Btc, "resolution", 1_501, 500);
        assert_eq!(stale.age_ms, Some(501));
        assert!(stale.is_stale);
    }

    #[test]
    fn book_freshness_marks_missing_fresh_and_stale_states() {
        let mut store = StateStore::new();
        let market = sample_market();

        assert!(store.is_book_stale(&market.market_id, "token-up", 1_000, 500));

        store
            .apply_event(&envelope(
                1,
                1_000,
                NormalizedEvent::BookSnapshot {
                    book: OrderBookSnapshot {
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
                        source_ts: Some(100),
                    },
                },
            ))
            .expect("book snapshot applies");

        let snapshot = store
            .order_books()
            .token_snapshot("token-up")
            .expect("book snapshot exists");
        assert_eq!(snapshot.last_update_ts, Some(100));
        assert_eq!(snapshot.last_recv_wall_ts, Some(1_000));
        assert!(!store.is_book_stale(&market.market_id, "token-up", 1_400, 500));
        assert!(store.is_book_stale(&market.market_id, "token-up", 1_501, 500));
    }

    #[test]
    fn decision_snapshot_is_coherent_and_read_only() {
        let mut store = StateStore::new();
        let market = sample_market();

        store
            .apply_event(&envelope(
                1,
                1_000,
                NormalizedEvent::MarketDiscovered {
                    market: market.clone(),
                },
            ))
            .expect("market applies");
        for (event_seq, token_id) in [(2, "token-up"), (3, "token-down")] {
            store
                .apply_event(&envelope(
                    event_seq,
                    1_000,
                    NormalizedEvent::BookSnapshot {
                        book: OrderBookSnapshot {
                            market_id: market.market_id.clone(),
                            token_id: token_id.to_string(),
                            bids: vec![OrderBookLevel {
                                price: 0.49,
                                size: 100.0,
                            }],
                            asks: vec![OrderBookLevel {
                                price: 0.51,
                                size: 100.0,
                            }],
                            hash: Some(format!("{token_id}-hash")),
                            source_ts: Some(990),
                        },
                    },
                ))
                .expect("book applies");
        }
        store
            .apply_event(&envelope(
                4,
                1_010,
                NormalizedEvent::BookDelta {
                    market_id: market.market_id.clone(),
                    token_id: "token-up".to_string(),
                    bids: vec![OrderBookLevel {
                        price: 0.50,
                        size: 75.0,
                    }],
                    asks: Vec::new(),
                    hash: Some("token-up-delta-hash".to_string()),
                    source_ts: Some(1_010),
                },
            ))
            .expect("delta applies");
        store
            .apply_event(&envelope(
                5,
                1_020,
                NormalizedEvent::LastTrade {
                    market_id: market.market_id.clone(),
                    token_id: "token-up".to_string(),
                    side: Side::Buy,
                    price: 0.50,
                    size: 25.0,
                    fee_rate_bps: Some(0.0),
                    source_ts: Some(1_020),
                },
            ))
            .expect("last trade applies");
        store
            .apply_event(&envelope(
                6,
                1_000,
                NormalizedEvent::ReferenceTick {
                    price: ReferencePrice {
                        asset: Asset::Btc,
                        source: "resolution".to_string(),
                        price: 65_000.0,
                        source_ts: Some(995),
                        recv_wall_ts: 1_000,
                    },
                },
            ))
            .expect("reference applies");
        store
            .apply_event(&envelope(
                7,
                1_000,
                NormalizedEvent::PredictiveTick {
                    price: ReferencePrice {
                        asset: Asset::Btc,
                        source: "binance".to_string(),
                        price: 65_005.0,
                        source_ts: Some(995),
                        recv_wall_ts: 1_000,
                    },
                },
            ))
            .expect("predictive applies");

        let snapshot = store
            .decision_snapshot(&market.market_id, 1_250, 500, 500)
            .expect("decision snapshot exists");

        assert_eq!(snapshot.market.market_id, market.market_id);
        assert_eq!(snapshot.lifecycle_state, MarketLifecycleState::Active);
        assert_eq!(snapshot.token_books.len(), 2);
        let token_up = snapshot
            .token_books
            .iter()
            .find(|book| book.token_id == "token-up")
            .expect("token-up book is present");
        assert_eq!(token_up.best_bid, Some(0.50));
        assert!(token_up.last_trade.is_some());
        assert_eq!(snapshot.reference_prices.len(), 1);
        assert_eq!(snapshot.reference_prices[0].source, "resolution");
        assert_eq!(snapshot.predictive_prices.len(), 1);
        assert_eq!(snapshot.predictive_prices[0].source, "binance");
        assert!(snapshot.positions.is_empty());
        assert!(snapshot
            .reference_freshness
            .iter()
            .all(|freshness| !freshness.is_stale));
        assert!(snapshot
            .book_freshness
            .iter()
            .all(|freshness| !(*freshness).is_stale()));

        store
            .apply_event(&envelope(
                8,
                1_300,
                NormalizedEvent::MarketResolved {
                    market_id: market.market_id.clone(),
                    outcome_token_id: "token-up".to_string(),
                    resolved_ts: 1_300,
                },
            ))
            .expect("resolution applies");

        assert_eq!(snapshot.lifecycle_state, MarketLifecycleState::Active);
        assert_eq!(
            store.market_lifecycle(&market.market_id),
            Some(&MarketLifecycleState::Resolved)
        );
    }

    fn envelope(seq: u64, recv_wall_ts: i64, payload: NormalizedEvent) -> EventEnvelope {
        EventEnvelope::new(
            "run-1",
            format!("event-{seq}"),
            "unit-test",
            recv_wall_ts,
            seq,
            seq,
            payload,
        )
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
