use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use polymarket_15m_arb_bot::{
    compliance::{ComplianceClient, ComplianceError},
    config::AppConfig,
    domain::{Asset, MarketLifecycleState},
    feed_ingestion::{
        binance_combined_trade_url, coinbase_ticker_subscription, FeedConnectionConfig,
        FeedHealthTracker, FeedRecorder, PolymarketBookSnapshotClient,
        PolymarketMarketSubscription, ReadOnlyWebSocketClient,
    },
    market_discovery::{emit_market_lifecycle_events, MarketDiscoveryClient},
    metrics::{m8_smoke_metrics_snapshot, required_m8_metric_families, serve_prometheus_once},
    module_names,
    normalization::{SOURCE_BINANCE, SOURCE_COINBASE, SOURCE_POLYMARKET_CLOB},
    safety,
    shutdown::{GracefulShutdownState, RuntimeMode},
    storage::{InMemoryStorage, PostgresMarketStore, StorageError},
};
use tokio::net::TcpListener;
use tracing::field::{Field, Visit};
use tracing::info;
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

static RUN_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Parser)]
#[command(name = "polymarket-15m-arb-bot")]
#[command(about = "Replay-first and paper-trading-first Polymarket 15m crypto bot")]
struct Cli {
    #[arg(
        short,
        long,
        global = true,
        default_value = "config/default.toml",
        help = "Path to the TOML config file"
    )]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate local config and M0 safety invariants.
    Validate {
        #[arg(long, help = "Skip M2 online geoblock and market discovery checks")]
        local_only: bool,
        #[arg(long, help = "Run M3 read-only feed WebSocket smoke checks")]
        feed_smoke: bool,
        #[arg(long, help = "Run M8 local loopback metrics endpoint smoke check")]
        metrics_smoke: bool,
        #[arg(long, help = "Override feed smoke message limit")]
        feed_message_limit: Option<usize>,
    },
    /// Load config for future paper mode. Strategy execution starts in later milestones.
    Paper,
    /// Load config for future replay mode. Replay execution starts in later milestones.
    Replay {
        #[arg(long, help = "Run ID to replay in later milestones")]
        run_id: Option<String>,
    },
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Commands::Validate { .. } => "validate",
            Commands::Paper => "paper",
            Commands::Replay { .. } => "replay",
        }
    }

    fn runtime_mode(&self) -> RuntimeMode {
        match self {
            Commands::Validate { .. } => RuntimeMode::Validate,
            Commands::Paper => RuntimeMode::Paper,
            Commands::Replay { .. } => RuntimeMode::Replay,
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = AppConfig::from_path(&cli.config)?;

    init_tracing(&config.runtime.log_level)?;

    let run_id = generate_run_id();
    let modules = module_names();
    let mode = cli.command.name();
    let runtime_mode = cli.command.runtime_mode();
    let mut shutdown = GracefulShutdownState::new(run_id.clone(), runtime_mode);

    info!(
        %run_id,
        mode,
        config_path = %cli.config.display(),
        assets = %config.asset_list(),
        module_count = modules.len(),
        "startup validation complete"
    );

    let command = cli.command;
    let command_result: Result<(), Box<dyn std::error::Error>> = async {
        match command {
            Commands::Validate {
                local_only,
                feed_smoke,
                metrics_smoke,
                feed_message_limit,
            } => {
                println!("validation_status=ok");
                println!("run_id={run_id}");
                println!("mode=validate");
                println!("config_path={}", cli.config.display());
                println!("assets={}", config.asset_list());
                println!("modules={}", modules.join(","));
                println!(
                    "live_order_placement_enabled={}",
                    safety::LIVE_ORDER_PLACEMENT_ENABLED
                );
                if local_only {
                    println!("online_validation_status=skipped");
                } else {
                    run_m2_online_validation(&config, &run_id).await?;
                }
                if feed_smoke {
                    run_m3_feed_smoke(&config, &run_id, feed_message_limit).await?;
                }
                if metrics_smoke {
                    run_m8_metrics_smoke(&config, &run_id, mode).await?;
                }
            }
            Commands::Paper => {
                let geoblock = compliance_client(&config)?.check_geoblock().await?;
                ComplianceError::fail_if_blocked(&geoblock)?;
                println!("validation_status=ok");
                println!("run_id={run_id}");
                println!("mode=paper");
                println!("paper_mode_status=stubbed_until_later_milestones");
                println!(
                    "live_order_placement_enabled={}",
                    safety::LIVE_ORDER_PLACEMENT_ENABLED
                );
            }
            Commands::Replay {
                run_id: replay_run_id,
            } => {
                println!("validation_status=ok");
                println!("run_id={run_id}");
                println!("mode=replay");
                println!(
                    "target_replay_run_id={}",
                    replay_run_id.unwrap_or_else(|| "not_provided".to_string())
                );
                println!("replay_status=stubbed_until_later_milestones");
                println!(
                    "live_order_placement_enabled={}",
                    safety::LIVE_ORDER_PLACEMENT_ENABLED
                );
            }
        }

        Ok(())
    }
    .await;

    let shutdown_reason = if command_result.is_ok() {
        "command_completed"
    } else {
        "command_failed"
    };
    let command_status = if command_result.is_ok() {
        "ok"
    } else {
        "error"
    };
    let command_error = command_result
        .as_ref()
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();

    shutdown.request_shutdown(shutdown_reason);
    shutdown.complete();
    info!(
        run_id = %shutdown.run_id(),
        mode = shutdown.mode().as_str(),
        shutdown_phase = shutdown.phase_name(),
        accepting_new_work = shutdown.accepting_new_work(),
        reason = shutdown.reason().unwrap_or(shutdown_reason),
        command_status,
        error = command_error.as_str(),
        "runtime shutdown complete"
    );

    command_result?;
    Ok(())
}

async fn run_m8_metrics_smoke(
    config: &AppConfig,
    run_id: &str,
    mode: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let metrics_body = m8_smoke_metrics_snapshot().render_prometheus();
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(serve_prometheus_once(listener, metrics_body));
    let response = reqwest::get(format!("http://{address}/metrics")).await?;
    let body = response.text().await?;

    server.await??;

    let missing_metrics = required_m8_metric_families()
        .iter()
        .filter(|metric| !body.contains(**metric))
        .copied()
        .collect::<Vec<_>>();
    if !missing_metrics.is_empty() {
        return Err(format!(
            "metrics smoke response missed required M8 metrics: {}",
            missing_metrics.join(",")
        )
        .into());
    }

    info!(
        %run_id,
        mode,
        source = "local_metrics_endpoint",
        event_type = "metrics_smoke",
        metrics_bind_addr = %config.metrics.bind_addr,
        reason = "metrics_endpoint_returned_expected_metrics",
        "metrics smoke complete"
    );
    println!("metrics_smoke_status=ok");
    println!("metrics_config_bind_addr={}", config.metrics.bind_addr);
    println!("metrics_smoke_url=http://{address}/metrics");
    println!("metrics_smoke_bytes={}", body.len());

    Ok(())
}

async fn run_m3_feed_smoke(
    config: &AppConfig,
    run_id: &str,
    feed_message_limit: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let message_limit =
        feed_message_limit.unwrap_or(usize::from(config.feeds.feed_smoke_message_limit));
    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let discovery_run = discovery.discover_crypto_15m_markets().await?;
    let Some(market) = discovery_run
        .markets
        .iter()
        .find(|market| market.ineligibility_reason.is_none() && market.outcomes.len() == 2)
    else {
        return Err("feed smoke requires one eligible market with two outcome tokens".into());
    };
    let asset_ids = market
        .outcomes
        .iter()
        .map(|outcome| outcome.token_id.clone())
        .collect::<Vec<_>>();
    let polymarket_subscription = PolymarketMarketSubscription::new(asset_ids);
    let client = ReadOnlyWebSocketClient;
    let storage = InMemoryStorage::default();
    let snapshot_client = PolymarketBookSnapshotClient::new(
        &config.polymarket.clob_rest_url,
        config.polymarket.request_timeout_ms,
    )?;
    let snapshot_payload = snapshot_client
        .fetch_book(&market.outcomes[0].token_id)
        .await?;
    let snapshot_recorder = FeedRecorder::new(&storage, run_id, SOURCE_POLYMARKET_CLOB);
    let snapshot_recorded = snapshot_recorder.record_message(
        snapshot_payload,
        unix_time_ms(),
        monotonic_like_ns(),
        0,
    )?;
    if snapshot_recorded.normalized_event_count == 0 {
        return Err("book snapshot recovery probe produced no normalized events".into());
    }
    println!(
        "book_snapshot_recovery_status=ok,normalized_events={}",
        snapshot_recorded.normalized_event_count
    );

    let probes = [
        FeedConnectionConfig {
            source: SOURCE_POLYMARKET_CLOB.to_string(),
            ws_url: config.polymarket.market_ws_url.clone(),
            subscribe_payload: Some(polymarket_subscription.to_payload()),
            message_limit,
            connect_timeout_ms: config.feeds.connect_timeout_ms,
            read_timeout_ms: config.feeds.read_timeout_ms,
        },
        FeedConnectionConfig {
            source: SOURCE_BINANCE.to_string(),
            ws_url: binance_combined_trade_url(&config.feeds.binance_ws_url),
            subscribe_payload: None,
            message_limit,
            connect_timeout_ms: config.feeds.connect_timeout_ms,
            read_timeout_ms: config.feeds.read_timeout_ms,
        },
        FeedConnectionConfig {
            source: SOURCE_COINBASE.to_string(),
            ws_url: config.feeds.coinbase_ws_url.clone(),
            subscribe_payload: Some(coinbase_ticker_subscription()),
            message_limit: message_limit.max(3),
            connect_timeout_ms: config.feeds.connect_timeout_ms,
            read_timeout_ms: config.feeds.read_timeout_ms,
        },
    ];

    for probe in probes {
        let result = client.connect_and_capture(&probe).await?;
        let mut health = FeedHealthTracker::new(&probe.source, config.feeds.stale_after_ms);
        health.mark_connected(unix_time_ms());
        let recorder = FeedRecorder::new(&storage, run_id, probe.source.clone());
        let mut normalized_count = 0usize;
        let mut unknown_count = 0usize;
        for (index, message) in result.received_text_messages.iter().enumerate() {
            let recv_wall_ts = unix_time_ms();
            let recorded = recorder.record_message(
                message.as_str(),
                recv_wall_ts,
                monotonic_like_ns(),
                1_000 + index as u64,
            )?;
            normalized_count += recorded.normalized_event_count;
            if recorded.unknown_event_type.is_some() {
                unknown_count += 1;
            }
            health.mark_message(recv_wall_ts, None);
        }
        let observed_health = health.observe(unix_time_ms());

        println!(
            "feed_smoke_source={},connected={},raw_messages={},normalized_events={},unknown_messages={},health={:?}",
            probe.source,
            result.connected,
            result.received_text_messages.len(),
            normalized_count,
            unknown_count,
            observed_health.status
        );
        if normalized_count == 0 {
            return Err(format!(
                "feed smoke source {} connected but produced no normalized events",
                probe.source
            )
            .into());
        }
    }
    println!(
        "feed_smoke_persisted_raw_count={}",
        storage.raw_message_count()?
    );
    println!(
        "feed_smoke_persisted_normalized_count={}",
        storage.normalized_event_count()?
    );

    Ok(())
}

async fn run_m2_online_validation(
    config: &AppConfig,
    run_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let geoblock = compliance_client(config)?.check_geoblock().await?;
    let masked_geoblock = geoblock.masked_for_logs();
    println!("geoblock_blocked={}", masked_geoblock.blocked);
    println!(
        "geoblock_country={}",
        masked_geoblock.country.as_deref().unwrap_or("unknown")
    );
    println!(
        "geoblock_region={}",
        masked_geoblock.region.as_deref().unwrap_or("unknown")
    );

    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let discovery_run = discovery.discover_crypto_15m_markets().await?;
    let ineligible_count = discovery_run
        .markets
        .iter()
        .filter(|market| market.ineligibility_reason.is_some())
        .count();
    let market_ids = discovery_run
        .markets
        .iter()
        .map(|market| market.market_id.clone())
        .collect::<Vec<_>>();
    let postgres_url = config.storage.postgres_url.clone();
    let markets_for_postgres = discovery_run.markets.clone();
    let market_ids_for_readback = market_ids.clone();
    let (persisted_count, readback_count) =
        tokio::task::spawn_blocking(move || -> Result<(usize, usize), StorageError> {
            let mut postgres = PostgresMarketStore::connect(&postgres_url)?;
            let persisted_count = postgres.upsert_markets(&markets_for_postgres)?;
            let readback_count = postgres.count_markets_by_ids(&market_ids_for_readback)?;
            Ok((persisted_count, readback_count))
        })
        .await??;
    if readback_count != market_ids.len() {
        return Err(StorageError::backend(
            "postgres_market_readback",
            format!(
                "expected {} discovered markets in Postgres, read back {readback_count}",
                market_ids.len()
            ),
        )
        .into());
    }

    let lifecycle_event_storage = InMemoryStorage::default();
    let event_count = emit_market_lifecycle_events(
        &lifecycle_event_storage,
        run_id,
        unix_time_ms(),
        monotonic_like_ns(),
        &discovery_run.markets,
    )?;

    println!("market_discovery_status=ok");
    println!("market_discovery_pages={}", discovery_run.pages_fetched);
    println!("market_discovery_count={}", discovery_run.markets.len());
    println!("market_discovery_ineligible_count={ineligible_count}");
    println!("market_discovery_postgres_persisted_count={persisted_count}");
    println!("market_discovery_postgres_readback_count={readback_count}");
    println!("market_lifecycle_event_count={event_count}");
    for market in &discovery_run.markets {
        let outcomes = market
            .outcomes
            .iter()
            .map(|outcome| outcome.outcome.as_str())
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "market_discovery_market=asset={},slug={},state={},start_ts={},end_ts={},outcomes={}",
            asset_symbol(market.asset),
            market.slug,
            lifecycle_state_name(&market.lifecycle_state),
            market.start_ts,
            market.end_ts,
            outcomes
        );
    }

    Ok(())
}

