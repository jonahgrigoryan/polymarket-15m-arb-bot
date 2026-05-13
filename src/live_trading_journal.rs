use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_trading_journal";
pub const LIVE_TRADING_JOURNAL_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingJournalEvent {
    pub schema_version: u16,
    pub run_id: String,
    pub event_id: String,
    pub recorded_at_ms: i64,
    #[serde(flatten)]
    pub event: LiveTradingJournalEventKind,
}

impl LiveTradingJournalEvent {
    pub fn new(
        run_id: impl Into<String>,
        event_id: impl Into<String>,
        recorded_at_ms: i64,
        event: LiveTradingJournalEventKind,
    ) -> Self {
        Self {
            schema_version: LIVE_TRADING_JOURNAL_SCHEMA_VERSION,
            run_id: run_id.into(),
            event_id: event_id.into(),
            recorded_at_ms,
            event,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "payload", rename_all = "snake_case")]
pub enum LiveTradingJournalEventKind {
    MakerOrderIntended(IntendedMakerOrder),
    CancelIntended(IntendedCancel),
    AcceptedOrderObserved(AcceptedOrder),
    FillObserved(FillRecord),
    FeeObserved(FeeRecord),
    BalanceObserved(BalanceRecord),
    PositionObserved(PositionRecord),
    SettlementObserved(SettlementRecord),
    IncidentOpened(IncidentRecord),
    IncidentReviewed(IncidentReview),
    ReconciliationPassed(ReconciliationMarker),
    ReconciliationHalted(ReconciliationHalt),
    HeartbeatObserved(FreshnessObservation),
    GeoblockObserved(FreshnessObservation),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntendedMakerOrder {
    pub intent_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub price_microusd: u64,
    pub size_units: u64,
    pub post_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntendedCancel {
    pub cancel_intent_id: String,
    pub venue_order_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedOrder {
    pub intent_id: String,
    pub venue_order_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FillRecord {
    pub trade_id: String,
    pub venue_order_id: String,
    pub market: String,
    pub asset_id: String,
    pub size_units: u64,
    pub price_microusd: u64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeRecord {
    pub trade_id: String,
    pub fee_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceRecord {
    pub available_pusd_units: u64,
    pub reserved_pusd_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionRecord {
    pub asset_id: String,
    pub size_units: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementRecord {
    pub market: String,
    pub status: String,
    pub payout_units: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentRecord {
    pub incident_id: String,
    pub reason: String,
    pub related_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentReview {
    pub incident_id: String,
    pub reviewed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationMarker {
    pub reconciliation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationHalt {
    pub reconciliation_id: String,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreshnessObservation {
    pub status: String,
    pub observed_at_ms: i64,
    pub max_age_ms: i64,
    pub age_ms: i64,
}

impl FreshnessObservation {
    pub fn fresh(observed_at_ms: i64) -> Self {
        Self {
            status: "fresh".to_string(),
            observed_at_ms,
            max_age_ms: 15_000,
            age_ms: 0,
        }
    }

    pub fn stale(observed_at_ms: i64, age_ms: i64) -> Self {
        Self {
            status: "stale".to_string(),
            observed_at_ms,
            max_age_ms: 15_000,
            age_ms,
        }
    }

    pub fn is_fresh(&self) -> bool {
        self.status == "fresh" && self.age_ms <= self.max_age_ms
    }

    pub fn is_fresh_at(&self, checked_at_ms: i64) -> bool {
        self.is_fresh()
            && self.max_age_ms >= 0
            && checked_at_ms
                .checked_sub(self.observed_at_ms)
                .is_some_and(|age_ms| age_ms >= 0 && age_ms <= self.max_age_ms)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingJournalState {
    pub event_count: usize,
    pub intended_maker_orders: BTreeMap<String, IntendedMakerOrder>,
    pub accepted_orders: BTreeMap<String, AcceptedOrder>,
    pub intended_cancels: BTreeMap<String, IntendedCancel>,
    pub fills: BTreeMap<String, FillRecord>,
    pub fees: BTreeMap<String, FeeRecord>,
    pub fee_total_units: u64,
    pub latest_balance: Option<BalanceRecord>,
    pub positions: BTreeMap<String, PositionRecord>,
    pub settlements: BTreeMap<String, SettlementRecord>,
    pub incidents: BTreeMap<String, IncidentRecord>,
    pub reviewed_incidents: BTreeSet<String>,
    pub reconciliation_passed: bool,
    pub reconciliation_halted: bool,
    pub latest_heartbeat: Option<FreshnessObservation>,
    pub latest_geoblock: Option<FreshnessObservation>,
}

impl LiveTradingJournalState {
    pub fn has_unreviewed_incidents(&self) -> bool {
        self.incidents
            .keys()
            .any(|incident_id| !self.reviewed_incidents.contains(incident_id))
    }

    pub fn previous_order_reconciled_or_incident_reviewed(&self) -> bool {
        self.intended_maker_orders.is_empty()
            || self.reconciliation_passed
            || (self.reconciliation_halted && !self.has_unreviewed_incidents())
    }

    pub fn accepted_order_ids(&self) -> BTreeSet<String> {
        self.accepted_orders.keys().cloned().collect()
    }
}

#[derive(Debug)]
pub enum LiveTradingJournalError {
    SchemaVersionMismatch {
        event_id: String,
        schema_version: u16,
    },
}

impl Display for LiveTradingJournalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SchemaVersionMismatch {
                event_id,
                schema_version,
            } => write!(
                formatter,
                "live trading journal event {event_id} has unsupported schema version {schema_version}"
            ),
        }
    }
}

impl Error for LiveTradingJournalError {}

pub fn reduce_live_trading_journal_events(
    events: &[LiveTradingJournalEvent],
) -> Result<LiveTradingJournalState, LiveTradingJournalError> {
    let mut state = LiveTradingJournalState::default();

    for event in events {
        if event.schema_version != LIVE_TRADING_JOURNAL_SCHEMA_VERSION {
            return Err(LiveTradingJournalError::SchemaVersionMismatch {
                event_id: event.event_id.clone(),
                schema_version: event.schema_version,
            });
        }
        state.event_count += 1;
        match &event.event {
            LiveTradingJournalEventKind::MakerOrderIntended(order) => {
                state
                    .intended_maker_orders
                    .insert(order.intent_id.clone(), order.clone());
                state.reconciliation_passed = false;
                state.reconciliation_halted = false;
            }
            LiveTradingJournalEventKind::CancelIntended(cancel) => {
                state
                    .intended_cancels
                    .insert(cancel.cancel_intent_id.clone(), cancel.clone());
            }
            LiveTradingJournalEventKind::AcceptedOrderObserved(order) => {
                state
                    .accepted_orders
                    .insert(order.venue_order_id.clone(), order.clone());
            }
            LiveTradingJournalEventKind::FillObserved(fill) => {
                state.fills.insert(fill.trade_id.clone(), fill.clone());
            }
            LiveTradingJournalEventKind::FeeObserved(fee) => {
                state.fee_total_units = state.fee_total_units.saturating_add(fee.fee_units);
                state.fees.insert(fee.trade_id.clone(), fee.clone());
            }
            LiveTradingJournalEventKind::BalanceObserved(balance) => {
                state.latest_balance = Some(balance.clone());
            }
            LiveTradingJournalEventKind::PositionObserved(position) => {
                state
                    .positions
                    .insert(position.asset_id.clone(), position.clone());
            }
            LiveTradingJournalEventKind::SettlementObserved(settlement) => {
                state
                    .settlements
                    .insert(settlement.market.clone(), settlement.clone());
            }
            LiveTradingJournalEventKind::IncidentOpened(incident) => {
                state
                    .incidents
                    .insert(incident.incident_id.clone(), incident.clone());
            }
            LiveTradingJournalEventKind::IncidentReviewed(review) => {
                state.reviewed_incidents.insert(review.incident_id.clone());
            }
            LiveTradingJournalEventKind::ReconciliationPassed(_) => {
                state.reconciliation_passed = true;
                state.reconciliation_halted = false;
            }
            LiveTradingJournalEventKind::ReconciliationHalted(_) => {
                state.reconciliation_halted = true;
                state.reconciliation_passed = false;
            }
            LiveTradingJournalEventKind::HeartbeatObserved(observation) => {
                state.latest_heartbeat = Some(observation.clone());
            }
            LiveTradingJournalEventKind::GeoblockObserved(observation) => {
                state.latest_geoblock = Some(observation.clone());
            }
        }
    }

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_trading_journal_reducer_tracks_intended_activity_and_incidents() {
        let events = vec![
            event(
                "event-1",
                LiveTradingJournalEventKind::MakerOrderIntended(sample_order()),
            ),
            event(
                "event-2",
                LiveTradingJournalEventKind::AcceptedOrderObserved(sample_accepted_order()),
            ),
            event(
                "event-3",
                LiveTradingJournalEventKind::FillObserved(sample_fill()),
            ),
            event(
                "event-4",
                LiveTradingJournalEventKind::FeeObserved(FeeRecord {
                    trade_id: "trade-1".to_string(),
                    fee_units: 17,
                }),
            ),
            event(
                "event-5",
                LiveTradingJournalEventKind::IncidentOpened(IncidentRecord {
                    incident_id: "incident-1".to_string(),
                    reason: "unexpected_fill".to_string(),
                    related_id: Some("trade-1".to_string()),
                }),
            ),
            event(
                "event-6",
                LiveTradingJournalEventKind::IncidentReviewed(IncidentReview {
                    incident_id: "incident-1".to_string(),
                    reviewed_at_ms: 2,
                }),
            ),
            event(
                "event-7",
                LiveTradingJournalEventKind::ReconciliationHalted(ReconciliationHalt {
                    reconciliation_id: "recon-1".to_string(),
                    mismatches: vec!["unexpected_fill".to_string()],
                }),
            ),
        ];

        let state = reduce_live_trading_journal_events(&events).expect("journal reduces");

        assert_eq!(state.event_count, 7);
        assert!(state.intended_maker_orders.contains_key("intent-1"));
        assert!(state.accepted_orders.contains_key("order-1"));
        assert!(state.fills.contains_key("trade-1"));
        assert_eq!(state.fee_total_units, 17);
        assert!(!state.has_unreviewed_incidents());
        assert!(state.previous_order_reconciled_or_incident_reviewed());
    }

    #[test]
    fn live_trading_journal_rejects_schema_version_mismatch() {
        let mut event = event(
            "event-1",
            LiveTradingJournalEventKind::MakerOrderIntended(sample_order()),
        );
        event.schema_version = 999;

        let error = reduce_live_trading_journal_events(&[event]).expect_err("schema mismatch");

        assert!(matches!(
            error,
            LiveTradingJournalError::SchemaVersionMismatch { .. }
        ));
    }

    #[test]
    fn live_trading_journal_reducer_resets_reconciliation_for_new_intent() {
        let mut second_order = sample_order();
        second_order.intent_id = "intent-2".to_string();

        let events = vec![
            event(
                "event-1",
                LiveTradingJournalEventKind::MakerOrderIntended(sample_order()),
            ),
            event(
                "event-2",
                LiveTradingJournalEventKind::ReconciliationPassed(ReconciliationMarker {
                    reconciliation_id: "recon-1".to_string(),
                }),
            ),
            event(
                "event-3",
                LiveTradingJournalEventKind::MakerOrderIntended(second_order),
            ),
        ];

        let state = reduce_live_trading_journal_events(&events).expect("journal reduces");

        assert!(!state.reconciliation_passed);
        assert!(!state.reconciliation_halted);
        assert!(!state.previous_order_reconciled_or_incident_reviewed());
    }

    pub(crate) fn sample_order() -> IntendedMakerOrder {
        IntendedMakerOrder {
            intent_id: "intent-1".to_string(),
            market: "condition-1".to_string(),
            asset_id: "asset-yes".to_string(),
            side: "BUY".to_string(),
            price_microusd: 490_000,
            size_units: 1_000_000,
            post_only: true,
        }
    }

    pub(crate) fn sample_accepted_order() -> AcceptedOrder {
        AcceptedOrder {
            intent_id: "intent-1".to_string(),
            venue_order_id: "order-1".to_string(),
            status: "ORDER_STATUS_LIVE".to_string(),
        }
    }

    pub(crate) fn sample_fill() -> FillRecord {
        FillRecord {
            trade_id: "trade-1".to_string(),
            venue_order_id: "order-1".to_string(),
            market: "condition-1".to_string(),
            asset_id: "asset-yes".to_string(),
            size_units: 1_000_000,
            price_microusd: 490_000,
            status: "TRADE_STATUS_CONFIRMED".to_string(),
        }
    }

    fn event(event_id: &str, event: LiveTradingJournalEventKind) -> LiveTradingJournalEvent {
        LiveTradingJournalEvent::new("lt2-test-run", event_id, 1, event)
    }
}
