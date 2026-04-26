use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;
use crate::domain::{Market, PaperFill, PaperOrder, RiskState};
use crate::events::EventEnvelope;

pub const MODULE: &str = "storage";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RawMessage {
    pub run_id: String,
    pub source: String,
    pub recv_wall_ts: i64,
    pub recv_mono_ns: u64,
    pub ingest_seq: u64,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ConfigSnapshot {
    pub run_id: String,
    pub captured_wall_ts: i64,
    pub config: Value,
}

impl ConfigSnapshot {
    pub fn from_config(
        run_id: impl Into<String>,
        captured_wall_ts: i64,
        config: &AppConfig,
    ) -> StorageResult<Self> {
        let config = serde_json::to_value(config).map_err(|source| StorageError::Serialize {
            operation: "serialize_config_snapshot",
            message: source.to_string(),
        })?;

        Ok(Self {
            run_id: run_id.into(),
            captured_wall_ts,
            config,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RiskEvent {
    pub run_id: String,
    pub event_id: String,
    pub risk_state: RiskState,
}

pub type StorageResult<T> = Result<T, StorageError>;

pub trait StorageBackend {
    fn append_raw_message(&self, message: RawMessage) -> StorageResult<()>;

    fn append_normalized_event(&self, event: EventEnvelope) -> StorageResult<()>;

    fn upsert_market(&self, market: Market) -> StorageResult<()>;

    fn insert_config_snapshot(&self, snapshot: ConfigSnapshot) -> StorageResult<()>;

    fn insert_paper_order(&self, order: PaperOrder) -> StorageResult<()>;

    fn insert_paper_fill(&self, fill: PaperFill) -> StorageResult<()>;

    fn insert_risk_event(&self, event: RiskEvent) -> StorageResult<()>;

    fn read_run_events(&self, run_id: &str) -> StorageResult<Vec<EventEnvelope>>;
}

#[derive(Debug)]
pub enum StorageError {
    Serialize {
        operation: &'static str,
        message: String,
    },
    Backend {
        operation: &'static str,
        message: String,
    },
}

impl StorageError {
    pub fn backend(operation: &'static str, message: impl Into<String>) -> Self {
        Self::Backend {
            operation,
            message: message.into(),
        }
    }

    pub fn operation(&self) -> &'static str {
        match self {
            StorageError::Serialize { operation, .. } | StorageError::Backend { operation, .. } => {
                operation
            }
        }
    }
}

impl Display for StorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Serialize { operation, message }
            | StorageError::Backend { operation, message } => {
                write!(formatter, "storage operation {operation} failed: {message}")
            }
        }
    }
}

impl Error for StorageError {}

#[derive(Debug, Default)]
pub struct InMemoryStorage {
    state: Mutex<InMemoryState>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    raw_messages: Vec<RawMessage>,
    events: Vec<EventEnvelope>,
    markets: HashMap<String, Market>,
    config_snapshots: HashMap<String, ConfigSnapshot>,
    paper_orders: HashMap<String, PaperOrder>,
    paper_fills: HashMap<String, PaperFill>,
    risk_events: Vec<RiskEvent>,
}

impl StorageBackend for InMemoryStorage {
    fn append_raw_message(&self, message: RawMessage) -> StorageResult<()> {
        self.with_state("append_raw_message", |state| {
            state.raw_messages.push(message);
            Ok(())
        })
    }

    fn append_normalized_event(&self, event: EventEnvelope) -> StorageResult<()> {
        self.with_state("append_normalized_event", |state| {
            state.events.push(event);
            Ok(())
        })
    }

    fn upsert_market(&self, market: Market) -> StorageResult<()> {
        self.with_state("upsert_market", |state| {
            state.markets.insert(market.market_id.clone(), market);
            Ok(())
        })
    }

    fn insert_config_snapshot(&self, snapshot: ConfigSnapshot) -> StorageResult<()> {
        self.with_state("insert_config_snapshot", |state| {
            state
                .config_snapshots
                .insert(snapshot.run_id.clone(), snapshot);
            Ok(())
        })
    }

    fn insert_paper_order(&self, order: PaperOrder) -> StorageResult<()> {
        self.with_state("insert_paper_order", |state| {
            state.paper_orders.insert(order.order_id.clone(), order);
            Ok(())
        })
    }

    fn insert_paper_fill(&self, fill: PaperFill) -> StorageResult<()> {
        self.with_state("insert_paper_fill", |state| {
            state.paper_fills.insert(fill.fill_id.clone(), fill);
            Ok(())
        })
    }

    fn insert_risk_event(&self, event: RiskEvent) -> StorageResult<()> {
        self.with_state("insert_risk_event", |state| {
            state.risk_events.push(event);
            Ok(())
        })
    }

    fn read_run_events(&self, run_id: &str) -> StorageResult<Vec<EventEnvelope>> {
        self.with_state("read_run_events", |state| {
            let mut events = state
                .events
                .iter()
                .filter(|event| event.run_id == run_id)
                .cloned()
                .collect::<Vec<_>>();
            events.sort_by(|left, right| {
                left.replay_ordering_key().cmp(&right.replay_ordering_key())
            });
            Ok(events)
        })
    }
}

impl InMemoryStorage {
    fn with_state<T>(
        &self,
        operation: &'static str,
        action: impl FnOnce(&mut InMemoryState) -> StorageResult<T>,
    ) -> StorageResult<T> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StorageError::backend(operation, "in-memory storage lock poisoned"))?;
        action(&mut state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::domain::{
        Asset, FeeParameters, MarketLifecycleState, OrderKind, OutcomeToken, PaperOrderStatus,
        RiskHaltReason, Side,
    };
    use crate::domain::{Market, PaperFill, PaperOrder, RiskState};
    use crate::events::{EventEnvelope, NormalizedEvent};

    const DEFAULT_CONFIG: &str = include_str!("../config/default.toml");
    const CLICKHOUSE_MIGRATION: &str = include_str!("../migrations/clickhouse/0001_events.sql");
    const POSTGRES_MIGRATION: &str =
        include_str!("../migrations/postgres/0001_relational_state.sql");

    #[test]
    fn migration_files_define_required_tables() {
        for expected in [
            "raw_messages",
            "normalized_events",
            "ORDER BY (run_id, recv_mono_ns, ingest_seq, event_id)",
        ] {
            assert!(
                CLICKHOUSE_MIGRATION.contains(expected),
                "ClickHouse migration missing {expected}"
            );
        }

        for expected in [
            "markets",
            "config_snapshots",
            "paper_orders",
            "paper_fills",
            "risk_events",
            "replay_runs",
        ] {
            assert!(
                POSTGRES_MIGRATION.contains(expected),
                "Postgres migration missing {expected}"
            );
        }
    }

    #[test]
    fn in_memory_storage_round_trips_sample_records() {
        let storage = InMemoryStorage::default();
        let market = sample_market();
        let order = sample_order();
        let fill = sample_fill();
        let risk_state = RiskState {
            halted: true,
            active_halts: vec![RiskHaltReason::StorageUnavailable],
            reason: Some("unit sample".to_string()),
            updated_ts: 1_777_000_000_000,
        };
        let event = EventEnvelope::new(
            "run-1",
            "event-1",
            "unit-test",
            1_777_000_000_000,
            2,
            3,
            NormalizedEvent::MarketDiscovered {
                market: market.clone(),
            },
        );
        let config: AppConfig = toml::from_str(DEFAULT_CONFIG).expect("default config parses");
        let snapshot =
            ConfigSnapshot::from_config("run-1", 1_777_000_000_001, &config).expect("snapshot");

        storage
            .append_raw_message(RawMessage {
                run_id: "run-1".to_string(),
                source: "unit-test".to_string(),
                recv_wall_ts: 1_777_000_000_000,
                recv_mono_ns: 1,
                ingest_seq: 1,
                payload: "{}".to_string(),
            })
            .expect("raw message writes");
        storage.upsert_market(market).expect("market writes");
        storage
            .insert_config_snapshot(snapshot)
            .expect("config snapshot writes");
        storage.insert_paper_order(order).expect("order writes");
        storage.insert_paper_fill(fill).expect("fill writes");
        storage
            .insert_risk_event(RiskEvent {
                run_id: "run-1".to_string(),
                event_id: "risk-1".to_string(),
                risk_state,
            })
            .expect("risk event writes");
        storage
            .append_normalized_event(event.clone())
            .expect("event writes");

        let events = storage.read_run_events("run-1").expect("events read");

        assert_eq!(events, vec![event]);
    }

    #[test]
    fn storage_error_exposes_operation_name() {
        let error = StorageError::backend("append_normalized_event", "database unavailable");

        assert_eq!(error.operation(), "append_normalized_event");
        assert!(error.to_string().contains("database unavailable"));
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
            start_ts: 1_777_000_000_000,
            end_ts: 1_777_000_900_000,
            resolution_source: Some("unit-resolution-source".to_string()),
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

    fn sample_order() -> PaperOrder {
        PaperOrder {
            order_id: "order-1".to_string(),
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            order_kind: OrderKind::Maker,
            price: 0.49,
            size: 10.0,
            filled_size: 0.0,
            status: PaperOrderStatus::Open,
            reason: "unit sample".to_string(),
            created_ts: 1_777_000_000_000,
            updated_ts: 1_777_000_000_000,
        }
    }

    fn sample_fill() -> PaperFill {
        PaperFill {
            fill_id: "fill-1".to_string(),
            order_id: "order-1".to_string(),
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            price: 0.49,
            size: 5.0,
            fee_paid: 0.0,
            liquidity: OrderKind::Maker,
            filled_ts: 1_777_000_000_001,
        }
    }
}
