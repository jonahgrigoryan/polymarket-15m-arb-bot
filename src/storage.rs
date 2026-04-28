use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use postgres::{Client, NoTls};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;
use crate::domain::{Market, PaperFill, PaperOrder, RiskState};
use crate::events::EventEnvelope;
use crate::state::PositionSnapshot;

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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PaperBalanceSnapshot {
    pub run_id: String,
    pub starting_balance: f64,
    pub cash_balance: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub updated_ts: i64,
}

pub type StorageResult<T> = Result<T, StorageError>;

pub trait StorageBackend {
    fn append_raw_message(&self, message: RawMessage) -> StorageResult<()>;

    fn append_normalized_event(&self, event: EventEnvelope) -> StorageResult<()>;

    fn upsert_market(&self, market: Market) -> StorageResult<()>;

    fn insert_config_snapshot(&self, snapshot: ConfigSnapshot) -> StorageResult<()>;

    fn read_config_snapshot(&self, run_id: &str) -> StorageResult<Option<ConfigSnapshot>>;

    fn insert_paper_order(&self, order: PaperOrder) -> StorageResult<()>;

    fn insert_paper_fill(&self, fill: PaperFill) -> StorageResult<()>;

    fn upsert_paper_position(&self, position: PositionSnapshot) -> StorageResult<()>;

    fn upsert_paper_balance(&self, balance: PaperBalanceSnapshot) -> StorageResult<()>;

    fn insert_risk_event(&self, event: RiskEvent) -> StorageResult<()>;

    fn read_run_events(&self, run_id: &str) -> StorageResult<Vec<EventEnvelope>>;
}

#[derive(Debug, Clone)]
pub struct FileSessionStorage {
    root: PathBuf,
    active_run_id: Option<String>,
}

impl FileSessionStorage {
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            root: output_dir.as_ref().join("sessions"),
            active_run_id: None,
        }
    }

    pub fn for_run(output_dir: impl AsRef<Path>, run_id: impl Into<String>) -> StorageResult<Self> {
        let run_id = run_id.into();
        validate_path_segment(&run_id, "run_id")?;
        Ok(Self {
            root: output_dir.as_ref().join("sessions"),
            active_run_id: Some(run_id),
        })
    }

    pub fn session_dir(&self, run_id: &str) -> StorageResult<PathBuf> {
        Ok(self.root.join(validate_path_segment(run_id, "run_id")?))
    }

    pub fn session_exists(&self, run_id: &str) -> StorageResult<bool> {
        Ok(self.session_dir(run_id)?.exists())
    }

    pub fn write_session_artifact(
        &self,
        run_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> StorageResult<PathBuf> {
        let file_name = validate_path_segment(file_name, "file_name")?;
        let path = self.session_dir(run_id)?.join(file_name);
        ensure_parent_dir(&path, "write_session_artifact")?;
        fs::write(&path, bytes).map_err(|source| {
            StorageError::backend("write_session_artifact", source.to_string())
        })?;
        Ok(path)
    }

    pub fn sync_session(&self, run_id: &str) -> StorageResult<()> {
        let session_dir = self.session_dir(run_id)?;
        if !session_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&session_dir)
            .map_err(|source| StorageError::backend("sync_session", source.to_string()))?
        {
            let entry = entry
                .map_err(|source| StorageError::backend("sync_session", source.to_string()))?;
            let file_type = entry
                .file_type()
                .map_err(|source| StorageError::backend("sync_session", source.to_string()))?;
            if file_type.is_file() {
                File::open(entry.path())
                    .and_then(|file| file.sync_all())
                    .map_err(|source| StorageError::backend("sync_session", source.to_string()))?;
            }
        }

        if let Ok(file) = File::open(&session_dir) {
            file.sync_all()
                .map_err(|source| StorageError::backend("sync_session", source.to_string()))?;
        }
        Ok(())
    }

    fn append_json_line<T: Serialize>(
        &self,
        run_id: &str,
        file_name: &'static str,
        value: &T,
        operation: &'static str,
    ) -> StorageResult<()> {
        let path = self.session_dir(run_id)?.join(file_name);
        ensure_parent_dir(&path, operation)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|source| StorageError::backend(operation, source.to_string()))?;
        serde_json::to_writer(&mut file, value).map_err(|source| StorageError::Serialize {
            operation,
            message: source.to_string(),
        })?;
        file.write_all(b"\n")
            .and_then(|_| file.flush())
            .map_err(|source| StorageError::backend(operation, source.to_string()))?;
        Ok(())
    }

    fn active_run_id(&self, operation: &'static str) -> StorageResult<&str> {
        self.active_run_id.as_deref().ok_or_else(|| {
            StorageError::backend(operation, "file session storage is not scoped to a run_id")
        })
    }

    fn read_json_lines<T: for<'de> Deserialize<'de>>(
        &self,
        run_id: &str,
        file_name: &'static str,
        operation: &'static str,
    ) -> StorageResult<Vec<T>> {
        let path = self.session_dir(run_id)?.join(file_name);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)
            .map_err(|source| StorageError::backend(operation, source.to_string()))?;
        let reader = BufReader::new(file);
        let mut values = Vec::new();
        for line in reader.lines() {
            let line =
                line.map_err(|source| StorageError::backend(operation, source.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str(&line).map_err(|source| StorageError::Serialize {
                operation,
                message: source.to_string(),
            })?;
            values.push(value);
        }
        Ok(values)
    }
}

