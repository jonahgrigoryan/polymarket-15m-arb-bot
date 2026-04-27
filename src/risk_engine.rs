use crate::config::RiskConfig;
use crate::domain::{
    is_asset_matched_chainlink_resolution_source, Asset, MarketLifecycleState, PaperOrderIntent,
    RiskHaltReason, RiskState, Side,
};
use crate::state::{DecisionSnapshot, PositionSnapshot};

pub const MODULE: &str = "risk_engine";

#[derive(Debug, Clone, PartialEq)]
pub struct RiskLimits {
    pub max_loss_per_market: f64,
    pub max_notional_per_market: f64,
    pub max_notional_per_asset: f64,
    pub max_total_notional: f64,
    pub max_correlated_notional: f64,
    pub stale_reference_ms: u64,
    pub stale_book_ms: u64,
    pub max_orders_per_minute: u64,
    pub daily_drawdown_limit: f64,
}

impl From<&RiskConfig> for RiskLimits {
    fn from(config: &RiskConfig) -> Self {
        Self {
            max_loss_per_market: config.max_loss_per_market,
            max_notional_per_market: config.max_notional_per_market,
            max_notional_per_asset: config.max_notional_per_asset,
            max_total_notional: config.max_total_notional,
            max_correlated_notional: config.max_correlated_notional,
            stale_reference_ms: config.stale_reference_ms,
            stale_book_ms: config.stale_book_ms,
            max_orders_per_minute: config.max_orders_per_minute,
            daily_drawdown_limit: config.daily_drawdown_limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperExposure {
    pub market_id: String,
    pub asset: Asset,
    pub notional: f64,
    pub loss_at_risk: f64,
}

impl PaperExposure {
    pub fn new(
        market_id: impl Into<String>,
        asset: Asset,
        notional: f64,
        loss_at_risk: f64,
    ) -> Self {
        Self {
            market_id: market_id.into(),
            asset,
            notional,
            loss_at_risk,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RiskContext {
    pub geoblocked: bool,
    pub additional_exposures: Vec<PaperExposure>,
    pub recent_order_timestamps_ms: Vec<i64>,
    pub daily_realized_pnl: f64,
    pub daily_unrealized_pnl: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskViolation {
    pub reason: RiskHaltReason,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskGateDecision {
    pub approved: bool,
    pub violations: Vec<RiskViolation>,
    pub risk_state: RiskState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiskEngine {
    limits: RiskLimits,
}

impl RiskEngine {
    pub fn new(limits: RiskLimits) -> Self {
        Self { limits }
    }

    pub fn from_config(config: &RiskConfig) -> Self {
        Self::new(RiskLimits::from(config))
    }

    pub fn evaluate(
        &self,
        intent: &PaperOrderIntent,
        snapshot: &DecisionSnapshot,
        context: &RiskContext,
    ) -> RiskGateDecision {
        let mut violations = Vec::new();
        let candidate = candidate_exposure(intent);

        if snapshot.market.market_id != intent.market_id || snapshot.market.asset != intent.asset {
            push_violation(
                &mut violations,
                RiskHaltReason::Unknown,
                format!(
                    "intent market/asset does not match decision snapshot: intent={} {:?}, snapshot={} {:?}",
                    intent.market_id, intent.asset, snapshot.market.market_id, snapshot.market.asset
                ),
            );
        }

        if candidate.is_none() {
            push_violation(
                &mut violations,
                RiskHaltReason::Unknown,
                "intent has invalid price, size, or notional".to_string(),
            );
        }

        if market_is_ineligible(snapshot) {
            push_violation(
                &mut violations,
                RiskHaltReason::IneligibleMarket,
                ineligible_market_message(snapshot),
            );
        }

        if self.reference_is_stale(snapshot) {
            push_violation(
                &mut violations,
                RiskHaltReason::StaleReference,
                stale_reference_message(snapshot),
            );
        }

        if self.book_is_stale(intent, snapshot) {
            push_violation(
                &mut violations,
                RiskHaltReason::StaleBook,
                stale_book_message(intent, snapshot),
            );
        }

        if context.geoblocked {
            push_violation(
                &mut violations,
                RiskHaltReason::Geoblocked,
                "geoblock check reports restricted access".to_string(),
            );
        }

        if let Some(candidate) = candidate {
            self.evaluate_exposure_limits(intent, snapshot, context, &candidate, &mut violations);
        }

        if self.order_rate_exceeded(intent, context) {
            push_violation(
                &mut violations,
                RiskHaltReason::OrderRateExceeded,
                format!(
                    "order rate would exceed {} orders per minute",
                    self.limits.max_orders_per_minute
                ),
            );
        }

        let daily_drawdown = daily_drawdown(context);
        if daily_drawdown >= self.limits.daily_drawdown_limit {
            push_violation(
                &mut violations,
                RiskHaltReason::DailyDrawdown,
                format!(
                    "daily drawdown {:.6} reached limit {:.6}",
                    daily_drawdown, self.limits.daily_drawdown_limit
                ),
            );
        }

        let risk_state = risk_state_from_violations(&violations, snapshot.snapshot_wall_ts);

        RiskGateDecision {
            approved: violations.is_empty(),
            violations,
            risk_state,
        }
    }

    fn reference_is_stale(&self, snapshot: &DecisionSnapshot) -> bool {
        snapshot.reference_freshness.is_empty()
            || snapshot.reference_freshness.iter().any(|freshness| {
                freshness.is_stale
                    || freshness
                        .age_ms
                        .map(|age_ms| age_ms > self.limits.stale_reference_ms as i64)
                        .unwrap_or(true)
            })
    }

    fn book_is_stale(&self, intent: &PaperOrderIntent, snapshot: &DecisionSnapshot) -> bool {
        snapshot
            .book_freshness
            .iter()
            .find(|freshness| {
                freshness.market_id == intent.market_id && freshness.token_id == intent.token_id
            })
            .map(|freshness| {
                freshness.is_stale
                    || freshness
                        .age_ms
                        .map(|age_ms| age_ms > self.limits.stale_book_ms as i64)
                        .unwrap_or(true)
            })
            .unwrap_or(true)
    }

    fn evaluate_exposure_limits(
        &self,
        intent: &PaperOrderIntent,
        snapshot: &DecisionSnapshot,
        context: &RiskContext,
        candidate: &PaperExposure,
        violations: &mut Vec<RiskViolation>,
    ) {
        let mut existing_market_loss = 0.0;
        let mut existing_market_notional = 0.0;
        let mut existing_asset_notional = 0.0;
        let mut existing_total_notional = 0.0;
        let mut existing_correlated_notional = 0.0;

        for position in &snapshot.positions {
            if !valid_position(position) {
                push_violation(
                    violations,
                    RiskHaltReason::Unknown,
                    format!(
                        "position has invalid size or average price for market {} token {}",
                        position.market_id, position.token_id
                    ),
                );
                continue;
            }

            let notional = position_notional(position);
            existing_total_notional += notional;
            if is_correlated_asset(position.asset) {
                existing_correlated_notional += notional;
            }
            if position.asset == intent.asset {
                existing_asset_notional += notional;
            }
            if position.market_id == intent.market_id {
                existing_market_notional += notional;
                existing_market_loss += position_loss_at_risk(position);
            }
        }

        for exposure in &context.additional_exposures {
            if !valid_exposure(exposure) {
                push_violation(
                    violations,
                    RiskHaltReason::Unknown,
                    format!(
                        "additional exposure is invalid for market {}",
                        exposure.market_id
                    ),
                );
                continue;
            }

            existing_total_notional += exposure.notional;
            if is_correlated_asset(exposure.asset) {
                existing_correlated_notional += exposure.notional;
            }
            if exposure.asset == intent.asset {
                existing_asset_notional += exposure.notional;
            }
            if exposure.market_id == intent.market_id {
                existing_market_notional += exposure.notional;
                existing_market_loss += exposure.loss_at_risk;
            }
        }

        let next_market_loss = existing_market_loss + candidate.loss_at_risk;
        if next_market_loss > self.limits.max_loss_per_market {
            push_violation(
                violations,
                RiskHaltReason::MaxLossPerMarket,
                format!(
                    "market loss at risk {:.6} would exceed limit {:.6}",
                    next_market_loss, self.limits.max_loss_per_market
                ),
            );
        }

        let next_market_notional = existing_market_notional + candidate.notional;
        if next_market_notional > self.limits.max_notional_per_market {
            push_violation(
                violations,
                RiskHaltReason::MaxNotionalPerMarket,
                format!(
                    "market notional {:.6} would exceed limit {:.6}",
                    next_market_notional, self.limits.max_notional_per_market
                ),
            );
        }

        let next_asset_notional = existing_asset_notional + candidate.notional;
        if next_asset_notional > self.limits.max_notional_per_asset {
            push_violation(
                violations,
                RiskHaltReason::MaxNotionalPerAsset,
                format!(
                    "asset notional {:.6} would exceed limit {:.6}",
                    next_asset_notional, self.limits.max_notional_per_asset
                ),
            );
        }

        let next_total_notional = existing_total_notional + candidate.notional;
        if next_total_notional > self.limits.max_total_notional {
            push_violation(
                violations,
                RiskHaltReason::MaxTotalNotional,
                format!(
                    "total notional {:.6} would exceed limit {:.6}",
                    next_total_notional, self.limits.max_total_notional
                ),
            );
        }

        let next_correlated_notional = if is_correlated_asset(candidate.asset) {
            existing_correlated_notional + candidate.notional
        } else {
            existing_correlated_notional
        };
        if next_correlated_notional > self.limits.max_correlated_notional {
            push_violation(
                violations,
                RiskHaltReason::MaxCorrelatedNotional,
                format!(
                    "correlated notional {:.6} would exceed limit {:.6}",
                    next_correlated_notional, self.limits.max_correlated_notional
                ),
            );
        }
    }

    fn order_rate_exceeded(&self, intent: &PaperOrderIntent, context: &RiskContext) -> bool {
        let window_start = intent.created_ts.saturating_sub(60_000);
        let recent_order_count = context
            .recent_order_timestamps_ms
            .iter()
            .filter(|timestamp| **timestamp >= window_start && **timestamp <= intent.created_ts)
            .count() as u64;

        recent_order_count >= self.limits.max_orders_per_minute
    }
}

fn candidate_exposure(intent: &PaperOrderIntent) -> Option<PaperExposure> {
    if !intent.price.is_finite()
        || !intent.size.is_finite()
        || !intent.notional.is_finite()
        || intent.price < 0.0
        || intent.price > 1.0
        || intent.size <= 0.0
        || intent.notional < 0.0
    {
        return None;
    }

    let derived_notional = intent.price * intent.size.abs();
    if !derived_notional.is_finite() {
        return None;
    }

    let notional = intent.notional.max(derived_notional);
    let side_loss = match intent.side {
        Side::Buy => derived_notional,
        Side::Sell => (1.0 - intent.price) * intent.size.abs(),
    };
    let loss_at_risk = notional.max(side_loss);

    Some(PaperExposure::new(
        intent.market_id.clone(),
        intent.asset,
        notional,
        loss_at_risk,
    ))
}

fn valid_position(position: &PositionSnapshot) -> bool {
    position.size.is_finite()
        && position.average_price.is_finite()
        && position.average_price >= 0.0
        && position.average_price <= 1.0
}

fn position_notional(position: &PositionSnapshot) -> f64 {
    position.size.abs() * position.average_price
}

fn position_loss_at_risk(position: &PositionSnapshot) -> f64 {
    if position.size >= 0.0 {
        position.size * position.average_price
    } else {
        position.size.abs() * (1.0 - position.average_price)
    }
}

fn valid_exposure(exposure: &PaperExposure) -> bool {
    exposure.notional.is_finite()
        && exposure.loss_at_risk.is_finite()
        && exposure.notional >= 0.0
        && exposure.loss_at_risk >= 0.0
}

fn is_correlated_asset(asset: Asset) -> bool {
    matches!(asset, Asset::Btc | Asset::Eth | Asset::Sol)
}

fn daily_drawdown(context: &RiskContext) -> f64 {
    let pnl = context.daily_realized_pnl + context.daily_unrealized_pnl;
    if pnl.is_finite() && pnl < 0.0 {
        -pnl
    } else if pnl.is_finite() {
        0.0
    } else {
        f64::INFINITY
    }
}

fn market_is_ineligible(snapshot: &DecisionSnapshot) -> bool {
    snapshot.lifecycle_state != MarketLifecycleState::Active
        || snapshot.market.lifecycle_state != MarketLifecycleState::Active
        || snapshot.market.ineligibility_reason.is_some()
        || snapshot
            .market
            .resolution_source
            .as_deref()
            .map(|source| {
                !is_asset_matched_chainlink_resolution_source(snapshot.market.asset, source)
            })
            .unwrap_or(true)
}

fn ineligible_market_message(snapshot: &DecisionSnapshot) -> String {
    if let Some(reason) = snapshot.market.ineligibility_reason.as_deref() {
        return format!(
            "market {} is ineligible: {reason}",
            snapshot.market.market_id
        );
    }
    format!(
        "market {} is not eligible for paper execution",
        snapshot.market.market_id
    )
}

fn stale_reference_message(snapshot: &DecisionSnapshot) -> String {
    if snapshot.reference_freshness.is_empty() {
        return format!(
            "reference freshness missing for asset {:?}",
            snapshot.market.asset
        );
    }

    let stale_sources = snapshot
        .reference_freshness
        .iter()
        .filter(|freshness| freshness.is_stale || freshness.age_ms.is_none())
        .map(|freshness| freshness.key.source.as_str())
        .collect::<Vec<_>>();

    if stale_sources.is_empty() {
        "reference age exceeds risk limit".to_string()
    } else {
        format!("stale reference source(s): {}", stale_sources.join(","))
    }
}

fn stale_book_message(intent: &PaperOrderIntent, snapshot: &DecisionSnapshot) -> String {
    snapshot
        .book_freshness
        .iter()
        .find(|freshness| {
            freshness.market_id == intent.market_id && freshness.token_id == intent.token_id
        })
        .map(|freshness| {
            format!(
                "stale book for market {} token {} age_ms={:?}",
                freshness.market_id, freshness.token_id, freshness.age_ms
            )
        })
        .unwrap_or_else(|| {
            format!(
                "book freshness missing for market {} token {}",
                intent.market_id, intent.token_id
            )
        })
}

fn push_violation(violations: &mut Vec<RiskViolation>, reason: RiskHaltReason, message: String) {
    if violations
        .iter()
        .any(|violation| violation.reason == reason)
    {
        return;
    }

    violations.push(RiskViolation { reason, message });
}

fn risk_state_from_violations(violations: &[RiskViolation], updated_ts: i64) -> RiskState {
    RiskState {
        halted: !violations.is_empty(),
        active_halts: violations
            .iter()
            .map(|violation| violation.reason.clone())
            .collect(),
        reason: if violations.is_empty() {
            None
        } else {
            Some(
                violations
                    .iter()
                    .map(|violation| violation.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        },
        updated_ts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FeeParameters, Market, MarketLifecycleState, OrderKind, OutcomeToken, ReferencePrice,
    };
    use crate::state::{
        AssetPriceKey, BookFreshness, BookSideSnapshot, PriceLevelSnapshot, ReferenceFreshness,
        TokenBookSnapshot,
    };

    const NOW: i64 = 1_777_000_000_000;

    #[test]
    fn approves_when_all_gates_pass() {
        let decision = evaluate(limits(), sample_snapshot(), RiskContext::default());

        assert!(decision.approved);
        assert!(decision.violations.is_empty());
        assert_eq!(
            decision.risk_state,
            RiskState {
                halted: false,
                active_halts: Vec::new(),
                reason: None,
                updated_ts: NOW,
            }
        );
    }

    #[test]
    fn stale_reference_halts() {
        let mut snapshot = sample_snapshot();
        snapshot.reference_freshness[0].age_ms = Some(1_001);
        snapshot.reference_freshness[0].is_stale = true;

        assert_halt(
            evaluate(limits(), snapshot, RiskContext::default()),
            RiskHaltReason::StaleReference,
        );
    }

    #[test]
    fn stale_book_halts() {
        let mut snapshot = sample_snapshot();
        snapshot.book_freshness[0].age_ms = Some(1_001);
        snapshot.book_freshness[0].is_stale = true;

        assert_halt(
            evaluate(limits(), snapshot, RiskContext::default()),
            RiskHaltReason::StaleBook,
        );
    }

    #[test]
    fn geoblock_halts() {
        let context = RiskContext {
            geoblocked: true,
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits(), sample_snapshot(), context),
            RiskHaltReason::Geoblocked,
        );
    }

    #[test]
    fn ineligible_market_halts() {
        let mut snapshot = sample_snapshot();
        snapshot.market.lifecycle_state = MarketLifecycleState::Ineligible;
        snapshot.lifecycle_state = MarketLifecycleState::Ineligible;
        snapshot.market.ineligibility_reason = Some("ambiguous resolution rules".to_string());

        assert_halt(
            evaluate(limits(), snapshot, RiskContext::default()),
            RiskHaltReason::IneligibleMarket,
        );
    }

    #[test]
    fn asset_mismatched_resolution_source_halts() {
        let mut snapshot = sample_snapshot();
        snapshot.market.resolution_source =
            Some(Asset::Eth.chainlink_resolution_source().to_string());

        assert_halt(
            evaluate(limits(), snapshot, RiskContext::default()),
            RiskHaltReason::IneligibleMarket,
        );
    }

    #[test]
    fn max_loss_per_market_halts() {
        let mut snapshot = sample_snapshot();
        snapshot.positions = vec![position("market-1", "token-down", Asset::Btc, 30.0, 0.70)];

        assert_halt(
            evaluate(limits(), snapshot, RiskContext::default()),
            RiskHaltReason::MaxLossPerMarket,
        );
    }

    #[test]
    fn max_notional_per_market_halts() {
        let mut limits = relaxed_limits();
        limits.max_notional_per_market = 100.0;
        let context = RiskContext {
            additional_exposures: vec![PaperExposure::new("market-1", Asset::Btc, 98.0, 0.0)],
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::MaxNotionalPerMarket,
        );
    }

    #[test]
    fn max_notional_per_asset_halts() {
        let mut limits = relaxed_limits();
        limits.max_notional_per_asset = 250.0;
        let context = RiskContext {
            additional_exposures: vec![PaperExposure::new("market-2", Asset::Btc, 248.0, 0.0)],
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::MaxNotionalPerAsset,
        );
    }

    #[test]
    fn max_total_notional_halts() {
        let mut limits = relaxed_limits();
        limits.max_total_notional = 500.0;
        let context = RiskContext {
            additional_exposures: vec![
                PaperExposure::new("market-2", Asset::Eth, 250.0, 0.0),
                PaperExposure::new("market-3", Asset::Sol, 248.0, 0.0),
            ],
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::MaxTotalNotional,
        );
    }

    #[test]
    fn correlated_exposure_halts() {
        let mut limits = relaxed_limits();
        limits.max_correlated_notional = 350.0;
        let context = RiskContext {
            additional_exposures: vec![
                PaperExposure::new("market-2", Asset::Eth, 175.0, 0.0),
                PaperExposure::new("market-3", Asset::Sol, 173.0, 0.0),
            ],
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::MaxCorrelatedNotional,
        );
    }

    #[test]
    fn order_rate_halts() {
        let mut limits = relaxed_limits();
        limits.max_orders_per_minute = 2;
        let context = RiskContext {
            recent_order_timestamps_ms: vec![NOW - 20_000, NOW - 10_000],
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::OrderRateExceeded,
        );
    }

    #[test]
    fn daily_drawdown_halts() {
        let mut limits = relaxed_limits();
        limits.daily_drawdown_limit = 100.0;
        let context = RiskContext {
            daily_realized_pnl: -101.0,
            ..RiskContext::default()
        };

        assert_halt(
            evaluate(limits, sample_snapshot(), context),
            RiskHaltReason::DailyDrawdown,
        );
    }

    #[test]
    fn collects_multiple_halt_reasons_without_early_return() {
        let mut snapshot = sample_snapshot();
        snapshot.reference_freshness[0].is_stale = true;
        let context = RiskContext {
            geoblocked: true,
            ..RiskContext::default()
        };

        let decision = evaluate(limits(), snapshot, context);

        assert!(!decision.approved);
        assert!(decision
            .risk_state
            .active_halts
            .contains(&RiskHaltReason::StaleReference));
        assert!(decision
            .risk_state
            .active_halts
            .contains(&RiskHaltReason::Geoblocked));
    }

    fn evaluate(
        limits: RiskLimits,
        snapshot: DecisionSnapshot,
        context: RiskContext,
    ) -> RiskGateDecision {
        RiskEngine::new(limits).evaluate(&sample_intent(), &snapshot, &context)
    }

    fn assert_halt(decision: RiskGateDecision, reason: RiskHaltReason) {
        assert!(!decision.approved);
        assert!(decision
            .violations
            .iter()
            .any(|violation| violation.reason == reason));
        assert!(decision.risk_state.halted);
        assert!(decision.risk_state.active_halts.contains(&reason));
        assert!(decision.risk_state.reason.is_some());
    }

    fn limits() -> RiskLimits {
        RiskLimits {
            max_loss_per_market: 25.0,
            max_notional_per_market: 100.0,
            max_notional_per_asset: 250.0,
            max_total_notional: 500.0,
            max_correlated_notional: 350.0,
            stale_reference_ms: 1_000,
            stale_book_ms: 1_000,
            max_orders_per_minute: 120,
            daily_drawdown_limit: 100.0,
        }
    }

    fn relaxed_limits() -> RiskLimits {
        RiskLimits {
            max_loss_per_market: 1_000.0,
            max_notional_per_market: 1_000.0,
            max_notional_per_asset: 1_000.0,
            max_total_notional: 1_000.0,
            max_correlated_notional: 1_000.0,
            stale_reference_ms: 1_000,
            stale_book_ms: 1_000,
            max_orders_per_minute: 120,
            daily_drawdown_limit: 1_000.0,
        }
    }

    fn sample_intent() -> PaperOrderIntent {
        PaperOrderIntent {
            asset: Asset::Btc,
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            outcome: "Up".to_string(),
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            price: 0.50,
            size: 10.0,
            notional: 5.0,
            fair_probability: 0.54,
            market_probability: 0.50,
            expected_value_bps: 100.0,
            reason: "unit signal".to_string(),
            required_inputs: vec!["reference".to_string(), "book".to_string()],
            created_ts: NOW,
        }
    }

    fn sample_snapshot() -> DecisionSnapshot {
        DecisionSnapshot {
            market: sample_market(),
            lifecycle_state: MarketLifecycleState::Active,
            token_books: vec![token_book("token-up")],
            book_freshness: vec![BookFreshness {
                market_id: "market-1".to_string(),
                token_id: "token-up".to_string(),
                last_recv_wall_ts: Some(NOW - 100),
                age_ms: Some(100),
                stale_after_ms: 1_000,
                is_stale: false,
            }],
            reference_prices: vec![ReferencePrice {
                asset: Asset::Btc,
                source: resolution_source(),
                price: 65_000.0,
                source_ts: Some(NOW - 110),
                recv_wall_ts: NOW - 100,
            }],
            predictive_prices: Vec::new(),
            positions: Vec::new(),
            reference_freshness: vec![ReferenceFreshness {
                key: AssetPriceKey::new(Asset::Btc, resolution_source()),
                last_recv_wall_ts: Some(NOW - 100),
                age_ms: Some(100),
                stale_after_ms: 1_000,
                is_stale: false,
            }],
            snapshot_wall_ts: NOW,
        }
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
            start_ts: NOW - 60_000,
            end_ts: NOW + 840_000,
            resolution_source: Some(resolution_source()),
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

    fn token_book(token_id: &str) -> TokenBookSnapshot {
        TokenBookSnapshot {
            market_id: "market-1".to_string(),
            token_id: token_id.to_string(),
            bids: BookSideSnapshot {
                levels: vec![PriceLevelSnapshot {
                    price: 0.49,
                    size: 100.0,
                }],
                visible_depth: 100.0,
            },
            asks: BookSideSnapshot {
                levels: vec![PriceLevelSnapshot {
                    price: 0.51,
                    size: 100.0,
                }],
                visible_depth: 100.0,
            },
            best_bid: Some(0.49),
            best_ask: Some(0.51),
            spread: Some(0.02),
            last_update_ts: Some(NOW - 100),
            last_recv_wall_ts: Some(NOW - 100),
            hash: Some("book-hash".to_string()),
            last_trade: None,
        }
    }

    fn position(
        market_id: &str,
        token_id: &str,
        asset: Asset,
        size: f64,
        average_price: f64,
    ) -> PositionSnapshot {
        PositionSnapshot {
            market_id: market_id.to_string(),
            token_id: token_id.to_string(),
            asset,
            size,
            average_price,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            updated_ts: NOW - 100,
        }
    }

    fn resolution_source() -> String {
        Asset::Btc.chainlink_resolution_source().to_string()
    }
}
