use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::Serialize;

use crate::config::AppConfig;
use crate::domain::{
    Asset, Market, MarketLifecycleState, OrderKind, PaperFill, PaperOrder, PaperOrderIntent,
    PaperOrderStatus, RiskHaltReason,
};
use crate::events::{EventEnvelope, NormalizedEvent};
use crate::execution_intent::ExecutionIntent;
use crate::live_alpha_config::LiveAlphaMode;
use crate::live_executor::{
    ExecutionDecision, ExecutionSink, ShadowLiveContext, ShadowLiveDecision, ShadowLiveExecution,
    ShadowLiveReasonCode,
};
use crate::paper_executor::{
    FillSimulationInput, MarketSettlement, PaperExecutionAuditEvent, PaperExecutionError,
    PaperExecutor, PaperExecutorConfig, PaperPositionBook,
};
use crate::reporting::{
    build_replay_report, PnlReport, ReplayReport, ReplayReportInput, ReplayRunMetadata,
    RiskReplayRecord, SignalReplayRecord,
};
use crate::risk_engine::{RiskContext, RiskEngine, RiskGateDecision};
use crate::signal_engine::{SignalEngine, SignalEvaluation};
use crate::state::{
    BookUpdateError, DecisionSnapshot, PositionSnapshot, StateStore, TokenBookSnapshot,
};
use crate::storage::{ConfigSnapshot, StorageBackend, StorageError};

pub const MODULE: &str = "replay";

#[derive(Debug, Clone)]
pub struct ReplayInput {
    pub run_id: String,
    pub config: AppConfig,
    pub events: Vec<EventEnvelope>,
}

impl ReplayInput {
    pub fn new(run_id: impl Into<String>, config: AppConfig, events: Vec<EventEnvelope>) -> Self {
        Self {
            run_id: run_id.into(),
            config,
            events,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Serialize)]
pub struct ReplayDeterminismCheck {
    pub passed: bool,
    pub left_fingerprint: String,
    pub right_fingerprint: String,
    pub divergence: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayRunResult {
    pub report: ReplayReport,
    pub generated_events: Vec<NormalizedEvent>,
    pub generated_paper_events: Vec<NormalizedEvent>,
    pub recorded_paper_events: Vec<NormalizedEvent>,
    pub generated_orders: Vec<PaperOrder>,
    pub generated_fills: Vec<PaperFill>,
    pub position_snapshots: Vec<PositionSnapshot>,
    pub audit_events: Vec<PaperExecutionAuditEvent>,
    pub shadow_live_decisions: Vec<ShadowLiveDecision>,
}

#[derive(Debug, Clone)]
pub struct ReplayEngine {
    config: AppConfig,
    signal_engine: SignalEngine,
    risk_engine: RiskEngine,
    shadow_live_enabled: bool,
    shadow_live_readiness: ShadowLiveRuntimeReadiness,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShadowLiveRuntimeReadiness {
    pub geoblock_passed: bool,
    pub heartbeat_healthy: bool,
    pub reconciliation_clean: bool,
}

impl ShadowLiveRuntimeReadiness {
    pub fn passed() -> Self {
        Self {
            geoblock_passed: true,
            heartbeat_healthy: true,
            reconciliation_clean: true,
        }
    }
}

impl ReplayEngine {
    pub fn new(config: AppConfig) -> Self {
        Self {
            signal_engine: SignalEngine::from_strategy_config(&config.strategy),
            risk_engine: RiskEngine::from_config(&config.risk),
            config,
            shadow_live_enabled: false,
            shadow_live_readiness: ShadowLiveRuntimeReadiness::default(),
        }
    }

    pub fn from_config(config: AppConfig) -> Self {
        Self::new(config)
    }

    pub fn with_shadow_live(mut self, enabled: bool) -> Self {
        self.shadow_live_enabled = enabled;
        self
    }

    pub fn with_shadow_live_readiness(mut self, readiness: ShadowLiveRuntimeReadiness) -> Self {
        self.shadow_live_readiness = readiness;
        self
    }

    pub fn replay_from_storage(
        &self,
        storage: &impl StorageBackend,
        run_id: &str,
    ) -> ReplayResult<ReplayRunResult> {
        let events = storage
            .read_run_events(run_id)
            .map_err(ReplayError::Storage)?;
        self.replay_events(run_id, events)
    }

    pub fn replay_from_storage_snapshot(
        storage: &impl StorageBackend,
        run_id: &str,
    ) -> ReplayResult<ReplayRunResult> {
        Self::replay_from_storage_snapshot_with_shadow(
            storage,
            run_id,
            false,
            ShadowLiveRuntimeReadiness::default(),
        )
    }

    pub fn replay_from_storage_snapshot_with_shadow(
        storage: &impl StorageBackend,
        run_id: &str,
        shadow_live_enabled: bool,
        shadow_live_readiness: ShadowLiveRuntimeReadiness,
    ) -> ReplayResult<ReplayRunResult> {
        let snapshot = storage
            .read_config_snapshot(run_id)
            .map_err(ReplayError::Storage)?
            .ok_or_else(|| ReplayError::MissingConfigSnapshot(run_id.to_string()))?;
        let config = config_from_snapshot(snapshot)?;
        ReplayEngine::new(config)
            .with_shadow_live(shadow_live_enabled)
            .with_shadow_live_readiness(shadow_live_readiness)
            .replay_from_storage(storage, run_id)
    }

    pub fn replay_events(
        &self,
        run_id: impl Into<String>,
        events: Vec<EventEnvelope>,
    ) -> ReplayResult<ReplayRunResult> {
        let run_id = run_id.into();
        let replay = ReplayExecution::new(
            &self.config,
            &self.signal_engine,
            &self.risk_engine,
            self.shadow_live_enabled,
            self.shadow_live_readiness,
        );
        replay.run(run_id, events)
    }

    pub fn check_determinism(&self, input: ReplayInput) -> ReplayResult<ReplayDeterminismCheck> {
        let engine = ReplayEngine::new(input.config);
        let left = engine.replay_events(input.run_id.clone(), input.events.clone())?;
        let right = engine.replay_events(input.run_id, input.events)?;
        Ok(compare_replay_results(&left, &right))
    }

    pub fn check_paper_event_determinism(
        &self,
        input: ReplayInput,
    ) -> ReplayResult<ReplayDeterminismCheck> {
        let engine = ReplayEngine::new(input.config);
        let result = engine.replay_events(input.run_id, input.events)?;
        compare_generated_to_recorded_paper_events(&result)
    }
}

pub type ReplayResult<T> = Result<T, ReplayError>;

#[derive(Debug)]
pub enum ReplayError {
    Storage(StorageError),
    State(BookUpdateError),
    Paper(PaperExecutionError),
    Serialize(serde_json::Error),
    MissingConfigSnapshot(String),
    ConfigSnapshot(serde_json::Error),
}

impl Display for ReplayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Storage(source) => {
                write!(formatter, "replay storage read failed: {source}")
            }
            ReplayError::State(source) => write!(formatter, "replay state update failed: {source}"),
            ReplayError::Paper(source) => {
                write!(formatter, "replay paper execution failed: {source}")
            }
            ReplayError::Serialize(source) => {
                write!(
                    formatter,
                    "replay deterministic serialization failed: {source}"
                )
            }
            ReplayError::MissingConfigSnapshot(run_id) => {
                write!(
                    formatter,
                    "replay config snapshot missing for run_id={run_id}"
                )
            }
            ReplayError::ConfigSnapshot(source) => {
                write!(formatter, "replay config snapshot parse failed: {source}")
            }
        }
    }
}

impl Error for ReplayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ReplayError::Storage(source) => Some(source),
            ReplayError::State(source) => Some(source),
            ReplayError::Paper(source) => Some(source),
            ReplayError::Serialize(source) => Some(source),
            ReplayError::MissingConfigSnapshot(_) => None,
            ReplayError::ConfigSnapshot(source) => Some(source),
        }
    }
}

impl From<BookUpdateError> for ReplayError {
    fn from(source: BookUpdateError) -> Self {
        Self::State(source)
    }
}

impl From<PaperExecutionError> for ReplayError {
    fn from(source: PaperExecutionError) -> Self {
        Self::Paper(source)
    }
}

pub fn compare_replay_results(
    left: &ReplayRunResult,
    right: &ReplayRunResult,
) -> ReplayDeterminismCheck {
    let left_fingerprint = left.report.determinism_fingerprint();
    let right_fingerprint = right.report.determinism_fingerprint();
    let passed = left_fingerprint == right_fingerprint;

    ReplayDeterminismCheck {
        passed,
        left_fingerprint,
        right_fingerprint,
        divergence: if passed {
            None
        } else {
            Some("replay report fingerprint mismatch".to_string())
        },
    }
}

pub fn compare_generated_to_recorded_paper_events(
    result: &ReplayRunResult,
) -> ReplayResult<ReplayDeterminismCheck> {
    let left_fingerprint = stable_paper_event_fingerprint(&result.generated_paper_events)?;
    let right_fingerprint = stable_paper_event_fingerprint(&result.recorded_paper_events)?;
    let passed = left_fingerprint == right_fingerprint;

    Ok(ReplayDeterminismCheck {
        passed,
        left_fingerprint,
        right_fingerprint,
        divergence: if passed {
            None
        } else {
            Some(format!(
                "paper event mismatch: generated_count={} recorded_count={}",
                result.generated_paper_events.len(),
                result.recorded_paper_events.len()
            ))
        },
    })
}

