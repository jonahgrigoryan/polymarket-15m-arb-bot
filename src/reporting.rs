use std::collections::BTreeMap;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::domain::{
    Asset, OrderKind, PaperFill, PaperOrder, PaperOrderStatus, RiskHaltReason, SignalDecision,
};
use crate::events::{EventEnvelope, EventType};
use crate::paper_executor::PaperExecutionAuditEvent;
use crate::risk_engine::RiskGateDecision;
use crate::signal_engine::{SignalEvaluation, SignalSkipReason};
use crate::state::PositionSnapshot;

pub const MODULE: &str = "reporting";

pub const REPORT_SCHEMA_VERSION: &str = "m7_replay_report_v1";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReplayReportInput {
    pub metadata: ReplayRunMetadata,
    pub feed_stale_after_ms: Option<u64>,
    pub events: Vec<EventEnvelope>,
    pub signals: Vec<SignalReplayRecord>,
    pub risk_decisions: Vec<RiskReplayRecord>,
    pub paper_orders: Vec<PaperOrder>,
    pub paper_fills: Vec<PaperFill>,
    pub paper_audit_events: Vec<PaperExecutionAuditEvent>,
    pub pnl: PnlReport,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ReplayReport {
    pub schema_version: String,
    pub metadata: ReplayRunMetadata,
    pub events: EventReport,
    pub signals: SignalReport,
    pub risk: RiskReport,
    pub paper: PaperReport,
    pub pnl: PnlReport,
    pub diagnostics: ReplayDiagnostics,
}

impl ReplayReport {
    pub fn new(metadata: ReplayRunMetadata) -> Self {
        Self {
            schema_version: REPORT_SCHEMA_VERSION.to_string(),
            metadata,
            events: EventReport::default(),
            signals: SignalReport::default(),
            risk: RiskReport::default(),
            paper: PaperReport::default(),
            pnl: PnlReport::default(),
            diagnostics: ReplayDiagnostics::default(),
        }
    }

    pub fn from_input(input: ReplayReportInput) -> Self {
        let mut report = Self::new(input.metadata);

        for event in &input.events {
            report.record_event(event);
        }
        for signal in &input.signals {
            report.record_signal(signal);
        }
        for risk_decision in &input.risk_decisions {
            report.record_risk_decision(risk_decision);
        }
        for order in &input.paper_orders {
            report.record_paper_order(order);
        }
        for fill in &input.paper_fills {
            report.record_paper_fill(fill);
        }
        for audit_event in &input.paper_audit_events {
            report.record_paper_audit_event(audit_event);
        }

        report.diagnostics = ReplayDiagnostics::from_inputs(
            &input.events,
            &input.signals,
            &input.risk_decisions,
            &input.paper_orders,
            input.feed_stale_after_ms,
        );
        report.pnl = input.pnl;
        report
    }

    pub fn record_event(&mut self, event: &EventEnvelope) {
        self.events.record(event.event_type.clone());
        self.metadata.record_event_bounds(event);
    }

    pub fn record_signal(&mut self, signal: &SignalReplayRecord) {
        self.signals.record(signal);
    }

    pub fn record_signal_evaluation(&mut self, evaluation: &SignalEvaluation) {
        self.record_signal(&SignalReplayRecord::from_evaluation(evaluation));
    }

    pub fn record_risk_decision(&mut self, decision: &RiskReplayRecord) {
        self.risk.record(decision);
    }

    pub fn record_risk_gate_decision(
        &mut self,
        market_id: Option<String>,
        asset: Option<Asset>,
        decision: &RiskGateDecision,
    ) {
        self.record_risk_decision(&RiskReplayRecord::from_gate_decision(
            market_id, asset, decision,
        ));
    }

    pub fn record_paper_order(&mut self, order: &PaperOrder) {
        self.paper.record_order(order);
    }

    pub fn record_paper_fill(&mut self, fill: &PaperFill) {
        self.paper.record_fill(fill);
    }

    pub fn record_paper_audit_event(&mut self, event: &PaperExecutionAuditEvent) {
        self.paper.record_audit_event(event);
    }

    pub fn determinism_fingerprint(&self) -> String {
        determinism_fingerprint(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct ReplayRunMetadata {
    pub run_id: String,
    pub replay_run_id: String,
    pub input_source: Option<String>,
    pub input_fingerprint: Option<String>,
    pub config_fingerprint: Option<String>,
    pub code_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_market_evidence: Option<bool>,
    pub started_wall_ts: Option<i64>,
    pub completed_wall_ts: Option<i64>,
    pub first_event_recv_wall_ts: Option<i64>,
    pub last_event_recv_wall_ts: Option<i64>,
    pub first_event_source_ts: Option<i64>,
    pub last_event_source_ts: Option<i64>,
    pub source_timestamp_regressions: u64,
    #[serde(default)]
    pub reference_feed_mode: Option<String>,
    #[serde(default)]
    pub reference_provider: Option<String>,
    #[serde(default)]
    pub matches_market_resolution_source: Option<bool>,
    #[serde(default)]
    pub live_readiness_evidence: bool,
    #[serde(default)]
    pub settlement_reference_evidence: bool,
}

impl ReplayRunMetadata {
    fn record_event_bounds(&mut self, event: &EventEnvelope) {
        self.first_event_recv_wall_ts =
            min_i64_option(self.first_event_recv_wall_ts, event.recv_wall_ts);
        self.last_event_recv_wall_ts =
            max_i64_option(self.last_event_recv_wall_ts, event.recv_wall_ts);

        if let Some(source_ts) = event.source_ts {
            self.first_event_source_ts = min_i64_option(self.first_event_source_ts, source_ts);
            self.last_event_source_ts = max_i64_option(self.last_event_source_ts, source_ts);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct ReplayDiagnostics {
    pub latency: LatencyReport,
    pub feed_staleness: FeedStalenessReport,
    pub opportunities: OpportunityReport,
}

impl ReplayDiagnostics {
    fn from_inputs(
        events: &[EventEnvelope],
        signals: &[SignalReplayRecord],
        risk_decisions: &[RiskReplayRecord],
        paper_orders: &[PaperOrder],
        feed_stale_after_ms: Option<u64>,
    ) -> Self {
        Self {
            latency: LatencyReport::from_events(events),
            feed_staleness: FeedStalenessReport::from_events(events, feed_stale_after_ms),
            opportunities: OpportunityReport::from_inputs(signals, risk_decisions, paper_orders),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct LatencyReport {
    pub event_count_with_source_ts: u64,
    pub min_latency_ms: Option<i64>,
    pub max_latency_ms: Option<i64>,
    pub average_latency_ms: Option<f64>,
    pub negative_latency_count: u64,
    pub by_source: BTreeMap<String, LatencySourceReport>,
}

impl LatencyReport {
    fn from_events(events: &[EventEnvelope]) -> Self {
        let mut total = LatencyAccumulator::default();
        let mut by_source = BTreeMap::<String, LatencyAccumulator>::new();

        for event in events {
            let Some(source_ts) = event.source_ts else {
                continue;
            };
            let latency_ms = event.recv_wall_ts - source_ts;
            total.record(latency_ms);
            by_source
                .entry(event.source.clone())
                .or_default()
                .record(latency_ms);
        }

        let total_report = total.into_source_report();
        Self {
            event_count_with_source_ts: total_report.event_count,
            min_latency_ms: total_report.min_latency_ms,
            max_latency_ms: total_report.max_latency_ms,
            average_latency_ms: total_report.average_latency_ms,
            negative_latency_count: total_report.negative_latency_count,
            by_source: by_source
                .into_iter()
                .map(|(source, accumulator)| (source, accumulator.into_source_report()))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct LatencySourceReport {
    pub event_count: u64,
    pub min_latency_ms: Option<i64>,
    pub max_latency_ms: Option<i64>,
    pub average_latency_ms: Option<f64>,
    pub negative_latency_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct FeedStalenessReport {
    pub threshold_ms: Option<u64>,
    pub window_count: u64,
    pub windows: Vec<FeedStalenessWindow>,
}

impl FeedStalenessReport {
    fn from_events(events: &[EventEnvelope], threshold_ms: Option<u64>) -> Self {
        let Some(threshold_ms) = threshold_ms else {
            return Self::default();
        };

        let mut last_recv_by_source = BTreeMap::<String, i64>::new();
        let mut windows = Vec::new();

        for event in events {
            if let Some(previous_recv_wall_ts) =
                last_recv_by_source.insert(event.source.clone(), event.recv_wall_ts)
            {
                let gap_ms = event.recv_wall_ts.saturating_sub(previous_recv_wall_ts);
                if gap_ms > threshold_ms as i64 {
                    windows.push(FeedStalenessWindow {
                        source: event.source.clone(),
                        start_recv_wall_ts: previous_recv_wall_ts,
                        end_recv_wall_ts: event.recv_wall_ts,
                        duration_ms: gap_ms,
                    });
                }
            }
        }

        Self {
            threshold_ms: Some(threshold_ms),
            window_count: windows.len() as u64,
            windows,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct FeedStalenessWindow {
    pub source: String,
    pub start_recv_wall_ts: i64,
    pub end_recv_wall_ts: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct OpportunityReport {
    pub evaluated_signal_count: u64,
    pub emitted_order_intent_count: u64,
    pub skipped_signal_count: u64,
    pub risk_approval_count: u64,
    pub risk_rejection_count: u64,
    pub unfilled_order_count: u64,
    pub partial_fill_order_count: u64,
    pub filled_order_count: u64,
    pub missed_opportunity_count: u64,
}

impl OpportunityReport {
    fn from_inputs(
        signals: &[SignalReplayRecord],
        risk_decisions: &[RiskReplayRecord],
        paper_orders: &[PaperOrder],
    ) -> Self {
        let skipped_signal_count = signals
            .iter()
            .filter(|signal| !signal.emitted_order_intent)
            .count() as u64;
        let emitted_order_intent_count = signals
            .iter()
            .filter(|signal| signal.emitted_order_intent)
            .count() as u64;
        let risk_approval_count = risk_decisions
            .iter()
            .filter(|decision| decision.approved)
            .count() as u64;
        let risk_rejection_count = risk_decisions
            .iter()
            .filter(|decision| !decision.approved)
            .count() as u64;
        let unfilled_order_count = paper_orders
            .iter()
            .filter(|order| order.filled_size <= 0.0)
            .count() as u64;
        let partial_fill_order_count = paper_orders
            .iter()
            .filter(|order| order.status == PaperOrderStatus::PartiallyFilled)
            .count() as u64;
        let filled_order_count = paper_orders
            .iter()
            .filter(|order| order.status == PaperOrderStatus::Filled)
            .count() as u64;

        Self {
            evaluated_signal_count: signals.len() as u64,
            emitted_order_intent_count,
            skipped_signal_count,
            risk_approval_count,
            risk_rejection_count,
            unfilled_order_count,
            partial_fill_order_count,
            filled_order_count,
            missed_opportunity_count: skipped_signal_count
                + risk_rejection_count
                + unfilled_order_count
                + partial_fill_order_count,
        }
    }
}

#[derive(Debug, Default)]
struct LatencyAccumulator {
    event_count: u64,
    latency_sum_ms: i128,
    min_latency_ms: Option<i64>,
    max_latency_ms: Option<i64>,
    negative_latency_count: u64,
}

impl LatencyAccumulator {
    fn record(&mut self, latency_ms: i64) {
        self.event_count += 1;
        self.latency_sum_ms += latency_ms as i128;
        self.min_latency_ms = min_i64_option(self.min_latency_ms, latency_ms);
        self.max_latency_ms = max_i64_option(self.max_latency_ms, latency_ms);
        if latency_ms < 0 {
            self.negative_latency_count += 1;
        }
    }

    fn into_source_report(self) -> LatencySourceReport {
        LatencySourceReport {
            event_count: self.event_count,
            min_latency_ms: self.min_latency_ms,
            max_latency_ms: self.max_latency_ms,
            average_latency_ms: if self.event_count == 0 {
                None
            } else {
                Some(self.latency_sum_ms as f64 / self.event_count as f64)
            },
            negative_latency_count: self.negative_latency_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct EventReport {
    pub total_count: u64,
    pub counts_by_type: BTreeMap<String, u64>,
}

impl EventReport {
    pub fn record(&mut self, event_type: EventType) {
        self.total_count += 1;
        increment(&mut self.counts_by_type, event_type.as_str());
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct SignalReport {
    pub evaluated_count: u64,
    pub emitted_order_intent_count: u64,
    pub skipped_count: u64,
    pub counts_by_asset: BTreeMap<String, u64>,
    pub counts_by_order_kind: BTreeMap<String, u64>,
    pub skip_reason_counts: BTreeMap<String, u64>,
    pub decision_reason_counts: BTreeMap<String, u64>,
    pub decisions: Vec<SignalReplayRecord>,
}

impl SignalReport {
    pub fn record(&mut self, signal: &SignalReplayRecord) {
        self.evaluated_count += 1;
        increment(&mut self.counts_by_asset, signal.decision.asset.symbol());
        increment(
            &mut self.counts_by_order_kind,
            order_kind_key(signal.decision.order_kind),
        );
        increment(&mut self.decision_reason_counts, &signal.decision.reason);

        if signal.emitted_order_intent {
            self.emitted_order_intent_count += 1;
        } else {
            self.skipped_count += 1;
        }

        for reason in &signal.skip_reasons {
            increment(&mut self.skip_reason_counts, reason);
        }

        self.decisions.push(signal.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SignalReplayRecord {
    pub decision: SignalDecision,
    pub emitted_order_intent: bool,
    pub skip_reasons: Vec<String>,
}

impl SignalReplayRecord {
    pub fn new(
        decision: SignalDecision,
        emitted_order_intent: bool,
        mut skip_reasons: Vec<String>,
    ) -> Self {
        skip_reasons.sort();
        skip_reasons.dedup();

        Self {
            decision,
            emitted_order_intent,
            skip_reasons,
        }
    }

    pub fn from_evaluation(evaluation: &SignalEvaluation) -> Self {
        let skip_reasons = evaluation
            .skip_reasons
            .iter()
            .map(|reason| signal_skip_reason_key(*reason).to_string())
            .collect();

        Self::new(
            evaluation.decision.clone(),
            evaluation.candidate.is_some(),
            skip_reasons,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct RiskReport {
    pub approval_count: u64,
    pub rejection_count: u64,
    pub halt_reason_counts: BTreeMap<String, u64>,
    pub decisions: Vec<RiskReplayRecord>,
}

impl RiskReport {
    pub fn record(&mut self, decision: &RiskReplayRecord) {
        if decision.approved {
            self.approval_count += 1;
        } else {
            self.rejection_count += 1;
        }

        for reason in &decision.halt_reasons {
            increment(&mut self.halt_reason_counts, risk_halt_reason_key(reason));
        }

        self.decisions.push(decision.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RiskReplayRecord {
    pub market_id: Option<String>,
    pub asset: Option<Asset>,
    pub approved: bool,
    pub halt_reasons: Vec<RiskHaltReason>,
    pub messages: Vec<String>,
    pub updated_ts: Option<i64>,
}

impl RiskReplayRecord {
    pub fn from_gate_decision(
        market_id: Option<String>,
        asset: Option<Asset>,
        decision: &RiskGateDecision,
    ) -> Self {
        let mut halt_reasons = decision
            .risk_state
            .active_halts
            .iter()
            .chain(
                decision
                    .violations
                    .iter()
                    .map(|violation| &violation.reason),
            )
            .cloned()
            .collect::<Vec<_>>();
        halt_reasons
            .sort_by(|left, right| risk_halt_reason_key(left).cmp(risk_halt_reason_key(right)));
        halt_reasons.dedup();

        let mut messages = decision
            .violations
            .iter()
            .map(|violation| violation.message.clone())
            .collect::<Vec<_>>();
        if let Some(reason) = decision.risk_state.reason.as_ref() {
            messages.push(reason.clone());
        }
        messages.sort();
        messages.dedup();

        Self {
            market_id,
            asset,
            approved: decision.approved,
            halt_reasons,
            messages,
            updated_ts: Some(decision.risk_state.updated_ts),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PaperReport {
    pub order_count: u64,
    pub fill_count: u64,
    pub cancel_count: u64,
    pub reject_count: u64,
    pub expiration_count: u64,
    pub risk_rejection_count: u64,
    pub order_status_counts: BTreeMap<String, u64>,
    pub audit_event_counts: BTreeMap<String, u64>,
    pub total_filled_size: f64,
    pub total_filled_notional: f64,
    pub total_fees_paid: f64,
    pub orders: Vec<PaperOrder>,
    pub fills: Vec<PaperFill>,
    pub cancellations: Vec<PaperOrderTransition>,
    pub rejections: Vec<PaperOrderTransition>,
    pub expirations: Vec<PaperOrderTransition>,
    pub risk_rejections: Vec<PaperRiskRejection>,
}

impl PaperReport {
    pub fn record_order(&mut self, order: &PaperOrder) {
        self.order_count += 1;
        increment(
            &mut self.order_status_counts,
            paper_order_status_key(order.status),
        );
        self.orders.push(order.clone());
    }

    pub fn record_fill(&mut self, fill: &PaperFill) {
        self.fill_count += 1;
        self.total_filled_size += fill.size;
        self.total_filled_notional += fill.price * fill.size;
        self.total_fees_paid += fill.fee_paid;
        self.fills.push(fill.clone());
    }

    pub fn record_audit_event(&mut self, event: &PaperExecutionAuditEvent) {
        increment(&mut self.audit_event_counts, paper_audit_event_key(event));

        match event {
            PaperExecutionAuditEvent::RiskRejected {
                market_id,
                token_id,
                reason,
            } => {
                self.risk_rejection_count += 1;
                self.risk_rejections.push(PaperRiskRejection {
                    market_id: market_id.clone(),
                    token_id: token_id.clone(),
                    reason: reason.clone(),
                });
            }
            PaperExecutionAuditEvent::OrderRejected {
                order_id,
                reason,
                rejected_ts,
            } => {
                self.reject_count += 1;
                self.rejections.push(PaperOrderTransition {
                    order_id: order_id.clone(),
                    reason: reason.clone(),
                    ts: *rejected_ts,
                });
            }
            PaperExecutionAuditEvent::OrderCanceled {
                order_id,
                reason,
                canceled_ts,
            } => {
                self.cancel_count += 1;
                self.cancellations.push(PaperOrderTransition {
                    order_id: order_id.clone(),
                    reason: reason.clone(),
                    ts: *canceled_ts,
                });
            }
            PaperExecutionAuditEvent::OrderExpired {
                order_id,
                reason,
                expired_ts,
            } => {
                self.expiration_count += 1;
                self.expirations.push(PaperOrderTransition {
                    order_id: order_id.clone(),
                    reason: reason.clone(),
                    ts: *expired_ts,
                });
            }
            PaperExecutionAuditEvent::OrderCreated { .. }
            | PaperExecutionAuditEvent::OrderOpened { .. }
            | PaperExecutionAuditEvent::FillSimulated { .. } => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaperOrderTransition {
    pub order_id: String,
    pub reason: String,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaperRiskRejection {
    pub market_id: String,
    pub token_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct PnlReport {
    pub totals: PnlTotals,
    pub by_asset: BTreeMap<String, PnlTotals>,
    pub by_market: BTreeMap<String, PnlTotals>,
}

impl PnlReport {
    pub fn from_totals(
        gross_realized_pnl: f64,
        realized_pnl: f64,
        unrealized_pnl: f64,
        fees_paid: f64,
    ) -> Self {
        Self {
            totals: PnlTotals::new(gross_realized_pnl, realized_pnl, unrealized_pnl, fees_paid),
            by_asset: BTreeMap::new(),
            by_market: BTreeMap::new(),
        }
    }

    pub fn from_positions(positions: &[PositionSnapshot]) -> Self {
        Self::from_positions_and_fills(positions, &[])
    }

    pub fn from_positions_and_fills(positions: &[PositionSnapshot], fills: &[PaperFill]) -> Self {
        let mut report = Self::default();
        for position in positions {
            report.record_position(position);
        }
        for fill in fills {
            report.record_fill_fee(fill);
        }
        report
    }

    pub fn record_position(&mut self, position: &PositionSnapshot) {
        let totals = PnlTotals::new(
            position.realized_pnl,
            position.realized_pnl,
            position.unrealized_pnl,
            0.0,
        );
        self.totals.add(&totals);
        self.by_asset
            .entry(position.asset.symbol().to_string())
            .or_default()
            .add(&totals);
        self.by_market
            .entry(position.market_id.clone())
            .or_default()
            .add(&totals);
    }

    fn record_fill_fee(&mut self, fill: &PaperFill) {
        if fill.fee_paid == 0.0 {
            return;
        }
        self.totals.add_fee(fill.fee_paid);
        self.by_asset
            .entry(fill.asset.symbol().to_string())
            .or_default()
            .add_fee(fill.fee_paid);
        self.by_market
            .entry(fill.market_id.clone())
            .or_default()
            .add_fee(fill.fee_paid);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, Default)]
pub struct PnlTotals {
    pub gross_realized_pnl: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub fees_paid: f64,
    pub total_pnl: f64,
}

impl PnlTotals {
    pub fn new(
        gross_realized_pnl: f64,
        realized_pnl: f64,
        unrealized_pnl: f64,
        fees_paid: f64,
    ) -> Self {
        Self {
            gross_realized_pnl,
            realized_pnl,
            unrealized_pnl,
            fees_paid,
            total_pnl: realized_pnl + unrealized_pnl,
        }
    }

    fn add(&mut self, other: &Self) {
        self.gross_realized_pnl += other.gross_realized_pnl;
        self.realized_pnl += other.realized_pnl;
        self.unrealized_pnl += other.unrealized_pnl;
        self.fees_paid += other.fees_paid;
        self.total_pnl = self.realized_pnl + self.unrealized_pnl;
    }

    fn add_fee(&mut self, fee_paid: f64) {
        self.gross_realized_pnl += fee_paid;
        self.fees_paid += fee_paid;
        self.total_pnl = self.realized_pnl + self.unrealized_pnl;
    }
}

pub fn build_replay_report(input: ReplayReportInput) -> ReplayReport {
    ReplayReport::from_input(input)
}

pub fn deterministic_report_json(report: &ReplayReport) -> Vec<u8> {
    serde_json::to_vec(report).expect("replay report serialization should not fail")
}

pub fn determinism_fingerprint(report: &ReplayReport) -> String {
    let bytes = deterministic_report_json(report);
    let hash = digest(&SHA256, &bytes);
    format!("sha256:{}", lowercase_hex(hash.as_ref()))
}

fn increment(counts: &mut BTreeMap<String, u64>, key: &str) {
    *counts.entry(key.to_string()).or_insert(0) += 1;
}

fn min_i64_option(current: Option<i64>, candidate: i64) -> Option<i64> {
    Some(current.map_or(candidate, |current| current.min(candidate)))
}

fn max_i64_option(current: Option<i64>, candidate: i64) -> Option<i64> {
    Some(current.map_or(candidate, |current| current.max(candidate)))
}

fn lowercase_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(hex_char(byte >> 4));
        output.push(hex_char(byte & 0x0f));
    }
    output
}

fn hex_char(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => unreachable!("hex nibble is always <= 15"),
    }
}

fn order_kind_key(order_kind: OrderKind) -> &'static str {
    match order_kind {
        OrderKind::Maker => "maker",
        OrderKind::Taker => "taker",
    }
}

fn paper_order_status_key(status: PaperOrderStatus) -> &'static str {
    match status {
        PaperOrderStatus::Created => "created",
        PaperOrderStatus::Open => "open",
        PaperOrderStatus::PartiallyFilled => "partially_filled",
        PaperOrderStatus::Filled => "filled",
        PaperOrderStatus::Canceled => "canceled",
        PaperOrderStatus::Expired => "expired",
        PaperOrderStatus::Rejected => "rejected",
    }
}

fn paper_audit_event_key(event: &PaperExecutionAuditEvent) -> &'static str {
    match event {
        PaperExecutionAuditEvent::RiskRejected { .. } => "risk_rejected",
        PaperExecutionAuditEvent::OrderCreated { .. } => "order_created",
        PaperExecutionAuditEvent::OrderOpened { .. } => "order_opened",
        PaperExecutionAuditEvent::OrderRejected { .. } => "order_rejected",
        PaperExecutionAuditEvent::OrderCanceled { .. } => "order_canceled",
        PaperExecutionAuditEvent::OrderExpired { .. } => "order_expired",
        PaperExecutionAuditEvent::FillSimulated { .. } => "fill_simulated",
    }
}

fn signal_skip_reason_key(reason: SignalSkipReason) -> &'static str {
    match reason {
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

fn risk_halt_reason_key(reason: &RiskHaltReason) -> &'static str {
    match reason {
        RiskHaltReason::Geoblocked => "geoblocked",
        RiskHaltReason::StaleReference => "stale_reference",
        RiskHaltReason::StaleBook => "stale_book",
        RiskHaltReason::MaxLossPerMarket => "max_loss_per_market",
        RiskHaltReason::MaxNotionalPerMarket => "max_notional_per_market",
        RiskHaltReason::MaxNotionalPerAsset => "max_notional_per_asset",
        RiskHaltReason::MaxTotalNotional => "max_total_notional",
        RiskHaltReason::MaxCorrelatedNotional => "max_correlated_notional",
        RiskHaltReason::OrderRateExceeded => "order_rate_exceeded",
        RiskHaltReason::DailyDrawdown => "daily_drawdown",
        RiskHaltReason::StorageUnavailable => "storage_unavailable",
        RiskHaltReason::IneligibleMarket => "ineligible_market",
        RiskHaltReason::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        FeeParameters, OrderBookSnapshot, PaperOrderStatus, ReferencePrice, RiskState, Side,
    };
    use crate::events::NormalizedEvent;

    #[test]
    fn fingerprint_is_stable_for_identical_reports() {
        let left = sample_report();
        let right = sample_report();

        assert_eq!(
            deterministic_report_json(&left),
            deterministic_report_json(&right)
        );
        assert_eq!(
            left.determinism_fingerprint(),
            right.determinism_fingerprint()
        );
        assert!(left.determinism_fingerprint().starts_with("sha256:"));
    }

    #[test]
    fn fingerprint_changes_when_report_field_drifts() {
        let baseline = sample_report();
        let mut drifted = sample_report();
        drifted.pnl.totals.realized_pnl += 0.01;
        drifted.pnl.totals.total_pnl += 0.01;

        assert_ne!(
            baseline.determinism_fingerprint(),
            drifted.determinism_fingerprint()
        );
    }

    #[test]
    fn report_input_builds_counts_and_audit_details() {
        let report = sample_report();

        assert_eq!(report.events.total_count, 3);
        assert_eq!(report.events.counts_by_type.get("book_snapshot"), Some(&1));
        assert_eq!(report.signals.evaluated_count, 1);
        assert_eq!(report.signals.emitted_order_intent_count, 1);
        assert_eq!(report.risk.rejection_count, 1);
        assert_eq!(report.risk.halt_reason_counts.get("stale_book"), Some(&1));
        assert_eq!(report.paper.order_count, 1);
        assert_eq!(report.paper.fill_count, 1);
        assert_eq!(report.paper.cancel_count, 1);
        assert_eq!(report.paper.total_fees_paid, 0.02);
        assert_eq!(report.diagnostics.latency.event_count_with_source_ts, 2);
        assert_eq!(report.diagnostics.latency.max_latency_ms, Some(1));
        assert_eq!(report.diagnostics.feed_staleness.window_count, 2);
        assert_eq!(report.diagnostics.opportunities.risk_rejection_count, 1);
        assert_eq!(report.diagnostics.opportunities.partial_fill_order_count, 1);
    }

    #[test]
    fn pnl_report_groups_positions_and_fees_by_asset_and_market() {
        let report = PnlReport::from_positions_and_fills(&[sample_position()], &[sample_fill()]);

        assert_close(report.totals.realized_pnl, 1.23);
        assert_close(report.totals.gross_realized_pnl, 1.25);
        assert_close(report.totals.fees_paid, 0.02);
        assert_close(report.totals.total_pnl, 1.63);
        assert_eq!(
            report.by_asset.get("BTC").map(|totals| totals.fees_paid),
            Some(0.02)
        );
        assert_eq!(
            report
                .by_market
                .get("market-1")
                .map(|totals| totals.unrealized_pnl),
            Some(0.40)
        );
    }

    #[test]
    fn report_metadata_can_label_pyth_proxy_as_not_live_readiness_evidence() {
        let mut report = sample_report();
        report.metadata.reference_feed_mode = Some("pyth_proxy".to_string());
        report.metadata.reference_provider = Some("pyth".to_string());
        report.metadata.matches_market_resolution_source = Some(false);
        report.metadata.live_readiness_evidence = false;
        report.metadata.settlement_reference_evidence = false;

        assert_eq!(
            report.metadata.reference_feed_mode.as_deref(),
            Some("pyth_proxy")
        );
        assert_eq!(report.metadata.reference_provider.as_deref(), Some("pyth"));
        assert_eq!(
            report.metadata.matches_market_resolution_source,
            Some(false)
        );
        assert!(!report.metadata.live_readiness_evidence);
        assert!(!report.metadata.settlement_reference_evidence);
    }

    fn sample_report() -> ReplayReport {
        build_replay_report(ReplayReportInput {
            metadata: ReplayRunMetadata {
                run_id: "source-run-1".to_string(),
                replay_run_id: "replay-run-1".to_string(),
                input_source: Some("fixture".to_string()),
                input_fingerprint: Some("sha256:input".to_string()),
                config_fingerprint: Some("sha256:config".to_string()),
                code_version: Some("unit-test".to_string()),
                evidence_type: None,
                live_market_evidence: None,
                started_wall_ts: Some(1_777_000_000_000),
                completed_wall_ts: Some(1_777_000_001_000),
                first_event_recv_wall_ts: None,
                last_event_recv_wall_ts: None,
                first_event_source_ts: None,
                last_event_source_ts: None,
                source_timestamp_regressions: 0,
                reference_feed_mode: Some("none".to_string()),
                reference_provider: None,
                matches_market_resolution_source: None,
                live_readiness_evidence: false,
                settlement_reference_evidence: false,
            },
            feed_stale_after_ms: Some(5),
            events: sample_events(),
            signals: vec![SignalReplayRecord::new(sample_signal(), true, Vec::new())],
            risk_decisions: vec![RiskReplayRecord {
                market_id: Some("market-1".to_string()),
                asset: Some(Asset::Btc),
                approved: false,
                halt_reasons: vec![RiskHaltReason::StaleBook],
                messages: vec!["book stale".to_string()],
                updated_ts: Some(1_777_000_000_020),
            }],
            paper_orders: vec![sample_order()],
            paper_fills: vec![sample_fill()],
            paper_audit_events: vec![PaperExecutionAuditEvent::OrderCanceled {
                order_id: "paper-order-1".to_string(),
                reason: "market resolved".to_string(),
                canceled_ts: 1_777_000_000_030,
            }],
            pnl: PnlReport::from_totals(1.25, 1.23, 0.40, 0.02),
        })
    }

    fn sample_events() -> Vec<EventEnvelope> {
        vec![
            EventEnvelope::new(
                "source-run-1",
                "event-1",
                "unit-test",
                1_777_000_000_001,
                10,
                1,
                NormalizedEvent::BookSnapshot {
                    book: OrderBookSnapshot {
                        market_id: "market-1".to_string(),
                        token_id: "token-up".to_string(),
                        bids: Vec::new(),
                        asks: Vec::new(),
                        hash: Some("book-hash".to_string()),
                        source_ts: Some(1_777_000_000_000),
                    },
                },
            ),
            EventEnvelope::new(
                "source-run-1",
                "event-2",
                "unit-test",
                1_777_000_000_010,
                20,
                2,
                NormalizedEvent::ReferenceTick {
                    price: ReferencePrice {
                        asset: Asset::Btc,
                        source: "chainlink".to_string(),
                        price: 65_000.0,
                        confidence: None,
                        provider: None,
                        matches_market_resolution_source: None,
                        source_ts: Some(1_777_000_000_009),
                        recv_wall_ts: 1_777_000_000_010,
                    },
                },
            ),
            EventEnvelope::new(
                "source-run-1",
                "event-3",
                "unit-test",
                1_777_000_000_020,
                30,
                3,
                NormalizedEvent::RiskHalt {
                    market_id: Some("market-1".to_string()),
                    asset: Some(Asset::Btc),
                    risk_state: RiskState {
                        halted: true,
                        active_halts: vec![RiskHaltReason::StaleBook],
                        reason: Some("book stale".to_string()),
                        updated_ts: 1_777_000_000_020,
                    },
                },
            ),
        ]
    }

    fn sample_signal() -> SignalDecision {
        SignalDecision {
            asset: Asset::Btc,
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            outcome: "Up".to_string(),
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            price: 0.50,
            size: 10.0,
            notional: 5.0,
            fair_probability: 0.55,
            market_probability: 0.50,
            expected_value_bps: 75.0,
            reason: "candidate:maker:phase=main:net_ev_bps=75.00:required_edge_bps=50.00"
                .to_string(),
            required_inputs: vec!["fresh_book".to_string()],
            created_ts: 1_777_000_000_015,
        }
    }

    fn sample_order() -> PaperOrder {
        PaperOrder {
            order_id: "paper-order-1".to_string(),
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            fee_parameters: fee_parameters(),
            price: 0.50,
            size: 10.0,
            filled_size: 4.0,
            status: PaperOrderStatus::PartiallyFilled,
            reason: "unit order".to_string(),
            created_ts: 1_777_000_000_016,
            updated_ts: 1_777_000_000_017,
        }
    }

    fn sample_fill() -> PaperFill {
        PaperFill {
            fill_id: "paper-fill-1".to_string(),
            order_id: "paper-order-1".to_string(),
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            price: 0.50,
            size: 4.0,
            fee_paid: 0.02,
            liquidity: OrderKind::Maker,
            filled_ts: 1_777_000_000_018,
        }
    }

    fn sample_position() -> PositionSnapshot {
        PositionSnapshot {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            size: 4.0,
            average_price: 0.50,
            realized_pnl: 1.23,
            unrealized_pnl: 0.40,
            updated_ts: 1_777_000_000_019,
        }
    }

    fn fee_parameters() -> FeeParameters {
        FeeParameters {
            fees_enabled: true,
            maker_fee_bps: 0.0,
            taker_fee_bps: 200.0,
            raw_fee_config: None,
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9,
            "actual={actual} expected={expected}"
        );
    }
}
