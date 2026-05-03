use serde::{Deserialize, Serialize};

use crate::domain::{Asset, Side};

pub const MODULE: &str = "execution_intent";
const NOTIONAL_TOLERANCE: f64 = 0.000_001;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ExecutionIntent {
    pub intent_id: String,
    pub strategy_snapshot_id: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub asset_symbol: String,
    pub asset: Asset,
    pub outcome: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub order_type: String,
    pub time_in_force: String,
    pub post_only: bool,
    pub expiry: Option<i64>,
    pub fair_probability: f64,
    pub edge_bps: f64,
    pub reference_price: f64,
    pub reference_source_timestamp: Option<i64>,
    pub book_snapshot_id: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub created_at: i64,
}

impl ExecutionIntent {
    pub fn is_live_order_request(&self) -> bool {
        false
    }

    pub fn validate_shape(&self) -> Result<(), Vec<&'static str>> {
        let mut errors = Vec::new();

        if self.intent_id.trim().is_empty() {
            errors.push("intent_id_missing");
        }
        if self.market_slug.trim().is_empty() {
            errors.push("market_slug_missing");
        }
        if self.condition_id.trim().is_empty() {
            errors.push("condition_id_missing");
        }
        if self.token_id.trim().is_empty() {
            errors.push("token_id_missing");
        }
        if !self.price.is_finite() || self.price <= 0.0 {
            errors.push("price_invalid");
        }
        if !self.size.is_finite() || self.size <= 0.0 {
            errors.push("size_invalid");
        }
        if !self.notional.is_finite() || self.notional <= 0.0 {
            errors.push("notional_invalid");
        }
        if self.price.is_finite()
            && self.price > 0.0
            && self.size.is_finite()
            && self.size > 0.0
            && self.notional.is_finite()
            && self.notional > 0.0
            && (self.price * self.size - self.notional).abs() > NOTIONAL_TOLERANCE
        {
            errors.push("notional_mismatch");
        }
        if !self.fair_probability.is_finite() || !(0.0..=1.0).contains(&self.fair_probability) {
            errors.push("fair_probability_invalid");
        }
        if !self.edge_bps.is_finite() {
            errors.push("edge_bps_invalid");
        }
        if !self.reference_price.is_finite() || self.reference_price <= 0.0 {
            errors.push("reference_price_invalid");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_intent_shape_validates_without_implying_live_order_request() {
        let intent = sample_intent();

        intent.validate_shape().expect("sample intent validates");
        assert!(!intent.is_live_order_request());
    }

    #[test]
    fn execution_intent_rejects_bad_numeric_shape() {
        let mut intent = sample_intent();
        intent.price = 0.0;
        intent.fair_probability = 1.5;

        let errors = intent.validate_shape().expect_err("bad shape should fail");

        assert!(errors.contains(&"price_invalid"));
        assert!(errors.contains(&"fair_probability_invalid"));
    }

    #[test]
    fn execution_intent_rejects_notional_that_disagrees_with_price_times_size() {
        let mut intent = sample_intent();
        intent.notional = 0.01;

        let errors = intent
            .validate_shape()
            .expect_err("notional mismatch should fail");

        assert!(errors.contains(&"notional_mismatch"));
    }

    pub fn sample_intent() -> ExecutionIntent {
        ExecutionIntent {
            intent_id: "intent-1".to_string(),
            strategy_snapshot_id: "snapshot-1".to_string(),
            market_slug: "btc-updown-15m-test".to_string(),
            condition_id: "condition-1".to_string(),
            token_id: "token-up".to_string(),
            asset_symbol: "BTC".to_string(),
            asset: Asset::Btc,
            outcome: "Up".to_string(),
            side: Side::Buy,
            price: 0.42,
            size: 5.0,
            notional: 2.1,
            order_type: "GTD".to_string(),
            time_in_force: "GTD".to_string(),
            post_only: true,
            expiry: Some(1_777_000_600),
            fair_probability: 0.47,
            edge_bps: 500.0,
            reference_price: 100_000.0,
            reference_source_timestamp: Some(1_777_000_000_000),
            book_snapshot_id: "book-1".to_string(),
            best_bid: Some(0.41),
            best_ask: Some(0.43),
            spread: Some(0.02),
            created_at: 1_777_000_000_000,
        }
    }
}