struct ReplayExecution<'a> {
    config: &'a AppConfig,
    signal_engine: &'a SignalEngine,
    risk_engine: &'a RiskEngine,
    shadow_live_enabled: bool,
    shadow_live_readiness: ShadowLiveRuntimeReadiness,
    shadow_live_executor: ShadowLiveExecution,
    state: StateStore,
    paper_executor: PaperExecutor,
    position_book: PaperPositionBook,
    markets: BTreeMap<String, Market>,
    order_timestamps_ms: Vec<i64>,
    shadow_intent_seq: u64,
    generated_events: Vec<NormalizedEvent>,
    generated_paper_events: Vec<NormalizedEvent>,
    recorded_paper_events: Vec<NormalizedEvent>,
    generated_orders: BTreeMap<String, PaperOrder>,
    generated_fills: Vec<PaperFill>,
    audit_events: Vec<PaperExecutionAuditEvent>,
    shadow_live_decisions: Vec<ShadowLiveDecision>,
    signals: Vec<SignalReplayRecord>,
    risk_decisions: Vec<RiskReplayRecord>,
}

impl<'a> ReplayExecution<'a> {
    fn new(
        config: &'a AppConfig,
        signal_engine: &'a SignalEngine,
        risk_engine: &'a RiskEngine,
        shadow_live_enabled: bool,
        shadow_live_readiness: ShadowLiveRuntimeReadiness,
    ) -> Self {
        Self {
            config,
            signal_engine,
            risk_engine,
            shadow_live_enabled,
            shadow_live_readiness,
            shadow_live_executor: ShadowLiveExecution::new(ShadowLiveContext::default()),
            state: StateStore::new(),
            paper_executor: PaperExecutor::new(PaperExecutorConfig::default()),
            position_book: PaperPositionBook::new(),
            markets: BTreeMap::new(),
            order_timestamps_ms: Vec::new(),
            shadow_intent_seq: 1,
            generated_events: Vec::new(),
            generated_paper_events: Vec::new(),
            recorded_paper_events: Vec::new(),
            generated_orders: BTreeMap::new(),
            generated_fills: Vec::new(),
            audit_events: Vec::new(),
            shadow_live_decisions: Vec::new(),
            signals: Vec::new(),
            risk_decisions: Vec::new(),
        }
    }

    fn run(mut self, run_id: String, events: Vec<EventEnvelope>) -> ReplayResult<ReplayRunResult> {
        let mut ordered_events = events;
        ordered_events
            .sort_by(|left, right| left.replay_ordering_key().cmp(&right.replay_ordering_key()));

        let metadata = replay_metadata(
            &run_id,
            &ordered_events,
            stable_fingerprint(&ordered_events)?,
            stable_fingerprint(self.config)?,
            self.config,
        );

        for envelope in &ordered_events {
            self.record_input_paper_event(envelope);
            self.remember_market(envelope);
            self.state.apply_event(envelope)?;
            self.apply_settlement(envelope);
            self.simulate_existing_order_fills(envelope)?;
            for market_id in self.market_ids_to_evaluate(envelope) {
                self.evaluate_market(&market_id, envelope.recv_wall_ts)?;
            }
            self.mark_positions(envelope.recv_wall_ts);
        }

        let generated_orders = self.generated_orders.into_values().collect::<Vec<_>>();
        let position_snapshots = self
            .position_book
            .position_snapshots(ordered_events.last().map_or(0, |event| event.recv_wall_ts));
        let report = build_replay_report(ReplayReportInput {
            metadata,
            feed_stale_after_ms: Some(self.config.feeds.stale_after_ms),
            events: ordered_events,
            signals: self.signals,
            risk_decisions: self.risk_decisions,
            paper_orders: generated_orders.clone(),
            paper_fills: self.generated_fills.clone(),
            paper_audit_events: self.audit_events.clone(),
            pnl: PnlReport::from_positions_and_fills(&position_snapshots, &self.generated_fills),
        });

        Ok(ReplayRunResult {
            report,
            generated_events: self.generated_events,
            generated_paper_events: self.generated_paper_events,
            recorded_paper_events: self.recorded_paper_events,
            generated_orders,
            generated_fills: self.generated_fills,
            position_snapshots,
            audit_events: self.audit_events,
            shadow_live_decisions: self.shadow_live_decisions,
        })
    }

    fn remember_market(&mut self, envelope: &EventEnvelope) {
        match &envelope.payload {
            NormalizedEvent::MarketDiscovered { market }
            | NormalizedEvent::MarketUpdated { market, .. } => {
                self.markets
                    .insert(market.market_id.clone(), market.clone());
            }
            _ => {}
        }
    }

    fn record_input_paper_event(&mut self, envelope: &EventEnvelope) {
        if is_paper_event(&envelope.payload) {
            self.recorded_paper_events.push(envelope.payload.clone());
        }
    }

    fn apply_settlement(&mut self, envelope: &EventEnvelope) {
        if let NormalizedEvent::MarketResolved {
            market_id,
            outcome_token_id,
            resolved_ts,
        } = &envelope.payload
        {
            let settlement = MarketSettlement::winning_token(
                market_id.clone(),
                outcome_token_id.clone(),
                "normalized_market_resolved",
                *resolved_ts,
            );
            self.position_book.settle_market(&settlement);
        }
    }

    fn simulate_existing_order_fills(&mut self, envelope: &EventEnvelope) -> ReplayResult<()> {
        let NormalizedEvent::LastTrade {
            market_id,
            token_id,
            ..
        } = &envelope.payload
        else {
            return Ok(());
        };

        let Some(book) = self.state.order_books().token_snapshot(token_id) else {
            return Ok(());
        };

        let matching_market_ids = self.market_ids_for_market_or_condition_id(market_id);
        let order_ids = self
            .paper_executor
            .orders()
            .into_iter()
            .filter(|order| {
                matching_market_ids.contains(&order.market_id)
                    && order.token_id == *token_id
                    && matches!(
                        order.status,
                        PaperOrderStatus::Open | PaperOrderStatus::PartiallyFilled
                    )
            })
            .map(|order| (order.order_id.clone(), order.market_id.clone()))
            .collect::<Vec<_>>();

        for (order_id, order_market_id) in order_ids {
            let mut book = book.clone();
            book.market_id = order_market_id;
            let result = self.paper_executor.simulate_fill(FillSimulationInput {
                order_id,
                book: book.clone(),
                last_trade: book.last_trade.clone(),
                now_ts: envelope.recv_wall_ts,
            })?;
            self.record_paper_result(result);
        }

        Ok(())
    }

    fn market_ids_to_evaluate(&self, envelope: &EventEnvelope) -> Vec<String> {
        match &envelope.payload {
            NormalizedEvent::MarketDiscovered { market }
            | NormalizedEvent::MarketUpdated { market, .. } => vec![market.market_id.clone()],
            NormalizedEvent::BookSnapshot { book } => {
                self.market_ids_for_market_or_condition_id(&book.market_id)
            }
            NormalizedEvent::BookDelta { market_id, .. }
            | NormalizedEvent::BestBidAsk { market_id, .. } => {
                self.market_ids_for_market_or_condition_id(market_id)
            }
            NormalizedEvent::ReferenceTick { price }
            | NormalizedEvent::PredictiveTick { price } => self.market_ids_for_asset(price.asset),
            NormalizedEvent::TickSizeChange { .. }
            | NormalizedEvent::MarketCreated { .. }
            | NormalizedEvent::MarketResolved { .. }
            | NormalizedEvent::LastTrade { .. }
            | NormalizedEvent::SignalUpdate { .. }
            | NormalizedEvent::PaperOrderPlaced { .. }
            | NormalizedEvent::PaperOrderCanceled { .. }
            | NormalizedEvent::PaperFill { .. }
            | NormalizedEvent::RiskHalt { .. }
            | NormalizedEvent::ReplayCheckpoint { .. } => Vec::new(),
        }
    }

    fn market_ids_for_asset(&self, asset: Asset) -> Vec<String> {
        self.markets
            .values()
            .filter(|market| market.asset == asset)
            .map(|market| market.market_id.clone())
            .collect()
    }

    fn market_ids_for_market_or_condition_id(&self, market_id: &str) -> Vec<String> {
        let matches = self
            .markets
            .values()
            .filter(|market| market.market_id == market_id || market.condition_id == market_id)
            .map(|market| market.market_id.clone())
            .collect::<Vec<_>>();
        if matches.is_empty() {
            vec![market_id.to_string()]
        } else {
            matches
        }
    }

    fn evaluate_market(&mut self, market_id: &str, now_wall_ts: i64) -> ReplayResult<()> {
        let Some(mut snapshot) = self.state.decision_snapshot(
            market_id,
            now_wall_ts,
            self.config.risk.stale_book_ms,
            self.config.risk.stale_reference_ms,
        ) else {
            return Ok(());
        };

        snapshot.positions = self.position_book.position_snapshots(now_wall_ts);
        if snapshot.lifecycle_state != MarketLifecycleState::Active {
            self.record_signal(self.signal_engine.evaluate(&snapshot));
            return Ok(());
        }

        let evaluation = self.signal_engine.evaluate(&snapshot);
        let candidate = evaluation.candidate.clone();
        self.record_signal(evaluation);

        let Some(intent) = candidate else {
            return Ok(());
        };

        let risk_decision = self
            .risk_engine
            .evaluate(&intent, &snapshot, &self.risk_context());
        self.record_risk_decision(market_id, snapshot.market.asset, &risk_decision);

        let book = matching_book_for_token(&snapshot, &intent.token_id);
        let shadow_payload = if self.shadow_live_enabled {
            Some((
                self.execution_intent_from_paper_intent(&intent, &snapshot),
                self.shadow_context_for_intent(&intent, &snapshot, &risk_decision),
            ))
        } else {
            None
        };
        let result = self.paper_executor.open_paper_order(
            intent.clone(),
            &risk_decision,
            &snapshot.market.fee_parameters,
            book.as_ref(),
            now_wall_ts,
        )?;
        if risk_decision.approved && result.order.is_some() {
            self.order_timestamps_ms.push(now_wall_ts);
        }
        self.record_paper_result(result);
        if let Some((shadow_execution_intent, shadow_context)) = shadow_payload {
            self.record_shadow_live_decision(shadow_execution_intent, shadow_context);
        }
        Ok(())
    }

