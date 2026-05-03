use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::{Asset, Side};
use crate::live_balance_tracker::nearly_equal;

pub const MODULE: &str = "live_position_book";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct LivePositionKey {
    pub market_id: String,
    pub token_id: String,
    pub asset: Asset,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LivePosition {
    pub key: LivePositionKey,
    pub size: f64,
    pub average_price: f64,
    pub fees_paid: f64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LivePositionBook {
    positions: BTreeMap<LivePositionKey, LivePosition>,
}

impl LivePositionBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert_position(&mut self, position: LivePosition) {
        if nearly_equal(position.size, 0.0) {
            self.positions.remove(&position.key);
        } else {
            self.positions.insert(position.key.clone(), position);
        }
    }

    pub fn apply_fill(
        &mut self,
        key: LivePositionKey,
        side: Side,
        price: f64,
        size: f64,
        fee_paid: f64,
        updated_at: i64,
    ) -> Result<(), LivePositionError> {
        if !price.is_finite() || price <= 0.0 || !size.is_finite() || size <= 0.0 {
            return Err(LivePositionError::InvalidFill);
        }

        match side {
            Side::Buy => {
                let existing = self.positions.get(&key).cloned();
                let new_position = if let Some(position) = existing {
                    let new_size = position.size + size;
                    let cost = position.average_price * position.size + price * size;
                    LivePosition {
                        key: key.clone(),
                        size: new_size,
                        average_price: cost / new_size,
                        fees_paid: position.fees_paid + fee_paid,
                        updated_at,
                    }
                } else {
                    LivePosition {
                        key: key.clone(),
                        size,
                        average_price: price,
                        fees_paid: fee_paid,
                        updated_at,
                    }
                };
                self.positions.insert(key, new_position);
                Ok(())
            }
            Side::Sell => {
                let mut position = self
                    .positions
                    .get(&key)
                    .cloned()
                    .ok_or(LivePositionError::InsufficientInventory)?;
                if position.size + f64::EPSILON < size {
                    return Err(LivePositionError::InsufficientInventory);
                }
                position.size -= size;
                position.fees_paid += fee_paid;
                position.updated_at = updated_at;
                if nearly_equal(position.size, 0.0) {
                    self.positions.remove(&key);
                } else {
                    self.positions.insert(key, position);
                }
                Ok(())
            }
        }
    }

    pub fn get(&self, key: &LivePositionKey) -> Option<&LivePosition> {
        self.positions.get(key)
    }

    pub fn positions(&self) -> &BTreeMap<LivePositionKey, LivePosition> {
        &self.positions
    }

    pub fn matches(&self, other: &Self) -> bool {
        if self.positions.len() != other.positions.len() {
            return false;
        }
        self.positions.iter().all(|(key, value)| {
            other.positions.get(key).is_some_and(|other_value| {
                nearly_equal(value.size, other_value.size)
                    && nearly_equal(value.average_price, other_value.average_price)
                    && nearly_equal(value.fees_paid, other_value.fees_paid)
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivePositionError {
    InvalidFill,
    InsufficientInventory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_position_book_tracks_buy_fill_average_price() {
        let key = sample_key();
        let mut book = LivePositionBook::new();

        book.apply_fill(key.clone(), Side::Buy, 0.40, 5.0, 0.01, 1)
            .expect("first buy applies");
        book.apply_fill(key.clone(), Side::Buy, 0.50, 5.0, 0.01, 2)
            .expect("second buy applies");

        let position = book.get(&key).expect("position exists");
        assert!(nearly_equal(position.size, 10.0));
        assert!(nearly_equal(position.average_price, 0.45));
        assert!(nearly_equal(position.fees_paid, 0.02));
    }

    #[test]
    fn live_position_book_rejects_inventory_blind_sell() {
        let mut book = LivePositionBook::new();

        let error = book
            .apply_fill(sample_key(), Side::Sell, 0.40, 1.0, 0.0, 1)
            .expect_err("sell without inventory fails");

        assert_eq!(error, LivePositionError::InsufficientInventory);
    }

    #[test]
    fn live_position_book_detects_position_mismatch() {
        let key = sample_key();
        let mut left = LivePositionBook::new();
        let mut right = LivePositionBook::new();

        left.apply_fill(key.clone(), Side::Buy, 0.40, 5.0, 0.0, 1)
            .expect("left applies");
        right
            .apply_fill(key, Side::Buy, 0.40, 4.0, 0.0, 1)
            .expect("right applies");

        assert!(!left.matches(&right));
    }

    pub fn sample_key() -> LivePositionKey {
        LivePositionKey {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            outcome: "Up".to_string(),
        }
    }
}