fn compliance_client(config: &AppConfig) -> Result<ComplianceClient, Box<dyn std::error::Error>> {
    Ok(ComplianceClient::new(
        &config.polymarket.geoblock_url,
        config.polymarket.request_timeout_ms,
    )?)
}

fn init_tracing(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_new(log_level)?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .event_format(JsonEventFormatter)
        .init();
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct JsonEventFormatter;

impl<S, N> FormatEvent<S, N> for JsonEventFormatter
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut visitor = JsonFieldVisitor::default();
        event.record(&mut visitor);

        let timestamp_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();

        write!(
            writer,
            "{{\"timestamp_unix_ms\":{timestamp_unix_ms},\"level\":\"{}\",\"target\":\"{}\"",
            event.metadata().level(),
            escape_json(event.metadata().target())
        )?;

        for field in visitor.fields {
            write!(
                writer,
                ",\"{}\":{}",
                escape_json(&field.name),
                field.encoded_value
            )?;
        }

        writeln!(writer, "}}")
    }
}

#[derive(Debug, Default)]
struct JsonFieldVisitor {
    fields: Vec<JsonField>,
}

#[derive(Debug)]
struct JsonField {
    name: String,
    encoded_value: String,
}

impl JsonFieldVisitor {
    fn push_raw(&mut self, field: &Field, encoded_value: String) {
        self.fields.push(JsonField {
            name: field.name().to_string(),
            encoded_value,
        });
    }

