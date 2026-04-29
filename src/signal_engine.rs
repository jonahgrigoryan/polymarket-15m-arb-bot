use crate::config::StrategyConfig;
use crate::domain::{
    is_asset_matched_chainlink_resolution_source, FeeParameters, Market, MarketLifecycleState,
    OrderKind, PaperOrderIntent, ReferencePrice, Side, SignalDecision,
};
use crate::state::{BookFreshness, DecisionSnapshot, PriceLevelSnapshot, TokenBookSnapshot};

pub const MODULE: &str = "signal_engine";

const DEFAULT_MIN_EDGE_BPS: f64 = 50.0;
const DEFAULT_LATENCY_BUFFER_BPS: f64 = 5.0;
const DEFAULT_ADVERSE_SELECTION_BPS: f64 = 25.0;
const DEFAULT_BASE_ORDER_SIZE: f64 = 10.0;
const DEFAULT_FAIR_PROBABILITY_SLOPE: f64 = 10.0;
const DEFAULT_OPENING_PHASE_MS: i64 = 60_000;
const DEFAULT_LATE_PHASE_MS: i64 = 120_000;
const DEFAULT_FINAL_SECONDS_NO_TRADE_MS: i64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalEngineConfig {
    pub min_edge_bps: f64,
    pub latency_buffer_bps: f64,
    pub adverse_selection_bps: f64,
    pub base_order_size: f64,
    pub fair_probability_slope: f64,
    pub opening_phase_ms: i64,
    pub late_phase_ms: i64,
    pub final_seconds_no_trade_ms: i64,
}

impl Default for SignalEngineConfig {
    fn default() -> Self {
        Self {
            min_edge_bps: DEFAULT_MIN_EDGE_BPS,
            latency_buffer_bps: DEFAULT_LATENCY_BUFFER_BPS,
            adverse_selection_bps: DEFAULT_ADVERSE_SELECTION_BPS,
            base_order_size: DEFAULT_BASE_ORDER_SIZE,
            fair_probability_slope: DEFAULT_FAIR_PROBABILITY_SLOPE,
            opening_phase_ms: DEFAULT_OPENING_PHASE_MS,
            late_phase_ms: DEFAULT_LATE_PHASE_MS,
            final_seconds_no_trade_ms: DEFAULT_FINAL_SECONDS_NO_TRADE_MS,
        }
    }
}