impl StorageBackend for FileSessionStorage {
    fn append_raw_message(&self, message: RawMessage) -> StorageResult<()> {
        self.append_json_line(
            &message.run_id.clone(),
            "raw_messages.jsonl",
            &message,
            "append_raw_message",
        )
    }

    fn append_normalized_event(&self, event: EventEnvelope) -> StorageResult<()> {
        self.append_json_line(
            &event.run_id.clone(),
            "normalized_events.jsonl",
            &event,
            "append_normalized_event",
        )
    }

    fn upsert_market(&self, market: Market) -> StorageResult<()> {
        let run_id = self.active_run_id("upsert_market")?;
        self.append_json_line(run_id, "markets.jsonl", &market, "upsert_market")
    }

    fn insert_config_snapshot(&self, snapshot: ConfigSnapshot) -> StorageResult<()> {
        let path = self
            .session_dir(&snapshot.run_id)?
            .join("config_snapshot.json");
        ensure_parent_dir(&path, "insert_config_snapshot")?;
        let bytes =
            serde_json::to_vec_pretty(&snapshot).map_err(|source| StorageError::Serialize {
                operation: "insert_config_snapshot",
                message: source.to_string(),
            })?;
        fs::write(path, bytes)
            .map_err(|source| StorageError::backend("insert_config_snapshot", source.to_string()))
    }

    fn read_config_snapshot(&self, run_id: &str) -> StorageResult<Option<ConfigSnapshot>> {
        let path = self.session_dir(run_id)?.join("config_snapshot.json");
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path)
            .map_err(|source| StorageError::backend("read_config_snapshot", source.to_string()))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| StorageError::Serialize {
                operation: "read_config_snapshot",
                message: source.to_string(),
            })
    }

    fn insert_paper_order(&self, order: PaperOrder) -> StorageResult<()> {
        let run_id = self.active_run_id("insert_paper_order")?;
        self.append_json_line(run_id, "paper_orders.jsonl", &order, "insert_paper_order")
    }

    fn insert_paper_fill(&self, fill: PaperFill) -> StorageResult<()> {
        let run_id = self.active_run_id("insert_paper_fill")?;
        self.append_json_line(run_id, "paper_fills.jsonl", &fill, "insert_paper_fill")
    }

    fn upsert_paper_position(&self, position: PositionSnapshot) -> StorageResult<()> {
        let run_id = self.active_run_id("upsert_paper_position")?;
        self.append_json_line(
            run_id,
            "paper_positions.jsonl",
            &position,
            "upsert_paper_position",
        )
    }

    fn upsert_paper_balance(&self, balance: PaperBalanceSnapshot) -> StorageResult<()> {
        self.append_json_line(
            &balance.run_id.clone(),
            "paper_balances.jsonl",
            &balance,
            "upsert_paper_balance",
        )
    }

    fn insert_risk_event(&self, event: RiskEvent) -> StorageResult<()> {
        self.append_json_line(
            &event.run_id.clone(),
            "risk_events.jsonl",
            &event,
            "insert_risk_event",
        )
    }

    fn read_run_events(&self, run_id: &str) -> StorageResult<Vec<EventEnvelope>> {
        let mut events = self
            .read_json_lines::<EventEnvelope>(run_id, "normalized_events.jsonl", "read_run_events")?
            .into_iter()
            .filter(|event| event.run_id == run_id)
            .collect::<Vec<_>>();
        events.sort_by(|left, right| left.replay_ordering_key().cmp(&right.replay_ordering_key()));
        Ok(events)
    }
}

fn validate_path_segment<'a>(value: &'a str, name: &'static str) -> StorageResult<&'a str> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value != "."
        && value != ".."
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        });
    if valid {
        Ok(value)
    } else {
        Err(StorageError::backend(
            "validate_path_segment",
            format!("{name} must be a safe path segment; got {value:?}"),
        ))
    }
}

