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
use crate::state::{BookUpdateError, DecisionSnapshot, StateStore, TokenBookSnapshot};
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
) -> ReplayRunMetadata {
    ReplayRunMetadata {
        run_id: source_run_id.to_string(),
        replay_run_id: format!("{source_run_id}:offline-replay"),
        input_source: Some("event_envelopes".to_string()),
        input_fingerprint: Some(input_fingerprint),
        config_fingerprint: Some(config_fingerprint),
        code_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        started_wall_ts: events.first().map(|event| event.recv_wall_ts),
        completed_wall_ts: events.last().map(|event| event.recv_wall_ts),
        first_event_recv_wall_ts: None,
        last_event_recv_wall_ts: None,
        first_event_source_ts: None,
        last_event_source_ts: None,
        source_timestamp_regressions: count_source_timestamp_regressions(events),
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
        FeeParameters, Market, OrderBookLevel, OrderBookSnapshot, OutcomeToken, ReferencePrice,
        Side,
    };
    use crate::events::EventType;
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
        vec![
            envelope(1, NormalizedEvent::MarketDiscovered { market: market() }),
            envelope(
                2,
                NormalizedEvent::BookSnapshot {
                    book: book(UP_TOKEN_ID, 0.49, 0.51),
                },
            ),
            envelope(
                3,
                NormalizedEvent::BookSnapshot {
                    book: book(DOWN_TOKEN_ID, 0.49, 0.51),
                },
            ),
            envelope(
                4,
                NormalizedEvent::ReferenceTick {
                    price: reference(
                        Asset::Btc.chainlink_resolution_source(),
                        100.0,
                        START_TS + 300_000,
                    ),
                },
            ),
            envelope(
                5,
                NormalizedEvent::PredictiveTick {
                    price: reference("binance", 101.0, START_TS + 300_100),
                },
            ),
            envelope(
                6,
                NormalizedEvent::LastTrade {
                    market_id: MARKET_ID.to_string(),
                    token_id: UP_TOKEN_ID.to_string(),
                    side: Side::Buy,
                    price: 0.50,
                    size: 10.0,
                    fee_rate_bps: Some(0.0),
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
        EventEnvelope::new(
            RUN_ID,
            format!("event-{seq}"),
            "synthetic-fixture",
            START_TS + 300_000 + seq as i64,
            seq,
            seq,
            payload,
        )
    }

    fn market() -> Market {
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
            end_ts: START_TS + 900_000,
            resolution_source: Some(Asset::Btc.chainlink_resolution_source().to_string()),
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

    fn book(token_id: &str, best_bid: f64, best_ask: f64) -> OrderBookSnapshot {
        OrderBookSnapshot {
            market_id: MARKET_ID.to_string(),
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

    fn reference(source: &str, price: f64, recv_wall_ts: i64) -> ReferencePrice {
        ReferencePrice {
            asset: Asset::Btc,
            source: source.to_string(),
            price,
            source_ts: Some(recv_wall_ts - 1),
            recv_wall_ts,
        }
    }

    fn config() -> AppConfig {
        toml::from_str(DEFAULT_CONFIG).expect("default config parses")
    }
}
