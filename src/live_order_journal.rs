use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::live_balance_tracker::{LiveBalanceSnapshot, LiveBalanceTracker};
use crate::live_position_book::{LivePosition, LivePositionBook};
use crate::secret_handling::REDACTED_VALUE;

pub const MODULE: &str = "live_order_journal";
pub const LIVE_JOURNAL_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveJournalEventType {
    LiveIntentCreated,
    LiveIntentRejectedByRisk,
    LiveIntentApprovedByRisk,
    LiveOrderSubmitRequested,
    LiveOrderSubmitAccepted,
    LiveOrderSubmitRejected,
    LiveOrderReadbackObserved,
    LiveOrderPartiallyFilled,
    LiveOrderFilled,
    LiveOrderCancelRequested,
    LiveOrderCancelAccepted,
    LiveOrderCancelRejected,
    LiveOrderCanceled,
    LiveOrderExpired,
    LiveTradeObserved,
    LiveTradeMatched,
    LiveTradeMined,
    LiveTradeConfirmed,
    LiveTradeFailed,
    LiveBalanceSnapshot,
    LiveBalanceDeltaObserved,
    LiveReservedBalanceObserved,
    LivePositionOpened,
    LivePositionReduced,
    LivePositionClosed,
    LiveSettlementObserved,
    LiveReconciliationPassed,
    LiveReconciliationMismatch,
    LiveRiskHalt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStatus {
    Clean,
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveJournalEvent {
    pub schema_version: u16,
    pub run_id: String,
    pub event_id: String,
    pub event_type: LiveJournalEventType,
    pub created_at: i64,
    pub payload: Value,
    pub redaction_status: RedactionStatus,
}

impl LiveJournalEvent {
    pub fn new(
        run_id: impl Into<String>,
        event_id: impl Into<String>,
        event_type: LiveJournalEventType,
        created_at: i64,
        payload: Value,
    ) -> Self {
        let (payload, redaction_status) = redact_payload(payload);
        Self {
            schema_version: LIVE_JOURNAL_SCHEMA_VERSION,
            run_id: run_id.into(),
            event_id: event_id.into(),
            event_type,
            created_at,
            payload,
            redaction_status,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveOrderJournal {
    path: PathBuf,
}

impl LiveOrderJournal {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn append(&self, event: &LiveJournalEvent) -> LiveJournalResult<()> {
        ensure_parent_dir(&self.path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| LiveJournalError::Io {
                operation: "append_live_journal_event",
                source,
            })?;
        serde_json::to_writer(&mut file, event).map_err(LiveJournalError::Serialize)?;
        file.write_all(b"\n")
            .and_then(|_| file.flush())
            .and_then(|_| file.sync_data())
            .map_err(|source| LiveJournalError::Io {
                operation: "append_live_journal_event",
                source,
            })?;
        Ok(())
    }

    pub fn replay(&self) -> LiveJournalResult<Vec<LiveJournalEvent>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&self.path).map_err(|source| LiveJournalError::Io {
            operation: "replay_live_journal",
            source,
        })?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line.map_err(|source| LiveJournalError::Io {
                operation: "replay_live_journal",
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let event = serde_json::from_str(&line).map_err(LiveJournalError::Serialize)?;
            events.push(event);
        }
        Ok(events)
    }

    pub fn replay_state(&self) -> LiveJournalResult<LiveJournalState> {
        let events = self.replay()?;
        Ok(reduce_live_journal_events(&events))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveOrderJournalState {
    pub event_count: usize,
    pub latest_status: Option<LiveJournalEventType>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveJournalState {
    pub intents: BTreeSet<String>,
    pub orders: BTreeMap<String, LiveOrderJournalState>,
    pub trades: BTreeSet<String>,
    pub trade_order_ids: BTreeSet<String>,
    pub trade_order_ids_by_trade: BTreeMap<String, String>,
    pub partially_filled_orders: BTreeSet<String>,
    pub canceled_orders: BTreeSet<String>,
    pub balance_tracker: LiveBalanceTracker,
    pub position_book: LivePositionBook,
    pub reconciliation_mismatch_count: usize,
    pub risk_halted: bool,
}

pub fn reduce_live_journal_events(events: &[LiveJournalEvent]) -> LiveJournalState {
    let mut state = LiveJournalState::default();

    for event in events {
        let event_order_id = payload_string(&event.payload, "order_id");
        if let Some(intent_id) = payload_string(&event.payload, "intent_id") {
            state.intents.insert(intent_id);
        }
        if event.event_type.creates_or_updates_venue_order_state() {
            if let Some(order_id) = &event_order_id {
                let order_state = state.orders.entry(order_id.clone()).or_default();
                order_state.event_count += 1;
                order_state.latest_status = Some(event.event_type);
            }
        }
        if let Some(order_id) = &event_order_id {
            match event.event_type {
                LiveJournalEventType::LiveOrderPartiallyFilled => {
                    state.partially_filled_orders.insert(order_id.clone());
                }
                LiveJournalEventType::LiveOrderCanceled => {
                    state.canceled_orders.insert(order_id.clone());
                }
                _ => {}
            }
        }
        if event.event_type.records_successful_trade_state() {
            if let Some(trade_id) = payload_string(&event.payload, "trade_id") {
                if let Some(order_id) = &event_order_id {
                    state.trade_order_ids.insert(order_id.clone());
                    state
                        .trade_order_ids_by_trade
                        .insert(trade_id.clone(), order_id.clone());
                }
                state.trades.insert(trade_id);
            }
        }
        if event.event_type == LiveJournalEventType::LiveBalanceSnapshot {
            if let Ok(snapshot) =
                serde_json::from_value::<LiveBalanceSnapshot>(event.payload.clone())
            {
                state.balance_tracker.apply_snapshot(snapshot);
            }
        }
        if matches!(
            event.event_type,
            LiveJournalEventType::LivePositionOpened
                | LiveJournalEventType::LivePositionReduced
                | LiveJournalEventType::LivePositionClosed
        ) {
            if let Ok(position) = serde_json::from_value::<LivePosition>(event.payload.clone()) {
                state.position_book.upsert_position(position);
            }
        }
        if event.event_type == LiveJournalEventType::LiveReconciliationMismatch {
            state.reconciliation_mismatch_count += 1;
        }
        if event.event_type == LiveJournalEventType::LiveRiskHalt {
            state.risk_halted = true;
        }
    }

    state
}

impl LiveJournalEventType {
    fn creates_or_updates_venue_order_state(self) -> bool {
        matches!(
            self,
            Self::LiveOrderSubmitAccepted
                | Self::LiveOrderReadbackObserved
                | Self::LiveOrderPartiallyFilled
                | Self::LiveOrderFilled
                | Self::LiveOrderCancelRequested
                | Self::LiveOrderCancelAccepted
                | Self::LiveOrderCancelRejected
                | Self::LiveOrderCanceled
                | Self::LiveOrderExpired
                | Self::LiveTradeMatched
                | Self::LiveTradeMined
                | Self::LiveTradeConfirmed
        )
    }

    fn records_successful_trade_state(self) -> bool {
        matches!(
            self,
            Self::LiveTradeMatched | Self::LiveTradeMined | Self::LiveTradeConfirmed
        )
    }
}

pub fn redact_payload(payload: Value) -> (Value, RedactionStatus) {
    let mut redacted = false;
    let payload = redact_value(payload, None, &mut redacted);
    let status = if redacted {
        RedactionStatus::Redacted
    } else {
        RedactionStatus::Clean
    };
    (payload, status)
}

fn redact_value(value: Value, key: Option<&str>, redacted: &mut bool) -> Value {
    if key.is_some_and(is_sensitive_key) {
        *redacted = true;
        return Value::String(REDACTED_VALUE.to_string());
    }

    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let value = redact_value(value, Some(&key), redacted);
                    (key, value)
                })
                .collect::<Map<_, _>>(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| redact_value(value, None, redacted))
                .collect(),
        ),
        value => value,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "private_key",
        "secret",
        "api_key",
        "passphrase",
        "signature",
        "auth",
        "credential",
        "mnemonic",
        "seed",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn payload_string(payload: &Value, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

fn ensure_parent_dir(path: &Path) -> LiveJournalResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| LiveJournalError::Io {
            operation: "create_live_journal_dir",
            source,
        })?;
    }
    Ok(())
}

pub type LiveJournalResult<T> = Result<T, LiveJournalError>;

#[derive(Debug)]
pub enum LiveJournalError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Serialize(serde_json::Error),
}

impl Display for LiveJournalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { operation, source } => {
                write!(formatter, "live journal {operation} failed: {source}")
            }
            Self::Serialize(source) => {
                write!(formatter, "live journal serialization failed: {source}")
            }
        }
    }
}

