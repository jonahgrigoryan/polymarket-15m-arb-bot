use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::Serialize;

use crate::config::AppConfig;
use crate::domain::{Asset, Market, MarketLifecycleState, PaperFill, PaperOrder, PaperOrderStatus};
use crate::events::{EventEnvelope, NormalizedEvent};
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
}

#[derive(Debug, Clone)]
pub struct ReplayEngine {
    config: AppConfig,
    signal_engine: SignalEngine,
    risk_engine: RiskEngine,
}

impl ReplayEngine {
    pub fn new(config: AppConfig) -> Self {
        Self {
            signal_engine: SignalEngine::from_strategy_config(&config.strategy),
            risk_engine: RiskEngine::from_config(&config.risk),
            config,
        }
    }

    pub fn from_config(config: AppConfig) -> Self {
        Self::new(config)
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
        let snapshot = storage
            .read_config_snapshot(run_id)
            .map_err(ReplayError::Storage)?
            .ok_or_else(|| ReplayError::MissingConfigSnapshot(run_id.to_string()))?;
        let config = config_from_snapshot(snapshot)?;
        ReplayEngine::new(config).replay_from_storage(storage, run_id)
    }

    pub fn replay_events(
        &self,
        run_id: impl Into<String>,
        events: Vec<EventEnvelope>,
    ) -> ReplayResult<ReplayRunResult> {
        let run_id = run_id.into();
        let replay = ReplayExecution::new(&self.config, &self.signal_engine, &self.risk_engine);
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
    let left_fingerprint = stable_fingerprint(&result.generated_paper_events)?;
    let right_fingerprint = stable_fingerprint(&result.recorded_paper_events)?;
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
    state: StateStore,
    paper_executor: PaperExecutor,
    position_book: PaperPositionBook,
    markets: BTreeMap<String, Market>,
    order_timestamps_ms: Vec<i64>,
    generated_events: Vec<NormalizedEvent>,
    generated_paper_events: Vec<NormalizedEvent>,
    recorded_paper_events: Vec<NormalizedEvent>,
    generated_orders: BTreeMap<String, PaperOrder>,
    generated_fills: Vec<PaperFill>,
    audit_events: Vec<PaperExecutionAuditEvent>,
    signals: Vec<SignalReplayRecord>,
    risk_decisions: Vec<RiskReplayRecord>,
}

impl<'a> ReplayExecution<'a> {
    fn new(
        config: &'a AppConfig,
        signal_engine: &'a SignalEngine,
        risk_engine: &'a RiskEngine,
    ) -> Self {
        Self {
            config,
            signal_engine,
            risk_engine,
            state: StateStore::new(),
            paper_executor: PaperExecutor::new(PaperExecutorConfig::default()),
            position_book: PaperPositionBook::new(),
            markets: BTreeMap::new(),
            order_timestamps_ms: Vec::new(),
            generated_events: Vec::new(),
            generated_paper_events: Vec::new(),
            recorded_paper_events: Vec::new(),
            generated_orders: BTreeMap::new(),
            generated_fills: Vec::new(),
            audit_events: Vec::new(),
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

        let order_ids = self
            .paper_executor
            .orders()
            .into_iter()
            .filter(|order| {
                order.market_id == *market_id
                    && order.token_id == *token_id
                    && matches!(
                        order.status,
                        PaperOrderStatus::Open | PaperOrderStatus::PartiallyFilled
                    )
            })
            .map(|order| order.order_id.clone())
            .collect::<Vec<_>>();

        for order_id in order_ids {
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
            NormalizedEvent::BookSnapshot { book } => vec![book.market_id.clone()],
            NormalizedEvent::BookDelta { market_id, .. }
            | NormalizedEvent::BestBidAsk { market_id, .. } => vec![market_id.clone()],
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

        if !risk_decision.approved {
            let book = matching_book_for_token(&snapshot, &intent.token_id);
            let result = self.paper_executor.open_paper_order(
                intent,
                &risk_decision,
                &snapshot.market.fee_parameters,
                book,
                now_wall_ts,
            )?;
            self.record_paper_result(result);
            return Ok(());
        }

        let book = matching_book_for_token(&snapshot, &intent.token_id);
        let result = self.paper_executor.open_paper_order(
            intent,
            &risk_decision,
            &snapshot.market.fee_parameters,
            book,
            now_wall_ts,
        )?;
        if result.order.is_some() {
            self.order_timestamps_ms.push(now_wall_ts);
        }
        self.record_paper_result(result);
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

fn matching_book_for_token<'a>(
    snapshot: &'a DecisionSnapshot,
    token_id: &str,
) -> Option<&'a TokenBookSnapshot> {
    snapshot.token_books.iter().find(|book| {
        snapshot
            .market
            .outcomes
            .iter()
            .any(|outcome| outcome.token_id == book.token_id && outcome.token_id == token_id)
    })
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
        reference_provider: is_pyth_proxy.then(|| "pyth".to_string()),
        matches_market_resolution_source: is_pyth_proxy.then_some(false),
        live_readiness_evidence: is_chainlink,
        settlement_reference_evidence: is_chainlink,
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
            "polymarket_clob" | "binance" | "coinbase" | "pyth_proxy"
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
        ReferencePrice, Side,
    };
    use crate::events::EventType;
    use crate::reference_feed::SOURCE_PYTH_PROXY;
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
