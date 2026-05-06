use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::{FeeParameters, OrderKind, Side};
use crate::execution_intent::ExecutionIntent;
use crate::live_alpha_config::LiveAlphaMakerConfig;
use crate::live_maker_micro::{build_live_maker_order_plan, LiveMakerOrderPlan};
use crate::live_order_journal::{LiveJournalEvent, LiveJournalEventType};
use crate::live_risk_engine::LiveRiskApproved;
use crate::paper_executor::fee_paid;

pub const MODULE: &str = "live_executor";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Disabled,
    Paper,
    ShadowLive,
    LiveMaker,
    LiveTaker,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Paper => "paper",
            Self::ShadowLive => "shadow_live",
            Self::LiveMaker => "live_maker",
            Self::LiveTaker => "live_taker",
        }
    }
}

pub trait ExecutionSink {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision;
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "decision_kind", rename_all = "snake_case")]
pub enum ExecutionDecision {
    Disabled {
        intent_id: String,
        reason_codes: Vec<String>,
    },
    Paper {
        intent_id: String,
    },
    ShadowLive(Box<ShadowLiveDecision>),
    LiveMaker(Box<LiveMakerDecision>),
    InertLive {
        mode: ExecutionMode,
        intent_id: String,
        reason_codes: Vec<String>,
    },
}

#[derive(Debug, Default)]
pub struct DisabledExecution;

impl ExecutionSink for DisabledExecution {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision {
        ExecutionDecision::Disabled {
            intent_id: intent.intent_id,
            reason_codes: vec![ShadowLiveReasonCode::ModeNotApproved.as_str().to_string()],
        }
    }
}

#[derive(Debug, Default)]
pub struct PaperExecution;