fn ensure_parent_dir(path: &Path, operation: &'static str) -> StorageResult<()> {
    let parent = path.parent().ok_or_else(|| {
        StorageError::backend(operation, format!("path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)
        .map_err(|source| StorageError::backend(operation, source.to_string()))
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
    paper_positions: HashMap<(String, String), PositionSnapshot>,
    paper_balances: HashMap<String, PaperBalanceSnapshot>,
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

    fn read_config_snapshot(&self, run_id: &str) -> StorageResult<Option<ConfigSnapshot>> {
        self.with_state("read_config_snapshot", |state| {
            Ok(state.config_snapshots.get(run_id).cloned())
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

    fn upsert_paper_position(&self, position: PositionSnapshot) -> StorageResult<()> {
        self.with_state("upsert_paper_position", |state| {
            state.paper_positions.insert(
                (position.market_id.clone(), position.token_id.clone()),
                position,
            );
            Ok(())
        })
    }

    fn upsert_paper_balance(&self, balance: PaperBalanceSnapshot) -> StorageResult<()> {
        self.with_state("upsert_paper_balance", |state| {
            state.paper_balances.insert(balance.run_id.clone(), balance);
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
    pub fn raw_message_count(&self) -> StorageResult<usize> {
        self.with_state("raw_message_count", |state| Ok(state.raw_messages.len()))
    }

    pub fn normalized_event_count(&self) -> StorageResult<usize> {
        self.with_state("normalized_event_count", |state| Ok(state.events.len()))
    }

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

pub struct PostgresMarketStore {
    client: Client,
}

impl PostgresMarketStore {
    pub fn connect(postgres_url: &str) -> StorageResult<Self> {
        let mut client = Client::connect(postgres_url, NoTls)
            .map_err(|source| StorageError::backend("postgres_connect", source.to_string()))?;
        client
            .batch_execute(include_str!(
                "../migrations/postgres/0001_relational_state.sql"
            ))
            .map_err(|source| {
                StorageError::backend("postgres_apply_migration", source.to_string())
            })?;

        Ok(Self { client })
    }

    pub fn upsert_markets(&mut self, markets: &[Market]) -> StorageResult<usize> {
        for market in markets {
            self.upsert_market(market)?;
        }

        Ok(markets.len())
    }

    pub fn count_markets_by_ids(&mut self, market_ids: &[String]) -> StorageResult<usize> {
        let unique_ids = market_ids.iter().collect::<HashSet<_>>();
        let mut found = 0usize;

        for market_id in &unique_ids {
            let row = self
                .client
                .query_one(
                    "SELECT count(*) FROM markets WHERE market_id = $1",
                    &[market_id],
                )
                .map_err(|source| {
                    StorageError::backend("postgres_count_markets", source.to_string())
                })?;
            let count: i64 = row.get(0);
            if count > 0 {
                found += 1;
            }
        }

        Ok(found)
    }

    fn upsert_market(&mut self, market: &Market) -> StorageResult<()> {
        let payload = serde_json::to_value(market).map_err(|source| StorageError::Serialize {
            operation: "serialize_market",
            message: source.to_string(),
        })?;
        let asset = asset_symbol(market.asset);
        let lifecycle_state = lifecycle_state_name(&market.lifecycle_state);

        self.client
            .execute(
                "
                INSERT INTO markets
                    (
                        market_id,
                        slug,
                        title,
                        asset,
                        condition_id,
                        start_ts,
                        end_ts,
                        resolution_source,
                        tick_size,
                        min_order_size,
                        lifecycle_state,
                        ineligibility_reason,
                        payload,
                        updated_at
                    )
                VALUES
                    ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())
                ON CONFLICT (market_id) DO UPDATE SET
                    slug = EXCLUDED.slug,
                    title = EXCLUDED.title,
                    asset = EXCLUDED.asset,
                    condition_id = EXCLUDED.condition_id,
                    start_ts = EXCLUDED.start_ts,
                    end_ts = EXCLUDED.end_ts,
                    resolution_source = EXCLUDED.resolution_source,
                    tick_size = EXCLUDED.tick_size,
                    min_order_size = EXCLUDED.min_order_size,
                    lifecycle_state = EXCLUDED.lifecycle_state,
                    ineligibility_reason = EXCLUDED.ineligibility_reason,
                    payload = EXCLUDED.payload,
                    updated_at = now()
                ",
                &[
                    &market.market_id,
                    &market.slug,
                    &market.title,
                    &asset,
                    &market.condition_id,
                    &market.start_ts,
                    &market.end_ts,
                    &market.resolution_source,
                    &market.tick_size,
                    &market.min_order_size,
                    &lifecycle_state,
                    &market.ineligibility_reason,
                    &payload,
                ],
            )
            .map_err(|source| {
                StorageError::backend("postgres_upsert_market", source.to_string())
            })?;

        Ok(())
    }
}

fn asset_symbol(asset: crate::domain::Asset) -> &'static str {
    match asset {
        crate::domain::Asset::Btc => "BTC",
        crate::domain::Asset::Eth => "ETH",
        crate::domain::Asset::Sol => "SOL",
    }
}

fn lifecycle_state_name(state: &crate::domain::MarketLifecycleState) -> &'static str {
    match state {
        crate::domain::MarketLifecycleState::Discovered => "discovered",
        crate::domain::MarketLifecycleState::Active => "active",
        crate::domain::MarketLifecycleState::Ineligible => "ineligible",
        crate::domain::MarketLifecycleState::Resolved => "resolved",
        crate::domain::MarketLifecycleState::Closed => "closed",
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
    use crate::state::PositionSnapshot;

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
            "paper_positions",
            "paper_balances",
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
        let position = sample_position();
        let balance = sample_balance();
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
        assert!(storage
            .read_config_snapshot("run-1")
            .expect("config snapshot reads")
            .is_some());
        storage.insert_paper_order(order).expect("order writes");
        storage.insert_paper_fill(fill).expect("fill writes");
        storage
            .upsert_paper_position(position)
            .expect("position writes");
        storage
            .upsert_paper_balance(balance)
            .expect("balance writes");
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
    fn file_session_storage_persists_session_records_for_replay() {
        let temp_dir = unique_temp_dir("file-session-storage");
        let storage =
            FileSessionStorage::for_run(&temp_dir, "run-1").expect("file storage scopes to run");
        let market = sample_market();
        let order = sample_order();
        let fill = sample_fill();
        let position = sample_position();
        let balance = sample_balance();
        let config: AppConfig = toml::from_str(DEFAULT_CONFIG).expect("default config parses");
        let snapshot =
            ConfigSnapshot::from_config("run-1", 1_777_000_000_001, &config).expect("snapshot");
        let later_event = EventEnvelope::new(
            "run-1",
            "event-2",
            "unit-test",
            1_777_000_000_002,
            20,
            2,
            NormalizedEvent::ReplayCheckpoint {
                replay_run_id: "checkpoint".to_string(),
                event_count: 2,
                checkpoint_ts: 1_777_000_000_002,
            },
        );
        let earlier_event = EventEnvelope::new(
            "run-1",
            "event-1",
            "unit-test",
            1_777_000_000_001,
            10,
            1,
            NormalizedEvent::MarketDiscovered {
                market: market.clone(),
            },
        );

        storage
            .insert_config_snapshot(snapshot)
            .expect("config snapshot writes");
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
            .append_normalized_event(later_event.clone())
            .expect("later event writes");
        storage
            .append_normalized_event(earlier_event.clone())
            .expect("earlier event writes");
        storage.insert_paper_order(order).expect("order writes");
        storage.insert_paper_fill(fill).expect("fill writes");
        storage
            .upsert_paper_position(position)
            .expect("position writes");
        storage
            .upsert_paper_balance(balance)
            .expect("balance writes");
        storage
            .insert_risk_event(RiskEvent {
                run_id: "run-1".to_string(),
                event_id: "risk-1".to_string(),
                risk_state: RiskState {
                    halted: false,
                    active_halts: Vec::new(),
                    reason: None,
                    updated_ts: 1_777_000_000_000,
                },
            })
            .expect("risk event writes");
        let artifact = storage
            .write_session_artifact("run-1", "paper_report.json", b"{}")
            .expect("artifact writes");
        storage.sync_session("run-1").expect("session syncs");

        let reader = FileSessionStorage::new(&temp_dir);
        assert!(reader
            .read_config_snapshot("run-1")
            .expect("config snapshot reads")
            .is_some());
        assert_eq!(
            reader.read_run_events("run-1").expect("events read"),
            vec![earlier_event, later_event]
        );
        assert!(artifact.exists());
        assert!(FileSessionStorage::for_run(&temp_dir, "../bad").is_err());

        let _ = fs::remove_dir_all(temp_dir);
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
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps: 200.0,
                raw_fee_config: None,
            },
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

    fn sample_position() -> PositionSnapshot {
        PositionSnapshot {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            size: 5.0,
            average_price: 0.49,
            realized_pnl: 0.0,
            unrealized_pnl: 0.05,
            updated_ts: 1_777_000_000_002,
        }
    }

    fn sample_balance() -> PaperBalanceSnapshot {
        PaperBalanceSnapshot {
            run_id: "run-1".to_string(),
            starting_balance: 1_000.0,
            cash_balance: 997.55,
            realized_pnl: 0.0,
            unrealized_pnl: 0.05,
            updated_ts: 1_777_000_000_002,
        }
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{now_ns}", std::process::id()))
    }
}