impl From<&StrategyConfig> for SignalEngineConfig {
    fn from(config: &StrategyConfig) -> Self {
        Self {
            min_edge_bps: config.min_edge_bps as f64,
            latency_buffer_bps: (config.latency_buffer_ms as f64 / 50.0).max(1.0),
            adverse_selection_bps: config.adverse_selection_bps as f64,
            final_seconds_no_trade_ms: (config.final_seconds_no_trade as i64) * 1_000,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketPhase {
    Opening,
    Main,
    Late,
    FinalSeconds,
}

impl MarketPhase {
    fn edge_multiplier(self) -> f64 {
        match self {
            MarketPhase::Opening => 2.0,
            MarketPhase::Main => 1.0,
            MarketPhase::Late => 1.5,
            MarketPhase::FinalSeconds => f64::INFINITY,
        }
    }

    fn size_multiplier(self) -> f64 {
        match self {
            MarketPhase::Opening => 0.5,
            MarketPhase::Main => 1.0,
            MarketPhase::Late => 0.5,
            MarketPhase::FinalSeconds => 0.0,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            MarketPhase::Opening => "opening",
            MarketPhase::Main => "main",
            MarketPhase::Late => "late",
            MarketPhase::FinalSeconds => "final_seconds",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FairProbabilityStatus {
    Ready,
    MissingReferencePrice,
    MissingPredictivePrice,
    InvalidReferencePrice,
    InvalidPredictivePrice,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FairProbabilityEstimate {
    pub probability_up: Option<f64>,
    pub confidence: f64,
    pub status: FairProbabilityStatus,
    pub reference_source: Option<String>,
    pub predictive_source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSkipReason {
    MarketIneligible,
    MarketNotActive,
    MarketNotStarted,
    InvalidMarketTime,
    MissingResolutionSource,
    AmbiguousResolutionSource,
    FinalSeconds,
    MissingReferencePrice,
    MissingPredictivePrice,
    InvalidReferencePrice,
    InvalidPredictivePrice,
    StaleReferencePrice,
    MissingOutcomeBook,
    MissingBestBid,
    MissingBestAsk,
    StaleBook,
    UnsupportedOutcome,
    InsufficientDepth,
    MakerWouldCross,
    EdgeBelowMinimum,
}

impl SignalSkipReason {
    fn as_str(self) -> &'static str {
        match self {
            SignalSkipReason::MarketIneligible => "market_ineligible",
            SignalSkipReason::MarketNotActive => "market_not_active",
            SignalSkipReason::MarketNotStarted => "market_not_started",
            SignalSkipReason::InvalidMarketTime => "invalid_market_time",
            SignalSkipReason::MissingResolutionSource => "missing_resolution_source",
            SignalSkipReason::AmbiguousResolutionSource => "ambiguous_resolution_source",
            SignalSkipReason::FinalSeconds => "final_seconds",
            SignalSkipReason::MissingReferencePrice => "missing_reference_price",
            SignalSkipReason::MissingPredictivePrice => "missing_predictive_price",
            SignalSkipReason::InvalidReferencePrice => "invalid_reference_price",
            SignalSkipReason::InvalidPredictivePrice => "invalid_predictive_price",
            SignalSkipReason::StaleReferencePrice => "stale_reference_price",
            SignalSkipReason::MissingOutcomeBook => "missing_outcome_book",
            SignalSkipReason::MissingBestBid => "missing_best_bid",
            SignalSkipReason::MissingBestAsk => "missing_best_ask",
            SignalSkipReason::StaleBook => "stale_book",
            SignalSkipReason::UnsupportedOutcome => "unsupported_outcome",
            SignalSkipReason::InsufficientDepth => "insufficient_depth",
            SignalSkipReason::MakerWouldCross => "maker_would_cross",
            SignalSkipReason::EdgeBelowMinimum => "edge_below_minimum",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalEvaluation {
    pub decision: SignalDecision,
    pub candidate: Option<PaperOrderIntent>,
    pub fair_probability: FairProbabilityEstimate,
    pub phase: MarketPhase,
    pub skip_reasons: Vec<SignalSkipReason>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarketProbabilityEstimate {
    pub probability: f64,
    pub best_bid: f64,
    pub best_ask: f64,
    pub spread_bps: f64,
    pub top_bid_depth: f64,
    pub top_ask_depth: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpectedValueBreakdown {
    pub gross_edge_bps: f64,
    pub spread_cost_bps: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub latency_buffer_bps: f64,
    pub adverse_selection_bps: f64,
    pub net_ev_bps: f64,
    pub required_edge_bps: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpectedValueInput<'a> {
    pub order_kind: OrderKind,
    pub fair_probability: f64,
    pub market_probability: f64,
    pub execution_price: f64,
    pub slippage_bps: f64,
    pub fee_parameters: &'a FeeParameters,
    pub required_edge_bps: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct CandidateEvaluation {
    token_id: String,
    outcome: String,
    order_kind: OrderKind,
    price: f64,
    size: f64,
    market_probability: f64,
    fair_probability: f64,
    ev: ExpectedValueBreakdown,
    skip_reasons: Vec<SignalSkipReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeDirection {
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub struct SignalEngine {
    config: SignalEngineConfig,
}

impl SignalEngine {
    pub fn new(config: SignalEngineConfig) -> Self {
        Self { config }
    }

    pub fn from_strategy_config(config: &StrategyConfig) -> Self {
        Self::new(SignalEngineConfig::from(config))
    }

    pub fn config(&self) -> SignalEngineConfig {
        self.config
    }

    pub fn evaluate(&self, snapshot: &DecisionSnapshot) -> SignalEvaluation {
        let phase = classify_market_phase_with_config(
            &snapshot.market,
            snapshot.snapshot_wall_ts,
            &self.config,
        );
        let fair_probability = self.estimate_fair_probability(snapshot, phase);
        let mut global_skip_reasons = global_skip_reasons(snapshot, phase, &fair_probability);
        let mut evaluated = Vec::new();

        if !global_skip_reasons
            .iter()
            .any(|reason| is_hard_global_skip(*reason))
        {
            for outcome in &snapshot.market.outcomes {
                evaluated.push(self.evaluate_outcome(
                    snapshot,
                    phase,
                    &fair_probability,
                    &outcome.token_id,
                    &outcome.outcome,
                ));
            }
        }

        let candidate_eval = choose_candidate(&evaluated);
        let representative = candidate_eval
            .as_ref()
            .or_else(|| best_evaluated_outcome(&evaluated));

        if let Some(candidate_eval) = candidate_eval {
            let reason = format!(
                "candidate:{}:phase={}:net_ev_bps={:.2}:required_edge_bps={:.2}",
                order_kind_name(candidate_eval.order_kind),
                phase.as_str(),
                candidate_eval.ev.net_ev_bps,
                candidate_eval.ev.required_edge_bps,
            );
            let required_inputs = required_inputs();
            let decision =
                self.decision_from_candidate(snapshot, &candidate_eval, &reason, &required_inputs);
            let candidate = self.intent_from_decision(&decision, &reason, &required_inputs);

            return SignalEvaluation {
                decision,
                candidate: Some(candidate),
                fair_probability,
                phase,
                skip_reasons: Vec::new(),
            };
        }

        for evaluation in &evaluated {
            extend_unique(&mut global_skip_reasons, &evaluation.skip_reasons);
        }

        if global_skip_reasons.is_empty() {
            global_skip_reasons.push(SignalSkipReason::EdgeBelowMinimum);
        }

        let decision = self.skip_decision(
            snapshot,
            representative,
            &fair_probability,
            &global_skip_reasons,
        );

        SignalEvaluation {
            decision,
            candidate: None,
            fair_probability,
            phase,
            skip_reasons: global_skip_reasons,
        }
    }

    pub fn estimate_fair_probability(
        &self,
        snapshot: &DecisionSnapshot,
        phase: MarketPhase,
    ) -> FairProbabilityEstimate {
        let reference = match select_reference_price(snapshot) {
            Some(price) if !positive_finite(price.price) => {
                return FairProbabilityEstimate {
                    probability_up: None,
                    confidence: 0.0,
                    status: FairProbabilityStatus::InvalidReferencePrice,
                    reference_source: Some(price.source.clone()),
                    predictive_source: None,
                };
            }
            Some(price) => price,
            None => {
                return FairProbabilityEstimate {
                    probability_up: None,
                    confidence: 0.0,
                    status: FairProbabilityStatus::MissingReferencePrice,
                    reference_source: snapshot.market.resolution_source.clone(),
                    predictive_source: None,
                };
            }
        };

        let predictive = match select_predictive_price(snapshot) {
            Some(price) if !positive_finite(price.price) => {
                return FairProbabilityEstimate {
                    probability_up: None,
                    confidence: 0.0,
                    status: FairProbabilityStatus::InvalidPredictivePrice,
                    reference_source: Some(reference.source.clone()),
                    predictive_source: Some(price.source.clone()),
                };
            }
            Some(price) => price,
            None => {
                return FairProbabilityEstimate {
                    probability_up: None,
                    confidence: 0.0,
                    status: FairProbabilityStatus::MissingPredictivePrice,
                    reference_source: Some(reference.source.clone()),
                    predictive_source: None,
                };
            }
        };

        let move_fraction = (predictive.price - reference.price) / reference.price;
        let probability_up =
            clamp_probability(0.5 + (move_fraction * self.config.fair_probability_slope));

        FairProbabilityEstimate {
            probability_up: Some(probability_up),
            confidence: fair_probability_confidence(move_fraction, phase),
            status: FairProbabilityStatus::Ready,
            reference_source: Some(reference.source.clone()),
            predictive_source: Some(predictive.source.clone()),
        }
    }

    fn evaluate_outcome(
        &self,
        snapshot: &DecisionSnapshot,
        phase: MarketPhase,
        fair_probability: &FairProbabilityEstimate,
        token_id: &str,
        outcome: &str,
    ) -> CandidateEvaluation {
        let mut skip_reasons = Vec::new();
        let required_edge_bps = required_edge_bps(phase, &self.config);
        let direction = match outcome_direction(outcome) {
            Some(direction) => direction,
            None => {
                skip_reasons.push(SignalSkipReason::UnsupportedOutcome);
                return empty_evaluation(
                    token_id,
                    outcome,
                    OrderKind::Maker,
                    required_edge_bps,
                    skip_reasons,
                );
            }
        };

        let fair_probability = match probability_for_outcome(fair_probability, direction) {
            Some(probability) => probability,
            None => {
                skip_reasons.push(skip_reason_for_fair_status(fair_probability.status));
                return empty_evaluation(
                    token_id,
                    outcome,
                    OrderKind::Maker,
                    required_edge_bps,
                    skip_reasons,
                );
            }
        };

        let book = match snapshot
            .token_books
            .iter()
            .find(|book| book.token_id == token_id)
        {
            Some(book) => book,
            None => {
                skip_reasons.push(SignalSkipReason::MissingOutcomeBook);
                return empty_evaluation(
                    token_id,
                    outcome,
                    OrderKind::Maker,
                    required_edge_bps,
                    skip_reasons,
                );
            }
        };

        if is_book_stale(&snapshot.book_freshness, token_id) {
            skip_reasons.push(SignalSkipReason::StaleBook);
        }

        let market_probability = match market_probability(book) {
            Ok(probability) => probability,
            Err(reason) => {
                skip_reasons.push(reason);
                return empty_evaluation(
                    token_id,
                    outcome,
                    OrderKind::Maker,
                    required_edge_bps,
                    skip_reasons,
                );
            }
        };

        if !skip_reasons.is_empty() {
            return empty_evaluation_with_market(
                token_id,
                outcome,
                OrderKind::Maker,
                market_probability.probability,
                fair_probability,
                required_edge_bps,
                skip_reasons,
            );
        }

        let size = target_order_size(&snapshot.market, phase, &self.config);
        let maker = self.evaluate_maker(
            snapshot,
            book,
            token_id,
            outcome,
            size,
            fair_probability,
            market_probability,
            required_edge_bps,
        );

        if maker.skip_reasons.is_empty() && maker.ev.net_ev_bps >= required_edge_bps {
            return maker;
        }

        let taker = self.evaluate_taker(
            snapshot,
            book,
            token_id,
            outcome,
            size,
            fair_probability,
            market_probability,
            required_edge_bps,
        );

        if taker.skip_reasons.is_empty() && taker.ev.net_ev_bps >= required_edge_bps {
            return taker;
        }

        if maker.ev.net_ev_bps >= taker.ev.net_ev_bps {
            maker
        } else {
            taker
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_maker(
        &self,
        snapshot: &DecisionSnapshot,
        book: &TokenBookSnapshot,
        token_id: &str,
        outcome: &str,
        size: f64,
        fair_probability: f64,
        market_probability: MarketProbabilityEstimate,
        required_edge_bps: f64,
    ) -> CandidateEvaluation {
        let mut skip_reasons = Vec::new();
        let maker_price =
            book.best_bid.unwrap_or(market_probability.best_bid) + snapshot.market.tick_size;

        if maker_price >= market_probability.best_ask {
            skip_reasons.push(SignalSkipReason::MakerWouldCross);
        }

        let ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Maker,
                fair_probability,
                market_probability: market_probability.probability,
                execution_price: maker_price,
                slippage_bps: 0.0,
                fee_parameters: &snapshot.market.fee_parameters,
                required_edge_bps,
            },
            &self.config,
        );

        if ev.net_ev_bps < required_edge_bps {
            skip_reasons.push(SignalSkipReason::EdgeBelowMinimum);
        }

        CandidateEvaluation {
            token_id: token_id.to_string(),
            outcome: outcome.to_string(),
            order_kind: OrderKind::Maker,
            price: maker_price,
            size,
            market_probability: market_probability.probability,
            fair_probability,
            ev,
            skip_reasons,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_taker(
        &self,
        snapshot: &DecisionSnapshot,
        book: &TokenBookSnapshot,
        token_id: &str,
        outcome: &str,
        size: f64,
        fair_probability: f64,
        market_probability: MarketProbabilityEstimate,
        required_edge_bps: f64,
    ) -> CandidateEvaluation {
        let mut skip_reasons = Vec::new();
        let average_price = match taker_average_price(book, size) {
            Some(price) => price,
            None => {
                skip_reasons.push(SignalSkipReason::InsufficientDepth);
                market_probability.best_ask
            }
        };
        let slippage_bps =
            ((average_price - market_probability.best_ask).max(0.0) * 10_000.0).max(0.0);
        let ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Taker,
                fair_probability,
                market_probability: market_probability.probability,
                execution_price: average_price,
                slippage_bps,
                fee_parameters: &snapshot.market.fee_parameters,
                required_edge_bps,
            },
            &self.config,
        );

        if ev.net_ev_bps < required_edge_bps {
            skip_reasons.push(SignalSkipReason::EdgeBelowMinimum);
        }

        CandidateEvaluation {
            token_id: token_id.to_string(),
            outcome: outcome.to_string(),
            order_kind: OrderKind::Taker,
            price: average_price,
            size,
            market_probability: market_probability.probability,
            fair_probability,
            ev,
            skip_reasons,
        }
    }

    fn decision_from_candidate(
        &self,
        snapshot: &DecisionSnapshot,
        candidate: &CandidateEvaluation,
        reason: &str,
        required_inputs: &[String],
    ) -> SignalDecision {
        SignalDecision {
            asset: snapshot.market.asset,
            market_id: snapshot.market.market_id.clone(),
            token_id: candidate.token_id.clone(),
            outcome: candidate.outcome.clone(),
            side: Side::Buy,
            order_kind: candidate.order_kind,
            price: candidate.price,
            size: candidate.size,
            notional: candidate.price * candidate.size,
            fair_probability: candidate.fair_probability,
            market_probability: candidate.market_probability,
            expected_value_bps: candidate.ev.net_ev_bps,
            reason: reason.to_string(),
            required_inputs: required_inputs.to_vec(),
            created_ts: snapshot.snapshot_wall_ts,
        }
    }

    fn intent_from_decision(
        &self,
        decision: &SignalDecision,
        reason: &str,
        required_inputs: &[String],
    ) -> PaperOrderIntent {
        PaperOrderIntent {
            asset: decision.asset,
            market_id: decision.market_id.clone(),
            token_id: decision.token_id.clone(),
            outcome: decision.outcome.clone(),
            side: decision.side,
            order_kind: decision.order_kind,
            price: decision.price,
            size: decision.size,
            notional: decision.notional,
            fair_probability: decision.fair_probability,
            market_probability: decision.market_probability,
            expected_value_bps: decision.expected_value_bps,
            reason: reason.to_string(),
            required_inputs: required_inputs.to_vec(),
            created_ts: decision.created_ts,
        }
    }

    fn skip_decision(
        &self,
        snapshot: &DecisionSnapshot,
        representative: Option<&CandidateEvaluation>,
        fair_probability: &FairProbabilityEstimate,
        skip_reasons: &[SignalSkipReason],
    ) -> SignalDecision {
        let fallback_outcome = snapshot.market.outcomes.first();
        let token_id = representative
            .map(|candidate| candidate.token_id.clone())
            .or_else(|| fallback_outcome.map(|outcome| outcome.token_id.clone()))
            .unwrap_or_default();
        let outcome = representative
            .map(|candidate| candidate.outcome.clone())
            .or_else(|| fallback_outcome.map(|outcome| outcome.outcome.clone()))
            .unwrap_or_default();
        let order_kind = representative
            .map(|candidate| candidate.order_kind)
            .unwrap_or(OrderKind::Maker);
        let price = representative
            .map(|candidate| candidate.price)
            .unwrap_or_default();
        let size = representative
            .map(|candidate| candidate.size)
            .unwrap_or_default();
        let market_probability = representative
            .map(|candidate| candidate.market_probability)
            .unwrap_or_default();
        let fair_probability = representative
            .map(|candidate| candidate.fair_probability)
            .or(fair_probability.probability_up)
            .unwrap_or_default();
        let expected_value_bps = representative
            .map(|candidate| candidate.ev.net_ev_bps)
            .unwrap_or_default();

        SignalDecision {
            asset: snapshot.market.asset,
            market_id: snapshot.market.market_id.clone(),
            token_id,
            outcome,
            side: Side::Buy,
            order_kind,
            price,
            size,
            notional: price * size,
            fair_probability,
            market_probability,
            expected_value_bps,
            reason: skip_reason_text(skip_reasons),
            required_inputs: required_inputs(),
            created_ts: snapshot.snapshot_wall_ts,
        }
    }
}

impl Default for SignalEngine {
    fn default() -> Self {
        Self::new(SignalEngineConfig::default())
    }
}

pub fn evaluate(snapshot: &DecisionSnapshot) -> SignalEvaluation {
    SignalEngine::default().evaluate(snapshot)
}

pub fn classify_market_phase(market: &Market, now_wall_ts: i64) -> MarketPhase {
    classify_market_phase_with_config(market, now_wall_ts, &SignalEngineConfig::default())
}

fn classify_market_phase_with_config(
    market: &Market,
    now_wall_ts: i64,
    config: &SignalEngineConfig,
) -> MarketPhase {
    let elapsed_ms = now_wall_ts.saturating_sub(market.start_ts);
    let remaining_ms = market.end_ts.saturating_sub(now_wall_ts);

    if remaining_ms <= config.final_seconds_no_trade_ms {
        MarketPhase::FinalSeconds
    } else if elapsed_ms <= config.opening_phase_ms {
        MarketPhase::Opening
    } else if remaining_ms <= config.late_phase_ms {
        MarketPhase::Late
    } else {
        MarketPhase::Main
    }
}

fn global_skip_reasons(
    snapshot: &DecisionSnapshot,
    phase: MarketPhase,
    fair_probability: &FairProbabilityEstimate,
) -> Vec<SignalSkipReason> {
    let mut reasons = Vec::new();

    if snapshot.lifecycle_state != MarketLifecycleState::Active {
        reasons.push(SignalSkipReason::MarketNotActive);
    }

    if snapshot.snapshot_wall_ts < snapshot.market.start_ts {
        reasons.push(SignalSkipReason::MarketNotStarted);
    }

    if snapshot.market.ineligibility_reason.is_some() {
        reasons.push(SignalSkipReason::MarketIneligible);
    }

    if let Some(reason) = resolution_source_skip_reason(&snapshot.market) {
        reasons.push(reason);
    }

    if snapshot.market.end_ts <= snapshot.market.start_ts {
        reasons.push(SignalSkipReason::InvalidMarketTime);
    }

    if phase == MarketPhase::FinalSeconds {
        reasons.push(SignalSkipReason::FinalSeconds);
    }

    if let Some(reason) = fair_status_skip_reason(fair_probability.status) {
        reasons.push(reason);
    }

    if is_reference_stale(snapshot, fair_probability.reference_source.as_deref()) {
        reasons.push(SignalSkipReason::StaleReferencePrice);
    }

    reasons
}

fn resolution_source_skip_reason(market: &Market) -> Option<SignalSkipReason> {
    let Some(source) = market.resolution_source.as_deref().map(str::trim) else {
        return Some(SignalSkipReason::MissingResolutionSource);
    };
    if source.is_empty() {
        return Some(SignalSkipReason::MissingResolutionSource);
    }
    if !is_asset_matched_chainlink_resolution_source(market.asset, source) {
        return Some(SignalSkipReason::AmbiguousResolutionSource);
    }
    None
}

fn is_hard_global_skip(reason: SignalSkipReason) -> bool {
    matches!(
        reason,
        SignalSkipReason::MarketNotActive
            | SignalSkipReason::MarketIneligible
            | SignalSkipReason::MarketNotStarted
            | SignalSkipReason::InvalidMarketTime
            | SignalSkipReason::MissingResolutionSource
            | SignalSkipReason::AmbiguousResolutionSource
            | SignalSkipReason::FinalSeconds
            | SignalSkipReason::MissingReferencePrice
            | SignalSkipReason::MissingPredictivePrice
            | SignalSkipReason::InvalidReferencePrice
            | SignalSkipReason::InvalidPredictivePrice
            | SignalSkipReason::StaleReferencePrice
    )
}

fn select_reference_price(snapshot: &DecisionSnapshot) -> Option<&ReferencePrice> {
    let source = snapshot.market.resolution_source.as_deref()?;
    snapshot
        .reference_prices
        .iter()
        .find(|price| price.source.eq_ignore_ascii_case(source))
}

fn select_predictive_price(snapshot: &DecisionSnapshot) -> Option<&ReferencePrice> {
    snapshot.predictive_prices.iter().max_by(|left, right| {
        left.recv_wall_ts
            .cmp(&right.recv_wall_ts)
            .then_with(|| left.source.cmp(&right.source))
    })
}

fn fair_probability_confidence(move_fraction: f64, phase: MarketPhase) -> f64 {
    let phase_multiplier = match phase {
        MarketPhase::Opening => 0.75,
        MarketPhase::Main => 1.0,
        MarketPhase::Late => 0.85,
        MarketPhase::FinalSeconds => 0.0,
    };
    let move_confidence = 0.55 + (move_fraction.abs() * 25.0).min(0.35);

    (move_confidence * phase_multiplier).clamp(0.0, 1.0)
}

fn fair_status_skip_reason(status: FairProbabilityStatus) -> Option<SignalSkipReason> {
    match status {
        FairProbabilityStatus::Ready => None,
        FairProbabilityStatus::MissingReferencePrice => {
            Some(SignalSkipReason::MissingReferencePrice)
        }
        FairProbabilityStatus::MissingPredictivePrice => {
            Some(SignalSkipReason::MissingPredictivePrice)
        }
        FairProbabilityStatus::InvalidReferencePrice => {
            Some(SignalSkipReason::InvalidReferencePrice)
        }
        FairProbabilityStatus::InvalidPredictivePrice => {
            Some(SignalSkipReason::InvalidPredictivePrice)
        }
    }
}

fn skip_reason_for_fair_status(status: FairProbabilityStatus) -> SignalSkipReason {
    fair_status_skip_reason(status).unwrap_or(SignalSkipReason::EdgeBelowMinimum)
}

fn is_reference_stale(snapshot: &DecisionSnapshot, source: Option<&str>) -> bool {
    let Some(source) = source else {
        return false;
    };

    snapshot
        .reference_freshness
        .iter()
        .any(|freshness| freshness.key.source.eq_ignore_ascii_case(source) && freshness.is_stale)
}

fn is_book_stale(freshness: &[BookFreshness], token_id: &str) -> bool {
    freshness
        .iter()
        .any(|freshness| freshness.token_id == token_id && freshness.is_stale)
}

fn outcome_direction(outcome: &str) -> Option<OutcomeDirection> {
    let lower = outcome.to_ascii_lowercase();

    if lower.contains("up") {
        Some(OutcomeDirection::Up)
    } else if lower.contains("down") {
        Some(OutcomeDirection::Down)
    } else {
        None
    }
}

fn probability_for_outcome(
    estimate: &FairProbabilityEstimate,
    direction: OutcomeDirection,
) -> Option<f64> {
    let probability_up = estimate.probability_up?;

    match direction {
        OutcomeDirection::Up => Some(probability_up),
        OutcomeDirection::Down => Some(1.0 - probability_up),
    }
}

fn market_probability(
    book: &TokenBookSnapshot,
) -> Result<MarketProbabilityEstimate, SignalSkipReason> {
    let best_bid = book
        .best_bid
        .or_else(|| book.bids.levels.first().map(|level| level.price))
        .filter(|price| probability_price(*price))
        .ok_or(SignalSkipReason::MissingBestBid)?;
    let best_ask = book
        .best_ask
        .or_else(|| book.asks.levels.first().map(|level| level.price))
        .filter(|price| probability_price(*price))
        .ok_or(SignalSkipReason::MissingBestAsk)?;
    let top_bid_depth = best_level_size(&book.bids.levels, best_bid);
    let top_ask_depth = best_level_size(&book.asks.levels, best_ask);
    let midpoint = (best_bid + best_ask) / 2.0;
    let spread = (best_ask - best_bid).max(0.0);
    let depth_total = top_bid_depth + top_ask_depth;
    let imbalance_adjustment = if depth_total > 0.0 {
        ((top_bid_depth - top_ask_depth) / depth_total) * spread * 0.25
    } else {
        0.0
    };

    Ok(MarketProbabilityEstimate {
        probability: clamp_probability(midpoint + imbalance_adjustment),
        best_bid,
        best_ask,
        spread_bps: spread * 10_000.0,
        top_bid_depth,
        top_ask_depth,
    })
}

pub fn expected_value(
    input: ExpectedValueInput<'_>,
    config: &SignalEngineConfig,
) -> ExpectedValueBreakdown {
    let gross_edge_bps = (input.fair_probability - input.market_probability) * 10_000.0;
    let spread_cost_bps = match input.order_kind {
        OrderKind::Maker => 0.0,
        OrderKind::Taker => (input.execution_price - input.market_probability).max(0.0) * 10_000.0,
    };
    let fee_bps = fee_bps(
        input.order_kind,
        input.fee_parameters,
        input.execution_price,
    );
    let net_ev_bps = gross_edge_bps
        - spread_cost_bps
        - fee_bps
        - input.slippage_bps
        - config.latency_buffer_bps
        - config.adverse_selection_bps;

    ExpectedValueBreakdown {
        gross_edge_bps,
        spread_cost_bps,
        fee_bps,
        slippage_bps: input.slippage_bps,
        latency_buffer_bps: config.latency_buffer_bps,
        adverse_selection_bps: config.adverse_selection_bps,
        net_ev_bps,
        required_edge_bps: input.required_edge_bps,
    }
}

fn fee_bps(order_kind: OrderKind, fee_parameters: &FeeParameters, execution_price: f64) -> f64 {
    if !fee_parameters.fees_enabled {
        return 0.0;
    }

    match order_kind {
        OrderKind::Maker => 0.0,
        OrderKind::Taker => raw_fee_rate(fee_parameters)
            .map(|rate| rate * execution_price * (1.0 - execution_price) * 10_000.0)
            .unwrap_or(fee_parameters.taker_fee_bps),
    }
}

fn raw_fee_rate(fee_parameters: &FeeParameters) -> Option<f64> {
    let raw = fee_parameters.raw_fee_config.as_ref()?;
    let rate = raw
        .get("r")
        .or_else(|| raw.get("rate"))
        .and_then(|value| value.as_f64())?;
    if rate.is_finite() && rate >= 0.0 {
        Some(rate)
    } else {
        None
    }
}

fn taker_average_price(book: &TokenBookSnapshot, size: f64) -> Option<f64> {
    if !positive_finite(size) {
        return None;
    }

    let mut remaining = size;
    let mut notional = 0.0;

    for level in &book.asks.levels {
        if remaining <= 0.0 {
            break;
        }

        if !probability_price(level.price) || !positive_finite(level.size) {
            continue;
        }

        let fill_size = remaining.min(level.size);
        notional += fill_size * level.price;
        remaining -= fill_size;
    }

    if remaining > 0.0 {
        None
    } else {
        Some(notional / size)
    }
}

fn target_order_size(market: &Market, phase: MarketPhase, config: &SignalEngineConfig) -> f64 {
    let phase_size = config.base_order_size * phase.size_multiplier();
    phase_size.max(market.min_order_size)
}

fn required_edge_bps(phase: MarketPhase, config: &SignalEngineConfig) -> f64 {
    config.min_edge_bps * phase.edge_multiplier()
}

fn best_level_size(levels: &[PriceLevelSnapshot], price: f64) -> f64 {
    levels
        .iter()
        .find(|level| level.price == price)
        .map(|level| level.size)
        .unwrap_or_default()
}

fn choose_candidate(evaluated: &[CandidateEvaluation]) -> Option<CandidateEvaluation> {
    let maker = evaluated
        .iter()
        .filter(|candidate| {
            candidate.order_kind == OrderKind::Maker
                && candidate.skip_reasons.is_empty()
                && candidate.ev.net_ev_bps >= candidate.ev.required_edge_bps
        })
        .max_by(|left, right| left.ev.net_ev_bps.total_cmp(&right.ev.net_ev_bps));

    if let Some(candidate) = maker {
        return Some(candidate.clone());
    }

    evaluated
        .iter()
        .filter(|candidate| {
            candidate.order_kind == OrderKind::Taker
                && candidate.skip_reasons.is_empty()
                && candidate.ev.net_ev_bps >= candidate.ev.required_edge_bps
        })
        .max_by(|left, right| left.ev.net_ev_bps.total_cmp(&right.ev.net_ev_bps))
        .cloned()
}

fn best_evaluated_outcome(evaluated: &[CandidateEvaluation]) -> Option<&CandidateEvaluation> {
    evaluated
        .iter()
        .max_by(|left, right| left.ev.net_ev_bps.total_cmp(&right.ev.net_ev_bps))
}

fn empty_evaluation(
    token_id: &str,
    outcome: &str,
    order_kind: OrderKind,
    required_edge_bps: f64,
    skip_reasons: Vec<SignalSkipReason>,
) -> CandidateEvaluation {
    empty_evaluation_with_market(
        token_id,
        outcome,
        order_kind,
        0.0,
        0.0,
        required_edge_bps,
        skip_reasons,
    )
}

fn empty_evaluation_with_market(
    token_id: &str,
    outcome: &str,
    order_kind: OrderKind,
    market_probability: f64,
    fair_probability: f64,
    required_edge_bps: f64,
    skip_reasons: Vec<SignalSkipReason>,
) -> CandidateEvaluation {
    CandidateEvaluation {
        token_id: token_id.to_string(),
        outcome: outcome.to_string(),
        order_kind,
        price: 0.0,
        size: 0.0,
        market_probability,
        fair_probability,
        ev: ExpectedValueBreakdown {
            gross_edge_bps: 0.0,
            spread_cost_bps: 0.0,
            fee_bps: 0.0,
            slippage_bps: 0.0,
            latency_buffer_bps: 0.0,
            adverse_selection_bps: 0.0,
            net_ev_bps: f64::NEG_INFINITY,
            required_edge_bps,
        },
        skip_reasons,
    }
}

fn required_inputs() -> Vec<String> {
    vec![
        "active_market".to_string(),
        "resolution_reference_price".to_string(),
        "predictive_price".to_string(),
        "fresh_book".to_string(),
        "best_bid_ask_depth".to_string(),
        "fee_parameters".to_string(),
    ]
}

fn skip_reason_text(reasons: &[SignalSkipReason]) -> String {
    let reason_list = reasons
        .iter()
        .map(|reason| reason.as_str())
        .collect::<Vec<_>>()
        .join(",");
    format!("skip:{reason_list}")
}

fn extend_unique(target: &mut Vec<SignalSkipReason>, source: &[SignalSkipReason]) {
    for reason in source {
        if !target.contains(reason) {
            target.push(*reason);
        }
    }
}

fn order_kind_name(order_kind: OrderKind) -> &'static str {
    match order_kind {
        OrderKind::Maker => "maker",
        OrderKind::Taker => "taker",
    }
}

fn positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn probability_price(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value < 1.0
}

fn clamp_probability(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Asset, FeeParameters, Market, OutcomeToken, ReferencePrice};
    use crate::events::{EventType, NormalizedEvent};
    use crate::risk_engine::{RiskContext, RiskEngine, RiskLimits};
    use crate::state::{AssetPriceKey, BookSideSnapshot, PositionSnapshot, ReferenceFreshness};

    const MARKET_ID: &str = "market-1";
    const UP_TOKEN_ID: &str = "token-up";
    const DOWN_TOKEN_ID: &str = "token-down";
    const START_TS: i64 = 1_777_000_000_000;
    const END_TS: i64 = START_TS + 900_000;

    #[test]
    fn controlled_fair_probability_uses_resolution_reference_and_predictive_price() {
        let engine = SignalEngine::new(test_config());
        let snapshot = decision_snapshot(START_TS + 120_000, vec![book(0.49, 0.51)], vec![]);

        let estimate = engine.estimate_fair_probability(&snapshot, MarketPhase::Main);

        assert_eq!(estimate.status, FairProbabilityStatus::Ready);
        assert_eq!(estimate.reference_source, Some(resolution_source()));
        assert_eq!(estimate.predictive_source, Some("binance".to_string()));
        assert_close(estimate.probability_up.unwrap(), 0.60);
        assert!(estimate.confidence > 0.55);
    }

    #[test]
    fn fresh_pyth_proxy_reference_tick_proceeds_past_missing_reference() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 120_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.reference_prices[0].provider = Some("pyth".to_string());
        snapshot.reference_prices[0].matches_market_resolution_source = Some(false);

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_some());
        assert!(!evaluation
            .skip_reasons
            .contains(&SignalSkipReason::MissingReferencePrice));
        assert_eq!(
            evaluation.fair_probability.reference_source,
            Some(resolution_source())
        );
    }

    #[test]
    fn stale_pyth_proxy_reference_tick_fails_closed() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 120_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.reference_prices[0].provider = Some("pyth".to_string());
        snapshot.reference_prices[0].matches_market_resolution_source = Some(false);
        snapshot.reference_freshness[0].age_ms = Some(1_001);
        snapshot.reference_freshness[0].is_stale = true;

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::StaleReferencePrice));
    }

    #[test]
    fn maker_and_taker_ev_include_fees_spread_slippage_and_buffers() {
        let config = SignalEngineConfig {
            min_edge_bps: 50.0,
            latency_buffer_bps: 10.0,
            adverse_selection_bps: 20.0,
            ..test_config()
        };
        let fee_parameters = FeeParameters {
            fees_enabled: true,
            maker_fee_bps: 0.0,
            taker_fee_bps: 200.0,
            raw_fee_config: None,
        };

        let maker_ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Maker,
                fair_probability: 0.56,
                market_probability: 0.50,
                execution_price: 0.50,
                slippage_bps: 0.0,
                fee_parameters: &fee_parameters,
                required_edge_bps: 50.0,
            },
            &config,
        );
        let taker_ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Taker,
                fair_probability: 0.56,
                market_probability: 0.50,
                execution_price: 0.51,
                slippage_bps: 25.0,
                fee_parameters: &fee_parameters,
                required_edge_bps: 50.0,
            },
            &config,
        );

        assert_close(maker_ev.net_ev_bps, 570.0);
        assert_close(taker_ev.spread_cost_bps, 100.0);
        assert_close(taker_ev.fee_bps, 200.0);
        assert_close(taker_ev.slippage_bps, 25.0);
        assert_close(taker_ev.net_ev_bps, 245.0);
    }

    #[test]
    fn crypto_taker_fee_uses_raw_fee_formula_and_maker_stays_zero() {
        let config = SignalEngineConfig {
            latency_buffer_bps: 0.0,
            adverse_selection_bps: 0.0,
            ..test_config()
        };
        let fee_parameters = FeeParameters {
            fees_enabled: true,
            maker_fee_bps: 1_000.0,
            taker_fee_bps: 1_000.0,
            raw_fee_config: Some(serde_json::json!({"r": 0.072, "e": 1, "to": true})),
        };

        let maker_ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Maker,
                fair_probability: 0.56,
                market_probability: 0.50,
                execution_price: 0.50,
                slippage_bps: 0.0,
                fee_parameters: &fee_parameters,
                required_edge_bps: 50.0,
            },
            &config,
        );
        let taker_ev = expected_value(
            ExpectedValueInput {
                order_kind: OrderKind::Taker,
                fair_probability: 0.56,
                market_probability: 0.50,
                execution_price: 0.50,
                slippage_bps: 0.0,
                fee_parameters: &fee_parameters,
                required_edge_bps: 50.0,
            },
            &config,
        );

        assert_close(maker_ev.fee_bps, 0.0);
        assert_close(taker_ev.fee_bps, 180.0);
        assert_close(taker_ev.net_ev_bps, 420.0);
    }

    #[test]
    fn phase_classification_covers_opening_main_late_and_final_seconds() {
        let market = sample_market();

        assert_eq!(
            classify_market_phase(&market, START_TS + 30_000),
            MarketPhase::Opening
        );
        assert_eq!(
            classify_market_phase(&market, START_TS + 300_000),
            MarketPhase::Main
        );
        assert_eq!(
            classify_market_phase(&market, END_TS - 60_000),
            MarketPhase::Late
        );
        assert_eq!(
            classify_market_phase(&market, END_TS - 10_000),
            MarketPhase::FinalSeconds
        );
    }

    #[test]
    fn outputs_maker_candidate_and_normalized_event_compatible_decision() {
        let engine = SignalEngine::new(test_config());
        let snapshot = decision_snapshot(START_TS + 300_000, vec![book(0.49, 0.51)], vec![]);

        let evaluation = engine.evaluate(&snapshot);
        let candidate = evaluation.candidate.unwrap();

        assert!(evaluation.skip_reasons.is_empty());
        assert_eq!(candidate.order_kind, OrderKind::Maker);
        assert_eq!(candidate.side, Side::Buy);
        assert_eq!(candidate.token_id, UP_TOKEN_ID);
        assert_eq!(candidate.outcome, "Up");
        assert_close(candidate.price, 0.50);
        assert!(candidate.expected_value_bps > 50.0);

        let event = NormalizedEvent::SignalUpdate {
            decision: evaluation.decision,
        };
        assert_eq!(event.event_type(), EventType::SignalUpdate);
    }

    #[test]
    fn outputs_taker_candidate_only_when_maker_would_cross() {
        let engine = SignalEngine::new(SignalEngineConfig {
            min_edge_bps: 50.0,
            latency_buffer_bps: 0.0,
            adverse_selection_bps: 0.0,
            ..test_config()
        });
        let snapshot = decision_snapshot(
            START_TS + 300_000,
            vec![book_with_levels(
                vec![level(0.50, 10.0)],
                vec![level(0.51, 10.0), level(0.52, 10.0)],
            )],
            vec![],
        );

        let evaluation = engine.evaluate(&snapshot);
        let candidate = evaluation.candidate.unwrap();

        assert_eq!(candidate.order_kind, OrderKind::Taker);
        assert_eq!(candidate.token_id, UP_TOKEN_ID);
        assert_close(candidate.price, 0.51);
        assert!(candidate.expected_value_bps >= 50.0);
    }

    #[test]
    fn skip_cases_report_explicit_reasons() {
        let engine = SignalEngine::new(test_config());
        let stale_book = BookFreshness::from_last_recv(
            MARKET_ID,
            UP_TOKEN_ID,
            START_TS + 1_000,
            START_TS + 300_000,
            1_000,
        );
        let snapshot =
            decision_snapshot(START_TS + 300_000, vec![book(0.57, 0.59)], vec![stale_book]);

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::StaleBook));
    }