impl ExecutionSink for PaperExecution {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision {
        ExecutionDecision::Paper {
            intent_id: intent.intent_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveMakerDecision {
    pub intent_id: String,
    pub would_submit: bool,
    pub not_submitted: bool,
    pub reason_codes: Vec<String>,
    pub order_plan: Option<LiveMakerOrderPlan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveMakerExecutionContext {
    pub risk_approval: LiveRiskApproved,
    pub maker_config: LiveAlphaMakerConfig,
    pub now_unix: u64,
    pub human_approved: bool,
}

#[derive(Debug, Default)]
pub struct LiveMakerExecution {
    context: Option<LiveMakerExecutionContext>,
}

impl LiveMakerExecution {
    pub fn new(context: LiveMakerExecutionContext) -> Self {
        Self {
            context: Some(context),
        }
    }
}

impl ExecutionSink for LiveMakerExecution {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision {
        let Some(context) = &self.context else {
            return inert_live_decision(ExecutionMode::LiveMaker, intent.intent_id);
        };
        let intent_id = intent.intent_id.clone();
        let mut reason_codes = Vec::new();
        if !context.human_approved {
            reason_codes.push("human_approval_missing".to_string());
        }
        match build_live_maker_order_plan(
            &intent,
            &context.risk_approval,
            &context.maker_config,
            context.now_unix,
        ) {
            Ok(plan) => ExecutionDecision::LiveMaker(Box::new(LiveMakerDecision {
                intent_id,
                would_submit: context.human_approved,
                not_submitted: !context.human_approved,
                reason_codes,
                order_plan: Some(plan),
            })),
            Err(error) => {
                reason_codes.push("maker_order_plan_invalid".to_string());
                reason_codes.push(error.to_string());
                ExecutionDecision::LiveMaker(Box::new(LiveMakerDecision {
                    intent_id,
                    would_submit: false,
                    not_submitted: true,
                    reason_codes,
                    order_plan: None,
                }))
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct LiveTakerExecution;

impl ExecutionSink for LiveTakerExecution {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision {
        inert_live_decision(ExecutionMode::LiveTaker, intent.intent_id)
    }
}

fn inert_live_decision(mode: ExecutionMode, intent_id: String) -> ExecutionDecision {
    ExecutionDecision::InertLive {
        mode,
        intent_id,
        reason_codes: vec![ShadowLiveReasonCode::ModeNotApproved.as_str().to_string()],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowLiveContext {
    pub mode_approved: bool,
    pub risk_approved: bool,
    pub risk_reason_codes: Vec<ShadowLiveReasonCode>,
    pub geoblock_passed: bool,
    pub heartbeat_healthy: bool,
    pub reconciliation_clean: bool,
    pub book_fresh: bool,
    pub reference_fresh: bool,
    pub now_ms: Option<i64>,
    pub market_end_ms: Option<i64>,
    pub no_trade_seconds_before_close: u64,
    pub available_pusd: f64,
    pub reserved_pusd: f64,
    pub max_available_pusd_usage: f64,
    pub max_reserved_pusd: f64,
    pub inventory_by_token: BTreeMap<String, f64>,
    pub open_order_count: u64,
    pub max_open_orders: u64,
    pub current_market_notional: f64,
    pub max_market_notional: f64,
    pub current_asset_notional: f64,
    pub max_asset_notional: f64,
    pub current_total_live_notional: f64,
    pub max_single_order_notional: f64,
    pub max_total_live_notional: f64,
    pub min_edge_bps: f64,
    pub fee_parameters: FeeParameters,
}

impl Default for ShadowLiveContext {
    fn default() -> Self {
        Self {
            mode_approved: false,
            risk_approved: false,
            risk_reason_codes: Vec::new(),
            geoblock_passed: false,
            heartbeat_healthy: false,
            reconciliation_clean: false,
            book_fresh: false,
            reference_fresh: false,
            now_ms: None,
            market_end_ms: None,
            no_trade_seconds_before_close: 0,
            available_pusd: 0.0,
            reserved_pusd: 0.0,
            max_available_pusd_usage: 0.0,
            max_reserved_pusd: 0.0,
            inventory_by_token: BTreeMap::new(),
            open_order_count: 0,
            max_open_orders: 0,
            current_market_notional: 0.0,
            max_market_notional: 0.0,
            current_asset_notional: 0.0,
            max_asset_notional: 0.0,
            current_total_live_notional: 0.0,
            max_single_order_notional: 0.0,
            max_total_live_notional: 0.0,
            min_edge_bps: 0.0,
            fee_parameters: FeeParameters {
                fees_enabled: false,
                maker_fee_bps: 0.0,
                taker_fee_bps: 0.0,
                raw_fee_config: None,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowLiveReasonCode {
    EdgeTooSmall,
    BookStale,
    ReferenceStale,
    MarketTooCloseToClose,
    PostOnlyWouldCross,
    InsufficientPusd,
    InsufficientInventoryForSell,
    AvailablePusdUsageExceeded,
    ReservedPusdExceeded,
    MaxSingleOrderNotionalReached,
    MaxOpenOrdersReached,
    MaxMarketLossReached,
    MaxMarketNotionalReached,
    MaxAssetNotionalReached,
    MaxTotalLiveNotionalReached,
    MaxCorrelatedNotionalReached,
    HeartbeatNotHealthy,
    ReconciliationNotClean,
    GeoblockNotPassed,
    ModeNotApproved,
    IntentInvalid,
    LiveRiskRejected,
}

impl ShadowLiveReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EdgeTooSmall => "edge_too_small",
            Self::BookStale => "book_stale",
            Self::ReferenceStale => "reference_stale",
            Self::MarketTooCloseToClose => "market_too_close_to_close",
            Self::PostOnlyWouldCross => "post_only_would_cross",
            Self::InsufficientPusd => "insufficient_pusd",
            Self::InsufficientInventoryForSell => "insufficient_inventory_for_sell",
            Self::AvailablePusdUsageExceeded => "available_pusd_usage_exceeded",
            Self::ReservedPusdExceeded => "reserved_pusd_exceeded",
            Self::MaxSingleOrderNotionalReached => "max_single_order_notional_reached",
            Self::MaxOpenOrdersReached => "max_open_orders_reached",
            Self::MaxMarketLossReached => "max_market_loss_reached",
            Self::MaxMarketNotionalReached => "max_market_notional_reached",
            Self::MaxAssetNotionalReached => "max_asset_notional_reached",
            Self::MaxTotalLiveNotionalReached => "max_total_live_notional_reached",
            Self::MaxCorrelatedNotionalReached => "max_correlated_notional_reached",
            Self::HeartbeatNotHealthy => "heartbeat_not_healthy",
            Self::ReconciliationNotClean => "reconciliation_not_clean",
            Self::GeoblockNotPassed => "geoblock_not_passed",
            Self::ModeNotApproved => "mode_not_approved",
            Self::IntentInvalid => "intent_invalid",
            Self::LiveRiskRejected => "live_risk_rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ShadowLiveDecision {
    pub shadow_decision_id: String,
    pub shadow_intent_id: String,
    pub intent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy_snapshot_id: Option<String>,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub side: Side,
    pub would_submit: bool,
    pub would_cancel: bool,
    pub would_replace: bool,
    pub live_eligible: bool,
    pub risk_eligible: bool,
    pub post_only_safe: bool,
    pub inventory_valid: bool,
    pub balance_valid: bool,
    pub book_fresh: bool,
    pub reference_fresh: bool,
    pub market_time_valid: bool,
    pub reason_codes: Vec<String>,
    pub expected_order_type: String,
    pub expected_price: f64,
    pub expected_size: f64,
    pub expected_notional: f64,
    pub expected_edge_bps: f64,
    pub expected_edge: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_fee: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_ttl: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_bid: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_ask: Option<f64>,
    pub geoblock_passed: bool,
    pub heartbeat_healthy: bool,
    pub reconciliation_clean: bool,
    pub available_pusd: f64,
    pub reserved_pusd: f64,
    pub open_order_count: u64,
}

impl ShadowLiveDecision {
    pub fn to_journal_event(
        &self,
        run_id: impl Into<String>,
        event_id: impl Into<String>,
        created_at: i64,
    ) -> LiveJournalEvent {
        LiveJournalEvent::new(
            run_id,
            event_id,
            LiveJournalEventType::LiveShadowDecisionRecorded,
            created_at,
            serde_json::to_value(self).expect("shadow decision journal payload serializes"),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct ShadowLiveReport {
    pub decision_count: u64,
    pub paper_order_count: u64,
    pub paper_fill_count: u64,
    pub shadow_would_submit_count: u64,
    pub shadow_would_cancel_count: u64,
    pub shadow_would_replace_count: u64,
    pub shadow_rejected_count: u64,
    pub shadow_rejected_count_by_reason: BTreeMap<String, u64>,
    pub paper_live_intent_divergence_count: u64,
    pub estimated_fee_exposure: f64,
    pub estimated_reserved_pusd_exposure: f64,
}

impl ShadowLiveReport {
    pub fn from_decisions(
        decisions: &[ShadowLiveDecision],
        paper_order_count: u64,
        paper_fill_count: u64,
    ) -> Self {
        let mut report = Self {
            decision_count: decisions.len() as u64,
            paper_order_count,
            paper_fill_count,
            ..Self::default()
        };

        for decision in decisions {
            if decision.would_submit {
                report.shadow_would_submit_count += 1;
                report.estimated_fee_exposure += decision.expected_fee.unwrap_or_default();
                if decision.side == Side::Buy {
                    report.estimated_reserved_pusd_exposure +=
                        decision.expected_notional + decision.expected_fee.unwrap_or_default();
                }
            }
            if decision.would_cancel {
                report.shadow_would_cancel_count += 1;
            }
            if decision.would_replace {
                report.shadow_would_replace_count += 1;
            }
            if !decision.reason_codes.is_empty() {
                report.shadow_rejected_count += 1;
                for reason in &decision.reason_codes {
                    *report
                        .shadow_rejected_count_by_reason
                        .entry(reason.clone())
                        .or_insert(0) += 1;
                }
            }
        }

        report.paper_live_intent_divergence_count =
            paper_order_count.abs_diff(report.shadow_would_submit_count);
        report
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowLiveExecution {
    context: ShadowLiveContext,
    next_decision_seq: u64,
}

impl ShadowLiveExecution {
    pub fn new(context: ShadowLiveContext) -> Self {
        Self {
            context,
            next_decision_seq: 1,
        }
    }

    pub fn set_context(&mut self, context: ShadowLiveContext) {
        self.context = context;
    }

    pub fn decisions_seen(&self) -> u64 {
        self.next_decision_seq.saturating_sub(1)
    }

    fn next_decision_id(&mut self, intent_id: &str) -> (String, String) {
        let sequence = self.next_decision_seq;
        self.next_decision_seq += 1;
        (
            format!("shadow-decision-{sequence}"),
            format!("shadow-intent-{sequence}-{intent_id}"),
        )
    }
}

impl ExecutionSink for ShadowLiveExecution {
    fn handle_intent(&mut self, intent: ExecutionIntent) -> ExecutionDecision {
        let (shadow_decision_id, shadow_intent_id) = self.next_decision_id(&intent.intent_id);
        let context = self.context.clone();
        let mut reasons = Vec::<ShadowLiveReasonCode>::new();

        if intent.validate_shape().is_err() {
            reasons.push(ShadowLiveReasonCode::IntentInvalid);
        }
        if !context.mode_approved || !shadow_supports_intent(&intent) {
            reasons.push(ShadowLiveReasonCode::ModeNotApproved);
        }
        if !context.risk_approved {
            if context.risk_reason_codes.is_empty() {
                reasons.push(ShadowLiveReasonCode::LiveRiskRejected);
            } else {
                reasons.extend(context.risk_reason_codes.iter().copied());
            }
        }
        if intent.edge_bps < context.min_edge_bps {
            reasons.push(ShadowLiveReasonCode::EdgeTooSmall);
        }
        if !context.book_fresh {
            reasons.push(ShadowLiveReasonCode::BookStale);
        }
        if !context.reference_fresh {
            reasons.push(ShadowLiveReasonCode::ReferenceStale);
        }
        let market_time_valid = market_time_valid(&context);
        if !market_time_valid {
            reasons.push(ShadowLiveReasonCode::MarketTooCloseToClose);
        }
        let post_only_safe = post_only_safe(&intent);
        if shadow_supports_intent(&intent) && !post_only_safe {
            reasons.push(ShadowLiveReasonCode::PostOnlyWouldCross);
        }
        let expected_fee = expected_fee(&intent, &context);
        let collateral_required = intent.side == Side::Buy;
        let expected_reserved = if collateral_required {
            intent.notional + expected_fee.unwrap_or_default()
        } else {
            0.0
        };
        let available_sufficient = !collateral_required
            || (context.available_pusd.is_finite() && context.available_pusd >= expected_reserved);
        let available_usage_valid = !collateral_required
            || (context.max_available_pusd_usage > 0.0
                && expected_reserved <= context.max_available_pusd_usage);
        let reserved_valid = !collateral_required
            || (context.reserved_pusd.is_finite()
                && context.max_reserved_pusd >= 0.0
                && context.reserved_pusd + expected_reserved <= context.max_reserved_pusd);
        let single_order_notional_valid = context.max_single_order_notional > 0.0
            && intent.notional <= context.max_single_order_notional;
        let total_live_notional_valid = context.max_total_live_notional > 0.0
            && context.current_total_live_notional + intent.notional
                <= context.max_total_live_notional;
        let balance_valid = available_sufficient
            && available_usage_valid
            && reserved_valid
            && single_order_notional_valid
            && total_live_notional_valid;
        if !available_sufficient {
            reasons.push(ShadowLiveReasonCode::InsufficientPusd);
        }
        if !available_usage_valid {
            reasons.push(ShadowLiveReasonCode::AvailablePusdUsageExceeded);
        }
        if !reserved_valid {
            reasons.push(ShadowLiveReasonCode::ReservedPusdExceeded);
        }
        if !single_order_notional_valid {
            reasons.push(ShadowLiveReasonCode::MaxSingleOrderNotionalReached);
        }
        if !total_live_notional_valid {
            reasons.push(ShadowLiveReasonCode::MaxTotalLiveNotionalReached);
        }
        let inventory_valid = inventory_valid(&intent, &context);
        if !inventory_valid {
            reasons.push(ShadowLiveReasonCode::InsufficientInventoryForSell);
        }
        if context.open_order_count >= context.max_open_orders {
            reasons.push(ShadowLiveReasonCode::MaxOpenOrdersReached);
        }
        if context.max_market_notional <= 0.0
            || context.current_market_notional + intent.notional > context.max_market_notional
        {
            reasons.push(ShadowLiveReasonCode::MaxMarketNotionalReached);
        }
        if context.max_asset_notional <= 0.0
            || context.current_asset_notional + intent.notional > context.max_asset_notional
        {
            reasons.push(ShadowLiveReasonCode::MaxAssetNotionalReached);
        }
        if !context.heartbeat_healthy {
            reasons.push(ShadowLiveReasonCode::HeartbeatNotHealthy);
        }
        if !context.reconciliation_clean {
            reasons.push(ShadowLiveReasonCode::ReconciliationNotClean);
        }
        if !context.geoblock_passed {
            reasons.push(ShadowLiveReasonCode::GeoblockNotPassed);
        }

        let reason_codes = reason_code_strings(reasons);
        let live_eligible = context.mode_approved
            && context.geoblock_passed
            && context.heartbeat_healthy
            && context.reconciliation_clean;
        let would_submit = live_eligible
            && context.risk_approved
            && post_only_safe
            && inventory_valid
            && balance_valid
            && context.book_fresh
            && context.reference_fresh
            && market_time_valid
            && reason_codes.is_empty();

        ExecutionDecision::ShadowLive(Box::new(ShadowLiveDecision {
            shadow_decision_id,
            shadow_intent_id,
            strategy_snapshot_id: non_empty_string(intent.strategy_snapshot_id.clone()),
            intent_id: intent.intent_id,
            market_slug: intent.market_slug,
            condition_id: intent.condition_id,
            token_id: intent.token_id,
            side: intent.side,
            would_submit,
            would_cancel: false,
            would_replace: false,
            live_eligible,
            risk_eligible: context.risk_approved,
            post_only_safe,
            inventory_valid,
            balance_valid,
            book_fresh: context.book_fresh,
            reference_fresh: context.reference_fresh,
            market_time_valid,
            reason_codes,
            expected_order_type: intent.order_type,
            expected_price: intent.price,
            expected_size: intent.size,
            expected_notional: intent.notional,
            expected_edge_bps: intent.edge_bps,
            expected_edge: intent.fair_probability - intent.price,
            expected_fee,
            expected_ttl: expected_ttl(&context),
            book_snapshot_id: non_empty_string(intent.book_snapshot_id),
            best_bid: intent.best_bid,
            best_ask: intent.best_ask,
            geoblock_passed: context.geoblock_passed,
            heartbeat_healthy: context.heartbeat_healthy,
            reconciliation_clean: context.reconciliation_clean,
            available_pusd: context.available_pusd,
            reserved_pusd: context.reserved_pusd,
            open_order_count: context.open_order_count,
        }))
    }
}

fn shadow_supports_intent(intent: &ExecutionIntent) -> bool {
    intent.post_only
        && matches!(
            intent.order_type.trim().to_ascii_uppercase().as_str(),
            "GTC" | "GTD"
        )
}

fn post_only_safe(intent: &ExecutionIntent) -> bool {
    if !shadow_supports_intent(intent) {
        return false;
    }

    match intent.side {
        Side::Buy => intent
            .best_ask
            .map(|ask| intent.price < ask)
            .unwrap_or(false),
        Side::Sell => intent
            .best_bid
            .map(|bid| intent.price > bid)
            .unwrap_or(false),
    }
}

fn inventory_valid(intent: &ExecutionIntent, context: &ShadowLiveContext) -> bool {
    match intent.side {
        Side::Buy => true,
        Side::Sell => {
            context
                .inventory_by_token
                .get(&intent.token_id)
                .copied()
                .unwrap_or_default()
                >= intent.size
        }
    }
}

fn market_time_valid(context: &ShadowLiveContext) -> bool {
    let (Some(now_ms), Some(end_ms)) = (context.now_ms, context.market_end_ms) else {
        return false;
    };
    let cutoff_ms = (context.no_trade_seconds_before_close as i64).saturating_mul(1_000);
    now_ms.saturating_add(cutoff_ms) < end_ms
}

fn expected_ttl(context: &ShadowLiveContext) -> Option<i64> {
    let (Some(now_ms), Some(end_ms)) = (context.now_ms, context.market_end_ms) else {
        return None;
    };
    Some(end_ms.saturating_sub(now_ms).max(0))
}

fn expected_fee(intent: &ExecutionIntent, context: &ShadowLiveContext) -> Option<f64> {
    let liquidity = if intent.post_only {
        OrderKind::Maker
    } else {
        OrderKind::Taker
    };
    Some(fee_paid(
        intent.size,
        intent.price,
        liquidity,
        &context.fee_parameters,
    ))
}

fn reason_code_strings(mut reasons: Vec<ShadowLiveReasonCode>) -> Vec<String> {
    reasons.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    reasons.dedup();
    reasons
        .into_iter()
        .map(|reason| reason.as_str().to_string())
        .collect()
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Asset;

    #[test]
    fn live_executor_shadow_live_would_submit_when_all_checks_pass() {
        let mut executor = ShadowLiveExecution::new(approved_context());

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert!(decision.would_submit);
        assert!(!decision.would_cancel);
        assert!(!decision.would_replace);
        assert!(decision.live_eligible);
        assert!(decision.risk_eligible);
        assert!(decision.post_only_safe);
        assert!(decision.inventory_valid);
        assert!(decision.balance_valid);
        assert!(decision.book_fresh);
        assert!(decision.reference_fresh);
        assert!(decision.market_time_valid);
        assert!(decision.reason_codes.is_empty());
        assert_eq!(decision.expected_order_type, "GTD");
        assert_eq!(decision.expected_fee, Some(0.0));
        assert_eq!(executor.decisions_seen(), 1);
    }

    #[test]
    fn shadow_live_rejects_stale_book_and_reference() {
        let mut context = approved_context();
        context.book_fresh = false;
        context.reference_fresh = false;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "book_stale");
        assert_rejects(&decision, "reference_stale");
        assert!(!decision.would_submit);
    }

    #[test]
    fn shadow_live_rejects_market_too_close_to_close() {
        let mut context = approved_context();
        context.now_ms = Some(1_777_000_850_000);
        context.market_end_ms = Some(1_777_000_900_000);
        context.no_trade_seconds_before_close = 60;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "market_too_close_to_close");
        assert!(!decision.market_time_valid);
    }

    #[test]
    fn shadow_live_rejects_post_only_crossing() {
        let mut intent = sample_intent();
        intent.price = 0.44;
        intent.notional = intent.price * intent.size;
        intent.best_ask = Some(0.44);
        let mut executor = ShadowLiveExecution::new(approved_context());

        let decision = shadow_decision(executor.handle_intent(intent));

        assert_rejects(&decision, "post_only_would_cross");
        assert!(!decision.post_only_safe);
    }

    #[test]
    fn shadow_live_rejects_insufficient_pusd() {
        let mut context = approved_context();
        context.available_pusd = 1.0;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "insufficient_pusd");
        assert!(!decision.balance_valid);
    }

    #[test]
    fn shadow_live_rejects_insufficient_inventory_for_sell() {
        let mut intent = sample_intent();
        intent.side = Side::Sell;
        intent.price = 0.46;
        intent.notional = intent.price * intent.size;
        intent.best_bid = Some(0.45);
        let mut executor = ShadowLiveExecution::new(approved_context());

        let decision = shadow_decision(executor.handle_intent(intent));

        assert_rejects(&decision, "insufficient_inventory_for_sell");
        assert!(!decision.inventory_valid);
    }

    #[test]
    fn shadow_live_sell_intent_does_not_require_new_pusd_collateral() {
        let mut intent = sample_intent();
        intent.side = Side::Sell;
        intent.price = 0.46;
        intent.notional = intent.price * intent.size;
        intent.best_bid = Some(0.45);
        let mut context = approved_context();
        context.available_pusd = 0.0;
        context.max_available_pusd_usage = 0.0;
        context.max_reserved_pusd = 0.0;
        context
            .inventory_by_token
            .insert(intent.token_id.clone(), intent.size);
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(intent));

        assert!(decision.would_submit);
        assert!(decision.balance_valid);
        assert!(decision.inventory_valid);
        assert!(!decision
            .reason_codes
            .iter()
            .any(|reason| reason == "insufficient_pusd"));
        assert!(!decision
            .reason_codes
            .iter()
            .any(|reason| reason == "reserved_pusd_exceeded"));
        let report = ShadowLiveReport::from_decisions(&[decision], 1, 0);
        assert_eq!(report.estimated_reserved_pusd_exposure, 0.0);
    }

    #[test]
    fn shadow_live_rejects_open_order_and_notional_limits() {
        let mut context = approved_context();
        context.open_order_count = 2;
        context.max_open_orders = 2;
        context.current_market_notional = 99.0;
        context.max_market_notional = 100.0;
        context.current_asset_notional = 199.0;
        context.max_asset_notional = 200.0;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "max_open_orders_reached");
        assert_rejects(&decision, "max_market_notional_reached");
        assert_rejects(&decision, "max_asset_notional_reached");
    }

    #[test]
    fn shadow_live_rejects_heartbeat_reconciliation_geoblock_and_mode() {
        let mut context = approved_context();
        context.mode_approved = false;
        context.heartbeat_healthy = false;
        context.reconciliation_clean = false;
        context.geoblock_passed = false;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "mode_not_approved");
        assert_rejects(&decision, "heartbeat_not_healthy");
        assert_rejects(&decision, "reconciliation_not_clean");
        assert_rejects(&decision, "geoblock_not_passed");
        assert!(!decision.live_eligible);
    }

    #[test]
    fn shadow_live_rejects_edge_too_small() {
        let mut context = approved_context();
        context.min_edge_bps = 1_000.0;
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert_rejects(&decision, "edge_too_small");
    }

    #[test]
    fn live_risk_engine_rejection_maps_to_shadow_reason_codes() {
        let mut context = approved_context();
        context.risk_approved = false;
        context.risk_reason_codes = vec![
            ShadowLiveReasonCode::BookStale,
            ShadowLiveReasonCode::MaxMarketNotionalReached,
        ];
        let mut executor = ShadowLiveExecution::new(context);

        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        assert!(!decision.risk_eligible);
        assert_rejects(&decision, "book_stale");
        assert_rejects(&decision, "max_market_notional_reached");
    }

    #[test]
    fn shadow_live_rejects_taker_or_non_post_only_as_mode_not_approved() {
        let mut intent = sample_intent();
        intent.post_only = false;
        intent.order_type = "unsupported_taker".to_string();
        let mut executor = ShadowLiveExecution::new(approved_context());

        let decision = shadow_decision(executor.handle_intent(intent));

        assert_rejects(&decision, "mode_not_approved");
        assert!(!decision.would_submit);
    }

    #[test]
    fn live_executor_la4_never_emits_cancel_replace_or_transport_actions() {
        let intent = sample_intent();
        assert!(!intent.is_live_order_request());

        let mut shadow = ShadowLiveExecution::new(approved_context());
        let decision = shadow_decision(shadow.handle_intent(intent.clone()));
        assert!(decision.would_submit);
        assert!(!decision.would_cancel);
        assert!(!decision.would_replace);

        let mut maker = LiveMakerExecution::default();
        let maker_decision = maker.handle_intent(intent.clone());
        assert!(matches!(
            maker_decision,
            ExecutionDecision::InertLive {
                mode: ExecutionMode::LiveMaker,
                ..
            }
        ));

        let mut taker = LiveTakerExecution;
        let taker_decision = taker.handle_intent(intent);
        assert!(matches!(
            taker_decision,
            ExecutionDecision::InertLive {
                mode: ExecutionMode::LiveTaker,
                ..
            }
        ));
    }

    #[test]
    fn live_executor_maker_builds_post_only_gtd_plan_after_risk_approval() {
        let intent = sample_intent();
        let approval = LiveRiskApproved {
            intent_id: intent.intent_id.clone(),
            approved_token_id: intent.token_id.clone(),
            approved_outcome: intent.outcome.clone(),
            approved_notional: intent.notional,
            approved_size: intent.size,
            approved_ttl_seconds: 30,
            approved_side: Side::Buy,
            reason_codes: Vec::new(),
        };
        let mut maker = LiveMakerExecution::new(LiveMakerExecutionContext {
            risk_approval: approval,
            maker_config: LiveAlphaMakerConfig {
                enabled: true,
                post_only: true,
                order_type: "GTD".to_string(),
                ttl_seconds: 30,
                min_edge_bps: 0,
                replace_tolerance_bps: 0,
                min_quote_lifetime_ms: 0,
            },
            now_unix: 1_777_000_100,
            human_approved: true,
        });

        let decision = maker.handle_intent(intent);
        let ExecutionDecision::LiveMaker(decision) = decision else {
            panic!("expected live maker decision");
        };
        let plan = decision.order_plan.expect("order plan exists");

        assert!(decision.would_submit);
        assert!(!decision.not_submitted);
        assert_eq!(plan.post_only, true);
        assert_eq!(plan.order_type, "GTD");
        assert_eq!(plan.effective_quote_ttl_seconds, 30);
        assert_eq!(plan.cancel_after_unix, 1_777_000_130);
        assert_eq!(plan.gtd_expiration_unix, 1_777_000_190);
    }

    #[test]
    fn shadow_live_report_counts_rejections_and_exposure() {
        let mut executor = ShadowLiveExecution::new(approved_context());
        let submit = shadow_decision(executor.handle_intent(sample_intent()));
        let mut context = approved_context();
        context.heartbeat_healthy = false;
        executor.set_context(context);
        let rejected = shadow_decision(executor.handle_intent(sample_intent()));

        let report = ShadowLiveReport::from_decisions(&[submit, rejected], 1, 1);

        assert_eq!(report.decision_count, 2);
        assert_eq!(report.paper_fill_count, 1);
        assert_eq!(report.shadow_would_submit_count, 1);
        assert_eq!(report.shadow_rejected_count, 1);
        assert_eq!(
            report
                .shadow_rejected_count_by_reason
                .get("heartbeat_not_healthy"),
            Some(&1)
        );
        assert_eq!(report.shadow_would_cancel_count, 0);
        assert_eq!(report.shadow_would_replace_count, 0);
        assert_eq!(report.paper_live_intent_divergence_count, 0);
        assert!(report.estimated_reserved_pusd_exposure > 0.0);
    }

    #[test]
    fn shadow_live_decision_can_build_redacted_journal_event() {
        let mut executor = ShadowLiveExecution::new(approved_context());
        let decision = shadow_decision(executor.handle_intent(sample_intent()));

        let event = decision.to_journal_event("run-1", "shadow-journal-1", 1_777_000_000_000);

        assert_eq!(
            event.event_type,
            LiveJournalEventType::LiveShadowDecisionRecorded
        );
        assert_eq!(
            event.payload["shadow_decision_id"].as_str(),
            Some(decision.shadow_decision_id.as_str())
        );
    }

    fn shadow_decision(decision: ExecutionDecision) -> ShadowLiveDecision {
        match decision {
            ExecutionDecision::ShadowLive(decision) => *decision,
            other => panic!("expected shadow decision, got {other:?}"),
        }
    }

    fn assert_rejects(decision: &ShadowLiveDecision, reason: &str) {
        assert!(
            decision
                .reason_codes
                .iter()
                .any(|candidate| candidate == reason),
            "expected reason {reason}, got {:?}",
            decision.reason_codes
        );
    }

    fn approved_context() -> ShadowLiveContext {
        ShadowLiveContext {
            mode_approved: true,
            risk_approved: true,
            risk_reason_codes: Vec::new(),
            geoblock_passed: true,
            heartbeat_healthy: true,
            reconciliation_clean: true,
            book_fresh: true,
            reference_fresh: true,
            now_ms: Some(1_777_000_100_000),
            market_end_ms: Some(1_777_000_900_000),
            no_trade_seconds_before_close: 60,
            available_pusd: 100.0,
            reserved_pusd: 0.0,
            max_available_pusd_usage: 100.0,
            max_reserved_pusd: 100.0,
            inventory_by_token: BTreeMap::new(),
            open_order_count: 0,
            max_open_orders: 2,
            current_market_notional: 0.0,
            max_market_notional: 100.0,
            current_asset_notional: 0.0,
            max_asset_notional: 200.0,
            current_total_live_notional: 0.0,
            max_single_order_notional: 100.0,
            max_total_live_notional: 300.0,
            min_edge_bps: 50.0,
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps: 720.0,
                raw_fee_config: None,
            },
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