    fn record_signal(&mut self, evaluation: SignalEvaluation) {
        self.signals
            .push(SignalReplayRecord::from_evaluation(&evaluation));
        self.generated_events.push(NormalizedEvent::SignalUpdate {
            decision: evaluation.decision,
        });
    }

    fn record_risk_decision(&mut self, market_id: &str, asset: Asset, decision: &RiskGateDecision) {
        self.risk_decisions
            .push(RiskReplayRecord::from_gate_decision(
                Some(market_id.to_string()),
                Some(asset),
                decision,
            ));

        if decision.approved {
            return;
        }

        self.generated_events.push(NormalizedEvent::RiskHalt {
            market_id: Some(market_id.to_string()),
            asset: Some(asset),
            risk_state: decision.risk_state.clone(),
        });
    }

    fn record_paper_result(&mut self, result: crate::paper_executor::PaperExecutionResult) {
        for audit in result.audit_events {
            self.audit_events.push(audit);
        }
        if let Some(order) = result.order {
            self.generated_orders.insert(order.order_id.clone(), order);
        }
        for fill in result.fills {
            self.position_book.apply_fill(&fill);
            self.generated_fills.push(fill);
        }
        for event in result.normalized_events {
            if matches!(
                event,
                NormalizedEvent::PaperOrderPlaced { .. }
                    | NormalizedEvent::PaperOrderCanceled { .. }
                    | NormalizedEvent::PaperFill { .. }
            ) {
                self.generated_paper_events.push(event.clone());
            }
            self.generated_events.push(event);
        }
    }

    fn record_shadow_live_decision(&mut self, intent: ExecutionIntent, context: ShadowLiveContext) {
        if !self.shadow_live_enabled {
            return;
        }

        self.shadow_live_executor.set_context(context);
        let decision = self.shadow_live_executor.handle_intent(intent);
        match decision {
            ExecutionDecision::ShadowLive(decision) => self.shadow_live_decisions.push(*decision),
            other => panic!("shadow live executor returned unexpected decision: {other:?}"),
        }
    }

    fn execution_intent_from_paper_intent(
        &mut self,
        intent: &PaperOrderIntent,
        snapshot: &DecisionSnapshot,
    ) -> ExecutionIntent {
        let sequence = self.shadow_intent_seq;
        self.shadow_intent_seq += 1;
        let book = matching_book_for_token(snapshot, &intent.token_id);
        let reference = snapshot.reference_prices.first();
        let (order_type, time_in_force, post_only) = match intent.order_kind {
            OrderKind::Maker => ("GTD", "GTD", true),
            OrderKind::Taker => ("unsupported_taker", "unsupported_taker", false),
        };

        ExecutionIntent {
            intent_id: format!("shadow-runtime-intent-{sequence}"),
            strategy_snapshot_id: format!(
                "{}:{}:{}:{}",
                snapshot.market.market_id, intent.token_id, intent.created_ts, sequence
            ),
            market_slug: snapshot.market.slug.clone(),
            condition_id: snapshot.market.condition_id.clone(),
            token_id: intent.token_id.clone(),
            asset_symbol: intent.asset.symbol().to_string(),
            asset: intent.asset,
            outcome: intent.outcome.clone(),
            side: intent.side,
            price: intent.price,
            size: intent.size,
            notional: intent.notional,
            order_type: order_type.to_string(),
            time_in_force: time_in_force.to_string(),
            post_only,
            expiry: Some(snapshot.market.end_ts / 1_000),
            fair_probability: intent.fair_probability,
            edge_bps: intent.expected_value_bps,
            reference_price: reference.map(|price| price.price).unwrap_or_default(),
            reference_source_timestamp: reference.and_then(|price| price.source_ts),
            book_snapshot_id: book
                .as_ref()
                .and_then(|book| book.hash.clone())
                .unwrap_or_default(),
            best_bid: book.as_ref().and_then(|book| book.best_bid),
            best_ask: book.as_ref().and_then(|book| book.best_ask),
            spread: book.as_ref().and_then(|book| book.spread),
            created_at: intent.created_ts,
        }
    }

    fn shadow_context_for_intent(
        &self,
        intent: &PaperOrderIntent,
        snapshot: &DecisionSnapshot,
        risk_decision: &RiskGateDecision,
    ) -> ShadowLiveContext {
        let book_fresh = book_freshness_for_intent(snapshot, &intent.token_id)
            .map(|freshness| freshness.is_fresh())
            .unwrap_or(false);
        let reference_fresh = !snapshot.reference_freshness.is_empty()
            && snapshot
                .reference_freshness
                .iter()
                .all(|freshness| !freshness.is_stale);
        let inventory_by_token = snapshot
            .positions
            .iter()
            .filter(|position| position.size > 0.0)
            .map(|position| (position.token_id.clone(), position.size))
            .collect::<BTreeMap<_, _>>();
        let open_orders = self.open_paper_orders();
        let current_market_notional: f64 = snapshot
            .positions
            .iter()
            .filter(|position| position.market_id == intent.market_id)
            .map(|position| position.size.abs() * position.average_price)
            .sum::<f64>()
            + open_orders
                .iter()
                .filter(|order| order.market_id == intent.market_id)
                .map(|order| open_order_notional(order))
                .sum::<f64>();
        let current_asset_notional: f64 = snapshot
            .positions
            .iter()
            .filter(|position| position.asset == intent.asset)
            .map(|position| position.size.abs() * position.average_price)
            .sum::<f64>()
            + open_orders
                .iter()
                .filter(|order| order.asset == intent.asset)
                .map(|order| open_order_notional(order))
                .sum::<f64>();
        let current_total_live_notional: f64 = snapshot
            .positions
            .iter()
            .map(|position| position.size.abs() * position.average_price)
            .sum::<f64>()
            + open_orders
                .iter()
                .map(|order| open_order_notional(order))
                .sum::<f64>();
        let reserved_pusd: f64 = open_orders
            .iter()
            .map(|order| open_order_reserved_pusd(order))
            .sum();
        let available_pusd = shadow_available_pusd(
            self.config.paper.starting_balance,
            self.position_book.total_realized_pnl(),
            reserved_pusd,
            &snapshot.positions,
        );

        ShadowLiveContext {
            mode_approved: self.config.live_alpha.enabled
                && self.config.live_alpha.mode == LiveAlphaMode::Shadow,
            risk_approved: risk_decision.approved,
            risk_reason_codes: shadow_reason_codes_from_risk(risk_decision),
            geoblock_passed: self.shadow_live_readiness.geoblock_passed,
            heartbeat_healthy: self.shadow_live_readiness.heartbeat_healthy,
            reconciliation_clean: self.shadow_live_readiness.reconciliation_clean,
            book_fresh,
            reference_fresh,
            now_ms: Some(snapshot.snapshot_wall_ts),
            market_end_ms: Some(snapshot.market.end_ts),
            no_trade_seconds_before_close: self
                .config
                .live_alpha
                .risk
                .no_trade_seconds_before_close,
            available_pusd,
            reserved_pusd,
            max_available_pusd_usage: self.config.live_alpha.risk.max_available_pusd_usage,
            max_reserved_pusd: self.config.live_alpha.risk.max_reserved_pusd,
            inventory_by_token,
            open_order_count: open_orders.len() as u64,
            max_open_orders: self.config.live_alpha.risk.max_open_orders,
            current_market_notional,
            max_market_notional: self.config.live_alpha.risk.max_per_market_notional,
            current_asset_notional,
            max_asset_notional: self.config.live_alpha.risk.max_per_asset_notional,
            current_total_live_notional,
            max_single_order_notional: self.config.live_alpha.risk.max_single_order_notional,
            max_total_live_notional: self.config.live_alpha.risk.max_total_live_notional,
            min_edge_bps: self.config.live_alpha.maker.min_edge_bps as f64,
            fee_parameters: snapshot.market.fee_parameters.clone(),
        }
    }

    fn open_paper_orders(&self) -> Vec<&PaperOrder> {
        self.paper_executor
            .orders()
            .into_iter()
            .filter(|order| {
                matches!(
                    order.status,
                    PaperOrderStatus::Open | PaperOrderStatus::PartiallyFilled
                )
            })
            .collect()
    }

    fn mark_positions(&mut self, updated_ts: i64) {
        for snapshot in self.position_book.position_snapshots(updated_ts) {
            let Some(book) = self.state.order_books().token_snapshot(&snapshot.token_id) else {
                continue;
            };
            let Some(mark_price) = mark_price(&book) else {
                continue;
            };
            let key = crate::paper_executor::PositionKey::new(
                snapshot.market_id,
                snapshot.token_id,
                snapshot.asset,
            );
            self.position_book.mark(&key, mark_price);
        }
    }