    fn push_string(&mut self, field: &Field, value: &str) {
        self.push_raw(field, format!("\"{}\"", escape_json(value)));
    }
}

impl Visit for JsonFieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        if value.is_finite() {
            self.push_raw(field, value.to_string());
        } else {
            self.push_string(field, &value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push_raw(field, value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push_raw(field, value.to_string());
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.push_raw(field, value.to_string());
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.push_raw(field, value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push_raw(field, value.to_string());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push_string(field, value);
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push_string(field, &format!("{value:?}"));
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .ok()
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default()
}

fn monotonic_like_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .ok()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_default()
}

fn asset_symbol(asset: Asset) -> &'static str {
    match asset {
        Asset::Btc => "BTC",
        Asset::Eth => "ETH",
        Asset::Sol => "SOL",
    }
}

fn lifecycle_state_name(state: &MarketLifecycleState) -> &'static str {
    match state {
        MarketLifecycleState::Discovered => "discovered",
        MarketLifecycleState::Active => "active",
        MarketLifecycleState::Ineligible => "ineligible",
        MarketLifecycleState::Resolved => "resolved",
        MarketLifecycleState::Closed => "closed",
    }
}

fn generate_run_id() -> String {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id();
    let sequence = RUN_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{now_ns:x}-{pid:x}-{sequence:x}")
}
