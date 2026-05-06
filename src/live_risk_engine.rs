use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::Side;
use crate::execution_intent::ExecutionIntent;
use crate::live_alpha_config::LiveAlphaRiskConfig;

pub const MODULE: &str = "live_risk_engine";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveRiskApproved {
    pub intent_id: String,
    pub approved_token_id: String,
    pub approved_outcome: String,
    pub approved_notional: f64,
    pub approved_size: f64,
    pub approved_ttl_seconds: u64,
    pub approved_side: Side,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRiskRejected {
    pub intent_id: String,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRiskHalt {
    pub reason: String,
    pub intent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision_kind", rename_all = "snake_case")]
pub enum LiveRiskDecision {
    Approved(LiveRiskApproved),
    Rejected(LiveRiskRejected),
    Halt(LiveRiskHalt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveRiskEngine {
    limits: LiveAlphaRiskConfig,
}

impl LiveRiskEngine {
    pub fn new(limits: LiveAlphaRiskConfig) -> Self {
        Self { limits }
    }

    pub fn evaluate(
        &self,
        intent: &ExecutionIntent,
        context: &LiveRiskContext,
    ) -> LiveRiskDecision {
        if let Some(reason) = self.halt_reason(context) {
            return LiveRiskDecision::Halt(LiveRiskHalt {
                reason: reason.to_string(),
                intent_id: Some(intent.intent_id.clone()),
            });
        }

        let mut reasons = Vec::<String>::new();
        if let Err(shape_errors) = intent.validate_shape() {
            reasons.extend(
                shape_errors
                    .into_iter()
                    .map(|error| format!("intent_{error}")),
            );
        }
        if !intent.post_only {
            reasons.push("post_only_required".to_string());
        }
        if !intent.order_type.eq_ignore_ascii_case("GTD")
            || !intent.time_in_force.eq_ignore_ascii_case("GTD")
        {
            reasons.push("gtd_required".to_string());
        }

        let mapped = match inventory_aware_side(intent, context) {
            Ok(mapped) => mapped,
            Err(reason) => {
                reasons.push(reason.to_string());
                MappedIntent::from_intent(intent, intent.side, intent.size)
            }
        };

        let approved_notional = mapped.approved_size * intent.price;
        if mapped.approved_size <= 0.0 || !approved_notional.is_finite() || approved_notional <= 0.0
        {
            reasons.push("approved_size_invalid".to_string());
        }
        let collateral_required = mapped.side == Side::Buy;
        if collateral_required {
            if context.available_pusd < approved_notional {
                reasons.push("insufficient_pusd".to_string());
            }
            if limit_exceeded_f64(self.limits.max_available_pusd_usage, approved_notional) {
                reasons.push("max_available_pusd_usage".to_string());
            }
            if self.limits.max_reserved_pusd <= 0.0
                || context.reserved_pusd + approved_notional > self.limits.max_reserved_pusd
            {
                reasons.push("max_reserved_pusd".to_string());
            }
        }

        if limit_exceeded_f64(self.limits.max_single_order_notional, approved_notional) {
            reasons.push("max_single_order_notional".to_string());
        }
        if limit_exceeded_f64(
            self.limits.max_per_market_notional,
            context.current_market_notional + approved_notional,
        ) {
            reasons.push("max_per_market_notional".to_string());
        }
        if limit_exceeded_f64(
            self.limits.max_per_asset_notional,
            context.current_asset_notional + approved_notional,
        ) {
            reasons.push("max_per_asset_notional".to_string());
        }
        if limit_exceeded_f64(
            self.limits.max_total_live_notional,
            context.current_total_live_notional + approved_notional,
        ) {
            reasons.push("max_total_live_notional".to_string());
        }
        if self.limits.max_open_orders == 0
            || context.open_order_count >= self.limits.max_open_orders
        {
            reasons.push("max_open_orders".to_string());
        }
        if self.limits.max_open_orders_per_market == 0
            || context.open_orders_per_market >= self.limits.max_open_orders_per_market
        {
            reasons.push("max_open_orders_per_market".to_string());
        }
        if self.limits.max_open_orders_per_asset == 0
            || context.open_orders_per_asset >= self.limits.max_open_orders_per_asset
        {
            reasons.push("max_open_orders_per_asset".to_string());
        }
        if self.limits.max_fee_spend > 0.0 && context.fee_spend >= self.limits.max_fee_spend {
            reasons.push("max_fee_spend".to_string());
        }
        if self.limits.max_submit_rate_per_min == 0
            || context.submit_count_last_min >= self.limits.max_submit_rate_per_min
        {
            reasons.push("max_submit_rate_per_min".to_string());
        }
        if self.limits.max_book_staleness_ms == 0
            || context
                .book_age_ms
                .is_none_or(|age| age > self.limits.max_book_staleness_ms)
        {
            reasons.push("book_stale".to_string());
        }
        if self.limits.max_reference_staleness_ms == 0
            || context
                .reference_age_ms
                .is_none_or(|age| age > self.limits.max_reference_staleness_ms)
        {
            reasons.push("reference_stale".to_string());
        }
        if market_too_close_to_close(context, self.limits.no_trade_seconds_before_close) {
            reasons.push("market_too_close_to_close".to_string());
        }

        sort_dedup(&mut reasons);
        if reasons.is_empty() {
            LiveRiskDecision::Approved(LiveRiskApproved {
                intent_id: intent.intent_id.clone(),
                approved_token_id: mapped.token_id,
                approved_outcome: mapped.outcome,
                approved_notional,
                approved_size: mapped.approved_size,
                approved_ttl_seconds: context.effective_quote_ttl_seconds,
                approved_side: mapped.side,
                reason_codes: mapped.reason_codes,
            })
        } else {
            LiveRiskDecision::Rejected(LiveRiskRejected {
                intent_id: intent.intent_id.clone(),
                reason_codes: reasons,
            })
        }
    }

    fn halt_reason<'a>(&self, context: &'a LiveRiskContext) -> Option<&'a str> {
        if !context.geoblock_passed {
            return Some("geoblock_not_passed");
        }
        if !context.heartbeat_healthy {
            return Some("heartbeat_not_healthy");
        }
        if !context.reconciliation_clean {
            return Some("reconciliation_not_clean");
        }
        if context.unknown_open_order {
            return Some("unknown_open_order");
        }
        if !context.submit_readback_agree {
            return Some("submit_readback_disagreement");
        }
        if context.reserved_balance_mismatch {
            return Some("reserved_balance_mismatch");
        }
        if context.balance_mismatch {
            return Some("balance_mismatch");
        }
        if context.position_mismatch {
            return Some("position_mismatch");
        }
        if self.limits.max_daily_realized_loss > 0.0
            && context.daily_realized_loss >= self.limits.max_daily_realized_loss
        {
            return Some("max_daily_loss");
        }
        if self.limits.max_daily_unrealized_loss > 0.0
            && context.daily_unrealized_loss >= self.limits.max_daily_unrealized_loss
        {
            return Some("max_daily_loss");
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveRiskContext {
    pub now_ms: Option<i64>,
    pub market_end_ms: Option<i64>,
    pub effective_quote_ttl_seconds: u64,
    pub available_pusd: f64,
    pub reserved_pusd: f64,
    pub inventory_by_token: BTreeMap<String, f64>,
    pub conditional_token_allowance_by_token: BTreeMap<String, f64>,
    pub up_token_id: Option<String>,
    pub down_token_id: Option<String>,
    pub open_order_count: u64,
    pub open_orders_per_market: u64,
    pub open_orders_per_asset: u64,
    pub current_market_notional: f64,
    pub current_asset_notional: f64,
    pub current_total_live_notional: f64,
    pub daily_realized_loss: f64,
    pub daily_unrealized_loss: f64,
    pub fee_spend: f64,
    pub submit_count_last_min: u64,
    pub book_age_ms: Option<u64>,
    pub reference_age_ms: Option<u64>,
    pub geoblock_passed: bool,
    pub heartbeat_healthy: bool,
    pub reconciliation_clean: bool,
    pub unknown_open_order: bool,
    pub submit_readback_agree: bool,
    pub reserved_balance_mismatch: bool,
    pub balance_mismatch: bool,
    pub position_mismatch: bool,
}

impl Default for LiveRiskContext {
    fn default() -> Self {
        Self {
            now_ms: None,
            market_end_ms: None,
            effective_quote_ttl_seconds: 0,
            available_pusd: 0.0,
            reserved_pusd: 0.0,
            inventory_by_token: BTreeMap::new(),
            conditional_token_allowance_by_token: BTreeMap::new(),
            up_token_id: None,
            down_token_id: None,
            open_order_count: 0,
            open_orders_per_market: 0,
            open_orders_per_asset: 0,
            current_market_notional: 0.0,
            current_asset_notional: 0.0,
            current_total_live_notional: 0.0,
            daily_realized_loss: 0.0,
            daily_unrealized_loss: 0.0,
            fee_spend: 0.0,
            submit_count_last_min: 0,
            book_age_ms: None,
            reference_age_ms: None,
            geoblock_passed: false,
            heartbeat_healthy: false,
            reconciliation_clean: false,
            unknown_open_order: false,
            submit_readback_agree: true,
            reserved_balance_mismatch: false,
            balance_mismatch: false,
            position_mismatch: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MappedIntent {
    token_id: String,
    outcome: String,
    side: Side,
    approved_size: f64,
    reason_codes: Vec<String>,
}

impl MappedIntent {
    fn from_intent(intent: &ExecutionIntent, side: Side, approved_size: f64) -> Self {
        Self {
            token_id: intent.token_id.clone(),
            outcome: intent.outcome.clone(),
            side,
            approved_size,
            reason_codes: Vec::new(),
        }
    }
}

fn inventory_aware_side(
    intent: &ExecutionIntent,
    context: &LiveRiskContext,
) -> Result<MappedIntent, &'static str> {
    let outcome = normalize_outcome(&intent.outcome).ok_or("outcome_unsupported")?;
    match (outcome, intent.side) {
        (Outcome::Up, Side::Buy) | (Outcome::Down, Side::Buy) => {
            Ok(MappedIntent::from_intent(intent, Side::Buy, intent.size))
        }
        (Outcome::Up, Side::Sell) | (Outcome::Down, Side::Sell) => {
            let inventory = context
                .inventory_by_token
                .get(&intent.token_id)
                .copied()
                .unwrap_or_default();
            if inventory > 0.0 {
                let approved_size = intent.size.min(inventory);
                let allowance = context
                    .conditional_token_allowance_by_token
                    .get(&intent.token_id)
                    .copied()
                    .unwrap_or_default();
                if allowance + f64::EPSILON < approved_size {
                    return Err("conditional_token_allowance_below_required");
                }
                let mut reason_codes = vec!["reduce_only_sell".to_string()];
                if approved_size < intent.size {
                    reason_codes.push("sell_size_capped_to_inventory".to_string());
                }
                return Ok(MappedIntent {
                    token_id: intent.token_id.clone(),
                    outcome: intent.outcome.clone(),
                    side: Side::Sell,
                    approved_size,
                    reason_codes,
                });
            }

            let (token_id, outcome) = match outcome {
                Outcome::Up => (
                    context
                        .down_token_id
                        .clone()
                        .ok_or("opposite_token_missing")?,
                    "Down".to_string(),
                ),
                Outcome::Down => (
                    context
                        .up_token_id
                        .clone()
                        .ok_or("opposite_token_missing")?,
                    "Up".to_string(),
                ),
            };
            Ok(MappedIntent {
                token_id,
                outcome,
                side: Side::Buy,
                approved_size: intent.size,
                reason_codes: vec!["inventory_aware_buy_opposite".to_string()],
            })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Up,
    Down,
}

fn normalize_outcome(value: &str) -> Option<Outcome> {
    match value.trim().to_ascii_lowercase().as_str() {
        "up" | "yes" => Some(Outcome::Up),
        "down" | "no" => Some(Outcome::Down),
        _ => None,
    }
}

fn market_too_close_to_close(
    context: &LiveRiskContext,
    no_trade_seconds_before_close: u64,
) -> bool {
    let (Some(now_ms), Some(end_ms)) = (context.now_ms, context.market_end_ms) else {
        return true;
    };
    let cutoff_ms = no_trade_seconds_before_close
        .saturating_add(context.effective_quote_ttl_seconds)
        .min(i64::MAX as u64 / 1_000) as i64
        * 1_000;
    now_ms.saturating_add(cutoff_ms) >= end_ms
}

fn limit_exceeded_f64(limit: f64, value: f64) -> bool {
    limit <= 0.0 || !limit.is_finite() || !value.is_finite() || value > limit
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Asset;

    #[test]
    fn live_risk_engine_approves_bullish_up_buy_under_limits() {
        let decision = approved_decision(sample_intent(), healthy_context());

        assert_eq!(decision.approved_side, Side::Buy);
        assert_eq!(decision.approved_token_id, "token-up");
        assert_eq!(decision.approved_outcome, "Up");
        assert_eq!(decision.approved_ttl_seconds, 30);
        assert!(decision.reason_codes.is_empty());
    }

    #[test]
    fn inventory_aware_bearish_up_sells_only_existing_up_inventory() {
        let mut intent = sample_intent();
        intent.side = Side::Sell;
        intent.size = 5.0;
        intent.notional = intent.price * intent.size;
        let mut context = healthy_context();
        context
            .inventory_by_token
            .insert("token-up".to_string(), 2.0);
        context
            .conditional_token_allowance_by_token
            .insert("token-up".to_string(), 2.0);

        let decision = approved_decision(intent, context);

        assert_eq!(decision.approved_side, Side::Sell);
        assert_eq!(decision.approved_token_id, "token-up");
        assert_eq!(decision.approved_size, 2.0);
        assert!(decision
            .reason_codes
            .contains(&"reduce_only_sell".to_string()));
    }

    #[test]
    fn inventory_aware_bearish_up_buys_down_when_no_up_inventory() {
        let mut intent = sample_intent();
        intent.side = Side::Sell;
        let context = healthy_context();

        let decision = approved_decision(intent, context);

        assert_eq!(decision.approved_side, Side::Buy);
        assert_eq!(decision.approved_token_id, "token-down");
        assert_eq!(decision.approved_outcome, "Down");
        assert!(decision
            .reason_codes
            .contains(&"inventory_aware_buy_opposite".to_string()));
    }

    #[test]
    fn inventory_aware_sell_rejects_missing_conditional_token_allowance() {
        let mut intent = sample_intent();
        intent.side = Side::Sell;
        let mut context = healthy_context();
        context
            .inventory_by_token
            .insert("token-up".to_string(), 1.0);

        let rejected = rejected_decision(intent, context);

        assert!(rejected
            .reason_codes
            .contains(&"conditional_token_allowance_below_required".to_string()));
    }

    #[test]
    fn live_risk_engine_rejects_limit_violations_with_named_codes() {
        let mut context = healthy_context();
        context.reserved_pusd = 0.75;
        context.open_order_count = 1;
        context.open_orders_per_market = 1;
        context.open_orders_per_asset = 1;
        context.current_market_notional = 0.9;
        context.current_asset_notional = 0.9;
        context.current_total_live_notional = 0.9;
        context.book_age_ms = Some(1_001);
        context.reference_age_ms = Some(1_001);
        context.submit_count_last_min = 1;
        let mut limits = limits();
        limits.max_single_order_notional = 0.1;
        limits.max_reserved_pusd = 0.8;
        limits.max_open_orders = 1;
        limits.max_open_orders_per_market = 1;
        limits.max_open_orders_per_asset = 1;
        limits.max_per_market_notional = 1.0;
        limits.max_per_asset_notional = 1.0;
        limits.max_total_live_notional = 1.0;
        limits.max_book_staleness_ms = 1_000;
        limits.max_reference_staleness_ms = 1_000;
        limits.max_submit_rate_per_min = 1;

        let decision = LiveRiskEngine::new(limits).evaluate(&sample_intent(), &context);
        let LiveRiskDecision::Rejected(rejected) = decision else {
            panic!("expected rejection");
        };

        for reason in [
            "max_single_order_notional",
            "max_reserved_pusd",
            "max_open_orders",
            "max_open_orders_per_market",
            "max_open_orders_per_asset",
            "max_per_market_notional",
            "max_per_asset_notional",
            "max_total_live_notional",
            "book_stale",
            "reference_stale",
            "max_submit_rate_per_min",
        ] {
            assert!(
                rejected.reason_codes.contains(&reason.to_string()),
                "missing {reason}: {:?}",
                rejected.reason_codes
            );
        }
    }

    #[test]
    fn live_risk_engine_rejects_no_trade_window() {
        let mut context = healthy_context();
        context.now_ms = Some(1_777_000_870_000);
        context.market_end_ms = Some(1_777_000_900_000);

        let rejected = rejected_decision(sample_intent(), context);

        assert!(rejected
            .reason_codes
            .contains(&"market_too_close_to_close".to_string()));
    }

    #[test]
    fn live_risk_engine_rejects_ttl_that_would_cancel_inside_no_trade_window() {
        let mut context = healthy_context();
        context.now_ms = Some(1_777_000_830_000);
        context.market_end_ms = Some(1_777_000_900_000);
        context.effective_quote_ttl_seconds = 30;

        let rejected = rejected_decision(sample_intent(), context);

        assert!(rejected
            .reason_codes
            .contains(&"market_too_close_to_close".to_string()));
    }

    #[test]
    fn live_risk_engine_halts_on_geoblock_heartbeat_and_loss_before_approval() {
        let mut context = healthy_context();
        context.geoblock_passed = false;
        let halt = halt_decision(sample_intent(), context);
        assert_eq!(halt.reason, "geoblock_not_passed");

        let mut context = healthy_context();
        context.heartbeat_healthy = false;
        let halt = halt_decision(sample_intent(), context);
        assert_eq!(halt.reason, "heartbeat_not_healthy");

        let mut context = healthy_context();
        context.daily_realized_loss = 3.0;
        let halt = halt_decision(sample_intent(), context);
        assert_eq!(halt.reason, "max_daily_loss");
    }

    #[test]
    fn live_risk_engine_halts_on_reconciliation_mismatch_classes() {
        for (context, expected) in [
            (
                {
                    let mut context = healthy_context();
                    context.unknown_open_order = true;
                    context
                },
                "unknown_open_order",
            ),
            (
                {
                    let mut context = healthy_context();
                    context.submit_readback_agree = false;
                    context
                },
                "submit_readback_disagreement",
            ),
            (
                {
                    let mut context = healthy_context();
                    context.balance_mismatch = true;
                    context
                },
                "balance_mismatch",
            ),
            (
                {
                    let mut context = healthy_context();
                    context.position_mismatch = true;
                    context
                },
                "position_mismatch",
            ),
        ] {
            let halt = halt_decision(sample_intent(), context.clone());
            assert_eq!(halt.reason, expected);
        }
    }

    fn approved_decision(intent: ExecutionIntent, context: LiveRiskContext) -> LiveRiskApproved {
        match LiveRiskEngine::new(limits()).evaluate(&intent, &context) {
            LiveRiskDecision::Approved(approval) => approval,
            other => panic!("expected approval, got {other:?}"),
        }
    }

    fn rejected_decision(intent: ExecutionIntent, context: LiveRiskContext) -> LiveRiskRejected {
        match LiveRiskEngine::new(limits()).evaluate(&intent, &context) {
            LiveRiskDecision::Rejected(rejected) => rejected,
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    fn halt_decision(intent: ExecutionIntent, context: LiveRiskContext) -> LiveRiskHalt {
        match LiveRiskEngine::new(limits()).evaluate(&intent, &context) {
            LiveRiskDecision::Halt(halt) => halt,
            other => panic!("expected halt, got {other:?}"),
        }
    }

    fn limits() -> LiveAlphaRiskConfig {
        LiveAlphaRiskConfig {
            max_wallet_funding_pusd: 3.0,
            max_available_pusd_usage: 1.0,
            max_reserved_pusd: 1.0,
            max_single_order_notional: 1.0,
            max_per_market_notional: 1.0,
            max_per_asset_notional: 1.0,
            max_total_live_notional: 1.0,
            max_open_orders: 1,
            max_open_orders_per_market: 1,
            max_open_orders_per_asset: 1,
            max_daily_realized_loss: 3.0,
            max_daily_unrealized_loss: 3.0,
            max_fee_spend: 0.06,
            max_submit_rate_per_min: 1,
            max_cancel_rate_per_min: 1,
            max_reconciliation_lag_ms: 30_000,
            max_book_staleness_ms: 1_000,
            max_reference_staleness_ms: 1_000,
            no_trade_seconds_before_close: 60,
        }
    }

    fn healthy_context() -> LiveRiskContext {
        LiveRiskContext {
            now_ms: Some(1_777_000_100_000),
            market_end_ms: Some(1_777_000_900_000),
            effective_quote_ttl_seconds: 30,
            available_pusd: 10.0,
            reserved_pusd: 0.0,
            up_token_id: Some("token-up".to_string()),
            down_token_id: Some("token-down".to_string()),
            open_order_count: 0,
            open_orders_per_market: 0,
            open_orders_per_asset: 0,
            book_age_ms: Some(100),
            reference_age_ms: Some(100),
            geoblock_passed: true,
            heartbeat_healthy: true,
            reconciliation_clean: true,
            ..LiveRiskContext::default()
        }
    }

    fn sample_intent() -> ExecutionIntent {
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
            price: 0.20,
            size: 1.0,
            notional: 0.20,
            order_type: "GTD".to_string(),
            time_in_force: "GTD".to_string(),
            post_only: true,
            expiry: None,
            fair_probability: 0.23,
            edge_bps: 300.0,
            reference_price: 100_000.0,
            reference_source_timestamp: Some(1_777_000_000_000),
            book_snapshot_id: "book-1".to_string(),
            best_bid: Some(0.19),
            best_ask: Some(0.21),
            spread: Some(0.02),
            created_at: 1_777_000_100_000,
        }
    }
}