    fn risk_context(&self) -> RiskContext {
        RiskContext {
            geoblocked: false,
            additional_exposures: Vec::new(),
            recent_order_timestamps_ms: self.order_timestamps_ms.clone(),
            daily_realized_pnl: self.position_book.total_realized_pnl(),
            daily_unrealized_pnl: self.position_book.total_unrealized_pnl(),
        }
    }
}

fn matching_book_for_token(
    snapshot: &DecisionSnapshot,
    token_id: &str,
) -> Option<TokenBookSnapshot> {
    snapshot
        .token_books
        .iter()
        .find(|book| {
            snapshot
                .market
                .outcomes
                .iter()
                .any(|outcome| outcome.token_id == book.token_id && outcome.token_id == token_id)
        })
        .cloned()
        .map(|mut book| {
            if book.market_id == snapshot.market.condition_id {
                book.market_id.clone_from(&snapshot.market.market_id);
            }
            book
        })
}

fn book_freshness_for_intent<'a>(
    snapshot: &'a DecisionSnapshot,
    token_id: &str,
) -> Option<&'a crate::state::BookFreshness> {
    snapshot.book_freshness.iter().find(|freshness| {
        freshness.token_id == token_id
            && (freshness.market_id == snapshot.market.market_id
                || freshness.market_id == snapshot.market.condition_id)
    })
}

fn shadow_reason_codes_from_risk(decision: &RiskGateDecision) -> Vec<ShadowLiveReasonCode> {
    let mut reasons = decision
        .risk_state
        .active_halts
        .iter()
        .chain(
            decision
                .violations
                .iter()
                .map(|violation| &violation.reason),
        )
        .map(|reason| match reason {
            RiskHaltReason::Geoblocked => ShadowLiveReasonCode::GeoblockNotPassed,
            RiskHaltReason::StaleReference => ShadowLiveReasonCode::ReferenceStale,
            RiskHaltReason::StaleBook => ShadowLiveReasonCode::BookStale,
            RiskHaltReason::MaxLossPerMarket | RiskHaltReason::MaxNotionalPerMarket => {
                ShadowLiveReasonCode::MaxMarketNotionalReached
            }
            RiskHaltReason::MaxNotionalPerAsset => ShadowLiveReasonCode::MaxAssetNotionalReached,
            RiskHaltReason::MaxTotalNotional => ShadowLiveReasonCode::MaxTotalLiveNotionalReached,
            RiskHaltReason::MaxCorrelatedNotional => {
                ShadowLiveReasonCode::MaxCorrelatedNotionalReached
            }
            RiskHaltReason::OrderRateExceeded
            | RiskHaltReason::DailyDrawdown
            | RiskHaltReason::StorageUnavailable
            | RiskHaltReason::IneligibleMarket
            | RiskHaltReason::Unknown => ShadowLiveReasonCode::LiveRiskRejected,
        })
        .collect::<Vec<_>>();
    reasons.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    reasons.dedup();
    reasons
}

fn open_order_notional(order: &PaperOrder) -> f64 {
    let remaining_size = (order.size - order.filled_size).max(0.0);
    remaining_size * order.price
}

fn open_order_reserved_pusd(order: &PaperOrder) -> f64 {
    match order.side {
        crate::domain::Side::Buy => open_order_notional(order),
        crate::domain::Side::Sell => 0.0,
    }
}

fn shadow_available_pusd(
    starting_balance: f64,
    realized_pnl: f64,
    reserved_pusd: f64,
    positions: &[PositionSnapshot],
) -> f64 {
    if !starting_balance.is_finite() || !realized_pnl.is_finite() || !reserved_pusd.is_finite() {
        return 0.0;
    }

    let mut open_long_cost = 0.0;
    for position in positions.iter().filter(|position| position.size > 0.0) {
        let cost = position.size * position.average_price;
        if !cost.is_finite() || cost < 0.0 {
            return 0.0;
        }
        open_long_cost += cost;
    }

    (starting_balance + realized_pnl - open_long_cost - reserved_pusd).max(0.0)
}

fn mark_price(book: &TokenBookSnapshot) -> Option<f64> {
    match (book.best_bid, book.best_ask) {
        (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
        (Some(bid), None) => Some(bid),
        (None, Some(ask)) => Some(ask),
        (None, None) => None,
    }
}

fn is_paper_event(event: &NormalizedEvent) -> bool {
    matches!(
        event,
        NormalizedEvent::PaperOrderPlaced { .. }
            | NormalizedEvent::PaperOrderCanceled { .. }
            | NormalizedEvent::PaperFill { .. }
    )
}

fn config_from_snapshot(snapshot: ConfigSnapshot) -> ReplayResult<AppConfig> {
    serde_json::from_value(snapshot.config).map_err(ReplayError::ConfigSnapshot)
}

fn replay_metadata(
    source_run_id: &str,
    events: &[EventEnvelope],
    input_fingerprint: String,
    config_fingerprint: String,
    config: &AppConfig,
) -> ReplayRunMetadata {
    let is_pyth_proxy = config.reference_feed.is_pyth_proxy_enabled();
    let is_polymarket_rtds_chainlink = config.reference_feed.is_polymarket_rtds_chainlink_enabled();
    let is_chainlink = config.reference_feed.provider == "chainlink";
    ReplayRunMetadata {
        run_id: source_run_id.to_string(),
        replay_run_id: format!("{source_run_id}:offline-replay"),
        input_source: Some("event_envelopes".to_string()),
        input_fingerprint: Some(input_fingerprint),
        config_fingerprint: Some(config_fingerprint),
        code_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        evidence_type: evidence_type(events, config),
        live_market_evidence: live_market_evidence(events),
        started_wall_ts: events.first().map(|event| event.recv_wall_ts),
        completed_wall_ts: events.last().map(|event| event.recv_wall_ts),
        first_event_recv_wall_ts: None,
        last_event_recv_wall_ts: None,
        first_event_source_ts: None,
        last_event_source_ts: None,
        source_timestamp_regressions: count_source_timestamp_regressions(events),
        reference_feed_mode: Some(config.reference_feed.provider.clone()),
        reference_provider: if is_pyth_proxy {
            Some("pyth".to_string())
        } else if is_polymarket_rtds_chainlink {
            Some("polymarket_rtds_chainlink".to_string())
        } else if is_chainlink {
            Some("chainlink".to_string())
        } else {
            None
        },
        matches_market_resolution_source: if is_pyth_proxy {
            Some(false)
        } else if is_polymarket_rtds_chainlink || is_chainlink {
            Some(true)
        } else {
            None
        },
        live_readiness_evidence: is_chainlink,
        settlement_reference_evidence: is_chainlink || is_polymarket_rtds_chainlink,
    }
}

fn evidence_type(events: &[EventEnvelope], config: &AppConfig) -> Option<String> {
    if events
        .iter()
        .any(|event| event.source == "deterministic_fixture")
    {
        Some("deterministic_fixture".to_string())
    } else if config.reference_feed.is_pyth_proxy_enabled() {
        Some("pyth_proxy_live_ingestion".to_string())
    } else if config.reference_feed.is_polymarket_rtds_chainlink_enabled() {
        Some("polymarket_rtds_chainlink_live_ingestion".to_string())
    } else {
        None
    }
}

fn live_market_evidence(events: &[EventEnvelope]) -> Option<bool> {
    if events
        .iter()
        .any(|event| event.source == "deterministic_fixture")
    {
        Some(false)
    } else if events.iter().any(|event| {
        matches!(
            event.source.as_str(),
            "polymarket_clob" | "binance" | "coinbase" | "pyth_proxy" | "polymarket_rtds_chainlink"
        )
    }) {
        Some(true)
    } else {
        None
    }
}

fn count_source_timestamp_regressions(events: &[EventEnvelope]) -> u64 {
    let mut last_by_source = BTreeMap::<&str, i64>::new();
    let mut regressions = 0;

    for event in events {
        let Some(source_ts) = event.source_ts else {
            continue;
        };
        if let Some(previous) = last_by_source.insert(event.source.as_str(), source_ts) {
            if source_ts < previous {
                regressions += 1;
            }
        }
    }

    regressions
}

fn stable_fingerprint(value: &impl Serialize) -> ReplayResult<String> {
    let bytes = serde_json::to_vec(value).map_err(ReplayError::Serialize)?;
    Ok(format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, &bytes).as_ref())
    ))
}

fn stable_paper_event_fingerprint(events: &[NormalizedEvent]) -> ReplayResult<String> {
    let canonical = canonical_paper_events(events)?;
    stable_fingerprint(&canonical)
}

