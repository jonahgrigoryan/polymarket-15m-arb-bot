use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::domain::Side;

pub const MODULE: &str = "live_quote_manager";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct QuoteId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct IntentId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteStatus {
    Planned,
    Open,
    CancelRequested,
    CancelConfirmed,
    Replaced,
    Expired,
    Halted,
    Filled,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveQuoteState {
    pub quote_id: String,
    pub intent_id: String,
    pub order_id: Option<String>,
    pub market: String,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub fair_probability_at_submit: f64,
    pub edge_bps_at_submit: f64,
    pub submitted_at_ms: u64,
    pub last_validated_at_ms: u64,
    pub cancel_requested_at_ms: Option<u64>,
    pub replaced_by_quote_id: Option<String>,
    pub status: QuoteStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteMarketStatus {
    Open,
    Closed,
    Resolved,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteReconciliationStatus {
    Clean,
    Stale,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteMarketSnapshot {
    pub market: String,
    pub token_id: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub spread: Option<f64>,
    pub last_trade_price: Option<f64>,
    pub tick_size: Option<f64>,
    pub status: QuoteMarketStatus,
    pub time_remaining_seconds: Option<u64>,
    pub book_age_ms: Option<u64>,
    pub reference_age_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteProposal {
    pub intent_id: String,
    pub market: String,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub fair_probability: f64,
    pub edge_bps: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteRiskSnapshot {
    pub max_open_orders: u64,
    pub max_live_orders_for_approval: u64,
    pub open_orders_for_approval: u64,
    pub replacements_used_for_approval: u64,
    pub risk_limits_changed: bool,
    pub inventory_changed: bool,
    pub heartbeat_fresh: bool,
}

impl Default for QuoteRiskSnapshot {
    fn default() -> Self {
        Self {
            max_open_orders: 0,
            max_live_orders_for_approval: 0,
            open_orders_for_approval: 0,
            replacements_used_for_approval: 0,
            risk_limits_changed: false,
            inventory_changed: false,
            heartbeat_fresh: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteRateLimitSnapshot {
    pub submit_timestamps_ms: Vec<u64>,
    pub cancel_timestamps_ms: Vec<u64>,
    pub replacement_timestamps_ms: Vec<u64>,
    pub failed_submit_at_ms: Option<u64>,
    pub failed_cancel_at_ms: Option<u64>,
    pub reconciliation_mismatch_at_ms: Option<u64>,
}

impl QuoteRateLimitSnapshot {
    pub fn empty() -> Self {
        Self {
            submit_timestamps_ms: Vec::new(),
            cancel_timestamps_ms: Vec::new(),
            replacement_timestamps_ms: Vec::new(),
            failed_submit_at_ms: None,
            failed_cancel_at_ms: None,
            reconciliation_mismatch_at_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteManagerPolicy {
    pub ttl_seconds: u64,
    pub no_trade_seconds_before_close: u64,
    pub replace_tolerance_bps: u64,
    pub min_quote_lifetime_ms: u64,
    pub min_edge_improvement_bps: u64,
    pub max_cancel_rate_per_min: u64,
    pub max_replacement_rate_per_min: u64,
    pub max_submit_rate_per_min: u64,
    pub cooldown_after_failed_submit_ms: u64,
    pub cooldown_after_failed_cancel_ms: u64,
    pub cooldown_after_reconciliation_mismatch_ms: u64,
    pub max_session_duration_sec: u64,
    pub max_live_orders_for_approval: u64,
    pub max_replacements_for_approval: u64,
    pub leave_open_in_no_trade_window: bool,
}

impl QuoteManagerPolicy {
    pub fn validate(&self) -> Result<(), LiveQuoteManagerError> {
        let mut errors = Vec::new();
        if self.ttl_seconds == 0 {
            errors.push("quote_manager_ttl_seconds_zero".to_string());
        }
        if self.max_session_duration_sec == 0 {
            errors.push("quote_manager_max_session_duration_zero".to_string());
        }
        if self.max_submit_rate_per_min == 0 {
            errors.push("quote_manager_max_submit_rate_zero".to_string());
        }
        if self.max_cancel_rate_per_min == 0 {
            errors.push("quote_manager_max_cancel_rate_zero".to_string());
        }
        if self.max_replacement_rate_per_min == 0 {
            errors.push("quote_manager_max_replacement_rate_zero".to_string());
        }
        if self.max_live_orders_for_approval == 0 {
            errors.push("quote_manager_max_live_orders_zero".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(LiveQuoteManagerError::InvalidPolicy(errors))
        }
    }
}

impl Default for QuoteManagerPolicy {
    fn default() -> Self {
        Self {
            ttl_seconds: 30,
            no_trade_seconds_before_close: 600,
            replace_tolerance_bps: 25,
            min_quote_lifetime_ms: 5_000,
            min_edge_improvement_bps: 10,
            max_cancel_rate_per_min: 1,
            max_replacement_rate_per_min: 1,
            max_submit_rate_per_min: 1,
            cooldown_after_failed_submit_ms: 30_000,
            cooldown_after_failed_cancel_ms: 30_000,
            cooldown_after_reconciliation_mismatch_ms: 60_000,
            max_session_duration_sec: 300,
            max_live_orders_for_approval: 1,
            max_replacements_for_approval: 1,
            leave_open_in_no_trade_window: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteManagerTickInput {
    pub now_ms: u64,
    pub session_started_at_ms: u64,
    pub fair_probability: f64,
    pub edge_threshold_bps: f64,
    pub market: QuoteMarketSnapshot,
    pub own_open_quotes: Vec<LiveQuoteState>,
    pub own_inventory: f64,
    pub risk: QuoteRiskSnapshot,
    pub rate_limits: QuoteRateLimitSnapshot,
    pub reconciliation_status: QuoteReconciliationStatus,
    pub policy: QuoteManagerPolicy,
    pub proposed_quote: Option<QuoteProposal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QuoteDecisionReason {
    NewQuote,
    HealthyQuote,
    TtlExpired,
    FairValueMoved,
    BookMovedPostOnlyUnsafe,
    EdgeBelowThreshold,
    InventoryChanged,
    TimeToCloseStricter,
    RiskLimitsChanged,
    MarketClosed,
    MarketResolved,
    MarketPaused,
    UnknownVenueStatus,
    BookStale,
    ReferenceStale,
    HeartbeatStale,
    ReconciliationStale,
    ReconciliationMismatch,
    ReconciliationUnknown,
    NoTradeWindowBlocksNewOrders,
    NoTradeWindowCancelOpenQuote,
    NoTradeWindowTtlExitPending,
    MinQuoteLifetime,
    MaxCancelRate,
    MaxReplacementRate,
    MaxSubmitRate,
    MinEdgeImprovement,
    FailedSubmitCooldown,
    FailedCancelCooldown,
    ReconciliationMismatchCooldown,
    SessionDurationExceeded,
    MaxLiveOrdersReached,
    MaxReplacementsReached,
    ExactOrderIdMissing,
    ExactOrderIdInvalid,
    PostOnlyWouldCross,
    NoReplacementCondition,
    NoProposal,
}

impl QuoteDecisionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewQuote => "new_quote",
            Self::HealthyQuote => "healthy_quote",
            Self::TtlExpired => "ttl_expired",
            Self::FairValueMoved => "fair_value_moved",
            Self::BookMovedPostOnlyUnsafe => "book_moved_post_only_unsafe",
            Self::EdgeBelowThreshold => "edge_below_threshold",
            Self::InventoryChanged => "inventory_changed",
            Self::TimeToCloseStricter => "time_to_close_stricter",
            Self::RiskLimitsChanged => "risk_limits_changed",
            Self::MarketClosed => "market_closed",
            Self::MarketResolved => "market_resolved",
            Self::MarketPaused => "market_paused",
            Self::UnknownVenueStatus => "unknown_venue_status",
            Self::BookStale => "book_stale",
            Self::ReferenceStale => "reference_stale",
            Self::HeartbeatStale => "heartbeat_stale",
            Self::ReconciliationStale => "reconciliation_stale",
            Self::ReconciliationMismatch => "reconciliation_mismatch",
            Self::ReconciliationUnknown => "reconciliation_unknown",
            Self::NoTradeWindowBlocksNewOrders => "no_trade_window_blocks_new_orders",
            Self::NoTradeWindowCancelOpenQuote => "no_trade_window_cancel_open_quote",
            Self::NoTradeWindowTtlExitPending => "no_trade_window_ttl_exit_pending",
            Self::MinQuoteLifetime => "min_quote_lifetime",
            Self::MaxCancelRate => "max_cancel_rate",
            Self::MaxReplacementRate => "max_replacement_rate",
            Self::MaxSubmitRate => "max_submit_rate",
            Self::MinEdgeImprovement => "min_edge_improvement",
            Self::FailedSubmitCooldown => "failed_submit_cooldown",
            Self::FailedCancelCooldown => "failed_cancel_cooldown",
            Self::ReconciliationMismatchCooldown => "reconciliation_mismatch_cooldown",
            Self::SessionDurationExceeded => "session_duration_exceeded",
            Self::MaxLiveOrdersReached => "max_live_orders_reached",
            Self::MaxReplacementsReached => "max_replacements_reached",
            Self::ExactOrderIdMissing => "exact_order_id_missing",
            Self::ExactOrderIdInvalid => "exact_order_id_invalid",
            Self::PostOnlyWouldCross => "post_only_would_cross",
            Self::NoReplacementCondition => "no_replacement_condition",
            Self::NoProposal => "no_proposal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum QuoteManagerDecision {
    PlaceQuote {
        intent_id: String,
        market: String,
        token_id: String,
        side: Side,
        price: f64,
        size: f64,
        reasons: Vec<QuoteDecisionReason>,
    },
    LeaveQuote {
        quote_id: String,
        order_id: Option<String>,
        reasons: Vec<QuoteDecisionReason>,
    },
    CancelQuote {
        quote_id: String,
        order_id: String,
        reasons: Vec<QuoteDecisionReason>,
    },
    ReplaceQuote {
        quote_id: String,
        order_id: String,
        replacement_intent_id: String,
        replacement_price: f64,
        replacement_size: f64,
        reasons: Vec<QuoteDecisionReason>,
    },
    ExpireQuote {
        quote_id: String,
        order_id: String,
        reasons: Vec<QuoteDecisionReason>,
    },
    HaltQuote {
        quote_id: Option<String>,
        order_id: Option<String>,
        reasons: Vec<QuoteDecisionReason>,
    },
    SkipMarket {
        market: String,
        token_id: String,
        reasons: Vec<QuoteDecisionReason>,
    },
}

impl QuoteManagerDecision {
    pub fn reasons(&self) -> &[QuoteDecisionReason] {
        match self {
            Self::PlaceQuote { reasons, .. }
            | Self::LeaveQuote { reasons, .. }
            | Self::CancelQuote { reasons, .. }
            | Self::ReplaceQuote { reasons, .. }
            | Self::ExpireQuote { reasons, .. }
            | Self::HaltQuote { reasons, .. }
            | Self::SkipMarket { reasons, .. } => reasons,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::PlaceQuote { .. } => "place_quote",
            Self::LeaveQuote { .. } => "leave_quote",
            Self::CancelQuote { .. } => "cancel_quote",
            Self::ReplaceQuote { .. } => "replace_quote",
            Self::ExpireQuote { .. } => "expire_quote",
            Self::HaltQuote { .. } => "halt_quote",
            Self::SkipMarket { .. } => "skip_market",
        }
    }
}

pub fn evaluate_quote_manager_tick(
    input: &QuoteManagerTickInput,
) -> Result<Vec<QuoteManagerDecision>, LiveQuoteManagerError> {
    input.policy.validate()?;
    let mut decisions = Vec::new();

    if session_expired(input) {
        return Ok(halt_all_or_skip(
            input,
            vec![QuoteDecisionReason::SessionDurationExceeded],
        ));
    }
    if let Some(reason) = reconciliation_reason(input.reconciliation_status) {
        return Ok(halt_all_or_skip(input, vec![reason]));
    }
    if !input.risk.heartbeat_fresh {
        return Ok(cancel_each_exact_or_halt(
            input,
            vec![QuoteDecisionReason::HeartbeatStale],
        ));
    }
    if let Some(reason) = market_halt_reason(input.market.status) {
        return Ok(match input.own_open_quotes.is_empty() {
            true => vec![QuoteManagerDecision::SkipMarket {
                market: input.market.market.clone(),
                token_id: input.market.token_id.clone(),
                reasons: vec![reason],
            }],
            false => halt_all_or_skip(input, vec![reason]),
        });
    }
    if book_stale(input) {
        return Ok(cancel_each_exact_or_halt(
            input,
            vec![QuoteDecisionReason::BookStale],
        ));
    }
    if reference_stale(input) {
        return Ok(cancel_each_exact_or_halt(
            input,
            vec![QuoteDecisionReason::ReferenceStale],
        ));
    }
    if let Some(reason) = cooldown_reason(input) {
        return Ok(halt_all_or_skip(input, vec![reason]));
    }

    if input.own_open_quotes.is_empty() {
        decisions.push(evaluate_new_quote(input));
        return Ok(decisions);
    }

    for quote in &input.own_open_quotes {
        decisions.push(evaluate_existing_quote(input, quote));
    }
    Ok(decisions)
}

fn evaluate_new_quote(input: &QuoteManagerTickInput) -> QuoteManagerDecision {
    let mut reasons = Vec::new();
    if in_no_trade_window(input) {
        reasons.push(QuoteDecisionReason::NoTradeWindowBlocksNewOrders);
    }
    if rate_count_last_min(&input.rate_limits.submit_timestamps_ms, input.now_ms)
        >= input.policy.max_submit_rate_per_min
    {
        reasons.push(QuoteDecisionReason::MaxSubmitRate);
    }
    if input.risk.open_orders_for_approval >= input.policy.max_live_orders_for_approval
        || input.risk.open_orders_for_approval >= input.risk.max_live_orders_for_approval
        || input.risk.open_orders_for_approval >= input.risk.max_open_orders
    {
        reasons.push(QuoteDecisionReason::MaxLiveOrdersReached);
    }

    let Some(proposal) = &input.proposed_quote else {
        reasons.push(QuoteDecisionReason::NoProposal);
        return QuoteManagerDecision::SkipMarket {
            market: input.market.market.clone(),
            token_id: input.market.token_id.clone(),
            reasons,
        };
    };
    if proposal.edge_bps < input.edge_threshold_bps {
        reasons.push(QuoteDecisionReason::EdgeBelowThreshold);
    }
    if post_only_would_cross(
        proposal.side,
        proposal.price,
        input.market.best_bid,
        input.market.best_ask,
    ) {
        reasons.push(QuoteDecisionReason::PostOnlyWouldCross);
    }

    if reasons.is_empty() {
        QuoteManagerDecision::PlaceQuote {
            intent_id: proposal.intent_id.clone(),
            market: proposal.market.clone(),
            token_id: proposal.token_id.clone(),
            side: proposal.side,
            price: proposal.price,
            size: proposal.size,
            reasons: vec![QuoteDecisionReason::NewQuote],
        }
    } else {
        QuoteManagerDecision::SkipMarket {
            market: input.market.market.clone(),
            token_id: input.market.token_id.clone(),
            reasons,
        }
    }
}

fn evaluate_existing_quote(
    input: &QuoteManagerTickInput,
    quote: &LiveQuoteState,
) -> QuoteManagerDecision {
    let age_ms = input.now_ms.saturating_sub(quote.submitted_at_ms);
    if quote_is_ttl_expired(input, quote) {
        return exact_order_decision(input, quote, vec![QuoteDecisionReason::TtlExpired], true);
    }
    if in_no_trade_window(input) {
        if input.policy.leave_open_in_no_trade_window {
            return QuoteManagerDecision::LeaveQuote {
                quote_id: quote.quote_id.clone(),
                order_id: quote.order_id.clone(),
                reasons: vec![QuoteDecisionReason::NoTradeWindowTtlExitPending],
            };
        }
        return exact_order_decision(
            input,
            quote,
            vec![QuoteDecisionReason::NoTradeWindowCancelOpenQuote],
            false,
        );
    }

    let replace_reasons = approved_replacement_reasons(input, quote);
    if replace_reasons.is_empty() {
        return QuoteManagerDecision::LeaveQuote {
            quote_id: quote.quote_id.clone(),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::HealthyQuote],
        };
    }
    if age_ms < input.policy.min_quote_lifetime_ms {
        return QuoteManagerDecision::LeaveQuote {
            quote_id: quote.quote_id.clone(),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MinQuoteLifetime],
        };
    }
    if rate_count_last_min(&input.rate_limits.replacement_timestamps_ms, input.now_ms)
        >= input.policy.max_replacement_rate_per_min
    {
        return QuoteManagerDecision::HaltQuote {
            quote_id: Some(quote.quote_id.clone()),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MaxReplacementRate],
        };
    }
    if input.risk.replacements_used_for_approval >= input.policy.max_replacements_for_approval {
        return QuoteManagerDecision::HaltQuote {
            quote_id: Some(quote.quote_id.clone()),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MaxReplacementsReached],
        };
    }
    if rate_count_last_min(&input.rate_limits.submit_timestamps_ms, input.now_ms)
        >= input.policy.max_submit_rate_per_min
    {
        return QuoteManagerDecision::HaltQuote {
            quote_id: Some(quote.quote_id.clone()),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MaxSubmitRate],
        };
    }

    let Some(proposal) = &input.proposed_quote else {
        return exact_order_decision(input, quote, vec![QuoteDecisionReason::NoProposal], false);
    };
    if proposal.edge_bps < input.edge_threshold_bps {
        return exact_order_decision(
            input,
            quote,
            vec![QuoteDecisionReason::EdgeBelowThreshold],
            false,
        );
    }
    if proposal.edge_bps - quote.edge_bps_at_submit < input.policy.min_edge_improvement_bps as f64
        && !replace_reasons.contains(&QuoteDecisionReason::EdgeBelowThreshold)
        && !replace_reasons.contains(&QuoteDecisionReason::BookMovedPostOnlyUnsafe)
    {
        return QuoteManagerDecision::LeaveQuote {
            quote_id: quote.quote_id.clone(),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MinEdgeImprovement],
        };
    }
    if post_only_would_cross(
        proposal.side,
        proposal.price,
        input.market.best_bid,
        input.market.best_ask,
    ) {
        return exact_order_decision(
            input,
            quote,
            vec![QuoteDecisionReason::PostOnlyWouldCross],
            false,
        );
    }

    let Some(order_id) = exact_order_id_or_none(quote) else {
        return exact_order_halt(quote);
    };
    QuoteManagerDecision::ReplaceQuote {
        quote_id: quote.quote_id.clone(),
        order_id,
        replacement_intent_id: proposal.intent_id.clone(),
        replacement_price: proposal.price,
        replacement_size: proposal.size,
        reasons: replace_reasons,
    }
}

fn approved_replacement_reasons(
    input: &QuoteManagerTickInput,
    quote: &LiveQuoteState,
) -> Vec<QuoteDecisionReason> {
    let mut reasons = BTreeSet::new();
    let fair_move_bps =
        ((input.fair_probability - quote.fair_probability_at_submit).abs() * 10_000.0).round();
    if fair_move_bps >= input.policy.replace_tolerance_bps as f64 {
        reasons.insert(QuoteDecisionReason::FairValueMoved);
    }
    if post_only_would_cross(
        quote.side,
        quote.price,
        input.market.best_bid,
        input.market.best_ask,
    ) {
        reasons.insert(QuoteDecisionReason::BookMovedPostOnlyUnsafe);
    }
    if current_quote_edge_bps(input, quote) < input.edge_threshold_bps {
        reasons.insert(QuoteDecisionReason::EdgeBelowThreshold);
    }
    if input.risk.inventory_changed {
        reasons.insert(QuoteDecisionReason::InventoryChanged);
    }
    if in_no_trade_window(input) {
        reasons.insert(QuoteDecisionReason::TimeToCloseStricter);
    }
    if input.risk.risk_limits_changed {
        reasons.insert(QuoteDecisionReason::RiskLimitsChanged);
    }
    reasons.into_iter().collect()
}

fn current_quote_edge_bps(input: &QuoteManagerTickInput, quote: &LiveQuoteState) -> f64 {
    match quote.side {
        Side::Buy => (input.fair_probability - quote.price) * 10_000.0,
        Side::Sell => (quote.price - input.fair_probability) * 10_000.0,
    }
}

fn exact_order_decision(
    input: &QuoteManagerTickInput,
    quote: &LiveQuoteState,
    reasons: Vec<QuoteDecisionReason>,
    expire: bool,
) -> QuoteManagerDecision {
    if rate_count_last_min(&input.rate_limits.cancel_timestamps_ms, input.now_ms)
        >= input.policy.max_cancel_rate_per_min
    {
        return QuoteManagerDecision::HaltQuote {
            quote_id: Some(quote.quote_id.clone()),
            order_id: quote.order_id.clone(),
            reasons: vec![QuoteDecisionReason::MaxCancelRate],
        };
    }
    let Some(order_id) = exact_order_id_or_none(quote) else {
        return exact_order_halt(quote);
    };
    if expire {
        QuoteManagerDecision::ExpireQuote {
            quote_id: quote.quote_id.clone(),
            order_id,
            reasons,
        }
    } else {
        QuoteManagerDecision::CancelQuote {
            quote_id: quote.quote_id.clone(),
            order_id,
            reasons,
        }
    }
}

fn exact_order_halt(quote: &LiveQuoteState) -> QuoteManagerDecision {
    let reason = match &quote.order_id {
        Some(_) => QuoteDecisionReason::ExactOrderIdInvalid,
        None => QuoteDecisionReason::ExactOrderIdMissing,
    };
    QuoteManagerDecision::HaltQuote {
        quote_id: Some(quote.quote_id.clone()),
        order_id: quote.order_id.clone(),
        reasons: vec![reason],
    }
}

fn exact_order_id_or_none(quote: &LiveQuoteState) -> Option<String> {
    quote
        .order_id
        .as_deref()
        .filter(|order_id| is_exact_order_id(order_id))
        .map(str::to_string)
}

pub fn is_exact_order_id(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn cancel_each_exact_or_halt(
    input: &QuoteManagerTickInput,
    reasons: Vec<QuoteDecisionReason>,
) -> Vec<QuoteManagerDecision> {
    if input.own_open_quotes.is_empty() {
        return vec![QuoteManagerDecision::SkipMarket {
            market: input.market.market.clone(),
            token_id: input.market.token_id.clone(),
            reasons,
        }];
    }
    input
        .own_open_quotes
        .iter()
        .map(|quote| exact_order_decision(input, quote, reasons.clone(), false))
        .collect()
}

fn halt_all_or_skip(
    input: &QuoteManagerTickInput,
    reasons: Vec<QuoteDecisionReason>,
) -> Vec<QuoteManagerDecision> {
    if input.own_open_quotes.is_empty() {
        return vec![QuoteManagerDecision::SkipMarket {
            market: input.market.market.clone(),
            token_id: input.market.token_id.clone(),
            reasons,
        }];
    }
    input
        .own_open_quotes
        .iter()
        .map(|quote| QuoteManagerDecision::HaltQuote {
            quote_id: Some(quote.quote_id.clone()),
            order_id: quote.order_id.clone(),
            reasons: reasons.clone(),
        })
        .collect()
}

fn market_halt_reason(status: QuoteMarketStatus) -> Option<QuoteDecisionReason> {
    match status {
        QuoteMarketStatus::Open => None,
        QuoteMarketStatus::Closed => Some(QuoteDecisionReason::MarketClosed),
        QuoteMarketStatus::Resolved => Some(QuoteDecisionReason::MarketResolved),
        QuoteMarketStatus::Paused => Some(QuoteDecisionReason::MarketPaused),
        QuoteMarketStatus::Unknown => Some(QuoteDecisionReason::UnknownVenueStatus),
    }
}

fn reconciliation_reason(status: QuoteReconciliationStatus) -> Option<QuoteDecisionReason> {
    match status {
        QuoteReconciliationStatus::Clean => None,
        QuoteReconciliationStatus::Stale => Some(QuoteDecisionReason::ReconciliationStale),
        QuoteReconciliationStatus::Mismatch => Some(QuoteDecisionReason::ReconciliationMismatch),
        QuoteReconciliationStatus::Unknown => Some(QuoteDecisionReason::ReconciliationUnknown),
    }
}

fn cooldown_reason(input: &QuoteManagerTickInput) -> Option<QuoteDecisionReason> {
    if within_cooldown(
        input.rate_limits.failed_submit_at_ms,
        input.now_ms,
        input.policy.cooldown_after_failed_submit_ms,
    ) {
        return Some(QuoteDecisionReason::FailedSubmitCooldown);
    }
    if within_cooldown(
        input.rate_limits.failed_cancel_at_ms,
        input.now_ms,
        input.policy.cooldown_after_failed_cancel_ms,
    ) {
        return Some(QuoteDecisionReason::FailedCancelCooldown);
    }
    if within_cooldown(
        input.rate_limits.reconciliation_mismatch_at_ms,
        input.now_ms,
        input.policy.cooldown_after_reconciliation_mismatch_ms,
    ) {
        return Some(QuoteDecisionReason::ReconciliationMismatchCooldown);
    }
    None
}

fn within_cooldown(last_at_ms: Option<u64>, now_ms: u64, cooldown_ms: u64) -> bool {
    last_at_ms.is_some_and(|last| now_ms.saturating_sub(last) < cooldown_ms)
}

fn session_expired(input: &QuoteManagerTickInput) -> bool {
    input
        .now_ms
        .saturating_sub(input.session_started_at_ms)
        .saturating_div(1_000)
        >= input.policy.max_session_duration_sec
}

fn quote_is_ttl_expired(input: &QuoteManagerTickInput, quote: &LiveQuoteState) -> bool {
    input.now_ms
        >= quote
            .submitted_at_ms
            .saturating_add(input.policy.ttl_seconds.saturating_mul(1_000))
}

fn book_stale(input: &QuoteManagerTickInput) -> bool {
    input
        .market
        .book_age_ms
        .is_none_or(|age| age > max_staleness_ms(input.policy.ttl_seconds))
}

fn reference_stale(input: &QuoteManagerTickInput) -> bool {
    input
        .market
        .reference_age_ms
        .is_none_or(|age| age > max_staleness_ms(input.policy.ttl_seconds))
}

fn max_staleness_ms(ttl_seconds: u64) -> u64 {
    ttl_seconds.saturating_mul(1_000).min(5_000)
}

fn in_no_trade_window(input: &QuoteManagerTickInput) -> bool {
    input
        .market
        .time_remaining_seconds
        .is_none_or(|remaining| remaining <= input.policy.no_trade_seconds_before_close)
}

fn rate_count_last_min(timestamps_ms: &[u64], now_ms: u64) -> u64 {
    timestamps_ms
        .iter()
        .filter(|timestamp| now_ms.saturating_sub(**timestamp) < 60_000)
        .count() as u64
}

fn post_only_would_cross(
    side: Side,
    price: f64,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
) -> bool {
    match side {
        Side::Buy => best_ask.is_none_or(|ask| price >= ask),
        Side::Sell => best_bid.is_none_or(|bid| price <= bid),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteApprovalFields {
    pub approval_id: String,
    pub approved_wallet: String,
    pub approved_funder: String,
    pub approved_markets_assets: String,
    pub max_orders: u64,
    pub max_replacements: u64,
    pub max_duration_sec: u64,
    pub ttl_seconds: u64,
    pub gtd_policy: String,
    pub cancel_policy: String,
    pub no_trade_window_policy: String,
    pub risk_limits: String,
    pub rollback_owner: String,
    pub monitoring_owner: String,
    pub authenticated_readback_evidence: String,
    pub operator_approval_timestamp: String,
    pub available_pusd_units: u64,
    pub reserved_pusd_units: u64,
    pub open_order_count: u64,
    pub trade_count: u64,
    pub heartbeat_status: String,
    pub funder_allowance_units: u64,
}

pub const LA6_APPROVAL_REQUIRED_FIELDS: &[&str] = &[
    "approval_id",
    "approved_wallet",
    "approved_funder",
    "approved_markets_assets",
    "max_orders",
    "max_replacements",
    "max_duration_sec",
    "ttl_seconds",
    "gtd_policy",
    "cancel_policy",
    "no_trade_window_policy",
    "risk_limits",
    "rollback_owner",
    "monitoring_owner",
    "authenticated_readback_evidence",
    "operator_approval_timestamp",
    "available_pusd_units",
    "reserved_pusd_units",
    "open_order_count",
    "trade_count",
    "heartbeat_status",
    "funder_allowance_units",
];

pub fn validate_la6_approval_artifact_text(
    text: &str,
    approval_id: &str,
) -> Result<QuoteApprovalFields, LiveQuoteManagerError> {
    let mut errors = Vec::<String>::new();
    if !text.contains(approval_id) {
        errors.push("approval_id_missing".to_string());
    }
    if !text.contains("Status: LA6 APPROVED FOR THIS RUN ONLY") {
        errors.push("approval_status_missing".to_string());
    }
    if approval_artifact_indicates_consumed(text) {
        errors.push("approval_artifact_consumed".to_string());
    }
    for field in LA6_APPROVAL_REQUIRED_FIELDS {
        match approval_table_value(text, field) {
            Some(value) if approval_value_is_final(&value) => {}
            Some(_) => errors.push(format!("approval_field_pending:{field}")),
            None => errors.push(format!("approval_field_missing:{field}")),
        }
    }
    if !errors.is_empty() {
        errors.sort_unstable();
        errors.dedup();
        return Err(LiveQuoteManagerError::Approval(errors));
    }

    Ok(QuoteApprovalFields {
        approval_id: approval_string(text, "approval_id")?,
        approved_wallet: approval_string(text, "approved_wallet")?,
        approved_funder: approval_string(text, "approved_funder")?,
        approved_markets_assets: approval_string(text, "approved_markets_assets")?,
        max_orders: approval_u64(text, "max_orders")?,
        max_replacements: approval_u64(text, "max_replacements")?,
        max_duration_sec: approval_u64(text, "max_duration_sec")?,
        ttl_seconds: approval_u64(text, "ttl_seconds")?,
        gtd_policy: approval_string(text, "gtd_policy")?,
        cancel_policy: approval_string(text, "cancel_policy")?,
        no_trade_window_policy: approval_string(text, "no_trade_window_policy")?,
        risk_limits: approval_string(text, "risk_limits")?,
        rollback_owner: approval_string(text, "rollback_owner")?,
        monitoring_owner: approval_string(text, "monitoring_owner")?,
        authenticated_readback_evidence: approval_string(text, "authenticated_readback_evidence")?,
        operator_approval_timestamp: approval_string(text, "operator_approval_timestamp")?,
        available_pusd_units: approval_u64(text, "available_pusd_units")?,
        reserved_pusd_units: approval_u64(text, "reserved_pusd_units")?,
        open_order_count: approval_u64(text, "open_order_count")?,
        trade_count: approval_u64(text, "trade_count")?,
        heartbeat_status: approval_string(text, "heartbeat_status")?,
        funder_allowance_units: approval_u64(text, "funder_allowance_units")?,
    })
}

fn approval_artifact_indicates_consumed(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    [
        "EXECUTION GATE STATUS: LA6 RUN COMPLETED",
        "EXECUTION GATE STATUS: LA6 RUN CONSUMED",
        "APPROVAL CONSUMED",
        "AUTHORIZED SESSION COMPLETED",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn approval_table_value(text: &str, field: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let cells = line.split('|').map(str::trim).collect::<Vec<_>>();
        if cells.len() >= 3 && cells[1] == field {
            Some(cells[2].trim_matches('`').trim().to_string())
        } else {
            None
        }
    })
}

fn approval_value_is_final(value: &str) -> bool {
    let trimmed = value.trim();
    let upper = trimmed.to_ascii_uppercase();
    !trimmed.is_empty()
        && !upper.contains("PENDING")
        && !upper.contains("TBD")
        && !upper.contains("TODO")
        && !upper.contains("BLOCKED")
        && !upper.contains("UNAVAILABLE")
        && !upper.contains("NOT RUN")
        && !upper.contains("UNKNOWN")
        && !upper.contains("MISSING")
        && !trimmed.starts_with('[')
        && !trimmed.ends_with(']')
}

fn approval_string(text: &str, field: &'static str) -> Result<String, LiveQuoteManagerError> {
    approval_table_value(text, field).ok_or_else(|| {
        LiveQuoteManagerError::Approval(vec![format!("approval_field_missing:{field}")])
    })
}

fn approval_u64(text: &str, field: &'static str) -> Result<u64, LiveQuoteManagerError> {
    let value = approval_string(text, field)?;
    let start = value.find(|ch: char| ch.is_ascii_digit()).ok_or_else(|| {
        LiveQuoteManagerError::Approval(vec![format!("approval_field_parse_error:{field}")])
    })?;
    let tail = &value[start..];
    let end = tail
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(tail.len());
    tail[..end].parse::<u64>().map_err(|_| {
        LiveQuoteManagerError::Approval(vec![format!("approval_field_parse_error:{field}")])
    })
}

#[derive(Debug)]
pub enum LiveQuoteManagerError {
    InvalidPolicy(Vec<String>),
    Approval(Vec<String>),
}

impl Display for LiveQuoteManagerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy(errors) => {
                write!(
                    formatter,
                    "live quote manager policy invalid: {}",
                    errors.join(",")
                )
            }
            Self::Approval(errors) => {
                write!(
                    formatter,
                    "LA6 approval artifact is not final: {}",
                    errors.join(",")
                )
            }
        }
    }
}

impl Error for LiveQuoteManagerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_quote_manager_places_quote_when_all_gates_pass() {
        let input = sample_input_without_quotes();
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::PlaceQuote { .. }]
        ));
    }

    #[test]
    fn live_quote_manager_leaves_healthy_quote_alone() {
        let mut input = sample_input_with_quote();
        input.fair_probability = 0.232;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::LeaveQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::HealthyQuote]
        ));
    }

    #[test]
    fn live_quote_manager_cancel_replace_replaces_only_on_approved_condition() {
        let mut input = sample_input_with_quote();
        input.fair_probability = 0.25;
        input.proposed_quote.as_mut().expect("proposal").edge_bps = 500.0;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::ReplaceQuote { reasons, .. }]
                if reasons.contains(&QuoteDecisionReason::FairValueMoved)
        ));
    }

    #[test]
    fn live_quote_manager_replacement_proposal_below_edge_threshold_cancels() {
        let mut input = sample_input_with_quote();
        input.fair_probability = 0.192;
        input.proposed_quote.as_mut().expect("proposal").edge_bps = 20.0;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::CancelQuote { order_id, reasons, .. }]
                if order_id == &exact_order_id()
                    && reasons == &[QuoteDecisionReason::EdgeBelowThreshold]
        ));
    }

    #[test]
    fn live_quote_manager_cancel_replace_no_replacement_below_tolerance() {
        let mut input = sample_input_with_quote();
        input.fair_probability = 0.231;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::LeaveQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::HealthyQuote]
        ));
    }

    #[test]
    fn live_quote_manager_expires_ttl_quote_with_exact_order_id() {
        let mut input = sample_input_with_quote();
        input.now_ms = 1_031_000;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::ExpireQuote { order_id, reasons, .. }]
                if order_id == &exact_order_id() && reasons.contains(&QuoteDecisionReason::TtlExpired)
        ));
    }

    #[test]
    fn live_quote_manager_halts_unknown_venue_status_fail_closed() {
        let mut input = sample_input_with_quote();
        input.market.status = QuoteMarketStatus::Unknown;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::UnknownVenueStatus]
        ));
    }

    #[test]
    fn live_quote_manager_halts_partial_fill_ambiguity_via_reconciliation_mismatch() {
        let mut input = sample_input_with_quote();
        input.reconciliation_status = QuoteReconciliationStatus::Mismatch;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::ReconciliationMismatch]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_max_cancel_rate_halts() {
        let mut input = sample_input_with_quote();
        input.market.book_age_ms = Some(9_999);
        input.rate_limits.cancel_timestamps_ms = vec![input.now_ms - 10_000];
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::MaxCancelRate]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_max_replacement_rate_halts() {
        let mut input = sample_input_with_quote();
        input.fair_probability = 0.25;
        input.rate_limits.replacement_timestamps_ms = vec![input.now_ms - 10_000];
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::MaxReplacementRate]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_minimum_quote_lifetime_leaves_quote() {
        let mut input = sample_input_with_quote();
        input.now_ms = 1_002_000;
        input.fair_probability = 0.25;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::LeaveQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::MinQuoteLifetime]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_failed_submit_cooldown_halts() {
        let mut input = sample_input_without_quotes();
        input.rate_limits.failed_submit_at_ms = Some(input.now_ms - 1_000);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::SkipMarket { reasons, .. }]
                if reasons == &[QuoteDecisionReason::FailedSubmitCooldown]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_failed_cancel_cooldown_halts() {
        let mut input = sample_input_with_quote();
        input.rate_limits.failed_cancel_at_ms = Some(input.now_ms - 1_000);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::FailedCancelCooldown]
        ));
    }

    #[test]
    fn live_quote_manager_anti_churn_reconciliation_mismatch_cooldown_halts() {
        let mut input = sample_input_with_quote();
        input.rate_limits.reconciliation_mismatch_at_ms = Some(input.now_ms - 1_000);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::ReconciliationMismatchCooldown]
        ));
    }

    #[test]
    fn live_quote_manager_no_trade_window_blocks_new_orders() {
        let mut input = sample_input_without_quotes();
        input.market.time_remaining_seconds = Some(500);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::SkipMarket { reasons, .. }]
                if reasons.contains(&QuoteDecisionReason::NoTradeWindowBlocksNewOrders)
        ));
    }

    #[test]
    fn live_quote_manager_no_trade_window_existing_quote_cancels_by_default() {
        let mut input = sample_input_with_quote();
        input.market.time_remaining_seconds = Some(500);
        input.fair_probability = 0.30;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::CancelQuote { order_id, reasons, .. }]
                if order_id == &exact_order_id()
                    && reasons == &[QuoteDecisionReason::NoTradeWindowCancelOpenQuote]
        ));
    }

    #[test]
    fn live_quote_manager_no_trade_window_can_leave_only_when_policy_allows() {
        let mut input = sample_input_with_quote();
        input.market.time_remaining_seconds = Some(500);
        input.fair_probability = 0.30;
        input.policy.leave_open_in_no_trade_window = true;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::LeaveQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::NoTradeWindowTtlExitPending]
        ));
    }

    #[test]
    fn live_quote_manager_post_only_safety_blocks_marketable_place_and_replaces_unsafe_quote() {
        let mut place_input = sample_input_without_quotes();
        place_input.proposed_quote.as_mut().expect("proposal").price = 0.21;
        let decisions = evaluate_quote_manager_tick(&place_input).expect("tick evaluates");
        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::SkipMarket { reasons, .. }]
                if reasons.contains(&QuoteDecisionReason::PostOnlyWouldCross)
        ));

        let mut replace_input = sample_input_with_quote();
        replace_input.market.best_ask = Some(0.18);
        replace_input
            .proposed_quote
            .as_mut()
            .expect("proposal")
            .price = 0.17;
        let decisions = evaluate_quote_manager_tick(&replace_input).expect("tick evaluates");
        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::ReplaceQuote { reasons, .. }]
                if reasons.contains(&QuoteDecisionReason::BookMovedPostOnlyUnsafe)
        ));
    }

    #[test]
    fn live_quote_manager_cancel_replace_exact_order_cancel_only() {
        let mut input = sample_input_with_quote();
        input.own_open_quotes[0].order_id = Some("not-an-order-id".to_string());
        input.market.book_age_ms = Some(9_999);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::ExactOrderIdInvalid]
        ));
    }

    #[test]
    fn live_quote_manager_cancel_replace_has_no_cancel_all_decision() {
        let mut input = sample_input_with_quote();
        input.market.book_age_ms = Some(9_999);
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(decisions
            .iter()
            .all(|decision| decision.kind() != "cancel_all"));
        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::CancelQuote { order_id, .. }] if order_id == &exact_order_id()
        ));
    }

    #[test]
    fn live_quote_manager_reserved_balance_release_after_cancel_requires_clean_reconciliation() {
        let mut input = sample_input_with_quote();
        input.reconciliation_status = QuoteReconciliationStatus::Stale;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::HaltQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::ReconciliationStale]
        ));
    }

    #[test]
    fn live_quote_manager_replacement_links_old_and_new_quote_ids() {
        let mut quote = sample_quote();
        quote.replaced_by_quote_id = Some("quote-2".to_string());
        assert_eq!(quote.replaced_by_quote_id.as_deref(), Some("quote-2"));
    }

    #[test]
    fn live_quote_manager_approval_artifact_mismatch_fails_closed() {
        let artifact = valid_approval_artifact().replace("LA6-approval-1", "LA6-other");
        let error = validate_la6_approval_artifact_text(&artifact, "LA6-approval-1")
            .expect_err("approval mismatch must fail");

        assert!(error.to_string().contains("approval_id_missing"));
    }

    #[test]
    fn live_quote_manager_consumed_approval_artifact_fails_closed() {
        let artifact = valid_approval_artifact().replace(
            "Execution Gate Status: READY",
            "Execution Gate Status: LA6 RUN COMPLETED",
        );
        let error = validate_la6_approval_artifact_text(&artifact, "LA6-approval-1")
            .expect_err("consumed approval must fail");

        assert!(error.to_string().contains("approval_artifact_consumed"));
    }

    #[test]
    fn live_quote_manager_approval_artifact_requires_final_readback_fields() {
        let artifact = valid_approval_artifact()
            .replace(
                "| available_pusd_units | `6314318` |",
                "| available_pusd_units | `NOT RUN` |",
            )
            .replace(
                "| funder_allowance_units | `18446744073709551615` |",
                "| funder_allowance_units | `BLOCKED - NOT RUN` |",
            );
        let error = validate_la6_approval_artifact_text(&artifact, "LA6-approval-1")
            .expect_err("blocked readback fields must fail");
        let error = error.to_string();

        assert!(error.contains("approval_field_pending:available_pusd_units"));
        assert!(error.contains("approval_field_pending:funder_allowance_units"));
    }

    #[test]
    fn live_quote_manager_cap_reservation_prevents_reuse_races() {
        let mut used = BTreeSet::new();
        assert!(used.insert("LA6-approval-1".to_string()));
        assert!(!used.insert("LA6-approval-1".to_string()));
    }

    #[test]
    fn live_quote_manager_global_placement_disabled_fails_closed_via_policy() {
        let mut input = sample_input_without_quotes();
        input.policy.max_live_orders_for_approval = 0;
        let error = evaluate_quote_manager_tick(&input).expect_err("policy must fail");

        assert!(error
            .to_string()
            .contains("quote_manager_max_live_orders_zero"));
    }

    #[test]
    fn live_quote_manager_kill_switch_active_fails_closed_as_session_halt() {
        let mut input = sample_input_with_quote();
        input.risk.heartbeat_fresh = false;
        let decisions = evaluate_quote_manager_tick(&input).expect("tick evaluates");

        assert!(matches!(
            decisions.as_slice(),
            [QuoteManagerDecision::CancelQuote { reasons, .. }]
                if reasons == &[QuoteDecisionReason::HeartbeatStale]
        ));
    }

    #[test]
    fn live_quote_manager_feature_gate_missing_fails_closed_via_policy_validation() {
        let mut policy = QuoteManagerPolicy::default();
        policy.max_submit_rate_per_min = 0;
        let error = policy.validate().expect_err("zero submit rate fails");

        assert!(error
            .to_string()
            .contains("quote_manager_max_submit_rate_zero"));
    }

    #[test]
    fn live_quote_manager_redaction_no_secret_journal_payloads_have_no_secret_fields() {
        let decision = QuoteManagerDecision::PlaceQuote {
            intent_id: "intent-1".to_string(),
            market: "market".to_string(),
            token_id: "token".to_string(),
            side: Side::Buy,
            price: 0.2,
            size: 5.0,
            reasons: vec![QuoteDecisionReason::NewQuote],
        };
        let payload = serde_json::to_value(&decision).expect("decision serializes");
        let serialized = payload.to_string().to_ascii_lowercase();

        assert!(!serialized.contains("private"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("passphrase"));
        assert!(!serialized.contains("mnemonic"));
        assert!(!serialized.contains("seed"));
    }

    fn sample_input_without_quotes() -> QuoteManagerTickInput {
        QuoteManagerTickInput {
            now_ms: 1_010_000,
            session_started_at_ms: 1_000_000,
            fair_probability: 0.23,
            edge_threshold_bps: 50.0,
            market: QuoteMarketSnapshot {
                market: "btc-updown-15m-test".to_string(),
                token_id: "token-up".to_string(),
                best_bid: Some(0.19),
                best_ask: Some(0.21),
                spread: Some(0.02),
                last_trade_price: Some(0.20),
                tick_size: Some(0.01),
                status: QuoteMarketStatus::Open,
                time_remaining_seconds: Some(900),
                book_age_ms: Some(100),
                reference_age_ms: Some(100),
            },
            own_open_quotes: Vec::new(),
            own_inventory: 0.0,
            risk: QuoteRiskSnapshot {
                max_open_orders: 1,
                max_live_orders_for_approval: 1,
                open_orders_for_approval: 0,
                replacements_used_for_approval: 0,
                risk_limits_changed: false,
                inventory_changed: false,
                heartbeat_fresh: true,
            },
            rate_limits: QuoteRateLimitSnapshot::empty(),
            reconciliation_status: QuoteReconciliationStatus::Clean,
            policy: QuoteManagerPolicy::default(),
            proposed_quote: Some(QuoteProposal {
                intent_id: "intent-1".to_string(),
                market: "btc-updown-15m-test".to_string(),
                token_id: "token-up".to_string(),
                side: Side::Buy,
                price: 0.19,
                size: 5.0,
                fair_probability: 0.23,
                edge_bps: 400.0,
            }),
        }
    }

    fn sample_input_with_quote() -> QuoteManagerTickInput {
        let mut input = sample_input_without_quotes();
        input.own_open_quotes = vec![sample_quote()];
        input.risk.open_orders_for_approval = 1;
        input
    }

    fn sample_quote() -> LiveQuoteState {
        LiveQuoteState {
            quote_id: "quote-1".to_string(),
            intent_id: "intent-0".to_string(),
            order_id: Some(exact_order_id()),
            market: "btc-updown-15m-test".to_string(),
            token_id: "token-up".to_string(),
            side: Side::Buy,
            price: 0.19,
            size: 5.0,
            fair_probability_at_submit: 0.23,
            edge_bps_at_submit: 400.0,
            submitted_at_ms: 1_000_000,
            last_validated_at_ms: 1_000_000,
            cancel_requested_at_ms: None,
            replaced_by_quote_id: None,
            status: QuoteStatus::Open,
        }
    }

    fn exact_order_id() -> String {
        "0x1111111111111111111111111111111111111111111111111111111111111111".to_string()
    }

    fn valid_approval_artifact() -> String {
        r#"# Live Alpha LA6 Approval Artifact

Status: LA6 APPROVED FOR THIS RUN ONLY
Execution Gate Status: READY

| Field | Value |
| --- | --- |
| approval_id | `LA6-approval-1` |
| approved_wallet | `0x280ca8b14386Fe4203670538CCdE636C295d74E9` |
| approved_funder | `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159` |
| approved_markets_assets | `BTC/ETH/SOL only` |
| max_orders | `1` |
| max_replacements | `1` |
| max_duration_sec | `300` |
| ttl_seconds | `30` |
| gtd_policy | `post-only GTD now+60+ttl` |
| cancel_policy | `exact order ID only` |
| no_trade_window_policy | `TTL-bound exit` |
| risk_limits | `max_orders=1 max_replacements=1 max_duration_sec=300` |
| rollback_owner | `Jonah / operator` |
| monitoring_owner | `Jonah / operator` |
| authenticated_readback_evidence | `readback-run-1` |
| operator_approval_timestamp | `2026-05-06T22:00:00-07:00` |
| available_pusd_units | `6314318` |
| reserved_pusd_units | `0` |
| open_order_count | `0` |
| trade_count | `23` |
| heartbeat_status | `not_started_no_open_orders` |
| funder_allowance_units | `18446744073709551615` |
"#
        .to_string()
    }
}