    #[test]
    fn final_seconds_skip_prevents_new_candidate() {
        let engine = SignalEngine::new(test_config());
        let snapshot = decision_snapshot(END_TS - 5_000, vec![book(0.49, 0.51)], vec![]);

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert_eq!(evaluation.phase, MarketPhase::FinalSeconds);
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::FinalSeconds));
    }

    #[test]
    fn pre_start_market_is_a_hard_skip() {
        let engine = SignalEngine::new(test_config());
        let snapshot = decision_snapshot(START_TS - 5_000, vec![book(0.49, 0.51)], vec![]);

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::MarketNotStarted));
    }

    #[test]
    fn missing_predictive_price_is_a_hard_skip() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 300_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.predictive_prices.clear();

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert_eq!(
            evaluation.fair_probability.status,
            FairProbabilityStatus::MissingPredictivePrice
        );
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::MissingPredictivePrice));
    }

    #[test]
    fn missing_resolution_source_is_a_hard_skip() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 300_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.market.resolution_source = None;

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::MissingResolutionSource));
    }

    #[test]
    fn asset_mismatched_resolution_source_is_a_hard_skip() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 300_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.market.resolution_source =
            Some(Asset::Eth.chainlink_resolution_source().to_string());

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::AmbiguousResolutionSource));
    }

    #[test]
    fn ineligible_market_is_a_hard_skip() {
        let engine = SignalEngine::new(test_config());
        let mut snapshot = decision_snapshot(START_TS + 300_000, vec![book(0.49, 0.51)], vec![]);
        snapshot.market.lifecycle_state = MarketLifecycleState::Ineligible;
        snapshot.lifecycle_state = MarketLifecycleState::Ineligible;
        snapshot.market.ineligibility_reason = Some("ambiguous resolution rules".to_string());

        let evaluation = engine.evaluate(&snapshot);

        assert!(evaluation.candidate.is_none());
        assert!(evaluation
            .skip_reasons
            .contains(&SignalSkipReason::MarketIneligible));
    }

    #[test]
    fn paper_intent_remains_candidate_until_risk_gate_approves() {
        let engine = SignalEngine::new(test_config());
        let snapshot = decision_snapshot(
            START_TS + 300_000,
            vec![
                token_book_with_prices(UP_TOKEN_ID, 0.49, 0.51),
                token_book_with_prices(DOWN_TOKEN_ID, 0.48, 0.52),
            ],
            vec![
                fresh_book(UP_TOKEN_ID, START_TS + 300_000),
                fresh_book(DOWN_TOKEN_ID, START_TS + 300_000),
            ],
        );
        let evaluation = engine.evaluate(&snapshot);
        let candidate = evaluation.candidate.expect("signal produces intent");

        let risk_decision =
            RiskEngine::new(risk_limits()).evaluate(&candidate, &snapshot, &RiskContext::default());

        assert!(
            risk_decision.approved,
            "risk violations: {:?}",
            risk_decision.violations
        );
        assert!(!risk_decision.risk_state.halted);
        assert_eq!(candidate.market_id, evaluation.decision.market_id);
        assert_eq!(candidate.token_id, evaluation.decision.token_id);
    }

    fn test_config() -> SignalEngineConfig {
        SignalEngineConfig {
            min_edge_bps: 50.0,
            latency_buffer_bps: 0.0,
            adverse_selection_bps: 0.0,
            base_order_size: 10.0,
            fair_probability_slope: 10.0,
            opening_phase_ms: DEFAULT_OPENING_PHASE_MS,
            late_phase_ms: DEFAULT_LATE_PHASE_MS,
            final_seconds_no_trade_ms: DEFAULT_FINAL_SECONDS_NO_TRADE_MS,
        }
    }

    fn risk_limits() -> RiskLimits {
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

    fn fresh_book(token_id: &str, now_wall_ts: i64) -> BookFreshness {
        BookFreshness::from_last_recv(MARKET_ID, token_id, now_wall_ts - 100, now_wall_ts, 1_000)
    }

    fn decision_snapshot(
        now_wall_ts: i64,
        token_books: Vec<TokenBookSnapshot>,
        book_freshness: Vec<BookFreshness>,
    ) -> DecisionSnapshot {
        DecisionSnapshot {
            market: sample_market(),
            lifecycle_state: MarketLifecycleState::Active,
            token_books,
            book_freshness,
            reference_prices: vec![ReferencePrice {
                asset: Asset::Btc,
                source: resolution_source(),
                price: 100.0,
                confidence: None,
                provider: None,
                matches_market_resolution_source: None,
                source_ts: Some(now_wall_ts - 100),
                recv_wall_ts: now_wall_ts - 90,
            }],
            predictive_prices: vec![ReferencePrice {
                asset: Asset::Btc,
                source: "binance".to_string(),
                price: 101.0,
                confidence: None,
                provider: None,
                matches_market_resolution_source: None,
                source_ts: Some(now_wall_ts - 50),
                recv_wall_ts: now_wall_ts - 40,
            }],
            positions: Vec::<PositionSnapshot>::new(),
            reference_freshness: vec![ReferenceFreshness::from_last_recv(
                AssetPriceKey::new(Asset::Btc, resolution_source()),
                now_wall_ts - 90,
                now_wall_ts,
                1_000,
            )],
            snapshot_wall_ts: now_wall_ts,
        }
    }

    fn sample_market() -> Market {
        Market {
            market_id: MARKET_ID.to_string(),
            slug: "btc-up-down-15m".to_string(),
            title: "BTC Up or Down".to_string(),
            asset: Asset::Btc,
            condition_id: "condition-1".to_string(),
            outcomes: vec![
                OutcomeToken {
                    token_id: UP_TOKEN_ID.to_string(),
                    outcome: "Up".to_string(),
                },
                OutcomeToken {
                    token_id: DOWN_TOKEN_ID.to_string(),
                    outcome: "Down".to_string(),
                },
            ],
            start_ts: START_TS,
            end_ts: END_TS,
            resolution_source: Some(resolution_source()),
            tick_size: 0.01,
            min_order_size: 5.0,
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps: 0.0,
                raw_fee_config: None,
            },
            lifecycle_state: MarketLifecycleState::Active,
            ineligibility_reason: None,
        }
    }

    fn book(best_bid: f64, best_ask: f64) -> TokenBookSnapshot {
        token_book_with_prices(UP_TOKEN_ID, best_bid, best_ask)
    }

    fn token_book_with_prices(token_id: &str, best_bid: f64, best_ask: f64) -> TokenBookSnapshot {
        book_with_levels_for_token(
            token_id,
            vec![level(best_bid, 10.0)],
            vec![level(best_ask, 10.0)],
        )
    }

    fn book_with_levels(
        bids: Vec<PriceLevelSnapshot>,
        asks: Vec<PriceLevelSnapshot>,
    ) -> TokenBookSnapshot {
        book_with_levels_for_token(UP_TOKEN_ID, bids, asks)
    }

    fn book_with_levels_for_token(
        token_id: &str,
        bids: Vec<PriceLevelSnapshot>,
        asks: Vec<PriceLevelSnapshot>,
    ) -> TokenBookSnapshot {
        let best_bid = bids.first().map(|level| level.price);
        let best_ask = asks.first().map(|level| level.price);

        TokenBookSnapshot {
            market_id: MARKET_ID.to_string(),
            token_id: token_id.to_string(),
            bids: side(bids),
            asks: side(asks),
            best_bid,
            best_ask,
            spread: best_bid.zip(best_ask).map(|(bid, ask)| ask - bid),
            last_update_ts: Some(START_TS + 1),
            last_recv_wall_ts: Some(START_TS + 2),
            hash: Some("hash-1".to_string()),
            last_trade: None,
        }
    }

    fn side(levels: Vec<PriceLevelSnapshot>) -> BookSideSnapshot {
        BookSideSnapshot {
            visible_depth: levels.iter().map(|level| level.size).sum(),
            levels,
        }
    }

    fn level(price: f64, size: f64) -> PriceLevelSnapshot {
        PriceLevelSnapshot { price, size }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "actual={actual}, expected={expected}"
        );
    }

    fn resolution_source() -> String {
        Asset::Btc.chainlink_resolution_source().to_string()
    }
}