fn canonical_paper_events(events: &[NormalizedEvent]) -> ReplayResult<Vec<NormalizedEvent>> {
    let bytes = serde_json::to_vec(events).map_err(ReplayError::Serialize)?;
    serde_json::from_slice(&bytes).map_err(ReplayError::Serialize)
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FeeParameters, Market, OrderBookLevel, OrderBookSnapshot, OrderKind, OutcomeToken,
        PaperOrder, PaperOrderStatus, ReferencePrice, RiskState, Side,
    };
    use crate::events::EventType;
    use crate::live_executor::ShadowLiveReport;
    use crate::reference_feed::{
        PROVIDER_POLYMARKET_RTDS_CHAINLINK, SOURCE_POLYMARKET_RTDS_CHAINLINK, SOURCE_PYTH_PROXY,
    };
    use crate::risk_engine::RiskViolation;
    use crate::storage::{ConfigSnapshot, InMemoryStorage};

    const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");
    const RUN_ID: &str = "replay-run-1";
    const MARKET_ID: &str = "market-1";
    const UP_TOKEN_ID: &str = "token-up";
    const DOWN_TOKEN_ID: &str = "token-down";
    const START_TS: i64 = 1_777_000_000_000;

    #[test]
    fn synthetic_run_replays_deterministically_and_reports_paper_outcomes() {
        let config = config();
        let engine = ReplayEngine::new(config);
        let events = synthetic_events();

        let first = engine
            .replay_events(RUN_ID, events.clone())
            .expect("first replay succeeds");
        let second = engine
            .replay_events(RUN_ID, events)
            .expect("second replay succeeds");
        let check = compare_replay_results(&first, &second);

        assert!(check.passed);
        assert_eq!(first.report.events.total_count, 6);
        assert_eq!(
            first
                .report
                .events
                .counts_by_type
                .get(EventType::BookSnapshot.as_str()),
            Some(&2)
        );
        assert!(first.report.signals.evaluated_count >= 1);
        assert_eq!(first.report.risk.approval_count, 1);
        assert_eq!(first.report.paper.fill_count, 1);
        assert_eq!(first.generated_fills.len(), 1);
        assert!(first.report.pnl.totals.fees_paid >= 0.0);
        assert!(!first.report.determinism_fingerprint().is_empty());
    }

    #[test]
    fn pyth_proxy_reference_ticks_replay_deterministically_with_proxy_labels() {
        let mut config = config();
        config.reference_feed.provider = "pyth_proxy".to_string();
        config.reference_feed.pyth_enabled = true;
        let engine = ReplayEngine::new(config);
        let events = pyth_proxy_synthetic_events();

        let first = engine
            .replay_events(RUN_ID, events.clone())
            .expect("first proxy replay succeeds");
        let second = engine
            .replay_events(RUN_ID, events)
            .expect("second proxy replay succeeds");
        let check = compare_replay_results(&first, &second);

        assert!(check.passed);
        assert_eq!(
            first
                .report
                .events
                .counts_by_type
                .get(EventType::ReferenceTick.as_str()),
            Some(&1)
        );
        assert_eq!(
            first.report.metadata.reference_feed_mode.as_deref(),
            Some("pyth_proxy")
        );
        assert_eq!(
            first.report.metadata.reference_provider.as_deref(),
            Some("pyth")
        );
        assert_eq!(
            first.report.metadata.matches_market_resolution_source,
            Some(false)
        );
        assert!(!first.report.metadata.live_readiness_evidence);
        assert!(!first.report.metadata.settlement_reference_evidence);
        assert!(first.report.signals.emitted_order_intent_count > 0);
        assert!(
            first
                .report
                .signals
                .skip_reason_counts
                .get("missing_reference_price")
                .copied()
                .unwrap_or_default()
                < first.report.signals.evaluated_count
        );
    }

    #[test]
    fn polymarket_rtds_chainlink_reference_ticks_replay_with_settlement_source_labels() {
        let mut config = config();
        config.reference_feed.provider = "polymarket_rtds_chainlink".to_string();
        let engine = ReplayEngine::new(config);
        let events = polymarket_rtds_chainlink_synthetic_events();

        let first = engine
            .replay_events(RUN_ID, events.clone())
            .expect("first RTDS Chainlink replay succeeds");
        let second = engine
            .replay_events(RUN_ID, events)
            .expect("second RTDS Chainlink replay succeeds");
        let check = compare_replay_results(&first, &second);

        assert!(check.passed);
        assert_eq!(
            first.report.metadata.evidence_type.as_deref(),
            Some("polymarket_rtds_chainlink_live_ingestion")
        );
        assert_eq!(
            first.report.metadata.reference_feed_mode.as_deref(),
            Some("polymarket_rtds_chainlink")
        );
        assert_eq!(
            first.report.metadata.reference_provider.as_deref(),
            Some(PROVIDER_POLYMARKET_RTDS_CHAINLINK)
        );
        assert_eq!(
            first.report.metadata.matches_market_resolution_source,
            Some(true)
        );
        assert!(first.report.metadata.settlement_reference_evidence);
        assert!(!first.report.metadata.live_readiness_evidence);
        assert!(first.report.signals.emitted_order_intent_count > 0);
    }

    #[test]
    fn fresh_book_after_reference_tick_evaluates_without_stale_book_skip() {
        let asset = Asset::Btc;
        let up_token_id = asset_up_token_id(asset);
        let down_token_id = asset_down_token_id(asset);
        let decision_ts = START_TS + 320_000;
        let events = vec![
            timed_envelope(
                1,
                START_TS + 300_000,
                NormalizedEvent::MarketDiscovered {
                    market: market_for_asset(asset),
                },
            ),
            timed_envelope(
                2,
                START_TS + 300_001,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &up_token_id, 0.49, 0.51),
                },
            ),
            timed_envelope(
                3,
                START_TS + 300_002,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &down_token_id, 0.49, 0.51),
                },
            ),
            timed_envelope(
                4,
                decision_ts - 2,
                NormalizedEvent::PredictiveTick {
                    price: reference_for_asset(asset, "binance", 100.0, decision_ts - 2),
                },
            ),
            timed_envelope(
                5,
                decision_ts - 1,
                NormalizedEvent::ReferenceTick {
                    price: reference_for_asset(
                        asset,
                        asset.chainlink_resolution_source(),
                        100.0,
                        decision_ts - 1,
                    ),
                },
            ),
            timed_envelope(
                6,
                decision_ts,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &up_token_id, 0.49, 0.51),
                },
            ),
            timed_envelope(
                7,
                decision_ts + 1,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &down_token_id, 0.49, 0.51),
                },
            ),
        ];

        let result = ReplayEngine::new(config())
            .replay_events(RUN_ID, events)
            .expect("fresh post-reference book replay succeeds");
        let final_signal = result
            .report
            .signals
            .decisions
            .last()
            .expect("final signal was evaluated");

        assert!(!final_signal
            .skip_reasons
            .iter()
            .any(|reason| reason == "stale_book"));
        assert_eq!(final_signal.skip_reasons, vec!["edge_below_minimum"]);
        assert_eq!(result.report.paper.order_count, 0);
    }

    #[test]
    fn condition_id_book_can_open_gamma_market_maker_order() {
        let asset = Asset::Eth;
        let up_token_id = asset_up_token_id(asset);
        let down_token_id = asset_down_token_id(asset);
        let decision_ts = START_TS + 320_000;
        let events = vec![
            timed_envelope(
                1,
                START_TS + 300_000,
                NormalizedEvent::MarketDiscovered {
                    market: market_for_asset(asset),
                },
            ),
            timed_envelope(
                2,
                decision_ts - 4,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &up_token_id, 0.47, 0.55),
                },
            ),
            timed_envelope(
                3,
                decision_ts - 3,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &down_token_id, 0.45, 0.53),
                },
            ),
            timed_envelope(
                4,
                decision_ts - 2,
                NormalizedEvent::ReferenceTick {
                    price: reference_for_asset(
                        asset,
                        asset.chainlink_resolution_source(),
                        100.0,
                        decision_ts - 2,
                    ),
                },
            ),
            timed_envelope(
                5,
                decision_ts - 1,
                NormalizedEvent::PredictiveTick {
                    price: reference_for_asset(asset, "binance", 102.0, decision_ts - 1),
                },
            ),
        ];

        let result = ReplayEngine::new(config())
            .replay_events(RUN_ID, events)
            .expect("condition-id book paper replay succeeds");

        assert_eq!(result.report.signals.emitted_order_intent_count, 1);
        assert_eq!(result.report.risk.approval_count, 1);
        assert_eq!(result.report.risk.rejection_count, 0);
        assert_eq!(result.report.paper.order_count, 1);
        assert_eq!(result.report.paper.reject_count, 0);
        assert_eq!(result.generated_orders.len(), 1);
        assert_eq!(result.generated_orders[0].market_id, asset_market_id(asset));
        assert_eq!(result.generated_orders[0].token_id, up_token_id);
        assert_eq!(result.generated_paper_events.len(), 1);
    }

    #[test]
    fn shadow_live_replay_records_decisions_without_changing_paper_outputs() {
        let config = config();
        let events = synthetic_events();

        let baseline = ReplayEngine::new(config.clone())
            .replay_events(RUN_ID, events.clone())
            .expect("baseline replay succeeds");
        let shadow = ReplayEngine::new(config)
            .with_shadow_live(true)
            .replay_events(RUN_ID, events)
            .expect("shadow replay succeeds");

        assert_eq!(baseline.generated_orders, shadow.generated_orders);
        assert_eq!(baseline.generated_fills, shadow.generated_fills);
        assert_eq!(
            baseline.generated_paper_events,
            shadow.generated_paper_events
        );
        assert!(!shadow.shadow_live_decisions.is_empty());
        assert!(shadow
            .shadow_live_decisions
            .iter()
            .all(|decision| !decision.would_cancel && !decision.would_replace));
        assert!(shadow.shadow_live_decisions.iter().all(|decision| {
            decision
                .reason_codes
                .iter()
                .any(|reason| reason == "mode_not_approved")
        }));
        assert!(shadow.shadow_live_decisions.iter().all(|decision| {
            decision
                .reason_codes
                .iter()
                .any(|reason| reason == "geoblock_not_passed")
        }));

        let report = ShadowLiveReport::from_decisions(
            &shadow.shadow_live_decisions,
            shadow.report.paper.order_count,
            shadow.report.paper.fill_count,
        );
        assert_eq!(report.paper_fill_count, shadow.report.paper.fill_count);
        assert_eq!(
            report.decision_count,
            shadow.shadow_live_decisions.len() as u64
        );
    }

    #[test]
    fn shadow_live_replay_can_would_submit_with_approved_shadow_readiness() {
        let mut config = config();
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::Shadow;
        config.live_alpha.risk.max_available_pusd_usage = 100.0;
        config.live_alpha.risk.max_reserved_pusd = 100.0;
        config.live_alpha.risk.max_single_order_notional = 100.0;
        config.live_alpha.risk.max_per_market_notional = 100.0;
        config.live_alpha.risk.max_per_asset_notional = 100.0;
        config.live_alpha.risk.max_total_live_notional = 300.0;
        config.live_alpha.risk.max_open_orders = 5;
        config.live_alpha.risk.no_trade_seconds_before_close = 60;
        config.live_alpha.maker.min_edge_bps = 0;
        let asset = Asset::Eth;
        let up_token_id = asset_up_token_id(asset);
        let down_token_id = asset_down_token_id(asset);
        let decision_ts = START_TS + 320_000;
        let events = vec![
            timed_envelope(
                1,
                START_TS + 300_000,
                NormalizedEvent::MarketDiscovered {
                    market: market_for_asset(asset),
                },
            ),
            timed_envelope(
                2,
                decision_ts - 4,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &up_token_id, 0.47, 0.55),
                },
            ),
            timed_envelope(
                3,
                decision_ts - 3,
                NormalizedEvent::BookSnapshot {
                    book: condition_book_for_asset(asset, &down_token_id, 0.45, 0.53),
                },
            ),
            timed_envelope(
                4,
                decision_ts - 2,
                NormalizedEvent::ReferenceTick {
                    price: reference_for_asset(
                        asset,
                        asset.chainlink_resolution_source(),
                        100.0,
                        decision_ts - 2,
                    ),
                },
            ),
            timed_envelope(
                5,
                decision_ts - 1,
                NormalizedEvent::PredictiveTick {
                    price: reference_for_asset(asset, "binance", 102.0, decision_ts - 1),
                },
            ),
        ];

        let result = ReplayEngine::new(config)
            .with_shadow_live(true)
            .with_shadow_live_readiness(ShadowLiveRuntimeReadiness::passed())
            .replay_events(RUN_ID, events)
            .expect("shadow replay succeeds");

        assert_eq!(result.report.paper.order_count, 1);
        assert_eq!(result.shadow_live_decisions.len(), 1);
        let decision = &result.shadow_live_decisions[0];
        assert!(decision.would_submit);
        assert!(decision.reason_codes.is_empty());
        assert!(!decision.would_cancel);
        assert!(!decision.would_replace);
    }

    #[test]
    fn shadow_reason_codes_preserve_notional_risk_halt_specificity() {
        let decision = RiskGateDecision {
            approved: false,
            violations: vec![
                RiskViolation {
                    reason: RiskHaltReason::MaxNotionalPerAsset,
                    message: "asset notional exceeded".to_string(),
                },
                RiskViolation {
                    reason: RiskHaltReason::MaxTotalNotional,
                    message: "total notional exceeded".to_string(),
                },
                RiskViolation {
                    reason: RiskHaltReason::MaxCorrelatedNotional,
                    message: "correlated notional exceeded".to_string(),
                },
            ],
            risk_state: RiskState {
                halted: true,
                active_halts: Vec::new(),
                reason: None,
                updated_ts: START_TS,
            },
        };

        let reasons = shadow_reason_codes_from_risk(&decision)
            .into_iter()
            .map(ShadowLiveReasonCode::as_str)
            .collect::<Vec<_>>();

        assert_eq!(
            reasons,
            vec![
                "max_asset_notional_reached",
                "max_correlated_notional_reached",
                "max_total_live_notional_reached",
            ]
        );
    }

    #[test]
    fn shadow_context_subtracts_filled_long_cost_from_available_pusd() {
        let asset = Asset::Btc;
        let positions = vec![PositionSnapshot {
            market_id: asset_market_id(asset),
            token_id: asset_up_token_id(asset),
            asset,
            size: 10.0,
            average_price: 0.40,
            realized_pnl: 0.0,
            unrealized_pnl: 0.0,
            updated_ts: START_TS + 20,
        }];

        assert_close(shadow_available_pusd(1_000.0, 0.0, 0.0, &positions), 996.0);
        let reduced_positions = vec![PositionSnapshot {
            size: 6.0,
            ..positions[0].clone()
        }];
        assert_close(
            shadow_available_pusd(1_000.0, 1.2, 0.0, &reduced_positions),
            998.8,
        );
    }

    #[test]
    fn deterministic_fixture_source_labels_report_metadata() {
        let engine = ReplayEngine::new(config());
        let events = synthetic_events()
            .into_iter()
            .map(|mut event| {
                event.source = "deterministic_fixture".to_string();
                event
            })
            .collect::<Vec<_>>();

        let result = engine
            .replay_events(RUN_ID, events)
            .expect("fixture replay succeeds");

        assert_eq!(
            result.report.metadata.evidence_type.as_deref(),
            Some("deterministic_fixture")
        );
        assert_eq!(result.report.metadata.live_market_evidence, Some(false));
        assert!(!result.report.metadata.live_readiness_evidence);
        assert!(!result.report.metadata.settlement_reference_evidence);
    }

    #[test]
    fn m9_storage_backed_fixture_sessions_replay_for_default_assets() {
        let config = config();

        for asset in [Asset::Btc, Asset::Eth, Asset::Sol] {
            let run_id = captured_run_id(asset);
            let storage = captured_session_storage(&run_id, asset, &config);
            let generated = ReplayEngine::replay_from_storage_snapshot(&storage, &run_id)
                .expect("storage-backed fixture replay succeeds");
            assert!(
                generated.recorded_paper_events.is_empty(),
                "fixture should start with no recorded paper events"
            );
            assert_m9_captured_report(asset, &generated);

            append_recorded_paper_events(&storage, &run_id, &generated.generated_paper_events);

            let first = ReplayEngine::replay_from_storage_snapshot(&storage, &run_id)
                .expect("captured paper replay succeeds");
            let second = ReplayEngine::replay_from_storage_snapshot(&storage, &run_id)
                .expect("second captured paper replay succeeds");
            let replay_check = compare_replay_results(&first, &second);
            let paper_check = compare_generated_to_recorded_paper_events(&first)
                .expect("paper event comparison succeeds");

            assert!(
                replay_check.passed,
                "{} replay should be deterministic",
                asset.symbol()
            );
            assert!(first
                .report
                .determinism_fingerprint()
                .starts_with("sha256:"));
            assert!(
                paper_check.passed,
                "{} generated paper events should match recorded captured events",
                asset.symbol()
            );
            assert_m9_captured_report(asset, &first);
            assert_eq!(first.recorded_paper_events, first.generated_paper_events);
            assert_eq!(first.recorded_paper_events.len(), 2);
            assert_sha256_fingerprint(&replay_check.left_fingerprint);
            assert_eq!(
                replay_check.left_fingerprint,
                replay_check.right_fingerprint
            );
            assert_sha256_fingerprint(&paper_check.left_fingerprint);
            assert_eq!(paper_check.left_fingerprint, paper_check.right_fingerprint);
            assert_eq!(
                first
                    .report
                    .events
                    .counts_by_type
                    .get(EventType::PaperOrderPlaced.as_str()),
                Some(&1)
            );
            assert_eq!(
                first
                    .report
                    .events
                    .counts_by_type
                    .get(EventType::PaperFill.as_str()),
                Some(&1)
            );
            println!(
                "m9_storage_backed_fixture_session asset={} run_id={} report_fingerprint={} paper_fingerprint={} input_fingerprint={} config_fingerprint={} fills={} fees_paid={:.6} total_pnl={:.6}",
                asset.symbol(),
                run_id,
                first.report.determinism_fingerprint(),
                paper_check.left_fingerprint,
                first
                    .report
                    .metadata
                    .input_fingerprint
                    .as_deref()
                    .unwrap_or("missing"),
                first
                    .report
                    .metadata
                    .config_fingerprint
                    .as_deref()
                    .unwrap_or("missing"),
                first.report.paper.fill_count,
                first.report.paper.total_fees_paid,
                first.report.pnl.totals.total_pnl
            );
        }
    }

    #[test]
    fn m9_storage_backed_fixture_paper_event_determinism_fails_when_recorded_event_is_missing() {
        let config = config();
        let asset = Asset::Btc;
        let run_id = captured_run_id(asset);
        let storage = captured_session_storage(&run_id, asset, &config);
        let generated = ReplayEngine::replay_from_storage_snapshot(&storage, &run_id)
            .expect("storage-backed fixture replay succeeds");
        let mut recorded_paper_events = generated.generated_paper_events.clone();
        recorded_paper_events
            .pop()
            .expect("paper fixture has events");
        append_recorded_paper_events(&storage, &run_id, &recorded_paper_events);

        let replay = ReplayEngine::replay_from_storage_snapshot(&storage, &run_id)
            .expect("captured paper replay succeeds");
        let check = compare_generated_to_recorded_paper_events(&replay)
            .expect("paper event comparison succeeds");

        assert!(!check.passed);
        assert_ne!(check.left_fingerprint, check.right_fingerprint);
        assert_eq!(
            check.divergence.as_deref(),
            Some("paper event mismatch: generated_count=2 recorded_count=1")
        );
    }

    #[test]
    fn deterministic_check_fails_when_input_event_drifts() {
        let engine = ReplayEngine::new(config());
        let baseline = engine
            .replay_events(RUN_ID, synthetic_events())
            .expect("baseline replay succeeds");
        let mut drifted_events = synthetic_events();
        drifted_events.retain(|event| event.event_id != "event-6");
        let drifted = engine
            .replay_events(RUN_ID, drifted_events)
            .expect("drifted replay succeeds");
        let check = compare_replay_results(&baseline, &drifted);

        assert!(!check.passed);
        assert_ne!(check.left_fingerprint, check.right_fingerprint);
        assert_eq!(
            check.divergence.as_deref(),
            Some("replay report fingerprint mismatch")
        );
    }

    #[test]
    fn deterministic_check_fails_when_input_ordering_key_drifts() {
        let engine = ReplayEngine::new(config());
        let baseline = engine
            .replay_events(RUN_ID, synthetic_events())
            .expect("baseline replay succeeds");
        let mut reordered_events = synthetic_events();
        let last_trade = reordered_events
            .iter_mut()
            .find(|event| event.event_id == "event-6")
            .expect("last trade event exists");
        last_trade.recv_mono_ns = 1;
        last_trade.ingest_seq = 0;

        let reordered = engine
            .replay_events(RUN_ID, reordered_events)
            .expect("reordered replay succeeds");
        let check = compare_replay_results(&baseline, &reordered);

        assert!(!check.passed);
    }

    #[test]
    fn replay_can_load_ordered_events_from_storage() {
        let storage = InMemoryStorage::default();
        for event in synthetic_events().into_iter().rev() {
            storage
                .append_normalized_event(event)
                .expect("fixture event writes");
        }

        let result = ReplayEngine::new(config())
            .replay_from_storage(&storage, RUN_ID)
            .expect("storage replay succeeds");

        assert_eq!(result.report.events.total_count, 6);
        assert_eq!(result.report.paper.fill_count, 1);
    }

    #[test]
    fn replay_can_load_captured_config_snapshot_from_storage() {
        let storage = InMemoryStorage::default();
        let config = config();
        storage
            .insert_config_snapshot(
                ConfigSnapshot::from_config(RUN_ID, START_TS, &config).expect("snapshot builds"),
            )
            .expect("config snapshot writes");
        for event in synthetic_events() {
            storage
                .append_normalized_event(event)
                .expect("fixture event writes");
        }

        let result = ReplayEngine::replay_from_storage_snapshot(&storage, RUN_ID)
            .expect("snapshot-backed replay succeeds");

        assert_eq!(result.report.events.total_count, 6);
        assert_eq!(result.report.paper.fill_count, 1);
    }

    #[test]
    fn replay_determinism_runs_identical_input_twice() {
        let input = ReplayInput::new(RUN_ID, config(), synthetic_events());
        let engine = ReplayEngine::new(input.config.clone());

        let check = engine
            .check_determinism(input)
            .expect("determinism check succeeds");

        assert!(check.passed);
    }

    #[test]
    fn paper_event_determinism_compares_generated_to_recorded_events() {
        let engine = ReplayEngine::new(config());
        let baseline = engine
            .replay_events(RUN_ID, synthetic_events())
            .expect("baseline replay succeeds");
        let events_with_recorded_paper =
            synthetic_events_with_recorded_paper_events(baseline.generated_paper_events.clone());

        let check = engine
            .check_paper_event_determinism(ReplayInput::new(
                RUN_ID,
                config(),
                events_with_recorded_paper,
            ))
            .expect("paper event comparison succeeds");

        assert!(check.passed);
    }

    #[test]
    fn paper_event_fingerprint_canonicalizes_json_float_roundtrip() {
        let generated = vec![NormalizedEvent::PaperOrderPlaced {
            order: PaperOrder {
                order_id: "paper-order-1".to_string(),
                market_id: MARKET_ID.to_string(),
                token_id: UP_TOKEN_ID.to_string(),
                asset: Asset::Btc,
                side: Side::Buy,
                order_kind: OrderKind::Taker,
                fee_parameters: FeeParameters {
                    fees_enabled: true,
                    maker_fee_bps: 0.0,
                    taker_fee_bps: 180.0,
                    raw_fee_config: None,
                },
                price: 0.21000000000000002,
                size: 10.0,
                filled_size: 0.0,
                status: PaperOrderStatus::Open,
                reason: "unit".to_string(),
                created_ts: START_TS,
                updated_ts: START_TS,
            },
        }];
        let recorded: Vec<NormalizedEvent> =
            serde_json::from_slice(&serde_json::to_vec(&generated).expect("generated serializes"))
                .expect("recorded event parses");

        assert_eq!(
            stable_paper_event_fingerprint(&generated).expect("generated fingerprints"),
            stable_paper_event_fingerprint(&recorded).expect("recorded fingerprints")
        );
    }

    #[test]
    fn paper_event_determinism_fails_when_recorded_paper_event_is_missing() {
        let engine = ReplayEngine::new(config());
        let baseline = engine
            .replay_events(RUN_ID, synthetic_events())
            .expect("baseline replay succeeds");
        let mut recorded = baseline.generated_paper_events.clone();
        recorded.pop();
        let events_with_recorded_paper = synthetic_events_with_recorded_paper_events(recorded);

        let check = engine
            .check_paper_event_determinism(ReplayInput::new(
                RUN_ID,
                config(),
                events_with_recorded_paper,
            ))
            .expect("paper event comparison succeeds");

        assert!(!check.passed);
        assert!(check
            .divergence
            .as_deref()
            .unwrap_or_default()
            .contains("paper event mismatch"));
    }

    fn synthetic_events() -> Vec<EventEnvelope> {
        synthetic_events_for_run_and_asset(RUN_ID, Asset::Btc)
    }

    fn pyth_proxy_synthetic_events() -> Vec<EventEnvelope> {
        synthetic_events()
            .into_iter()
            .map(|mut envelope| {
                if let NormalizedEvent::ReferenceTick { price } = &mut envelope.payload {
                    price.provider = Some("pyth".to_string());
                    price.matches_market_resolution_source = Some(false);
                    envelope.source = SOURCE_PYTH_PROXY.to_string();
                }
                envelope
            })
            .collect()
    }

    fn polymarket_rtds_chainlink_synthetic_events() -> Vec<EventEnvelope> {
        synthetic_events()
            .into_iter()
            .map(|mut envelope| {
                if let NormalizedEvent::ReferenceTick { price } = &mut envelope.payload {
                    price.provider = Some(PROVIDER_POLYMARKET_RTDS_CHAINLINK.to_string());
                    price.matches_market_resolution_source = Some(true);
                    envelope.source = SOURCE_POLYMARKET_RTDS_CHAINLINK.to_string();
                }
                envelope
            })
            .collect()
    }

    fn synthetic_events_for_run_and_asset(run_id: &str, asset: Asset) -> Vec<EventEnvelope> {
        let market_id = asset_market_id(asset);
        let up_token_id = asset_up_token_id(asset);
        let down_token_id = asset_down_token_id(asset);

        vec![
            envelope_for_run(
                run_id,
                1,
                NormalizedEvent::MarketDiscovered {
                    market: market_for_asset(asset),
                },
            ),
            envelope_for_run(
                run_id,
                2,
                NormalizedEvent::BookSnapshot {
                    book: book_for_asset(asset, &up_token_id, 0.49, 0.51),
                },
            ),
            envelope_for_run(
                run_id,
                3,
                NormalizedEvent::BookSnapshot {
                    book: book_for_asset(asset, &down_token_id, 0.49, 0.51),
                },
            ),
            envelope_for_run(
                run_id,
                4,
                NormalizedEvent::ReferenceTick {
                    price: reference_for_asset(
                        asset,
                        asset.chainlink_resolution_source(),
                        100.0,
                        START_TS + 300_000,
                    ),
                },
            ),
            envelope_for_run(
                run_id,
                5,
                NormalizedEvent::PredictiveTick {
                    price: reference_for_asset(asset, "binance", 101.0, START_TS + 300_100),
                },
            ),
            envelope_for_run(
                run_id,
                6,
                NormalizedEvent::LastTrade {
                    market_id,
                    token_id: up_token_id,
                    side: Side::Buy,
                    price: 0.50,
                    size: 10.0,
                    fee_rate_bps: Some(0.0),
                    source_ts: Some(START_TS + 300_200),
                },
            ),
        ]
    }

    fn captured_fixture_events_for_run_and_asset(run_id: &str, asset: Asset) -> Vec<EventEnvelope> {
        let market_id = asset_market_id(asset);
        let up_token_id = asset_up_token_id(asset);
        let down_token_id = asset_down_token_id(asset);

        vec![
            envelope_for_run(
                run_id,
                1,
                NormalizedEvent::MarketDiscovered {
                    market: market_for_asset_with_taker_fee(asset, 200.0),
                },
            ),
            envelope_for_run(
                run_id,
                2,
                NormalizedEvent::BookSnapshot {
                    book: book_for_asset(asset, &up_token_id, 0.50, 0.51),
                },
            ),
            envelope_for_run(
                run_id,
                3,
                NormalizedEvent::BookSnapshot {
                    book: book_for_asset(asset, &down_token_id, 0.49, 0.51),
                },
            ),
            envelope_for_run(
                run_id,
                4,
                NormalizedEvent::ReferenceTick {
                    price: reference_for_asset(
                        asset,
                        asset.chainlink_resolution_source(),
                        100.0,
                        START_TS + 300_000,
                    ),
                },
            ),
            envelope_for_run(
                run_id,
                5,
                NormalizedEvent::PredictiveTick {
                    price: reference_for_asset(asset, "binance", 101.0, START_TS + 300_100),
                },
            ),
            envelope_for_run(
                run_id,
                6,
                NormalizedEvent::LastTrade {
                    market_id,
                    token_id: up_token_id,
                    side: Side::Buy,
                    price: 0.51,
                    size: 10.0,
                    fee_rate_bps: Some(200.0),
                    source_ts: Some(START_TS + 300_200),
                },
            ),
        ]
    }

    fn synthetic_events_with_recorded_paper_events(
        recorded_paper_events: Vec<NormalizedEvent>,
    ) -> Vec<EventEnvelope> {
        let mut events = synthetic_events();
        for (index, event) in recorded_paper_events.into_iter().enumerate() {
            events.push(envelope(100 + index as u64, event));
        }
        events
    }

    fn envelope(seq: u64, payload: NormalizedEvent) -> EventEnvelope {
        envelope_for_run(RUN_ID, seq, payload)
    }

    fn envelope_for_run(run_id: &str, seq: u64, payload: NormalizedEvent) -> EventEnvelope {
        EventEnvelope::new(
            run_id,
            format!("event-{seq}"),
            "synthetic-fixture",
            START_TS + 300_000 + seq as i64,
            seq,
            seq,
            payload,
        )
    }

    fn timed_envelope(seq: u64, recv_wall_ts: i64, payload: NormalizedEvent) -> EventEnvelope {
        EventEnvelope::new(
            RUN_ID,
            format!("event-{seq}"),
            "synthetic-fixture",
            recv_wall_ts,
            seq,
            seq,
            payload,
        )
    }

    fn recorded_paper_envelope(
        run_id: &str,
        index: usize,
        payload: NormalizedEvent,
    ) -> EventEnvelope {
        let seq = 10_000 + index as u64;
        EventEnvelope::new(
            run_id,
            format!("recorded-paper-{index}"),
            "captured-paper-fixture",
            START_TS + 700_000 + index as i64,
            seq,
            seq,
            payload,
        )
    }

    fn market_for_asset(asset: Asset) -> Market {
        market_for_asset_with_taker_fee(asset, 0.0)
    }

    fn market_for_asset_with_taker_fee(asset: Asset, taker_fee_bps: f64) -> Market {
        Market {
            market_id: asset_market_id(asset),
            slug: format!("{}-up-down-15m", asset.symbol().to_ascii_lowercase()),
            title: format!("{} Up or Down", asset.symbol()),
            asset,
            condition_id: format!("condition-{}", asset.symbol().to_ascii_lowercase()),
            outcomes: vec![
                OutcomeToken {
                    token_id: asset_up_token_id(asset),
                    outcome: "Up".to_string(),
                },
                OutcomeToken {
                    token_id: asset_down_token_id(asset),
                    outcome: "Down".to_string(),
                },
            ],
            start_ts: START_TS,
            end_ts: START_TS + 900_000,
            resolution_source: Some(asset.chainlink_resolution_source().to_string()),
            tick_size: 0.01,
            min_order_size: 5.0,
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps,
                raw_fee_config: None,
            },
            lifecycle_state: MarketLifecycleState::Active,
            ineligibility_reason: None,
        }
    }

    fn book_for_asset(
        asset: Asset,
        token_id: &str,
        best_bid: f64,
        best_ask: f64,
    ) -> OrderBookSnapshot {
        OrderBookSnapshot {
            market_id: asset_market_id(asset),
            token_id: token_id.to_string(),
            bids: vec![OrderBookLevel {
                price: best_bid,
                size: 100.0,
            }],
            asks: vec![OrderBookLevel {
                price: best_ask,
                size: 100.0,
            }],
            hash: Some(format!("{token_id}-hash")),
            source_ts: Some(START_TS + 299_000),
        }
    }

    fn condition_book_for_asset(
        asset: Asset,
        token_id: &str,
        best_bid: f64,
        best_ask: f64,
    ) -> OrderBookSnapshot {
        let mut book = book_for_asset(asset, token_id, best_bid, best_ask);
        book.market_id = format!("condition-{}", asset.symbol().to_ascii_lowercase());
        book
    }

    fn reference_for_asset(
        asset: Asset,
        source: &str,
        price: f64,
        recv_wall_ts: i64,
    ) -> ReferencePrice {
        ReferencePrice {
            asset,
            source: source.to_string(),
            price,
            confidence: None,
            provider: None,
            matches_market_resolution_source: None,
            source_ts: Some(recv_wall_ts - 1),
            recv_wall_ts,
        }
    }

    fn config() -> AppConfig {
        toml::from_str(DEFAULT_CONFIG).expect("default config parses")
    }

    fn captured_session_storage(run_id: &str, asset: Asset, config: &AppConfig) -> InMemoryStorage {
        let storage = InMemoryStorage::default();
        storage
            .insert_config_snapshot(
                ConfigSnapshot::from_config(run_id, START_TS, config).expect("snapshot builds"),
            )
            .expect("config snapshot writes");
        for event in captured_fixture_events_for_run_and_asset(run_id, asset) {
            storage
                .append_normalized_event(event)
                .expect("storage-backed fixture event writes");
        }
        storage
    }

    fn append_recorded_paper_events(
        storage: &InMemoryStorage,
        run_id: &str,
        paper_events: &[NormalizedEvent],
    ) {
        for (index, event) in paper_events.iter().cloned().enumerate() {
            storage
                .append_normalized_event(recorded_paper_envelope(run_id, index, event))
                .expect("recorded paper event writes");
        }
    }

    fn assert_m9_captured_report(asset: Asset, result: &ReplayRunResult) {
        let report = &result.report;
        let asset_symbol = asset.symbol();
        let market_id = asset_market_id(asset);

        assert_eq!(report.risk.approval_count, 1);
        assert_eq!(report.paper.order_count, 1);
        assert_eq!(report.paper.fill_count, 1);
        assert_eq!(result.generated_orders.len(), 1);
        assert_eq!(result.generated_fills.len(), 1);
        assert_eq!(result.generated_paper_events.len(), 2);
        assert_eq!(
            report.paper.audit_event_counts.get("fill_simulated"),
            Some(&1)
        );
        assert_eq!(
            report.signals.counts_by_asset.get(asset_symbol),
            Some(&report.signals.evaluated_count)
        );
        assert_sha256_fingerprint(&report.determinism_fingerprint());
        assert_sha256_fingerprint(
            report
                .metadata
                .input_fingerprint
                .as_deref()
                .expect("input fingerprint exists"),
        );
        assert_sha256_fingerprint(
            report
                .metadata
                .config_fingerprint
                .as_deref()
                .expect("config fingerprint exists"),
        );

        let fill = &result.generated_fills[0];
        assert_eq!(fill.asset, asset);
        assert_eq!(fill.liquidity, OrderKind::Taker);
        assert_close(fill.size, 10.0);
        assert_close(fill.price, 0.51);
        assert_close(fill.fee_paid, 0.2);
        assert_close(report.paper.total_filled_size, 10.0);
        assert_close(report.paper.total_filled_notional, 5.1);
        assert_close(report.paper.total_fees_paid, 0.2);

        let asset_pnl = report
            .pnl
            .by_asset
            .get(asset_symbol)
            .expect("P&L should be grouped by asset");
        let market_pnl = report
            .pnl
            .by_market
            .get(&market_id)
            .expect("P&L should be grouped by market");
        assert_close(asset_pnl.fees_paid, report.paper.total_fees_paid);
        assert_close(market_pnl.fees_paid, report.paper.total_fees_paid);
        assert!(
            asset_pnl.total_pnl < 0.0,
            "fees and conservative taker mark should be visible in P&L"
        );
        assert_close(report.pnl.totals.fees_paid, report.paper.total_fees_paid);
        assert_close(report.pnl.totals.total_pnl, asset_pnl.total_pnl);
    }

    fn assert_sha256_fingerprint(fingerprint: &str) {
        assert!(fingerprint.starts_with("sha256:"));
        assert_eq!(fingerprint.len(), "sha256:".len() + 64);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9,
            "actual={actual} expected={expected}"
        );
    }

    fn captured_run_id(asset: Asset) -> String {
        format!(
            "m9-{}-captured-paper-fixture",
            asset.symbol().to_ascii_lowercase()
        )
    }

    fn asset_market_id(asset: Asset) -> String {
        if asset == Asset::Btc {
            MARKET_ID.to_string()
        } else {
            format!("market-{}", asset.symbol().to_ascii_lowercase())
        }
    }

    fn asset_up_token_id(asset: Asset) -> String {
        if asset == Asset::Btc {
            UP_TOKEN_ID.to_string()
        } else {
            format!("token-{}-up", asset.symbol().to_ascii_lowercase())
        }
    }

    fn asset_down_token_id(asset: Asset) -> String {
        if asset == Asset::Btc {
            DOWN_TOKEN_ID.to_string()
        } else {
            format!("token-{}-down", asset.symbol().to_ascii_lowercase())
        }
    }
}