impl Error for LiveJournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Serialize(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Asset;
    use crate::live_position_book::{LivePosition, LivePositionKey};

    #[test]
    fn live_order_journal_appends_replays_and_reconstructs_state() {
        let path = temp_path("journal_replay");
        let journal = LiveOrderJournal::new(&path);
        let order_event = LiveJournalEvent::new(
            "run-1",
            "event-1",
            LiveJournalEventType::LiveOrderReadbackObserved,
            1,
            serde_json::json!({"order_id":"order-1","intent_id":"intent-1","status":"live"}),
        );
        let balance_event = LiveJournalEvent::new(
            "run-1",
            "event-2",
            LiveJournalEventType::LiveBalanceSnapshot,
            2,
            serde_json::json!({
                "p_usd_available": 10.0,
                "p_usd_reserved": 0.0,
                "p_usd_total": 10.0,
                "conditional_token_positions": {},
                "balance_snapshot_at": 2,
                "source": "fixture"
            }),
        );
        let trade_event = LiveJournalEvent::new(
            "run-1",
            "event-3",
            LiveJournalEventType::LiveTradeConfirmed,
            3,
            serde_json::json!({"order_id":"order-1","trade_id":"trade-1","status":"confirmed"}),
        );

        journal.append(&order_event).expect("order event appends");
        journal
            .append(&balance_event)
            .expect("balance event appends");
        journal.append(&trade_event).expect("trade event appends");

        let events = journal.replay().expect("journal replays");
        let state = reduce_live_journal_events(&events);

        assert_eq!(events.len(), 3);
        assert!(state.intents.contains("intent-1"));
        assert!(state.orders.contains_key("order-1"));
        assert!(state.trades.contains("trade-1"));
        assert!(state.trade_order_ids.contains("order-1"));
        assert_eq!(
            state.trade_order_ids_by_trade.get("trade-1"),
            Some(&"order-1".to_string())
        );
        assert_eq!(state.balance_tracker.snapshot_count(), 1);
    }

    #[test]
    fn live_order_journal_reducer_omits_rejected_submission_orders() {
        let events = vec![
            LiveJournalEvent::new(
                "run-1",
                "event-1",
                LiveJournalEventType::LiveOrderSubmitRequested,
                1,
                serde_json::json!({"order_id":"order-1","intent_id":"intent-1"}),
            ),
            LiveJournalEvent::new(
                "run-1",
                "event-2",
                LiveJournalEventType::LiveOrderSubmitRejected,
                2,
                serde_json::json!({"order_id":"order-1","intent_id":"intent-1","reason":"risk"}),
            ),
        ];

        let state = reduce_live_journal_events(&events);

        assert!(state.intents.contains("intent-1"));
        assert!(!state.orders.contains_key("order-1"));
    }

    #[test]
    fn live_order_journal_reducer_omits_failed_trades_from_fill_evidence() {
        let events = vec![LiveJournalEvent::new(
            "run-1",
            "event-1",
            LiveJournalEventType::LiveTradeFailed,
            1,
            serde_json::json!({"order_id":"order-1","trade_id":"trade-1","status":"failed"}),
        )];

        let state = reduce_live_journal_events(&events);

        assert!(!state.orders.contains_key("order-1"));
        assert!(!state.trades.contains("trade-1"));
        assert!(!state.trade_order_ids.contains("order-1"));
        assert!(!state.trade_order_ids_by_trade.contains_key("trade-1"));
    }

    #[test]
    fn live_order_journal_redacts_sensitive_payload_fields() {
        let event = LiveJournalEvent::new(
            "run-1",
            "event-1",
            LiveJournalEventType::LiveOrderSubmitRequested,
            1,
            serde_json::json!({
                "order_id": "order-1",
                "api_key": "do-not-commit",
                "nested": {"private_key": "also-secret"}
            }),
        );

        let rendered = serde_json::to_string(&event).expect("event serializes");

        assert_eq!(event.redaction_status, RedactionStatus::Redacted);
        assert!(!rendered.contains("do-not-commit"));
        assert!(!rendered.contains("also-secret"));
        assert!(rendered.contains(REDACTED_VALUE));
    }

    #[test]
    fn live_order_journal_reducer_tracks_positions_and_halts() {
        let position = LivePosition {
            key: LivePositionKey {
                market_id: "market-1".to_string(),
                token_id: "token-up".to_string(),
                asset: Asset::Btc,
                outcome: "Up".to_string(),
            },
            size: 5.0,
            average_price: 0.42,
            fees_paid: 0.01,
            updated_at: 1,
        };
        let events = vec![
            LiveJournalEvent::new(
                "run-1",
                "event-1",
                LiveJournalEventType::LivePositionOpened,
                1,
                serde_json::to_value(&position).expect("position serializes"),
            ),
            LiveJournalEvent::new(
                "run-1",
                "event-2",
                LiveJournalEventType::LiveRiskHalt,
                2,
                serde_json::json!({"reason":"fixture"}),
            ),
        ];

        let state = reduce_live_journal_events(&events);

        assert_eq!(state.position_book.positions().len(), 1);
        assert!(state.risk_halted);
    }

    fn temp_path(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "p15m-live-order-journal-{label}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        path
    }
}
