use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::domain::{OrderBookLevel, OrderBookSnapshot, Side};
use crate::events::NormalizedEvent;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PriceLevelSnapshot {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BookSideSnapshot {
    pub levels: Vec<PriceLevelSnapshot>,
    pub visible_depth: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LastTradeState {
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub fee_rate_bps: Option<f64>,
    pub source_ts: Option<i64>,
    pub recv_wall_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TokenBookSnapshot {
    pub market_id: String,
    pub token_id: String,
    pub bids: BookSideSnapshot,
    pub asks: BookSideSnapshot,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub last_update_ts: Option<i64>,
    pub last_recv_wall_ts: Option<i64>,
    pub hash: Option<String>,
    pub last_trade: Option<LastTradeState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct BookFreshness {
    pub market_id: String,
    pub token_id: String,
    pub last_recv_wall_ts: Option<i64>,
    pub age_ms: Option<i64>,
    pub stale_after_ms: u64,
    pub is_stale: bool,
}

impl BookFreshness {
    pub fn missing(
        market_id: impl Into<String>,
        token_id: impl Into<String>,
        stale_after_ms: u64,
    ) -> Self {
        Self {
            market_id: market_id.into(),
            token_id: token_id.into(),
            last_recv_wall_ts: None,
            age_ms: None,
            stale_after_ms,
            is_stale: true,
        }
    }

    pub fn from_last_recv(
        market_id: impl Into<String>,
        token_id: impl Into<String>,
        last_recv_wall_ts: i64,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> Self {
        let age_ms = now_wall_ts.saturating_sub(last_recv_wall_ts).max(0);
        Self {
            market_id: market_id.into(),
            token_id: token_id.into(),
            last_recv_wall_ts: Some(last_recv_wall_ts),
            age_ms: Some(age_ms),
            stale_after_ms,
            is_stale: age_ms > stale_after_ms as i64,
        }
    }

    pub fn is_fresh(&self) -> bool {
        !self.is_stale
    }

    pub fn is_stale(&self) -> bool {
        self.is_stale
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookUpdateError {
    MarketMismatch {
        token_id: String,
        existing_market_id: String,
        event_market_id: String,
    },
}

impl fmt::Display for BookUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BookUpdateError::MarketMismatch {
                token_id,
                existing_market_id,
                event_market_id,
            } => write!(
                f,
                "token {token_id} already belongs to market {existing_market_id}, not {event_market_id}"
            ),
        }
    }
}

impl std::error::Error for BookUpdateError {}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderBookState {
    books: BTreeMap<String, TokenBook>,
}

impl OrderBookState {
    pub fn new() -> Self {
        Self {
            books: BTreeMap::new(),
        }
    }

    pub fn apply_event(&mut self, event: &NormalizedEvent) -> Result<bool, BookUpdateError> {
        let recv_wall_ts = event.source_ts().unwrap_or_default();
        self.apply_event_with_recv_wall_ts(event, recv_wall_ts)
    }

    pub fn apply_event_with_recv_wall_ts(
        &mut self,
        event: &NormalizedEvent,
        recv_wall_ts: i64,
    ) -> Result<bool, BookUpdateError> {
        match event {
            NormalizedEvent::BookSnapshot { book } => {
                self.apply_snapshot(book.clone(), recv_wall_ts)?;
                Ok(true)
            }
            NormalizedEvent::BookDelta {
                market_id,
                token_id,
                bids,
                asks,
                hash,
                source_ts,
            } => {
                self.apply_delta(
                    market_id,
                    token_id,
                    bids.clone(),
                    asks.clone(),
                    hash.clone(),
                    *source_ts,
                    recv_wall_ts,
                )?;
                Ok(true)
            }
            NormalizedEvent::BestBidAsk {
                market_id,
                token_id,
                best_bid,
                best_ask,
                spread,
                source_ts,
            } => {
                self.apply_best_bid_ask(
                    market_id,
                    token_id,
                    *best_bid,
                    *best_ask,
                    *spread,
                    *source_ts,
                    recv_wall_ts,
                )?;
                Ok(true)
            }
            NormalizedEvent::LastTrade {
                market_id,
                token_id,
                side,
                price,
                size,
                fee_rate_bps,
                source_ts,
            } => {
                self.record_last_trade(
                    market_id,
                    token_id,
                    *side,
                    *price,
                    *size,
                    *fee_rate_bps,
                    *source_ts,
                    recv_wall_ts,
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn apply_snapshot(
        &mut self,
        snapshot: OrderBookSnapshot,
        recv_wall_ts: i64,
    ) -> Result<(), BookUpdateError> {
        let book = self.book_mut(&snapshot.market_id, &snapshot.token_id)?;
        book.apply_snapshot(&snapshot, recv_wall_ts);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_delta(
        &mut self,
        market_id: &str,
        token_id: &str,
        bids: Vec<OrderBookLevel>,
        asks: Vec<OrderBookLevel>,
        hash: Option<String>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) -> Result<(), BookUpdateError> {
        let book = self.book_mut(market_id, token_id)?;
        book.apply_delta(&bids, &asks, hash, source_ts, recv_wall_ts);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_best_bid_ask(
        &mut self,
        market_id: &str,
        token_id: &str,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        spread: Option<f64>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) -> Result<(), BookUpdateError> {
        let book = self.book_mut(market_id, token_id)?;
        book.apply_best_bid_ask(best_bid, best_ask, spread, source_ts, recv_wall_ts);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_last_trade(
        &mut self,
        market_id: &str,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        fee_rate_bps: Option<f64>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) -> Result<(), BookUpdateError> {
        self.record_last_trade(
            market_id,
            token_id,
            side,
            price,
            size,
            fee_rate_bps,
            source_ts,
            recv_wall_ts,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_last_trade(
        &mut self,
        market_id: &str,
        token_id: &str,
        side: Side,
        price: f64,
        size: f64,
        fee_rate_bps: Option<f64>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) -> Result<(), BookUpdateError> {
        let book = self.book_mut(market_id, token_id)?;
        book.record_last_trade(side, price, size, fee_rate_bps, source_ts, recv_wall_ts);
        Ok(())
    }

    pub fn get(&self, token_id: &str) -> Option<&TokenBook> {
        self.books.get(token_id)
    }

    pub fn token_snapshot(&self, token_id: &str) -> Option<TokenBookSnapshot> {
        self.get(token_id).map(TokenBook::snapshot)
    }

    pub fn snapshots(&self) -> Vec<TokenBookSnapshot> {
        self.books.values().map(TokenBook::snapshot).collect()
    }

    pub fn market_snapshots(&self, market_id: &str) -> Vec<TokenBookSnapshot> {
        self.books
            .values()
            .filter(|book| book.market_id == market_id)
            .map(TokenBook::snapshot)
            .collect()
    }

    pub fn freshness(
        &self,
        token_id: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> BookFreshness {
        self.get(token_id)
            .map(|book| book.freshness(now_wall_ts, stale_after_ms))
            .unwrap_or_else(|| BookFreshness::missing("", token_id, stale_after_ms))
    }

    pub fn book_freshness(
        &self,
        market_id: &str,
        token_id: &str,
        now_wall_ts: i64,
        stale_after_ms: u64,
    ) -> BookFreshness {
        self.get(token_id)
            .filter(|book| book.market_id == market_id)
            .map(|book| book.freshness(now_wall_ts, stale_after_ms))
            .unwrap_or_else(|| BookFreshness::missing(market_id, token_id, stale_after_ms))
    }

    fn book_mut(
        &mut self,
        market_id: &str,
        token_id: &str,
    ) -> Result<&mut TokenBook, BookUpdateError> {
        let book = self
            .books
            .entry(token_id.to_string())
            .or_insert_with(|| TokenBook::new(market_id.to_string(), token_id.to_string()));

        if book.market_id != market_id {
            return Err(BookUpdateError::MarketMismatch {
                token_id: token_id.to_string(),
                existing_market_id: book.market_id.clone(),
                event_market_id: market_id.to_string(),
            });
        }

        Ok(book)
    }
}

impl Default for OrderBookState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenBook {
    market_id: String,
    token_id: String,
    bids: Vec<OrderBookLevel>,
    asks: Vec<OrderBookLevel>,
    best_bid_hint: Option<f64>,
    best_ask_hint: Option<f64>,
    spread_hint: Option<f64>,
    last_update_ts: Option<i64>,
    last_recv_wall_ts: Option<i64>,
    hash: Option<String>,
    last_trade: Option<LastTradeState>,
}

impl TokenBook {
    pub fn new(market_id: impl Into<String>, token_id: impl Into<String>) -> Self {
        Self {
            market_id: market_id.into(),
            token_id: token_id.into(),
            bids: Vec::new(),
            asks: Vec::new(),
            best_bid_hint: None,
            best_ask_hint: None,
            spread_hint: None,
            last_update_ts: None,
            last_recv_wall_ts: None,
            hash: None,
            last_trade: None,
        }
    }

    pub fn market_id(&self) -> &str {
        &self.market_id
    }

    pub fn token_id(&self) -> &str {
        &self.token_id
    }

    pub fn last_update_ts(&self) -> Option<i64> {
        self.last_update_ts
    }

    pub fn last_recv_wall_ts(&self) -> Option<i64> {
        self.last_recv_wall_ts
    }

    pub fn hash(&self) -> Option<&str> {
        self.hash.as_deref()
    }

    pub fn last_trade(&self) -> Option<&LastTradeState> {
        self.last_trade.as_ref()
    }

    pub fn snapshot(&self) -> TokenBookSnapshot {
        let book_best_bid = self.book_best_bid();
        let book_best_ask = self.book_best_ask();
        let best_bid = book_best_bid.or(self.best_bid_hint);
        let best_ask = book_best_ask.or(self.best_ask_hint);

        TokenBookSnapshot {
            market_id: self.market_id.clone(),
            token_id: self.token_id.clone(),
            bids: side_snapshot(&self.bids),
            asks: side_snapshot(&self.asks),
            best_bid,
            best_ask,
            spread: match (book_best_bid, book_best_ask) {
                (Some(best_bid), Some(best_ask)) => spread(Some(best_bid), Some(best_ask)),
                _ => self.spread_hint.or_else(|| spread(best_bid, best_ask)),
            },
            last_update_ts: self.last_update_ts,
            last_recv_wall_ts: self.last_recv_wall_ts,
            hash: self.hash.clone(),
            last_trade: self.last_trade.clone(),
        }
    }

    pub fn freshness(&self, now_wall_ts: i64, stale_after_ms: u64) -> BookFreshness {
        self.last_recv_wall_ts.map_or_else(
            || {
                BookFreshness::missing(
                    self.market_id.clone(),
                    self.token_id.clone(),
                    stale_after_ms,
                )
            },
            |last_recv_wall_ts| {
                BookFreshness::from_last_recv(
                    self.market_id.clone(),
                    self.token_id.clone(),
                    last_recv_wall_ts,
                    now_wall_ts,
                    stale_after_ms,
                )
            },
        )
    }

    fn apply_snapshot(&mut self, snapshot: &OrderBookSnapshot, recv_wall_ts: i64) {
        self.bids = normalized_levels(&snapshot.bids, Side::Buy);
        self.asks = normalized_levels(&snapshot.asks, Side::Sell);
        self.best_bid_hint = None;
        self.best_ask_hint = None;
        self.spread_hint = None;
        self.last_update_ts = snapshot.source_ts;
        self.last_recv_wall_ts = Some(recv_wall_ts);
        self.hash = snapshot.hash.clone();
    }

    fn apply_delta(
        &mut self,
        bids: &[OrderBookLevel],
        asks: &[OrderBookLevel],
        hash: Option<String>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) {
        apply_side_updates(&mut self.bids, bids, Side::Buy);
        apply_side_updates(&mut self.asks, asks, Side::Sell);
        self.best_bid_hint = None;
        self.best_ask_hint = None;
        self.spread_hint = None;
        self.last_update_ts = source_ts;
        self.last_recv_wall_ts = Some(recv_wall_ts);
        self.hash = hash;
    }

    fn apply_best_bid_ask(
        &mut self,
        best_bid: Option<f64>,
        best_ask: Option<f64>,
        spread: Option<f64>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) {
        self.best_bid_hint = best_bid;
        self.best_ask_hint = best_ask;
        self.spread_hint = spread;
        self.last_update_ts = source_ts;
        self.last_recv_wall_ts = Some(recv_wall_ts);
    }

    fn record_last_trade(
        &mut self,
        side: Side,
        price: f64,
        size: f64,
        fee_rate_bps: Option<f64>,
        source_ts: Option<i64>,
        recv_wall_ts: i64,
    ) {
        self.last_trade = Some(LastTradeState {
            side,
            price,
            size,
            fee_rate_bps,
            source_ts,
            recv_wall_ts,
        });
    }

    fn book_best_bid(&self) -> Option<f64> {
        self.bids.first().map(|level| level.price)
    }

    fn book_best_ask(&self) -> Option<f64> {
        self.asks.first().map(|level| level.price)
    }
}

fn normalized_levels(levels: &[OrderBookLevel], side: Side) -> Vec<OrderBookLevel> {
    let mut normalized = Vec::new();

    for level in levels {
        apply_level_update(&mut normalized, level);
    }

    sort_levels(&mut normalized, side);
    normalized
}

fn apply_side_updates(levels: &mut Vec<OrderBookLevel>, updates: &[OrderBookLevel], side: Side) {
    for update in updates {
        apply_level_update(levels, update);
    }

    sort_levels(levels, side);
}

fn apply_level_update(levels: &mut Vec<OrderBookLevel>, update: &OrderBookLevel) {
    if update.size == 0.0 {
        levels.retain(|level| level.price != update.price);
        return;
    }

    if let Some(existing) = levels.iter_mut().find(|level| level.price == update.price) {
        existing.size = update.size;
    } else {
        levels.push(update.clone());
    }
}

fn sort_levels(levels: &mut [OrderBookLevel], side: Side) {
    match side {
        Side::Buy => levels.sort_by(|left, right| right.price.total_cmp(&left.price)),
        Side::Sell => levels.sort_by(|left, right| left.price.total_cmp(&right.price)),
    }
}

fn side_snapshot(levels: &[OrderBookLevel]) -> BookSideSnapshot {
    BookSideSnapshot {
        levels: levels
            .iter()
            .map(|level| PriceLevelSnapshot {
                price: level.price,
                size: level.size,
            })
            .collect(),
        visible_depth: levels.iter().map(|level| level.size).sum(),
    }
}

fn spread(best_bid: Option<f64>, best_ask: Option<f64>) -> Option<f64> {
    match (best_bid, best_ask) {
        (Some(best_bid), Some(best_ask)) => Some(best_ask - best_bid),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKET_ID: &str = "market-1";
    const TOKEN_ID: &str = "token-up";

    #[test]
    fn snapshot_replacement_discards_previous_levels() {
        let mut state = OrderBookState::new();

        state
            .apply_snapshot(
                book_snapshot(
                    vec![level(0.48, 40.0), level(0.49, 10.0)],
                    vec![level(0.52, 20.0)],
                    Some("snapshot-1"),
                    Some(1_000),
                ),
                1_010,
            )
            .expect("first snapshot applies");
        state
            .apply_snapshot(
                book_snapshot(
                    vec![level(0.47, 5.0)],
                    vec![level(0.53, 15.0), level(0.54, 25.0)],
                    Some("snapshot-2"),
                    Some(2_000),
                ),
                2_010,
            )
            .expect("replacement snapshot applies");

        let snapshot = state.token_snapshot(TOKEN_ID).expect("token snapshot");
        assert_eq!(snapshot.bids.levels, vec![price_level(0.47, 5.0)]);
        assert_eq!(
            snapshot.asks.levels,
            vec![price_level(0.53, 15.0), price_level(0.54, 25.0)]
        );
        assert_eq!(snapshot.best_bid, Some(0.47));
        assert_eq!(snapshot.best_ask, Some(0.53));
        assert_f64_close(snapshot.spread.expect("spread"), 0.06);
        assert_eq!(snapshot.bids.visible_depth, 5.0);
        assert_eq!(snapshot.asks.visible_depth, 40.0);
        assert_eq!(snapshot.hash.as_deref(), Some("snapshot-2"));
        assert_eq!(snapshot.last_update_ts, Some(2_000));
        assert_eq!(snapshot.last_recv_wall_ts, Some(2_010));
    }

    #[test]
    fn delta_updates_and_removes_price_levels() {
        let mut state = OrderBookState::new();
        state
            .apply_snapshot(
                book_snapshot(
                    vec![level(0.48, 40.0), level(0.49, 10.0)],
                    vec![level(0.51, 8.0), level(0.52, 20.0)],
                    Some("snapshot-1"),
                    Some(1_000),
                ),
                1_010,
            )
            .expect("snapshot applies");

        state
            .apply_delta(
                MARKET_ID,
                TOKEN_ID,
                vec![level(0.50, 12.0), level(0.49, 0.0)],
                vec![level(0.51, 0.0), level(0.53, 7.0)],
                Some("delta-1".to_string()),
                Some(1_500),
                1_510,
            )
            .expect("delta applies");

        let snapshot = state.token_snapshot(TOKEN_ID).expect("token snapshot");
        assert_eq!(
            snapshot.bids.levels,
            vec![price_level(0.50, 12.0), price_level(0.48, 40.0)]
        );
        assert_eq!(
            snapshot.asks.levels,
            vec![price_level(0.52, 20.0), price_level(0.53, 7.0)]
        );
        assert_eq!(snapshot.best_bid, Some(0.50));
        assert_eq!(snapshot.best_ask, Some(0.52));
        assert_f64_close(snapshot.spread.expect("spread"), 0.02);
        assert_eq!(snapshot.bids.visible_depth, 52.0);
        assert_eq!(snapshot.asks.visible_depth, 27.0);
        assert_eq!(snapshot.hash.as_deref(), Some("delta-1"));
        assert_eq!(snapshot.last_update_ts, Some(1_500));
        assert_eq!(snapshot.last_recv_wall_ts, Some(1_510));
    }

    #[test]
    fn best_bid_ask_tracks_top_of_book_when_depth_is_absent() {
        let mut state = OrderBookState::new();

        state
            .apply_best_bid_ask(
                MARKET_ID,
                TOKEN_ID,
                Some(0.44),
                Some(0.46),
                Some(0.02),
                Some(3_000),
                3_010,
            )
            .expect("best bid ask applies");

        let snapshot = state.token_snapshot(TOKEN_ID).expect("token snapshot");
        assert!(snapshot.bids.levels.is_empty());
        assert!(snapshot.asks.levels.is_empty());
        assert_eq!(snapshot.best_bid, Some(0.44));
        assert_eq!(snapshot.best_ask, Some(0.46));
        assert_eq!(snapshot.spread, Some(0.02));
        assert_eq!(snapshot.bids.visible_depth, 0.0);
        assert_eq!(snapshot.asks.visible_depth, 0.0);
        assert_eq!(snapshot.last_update_ts, Some(3_000));
        assert_eq!(snapshot.last_recv_wall_ts, Some(3_010));
    }

    #[test]
    fn last_trade_is_tracked_separately_from_book_freshness() {
        let mut state = OrderBookState::new();
        state
            .apply_snapshot(
                book_snapshot(
                    vec![level(0.49, 10.0)],
                    vec![level(0.51, 20.0)],
                    Some("snapshot-1"),
                    Some(1_000),
                ),
                1_010,
            )
            .expect("snapshot applies");

        state
            .apply_last_trade(
                MARKET_ID,
                TOKEN_ID,
                Side::Buy,
                0.50,
                3.25,
                Some(15.0),
                Some(1_900),
                1_910,
            )
            .expect("last trade applies");

        let snapshot = state.token_snapshot(TOKEN_ID).expect("token snapshot");
        assert_eq!(
            snapshot.last_trade,
            Some(LastTradeState {
                side: Side::Buy,
                price: 0.50,
                size: 3.25,
                fee_rate_bps: Some(15.0),
                source_ts: Some(1_900),
                recv_wall_ts: 1_910,
            })
        );
        assert_eq!(snapshot.last_update_ts, Some(1_000));
        assert_eq!(snapshot.last_recv_wall_ts, Some(1_010));
        let freshness = state.book_freshness(MARKET_ID, TOKEN_ID, 1_400, 500);
        assert_eq!(freshness.age_ms, Some(390));
        assert!(!freshness.is_stale);
    }

    #[test]
    fn freshness_reports_missing_fresh_and_stale_states() {
        let mut state = OrderBookState::new();

        let missing = state.book_freshness(MARKET_ID, "missing-token", 1_000, 500);
        assert_eq!(missing.last_recv_wall_ts, None);
        assert!(missing.is_stale);

        state
            .apply_last_trade(
                MARKET_ID,
                TOKEN_ID,
                Side::Sell,
                0.52,
                1.0,
                None,
                Some(900),
                910,
            )
            .expect("last trade applies");
        let no_book_update = state.book_freshness(MARKET_ID, TOKEN_ID, 1_000, 500);
        assert_eq!(no_book_update.last_recv_wall_ts, None);
        assert!(no_book_update.is_stale);

        state
            .apply_snapshot(
                book_snapshot(
                    vec![level(0.49, 10.0)],
                    vec![level(0.51, 20.0)],
                    Some("snapshot-1"),
                    Some(1_000),
                ),
                1_010,
            )
            .expect("snapshot applies");

        let fresh = state.book_freshness(MARKET_ID, TOKEN_ID, 1_400, 500);
        assert_eq!(fresh.age_ms, Some(390));
        assert!(!fresh.is_stale);
        assert!(fresh.is_fresh());

        let stale = state.book_freshness(MARKET_ID, TOKEN_ID, 1_511, 500);
        assert_eq!(stale.age_ms, Some(501));
        assert!(stale.is_stale);
        assert!(stale.is_stale());
    }

    #[test]
    fn identical_event_sequence_yields_identical_state() {
        let events = vec![
            NormalizedEvent::BookSnapshot {
                book: book_snapshot(
                    vec![level(0.48, 40.0), level(0.49, 10.0)],
                    vec![level(0.51, 8.0), level(0.52, 20.0)],
                    Some("snapshot-1"),
                    Some(1_000),
                ),
            },
            NormalizedEvent::BookDelta {
                market_id: MARKET_ID.to_string(),
                token_id: TOKEN_ID.to_string(),
                bids: vec![level(0.50, 12.0), level(0.49, 0.0)],
                asks: vec![level(0.51, 0.0), level(0.53, 7.0)],
                hash: Some("delta-1".to_string()),
                source_ts: Some(1_500),
            },
            NormalizedEvent::BestBidAsk {
                market_id: MARKET_ID.to_string(),
                token_id: TOKEN_ID.to_string(),
                best_bid: Some(0.50),
                best_ask: Some(0.52),
                spread: Some(0.02),
                source_ts: Some(1_600),
            },
            NormalizedEvent::LastTrade {
                market_id: MARKET_ID.to_string(),
                token_id: TOKEN_ID.to_string(),
                side: Side::Sell,
                price: 0.52,
                size: 4.0,
                fee_rate_bps: None,
                source_ts: Some(1_700),
            },
        ];
        let mut first = OrderBookState::new();
        let mut second = OrderBookState::new();

        for (index, event) in events.iter().enumerate() {
            let recv_wall_ts = 2_000 + index as i64;
            first
                .apply_event_with_recv_wall_ts(event, recv_wall_ts)
                .expect("first state applies event");
            second
                .apply_event_with_recv_wall_ts(event, recv_wall_ts)
                .expect("second state applies event");
        }

        assert_eq!(first, second);
        assert_eq!(first.snapshots(), second.snapshots());
    }

    fn book_snapshot(
        bids: Vec<OrderBookLevel>,
        asks: Vec<OrderBookLevel>,
        hash: Option<&str>,
        source_ts: Option<i64>,
    ) -> OrderBookSnapshot {
        OrderBookSnapshot {
            market_id: MARKET_ID.to_string(),
            token_id: TOKEN_ID.to_string(),
            bids,
            asks,
            hash: hash.map(ToOwned::to_owned),
            source_ts,
        }
    }

    fn level(price: f64, size: f64) -> OrderBookLevel {
        OrderBookLevel { price, size }
    }

    fn price_level(price: f64, size: f64) -> PriceLevelSnapshot {
        PriceLevelSnapshot { price, size }
    }

    fn assert_f64_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "expected {actual} to equal {expected}"
        );
    }
}
