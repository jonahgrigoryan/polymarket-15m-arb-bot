use std::env;
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use polymarket_15m_arb_bot::{
    compliance::{ComplianceClient, ComplianceError, GeoblockResponse},
    config::{AppConfig, LiveBetaSecretHandlesConfig},
    domain::{
        Asset, FeeParameters, Market, MarketLifecycleState, OrderBookLevel, OrderBookSnapshot,
        OutcomeToken, PaperOrderStatus, ReferencePrice, RiskHaltReason, Side,
    },
    events::{EventEnvelope, NormalizedEvent},
    feed_ingestion::{
        binance_combined_trade_url, coinbase_ticker_subscription, FeedConnectionConfig,
        FeedHealthTracker, FeedRecorder, PolymarketBookSnapshotClient,
        PolymarketMarketSubscription, ReadOnlyWebSocketClient,
    },
    live_beta_readback::{
        self, AccountPreflight, AuthenticatedReadbackInput, L2ReadbackCredentials,
        ReadbackPrerequisites, SignatureType,
    },
    live_beta_signing,
    market_discovery::{
        emit_market_lifecycle_events, persist_discovered_markets, MarketDiscoveryClient,
    },
    metrics::{
        m8_smoke_metrics_snapshot, required_m8_metric_families, serve_prometheus_once,
        MetricsSnapshot,
    },
    module_names,
    normalization::{SOURCE_BINANCE, SOURCE_COINBASE, SOURCE_POLYMARKET_CLOB},
    reference_feed::{
        parse_polymarket_rtds_chainlink_message,
        polymarket_rtds_chainlink_subscription_payload_for_asset, PythHermesClient,
        ReferenceFeedError, PROVIDER_POLYMARKET_RTDS_CHAINLINK, SOURCE_POLYMARKET_RTDS_CHAINLINK,
        SOURCE_PYTH_PROXY,
    },
    replay::{
        compare_generated_to_recorded_paper_events, compare_replay_results, ReplayEngine,
        ReplayRunResult,
    },
    reporting::deterministic_report_json,
    safety,
    secret_handling::{self, EnvSecretPresenceProvider},
    shutdown::{GracefulShutdownState, RuntimeMode},
    storage::{
        ConfigSnapshot, FileSessionStorage, InMemoryStorage, PaperBalanceSnapshot,
        PostgresMarketStore, RawMessage, RiskEvent, StorageBackend, StorageError,
    },
};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
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
        #[arg(long, help = "Evaluate the LB1 future live-mode gate")]
        live_beta_intent: bool,
        #[arg(
            long,
            help = "Validate LB2 secret handle names and backend presence without printing values"
        )]
        validate_secret_handles: bool,
        #[arg(
            long,
            help = "Build the LB3 sanitized signing dry-run artifact without network submission"
        )]
        live_beta_signing_dry_run: bool,
        #[arg(
            long,
            help = "Evaluate the LB4 readback/account preflight gate without live network calls"
        )]
        live_readback_preflight: bool,
        #[arg(long, help = "Override feed smoke message limit")]
        feed_message_limit: Option<usize>,
    },
    /// Run read-only paper trading against captured market/reference feeds.
    Paper {
        #[arg(long, help = "Override generated paper run ID")]
        run_id: Option<String>,
        #[arg(
            long,
            help = "Write an offline deterministic M9 paper lifecycle fixture session"
        )]
        deterministic_fixture: bool,
        #[arg(long, help = "Messages to capture per feed per cycle")]
        feed_message_limit: Option<usize>,
        #[arg(
            long,
            default_value_t = 1,
            help = "Paper capture cycles; set 0 to run until Ctrl-C"
        )]
        cycles: u64,
    },
    /// Replay a stored paper session offline and fail on paper-event divergence.
    Replay {
        #[arg(long, help = "Stored paper run ID to replay")]
        run_id: Option<String>,
    },
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Commands::Validate { .. } => "validate",
            Commands::Paper { .. } => "paper",
            Commands::Replay { .. } => "replay",
        }
    }

    fn runtime_mode(&self) -> RuntimeMode {
        match self {
            Commands::Validate { .. } => RuntimeMode::Validate,
            Commands::Paper { .. } => RuntimeMode::Paper,
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
                live_beta_intent,
                validate_secret_handles,
                live_beta_signing_dry_run,
                live_readback_preflight,
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
                println!(
                    "live_beta_config_intent_enabled={}",
                    config.live_beta.intent_enabled
                );
                println!("live_beta_cli_intent_enabled={live_beta_intent}");
                println!(
                    "live_beta_kill_switch_active={}",
                    config.live_beta.kill_switch_active
                );
                let secret_inventory = config.live_beta.secret_inventory();
                println!("live_beta_secret_backend={}", secret_inventory.backend);
                println!(
                    "live_beta_secret_handle_count={}",
                    secret_inventory.handles.len()
                );
                println!("live_beta_secret_values_loaded=false");
                let geoblock_gate_status = if local_only {
                    println!("online_validation_status=skipped");
                    safety::GeoblockGateStatus::Unknown
                } else if live_readback_preflight {
                    safety::GeoblockGateStatus::from_blocked(
                        run_geoblock_validation(&config).await?.blocked,
                    )
                } else {
                    safety::GeoblockGateStatus::from_blocked(
                        run_m2_online_validation(&config, &run_id).await?.blocked,
                    )
                };
                let live_beta_gate =
                    safety::evaluate_live_mode_gate(safety::LiveModeGateInput::lb1(
                        config.live_beta.intent_enabled,
                        live_beta_intent,
                        config.live_beta.kill_switch_active,
                        geoblock_gate_status,
                    ));
                println!("live_beta_geoblock_gate={}", geoblock_gate_status.as_str());
                println!("live_beta_gate_status={}", live_beta_gate.status());
                println!(
                    "live_beta_gate_block_reasons={}",
                    live_beta_gate.reason_list()
                );
                if live_beta_intent && !live_beta_gate.allowed {
                    return Err(format!(
                        "LB1 live-mode gate refused future live intent: {}",
                        live_beta_gate.reason_list()
                    )
                    .into());
                }
                if validate_secret_handles {
                    run_lb2_secret_handle_validation(&secret_inventory)?;
                }
                if live_beta_signing_dry_run {
                    run_lb3_signing_dry_run_validation(&config)?;
                }
                if live_readback_preflight {
                    run_lb4_readback_preflight_validation(
                        &config,
                        geoblock_gate_status,
                        local_only,
                    )
                    .await?;
                }
                if feed_smoke {
                    run_m3_feed_smoke(&config, &run_id, feed_message_limit).await?;
                }
                if metrics_smoke {
                    run_m8_metrics_smoke(&config, &run_id, mode).await?;
                }
            }
            Commands::Paper {
                run_id: paper_run_id,
                deterministic_fixture,
                feed_message_limit,
                cycles,
            } => {
                let paper_run_id = paper_run_id.unwrap_or(run_id.clone());
                if deterministic_fixture {
                    run_deterministic_lifecycle_fixture(&config, &paper_run_id)?;
                } else {
                    run_paper_runtime(&config, &paper_run_id, feed_message_limit, cycles).await?;
                }
            }
            Commands::Replay {
                run_id: replay_run_id,
            } => {
                let replay_run_id =
                    replay_run_id.ok_or("replay requires --run-id <stored paper run_id>")?;
                run_replay_runtime(&config, &run_id, &replay_run_id).await?;
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

async fn run_paper_runtime(
    config: &AppConfig,
    run_id: &str,
    feed_message_limit: Option<usize>,
    cycles: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let message_limit =
        feed_message_limit.unwrap_or(usize::from(config.feeds.feed_smoke_message_limit));
    if message_limit == 0 {
        return Err("paper --feed-message-limit must be greater than zero".into());
    }

    let storage = FileSessionStorage::for_run(&config.replay.output_dir, run_id)?;
    if storage.session_exists(run_id)? {
        return Err(format!(
            "paper run_id={run_id} already exists under {}; choose a new run_id to avoid duplicate session writes",
            config.replay.output_dir
        )
        .into());
    }

    let geoblock = compliance_client(config)?.check_geoblock().await?;
    ComplianceError::fail_if_blocked(&geoblock)?;
    storage.insert_config_snapshot(ConfigSnapshot::from_config(run_id, unix_time_ms(), config)?)?;

    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let discovery_run = discovery.discover_crypto_15m_markets().await?;
    let market_selection_ts = unix_time_ms();
    let markets = select_paper_markets(&discovery_run.markets, market_selection_ts)?;
    let lifecycle_event_count = persist_discovered_markets(
        &storage,
        run_id,
        unix_time_ms(),
        monotonic_like_ns(),
        &markets,
    )?;

    println!("validation_status=ok");
    println!("run_id={run_id}");
    println!("mode=paper");
    println!("paper_mode_status=runtime_enabled");
    println!("paper_storage_backend=file_session");
    let reference_provider = if config.reference_feed.is_polymarket_rtds_chainlink_enabled() {
        PROVIDER_POLYMARKET_RTDS_CHAINLINK
    } else if config.reference_feed.is_pyth_proxy_enabled() {
        "pyth"
    } else {
        "none"
    };
    let settlement_reference_evidence =
        config.reference_feed.is_polymarket_rtds_chainlink_enabled()
            || config.reference_feed.provider == "chainlink";
    println!("reference_feed_mode={}", config.reference_feed.provider);
    println!("reference_provider={reference_provider}");
    println!("settlement_reference_evidence={settlement_reference_evidence}");
    println!("live_readiness_evidence=false");
    println!(
        "paper_session_dir={}",
        storage.session_dir(run_id)?.display()
    );
    println!("market_discovery_pages={}", discovery_run.pages_fetched);
    println!("paper_selected_market_count={}", markets.len());
    for market in &markets {
        println!(
            "paper_selected_market=asset={},market_id={},slug={},start_ts={},start_utc={},end_ts={},end_utc={},selection_now_ts={},selection_now_utc={}",
            market.asset.symbol(),
            market.market_id,
            market.slug,
            market.start_ts,
            format_utc_ms(market.start_ts),
            market.end_ts,
            format_utc_ms(market.end_ts),
            market_selection_ts,
            format_utc_ms(market_selection_ts)
        );
    }
    println!("paper_market_lifecycle_event_count={lifecycle_event_count}");
    println!(
        "live_order_placement_enabled={}",
        safety::LIVE_ORDER_PLACEMENT_ENABLED
    );

    let max_cycles = if cycles == 0 { None } else { Some(cycles) };
    let mut completed_cycles = 0_u64;
    loop {
        if max_cycles.is_some_and(|limit| completed_cycles >= limit) {
            break;
        }

        let cycle_result = tokio::select! {
            result = capture_paper_cycle(
                config,
                run_id,
                &storage,
                &markets,
                completed_cycles,
                message_limit,
            ) => Some(result),
            signal = shutdown_signal(), if max_cycles.is_none() => {
                signal?;
                None
            }
        };

        let Some(cycle_counts) = cycle_result else {
            println!("paper_shutdown_signal=received");
            break;
        };
        let cycle_counts = cycle_counts?;
        completed_cycles += 1;

        let cycle_replay = ReplayEngine::replay_from_storage_snapshot(&storage, run_id)?;
        let new_paper_events = append_new_recorded_paper_events(&storage, run_id, &cycle_replay)?;
        info!(
            %run_id,
            mode = "paper",
            event_type = "paper_cycle_complete",
            source = "paper_runtime",
            paper_cycle = completed_cycles,
            raw_messages = cycle_counts.raw_messages,
            normalized_events = cycle_counts.normalized_events,
            new_paper_events,
            "paper cycle complete"
        );
        println!(
            "paper_cycle_complete={},raw_messages={},normalized_events={},new_paper_events={}",
            completed_cycles,
            cycle_counts.raw_messages,
            cycle_counts.normalized_events,
            new_paper_events
        );

        if max_cycles.is_none() {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(config.polymarket.market_discovery_poll_ms)) => {}
                signal = shutdown_signal() => {
                    signal?;
                    println!("paper_shutdown_signal=received");
                    break;
                }
            }
        }
    }

    let final_result = ReplayEngine::replay_from_storage_snapshot(&storage, run_id)?;
    let final_check = compare_generated_to_recorded_paper_events(&final_result)?;
    if !final_check.passed {
        return Err(format!(
            "paper session generated/recorded paper event divergence for run_id={run_id}: {}",
            final_check
                .divergence
                .as_deref()
                .unwrap_or("fingerprint mismatch")
        )
        .into());
    }

    persist_paper_outputs(&storage, run_id, config, &final_result)?;
    let report_path = write_runtime_artifacts(
        &storage,
        run_id,
        "paper_report.json",
        "paper_metrics.prom",
        &final_result,
        false,
    )?;
    storage.sync_session(run_id)?;

    println!("paper_runtime_status=ok");
    println!("paper_completed_cycles={completed_cycles}");
    println!(
        "paper_determinism_fingerprint={}",
        final_result.report.determinism_fingerprint()
    );
    println!("paper_report_path={}", report_path.display());
    println!(
        "paper_order_count={}",
        final_result.report.paper.order_count
    );
    println!("paper_fill_count={}", final_result.report.paper.fill_count);
    println!(
        "paper_total_pnl={:.6}",
        final_result.report.pnl.totals.total_pnl
    );

    Ok(())
}

async fn run_replay_runtime(
    config: &AppConfig,
    replay_command_run_id: &str,
    target_run_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = FileSessionStorage::new(&config.replay.output_dir);
    let result = ReplayEngine::replay_from_storage_snapshot(&storage, target_run_id)?;
    let check = compare_generated_to_recorded_paper_events(&result)?;
    if !check.passed {
        let _ = write_runtime_artifacts(
            &storage,
            target_run_id,
            "replay_report_diverged.json",
            "replay_metrics_diverged.prom",
            &result,
            true,
        );
        return Err(format!(
            "replay divergence for run_id={target_run_id}: {}",
            check
                .divergence
                .as_deref()
                .unwrap_or("fingerprint mismatch")
        )
        .into());
    }

    let report_path = write_runtime_artifacts(
        &storage,
        target_run_id,
        "replay_report.json",
        "replay_metrics.prom",
        &result,
        false,
    )?;
    storage.sync_session(target_run_id)?;

    println!("validation_status=ok");
    println!("run_id={replay_command_run_id}");
    println!("mode=replay");
    println!("target_replay_run_id={target_run_id}");
    println!("replay_status=deterministic");
    println!(
        "replay_generated_paper_event_count={}",
        result.generated_paper_events.len()
    );
    println!(
        "replay_recorded_paper_event_count={}",
        result.recorded_paper_events.len()
    );
    println!("replay_report_path={}", report_path.display());
    println!(
        "replay_determinism_fingerprint={}",
        result.report.determinism_fingerprint()
    );
    println!(
        "live_order_placement_enabled={}",
        safety::LIVE_ORDER_PLACEMENT_ENABLED
    );

    Ok(())
}

fn run_deterministic_lifecycle_fixture(
    config: &AppConfig,
    run_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    const FIXTURE_SOURCE: &str = "deterministic_fixture";
    const FIXTURE_START_TS: i64 = 1_777_000_000_000;

    let storage = FileSessionStorage::for_run(&config.replay.output_dir, run_id)?;
    if storage.session_exists(run_id)? {
        return Err(format!(
            "deterministic fixture run_id={run_id} already exists under {}; choose a new run_id to avoid duplicate session writes",
            config.replay.output_dir
        )
        .into());
    }

    storage.insert_config_snapshot(ConfigSnapshot::from_config(
        run_id,
        FIXTURE_START_TS,
        config,
    )?)?;

    let events = deterministic_lifecycle_fixture_events(run_id, FIXTURE_SOURCE, FIXTURE_START_TS);
    let mut markets_written = 0usize;
    for event in &events {
        if let NormalizedEvent::MarketDiscovered { market } = &event.payload {
            storage.upsert_market(market.clone())?;
            markets_written += 1;
        }
        storage.append_raw_message(RawMessage {
            run_id: run_id.to_string(),
            source: FIXTURE_SOURCE.to_string(),
            recv_wall_ts: event.recv_wall_ts,
            recv_mono_ns: event.recv_mono_ns,
            ingest_seq: event.ingest_seq,
            payload: serde_json::to_string(&event.payload)?,
        })?;
        storage.append_normalized_event(event.clone())?;
    }

    let generated = ReplayEngine::replay_from_storage_snapshot(&storage, run_id)?;
    if generated.generated_orders.is_empty() || generated.generated_fills.is_empty() {
        return Err(format!(
            "deterministic fixture did not produce required order/fill evidence: orders={} fills={}",
            generated.generated_orders.len(),
            generated.generated_fills.len()
        )
        .into());
    }

    let appended = append_recorded_paper_events_deterministic(
        &storage,
        run_id,
        &generated,
        FIXTURE_SOURCE,
        FIXTURE_START_TS + 700_000,
    )?;
    let final_result = ReplayEngine::replay_from_storage_snapshot(&storage, run_id)?;
    let repeated_result = ReplayEngine::replay_from_storage_snapshot(&storage, run_id)?;
    let replay_check = compare_replay_results(&final_result, &repeated_result);
    if !replay_check.passed {
        return Err(
            "deterministic fixture replay fingerprint changed across identical runs".into(),
        );
    }
    let paper_check = compare_generated_to_recorded_paper_events(&final_result)?;
    if !paper_check.passed {
        return Err(format!(
            "deterministic fixture generated/recorded paper event divergence for run_id={run_id}: {}",
            paper_check
                .divergence
                .as_deref()
                .unwrap_or("fingerprint mismatch")
        )
        .into());
    }

    persist_paper_outputs_at(
        &storage,
        run_id,
        config,
        &final_result,
        FIXTURE_START_TS + 800_000,
    )?;
    let paper_report_path = write_runtime_artifacts(
        &storage,
        run_id,
        "paper_report.json",
        "paper_metrics.prom",
        &final_result,
        false,
    )?;
    let replay_report_path = write_runtime_artifacts(
        &storage,
        run_id,
        "replay_report.json",
        "replay_metrics.prom",
        &final_result,
        false,
    )?;
    storage.sync_session(run_id)?;

    println!("validation_status=ok");
    println!("run_id={run_id}");
    println!("mode=paper");
    println!("paper_mode_status=deterministic_fixture");
    println!("evidence_type=deterministic_fixture");
    println!("live_market_evidence=false");
    println!("live_readiness_evidence=false");
    println!("settlement_reference_evidence=false");
    println!(
        "paper_session_dir={}",
        storage.session_dir(run_id)?.display()
    );
    println!("fixture_market_count={markets_written}");
    println!("fixture_input_event_count={}", events.len());
    println!("fixture_recorded_paper_event_count={appended}");
    println!(
        "paper_order_count={}",
        final_result.report.paper.order_count
    );
    println!("paper_fill_count={}", final_result.report.paper.fill_count);
    println!(
        "paper_filled_notional={:.6}",
        final_result.report.paper.total_filled_notional
    );
    println!(
        "paper_fees_paid={:.6}",
        final_result.report.paper.total_fees_paid
    );
    println!(
        "paper_total_pnl={:.6}",
        final_result.report.pnl.totals.total_pnl
    );
    println!("paper_event_fingerprint={}", paper_check.left_fingerprint);
    println!(
        "replay_determinism_fingerprint={}",
        final_result.report.determinism_fingerprint()
    );
    println!("paper_event_match_status=ok");
    println!("replay_status=deterministic");
    println!("paper_report_path={}", paper_report_path.display());
    println!("replay_report_path={}", replay_report_path.display());
    println!(
        "live_order_placement_enabled={}",
        safety::LIVE_ORDER_PLACEMENT_ENABLED
    );

    Ok(())
}

fn deterministic_lifecycle_fixture_events(
    run_id: &str,
    source: &str,
    start_ts: i64,
) -> Vec<EventEnvelope> {
    let market = deterministic_fixture_market(start_ts);
    let up_token_id = market.outcomes[0].token_id.clone();
    let down_token_id = market.outcomes[1].token_id.clone();
    let market_id = market.market_id.clone();

    vec![
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            1,
            NormalizedEvent::MarketDiscovered { market },
        ),
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            2,
            NormalizedEvent::BookSnapshot {
                book: deterministic_fixture_book(&market_id, &up_token_id, 0.50, 0.51, start_ts),
            },
        ),
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            3,
            NormalizedEvent::BookSnapshot {
                book: deterministic_fixture_book(&market_id, &down_token_id, 0.49, 0.51, start_ts),
            },
        ),
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            4,
            NormalizedEvent::ReferenceTick {
                price: deterministic_fixture_price(
                    Asset::Btc,
                    Asset::Btc.chainlink_resolution_source(),
                    100.0,
                    start_ts + 300_004,
                ),
            },
        ),
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            5,
            NormalizedEvent::PredictiveTick {
                price: deterministic_fixture_price(
                    Asset::Btc,
                    SOURCE_BINANCE,
                    101.0,
                    start_ts + 300_005,
                ),
            },
        ),
        deterministic_fixture_envelope(
            run_id,
            source,
            start_ts,
            6,
            NormalizedEvent::LastTrade {
                market_id,
                token_id: up_token_id,
                side: Side::Buy,
                price: 0.51,
                size: 10.0,
                fee_rate_bps: Some(200.0),
                source_ts: Some(start_ts + 300_200),
            },
        ),
    ]
}

fn deterministic_fixture_envelope(
    run_id: &str,
    source: &str,
    start_ts: i64,
    seq: u64,
    payload: NormalizedEvent,
) -> EventEnvelope {
    EventEnvelope::new(
        run_id,
        format!("deterministic-fixture-{seq}"),
        source,
        start_ts + 300_000 + seq as i64,
        seq,
        seq,
        payload,
    )
}

fn deterministic_fixture_market(start_ts: i64) -> Market {
    Market {
        market_id: "deterministic-btc-taker-market".to_string(),
        slug: "btc-up-down-15m-deterministic-fixture".to_string(),
        title: "BTC Up or Down Deterministic Fixture".to_string(),
        asset: Asset::Btc,
        condition_id: "deterministic-btc-taker-market".to_string(),
        outcomes: vec![
            OutcomeToken {
                token_id: "deterministic-btc-up-token".to_string(),
                outcome: "Up".to_string(),
            },
            OutcomeToken {
                token_id: "deterministic-btc-down-token".to_string(),
                outcome: "Down".to_string(),
            },
        ],
        start_ts,
        end_ts: start_ts + 900_000,
        resolution_source: Some(Asset::Btc.chainlink_resolution_source().to_string()),
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

fn deterministic_fixture_book(
    market_id: &str,
    token_id: &str,
    best_bid: f64,
    best_ask: f64,
    start_ts: i64,
) -> OrderBookSnapshot {
    OrderBookSnapshot {
        market_id: market_id.to_string(),
        token_id: token_id.to_string(),
        bids: vec![OrderBookLevel {
            price: best_bid,
            size: 100.0,
        }],
        asks: vec![OrderBookLevel {
            price: best_ask,
            size: 100.0,
        }],
        hash: Some(format!("{token_id}-fixture-hash")),
        source_ts: Some(start_ts + 299_000),
    }
}

fn deterministic_fixture_price(
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

#[derive(Debug, Default)]
struct PaperCaptureCounts {
    raw_messages: usize,
    normalized_events: usize,
}

async fn capture_paper_cycle(
    config: &AppConfig,
    run_id: &str,
    storage: &FileSessionStorage,
    markets: &[Market],
    cycle_index: u64,
    message_limit: usize,
) -> Result<PaperCaptureCounts, Box<dyn std::error::Error>> {
    let mut counts = PaperCaptureCounts::default();
    let mut ingest_seq = 1_000_000_u64.saturating_mul(cycle_index + 1);
    let snapshot_client = PolymarketBookSnapshotClient::new(
        &config.polymarket.clob_rest_url,
        config.polymarket.request_timeout_ms,
    )?;

    record_book_snapshots_for_markets(
        storage,
        run_id,
        &snapshot_client,
        markets,
        &mut ingest_seq,
        &mut counts,
    )
    .await?;

    let asset_ids = markets
        .iter()
        .flat_map(|market| {
            market
                .outcomes
                .iter()
                .map(|outcome| outcome.token_id.clone())
        })
        .collect::<Vec<_>>();
    let polymarket_subscription = PolymarketMarketSubscription::new(asset_ids);
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
    let client = ReadOnlyWebSocketClient;

    for probe in probes {
        let result = client.connect_and_capture(&probe).await?;
        let mut health = FeedHealthTracker::new(&probe.source, config.feeds.stale_after_ms);
        health.mark_connected(unix_time_ms());
        let recorder = FeedRecorder::new(storage, run_id, probe.source.clone());
        let mut normalized_count = 0usize;
        let mut unknown_count = 0usize;
        for message in result.received_text_messages {
            let recv_wall_ts = unix_time_ms();
            let recorded =
                recorder.record_message(message, recv_wall_ts, monotonic_like_ns(), ingest_seq)?;
            ingest_seq += 1;
            counts.raw_messages += 1;
            counts.normalized_events += recorded.normalized_event_count;
            normalized_count += recorded.normalized_event_count;
            if recorded.unknown_event_type.is_some() {
                unknown_count += 1;
            }
            health.mark_message(recv_wall_ts, None);
        }
        let observed_health = health.observe(unix_time_ms());

        println!(
            "paper_feed_source={},connected={},normalized_events={},unknown_messages={},health={:?}",
            probe.source,
            result.connected,
            normalized_count,
            unknown_count,
            observed_health.status
        );
        if normalized_count == 0 && paper_probe_requires_normalized_events(&probe.source) {
            return Err(format!(
                "paper feed source {} connected but produced no normalized events",
                probe.source
            )
            .into());
        }
    }

    record_reference_ticks(
        config,
        run_id,
        storage,
        markets,
        &mut ingest_seq,
        &mut counts,
        message_limit,
    )
    .await?;

    Ok(counts)
}

fn paper_probe_requires_normalized_events(source: &str) -> bool {
    source != SOURCE_POLYMARKET_CLOB
}

async fn record_reference_ticks(
    config: &AppConfig,
    run_id: &str,
    storage: &FileSessionStorage,
    markets: &[Market],
    ingest_seq: &mut u64,
    counts: &mut PaperCaptureCounts,
    message_limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if config.reference_feed.is_polymarket_rtds_chainlink_enabled() {
        record_polymarket_rtds_chainlink_reference_ticks(
            config,
            run_id,
            storage,
            markets,
            ingest_seq,
            counts,
            message_limit,
        )
        .await
    } else {
        record_pyth_proxy_reference_ticks(config, run_id, storage, ingest_seq, counts).await
    }
}

async fn record_pyth_proxy_reference_ticks(
    config: &AppConfig,
    run_id: &str,
    storage: &FileSessionStorage,
    ingest_seq: &mut u64,
    counts: &mut PaperCaptureCounts,
) -> Result<(), Box<dyn std::error::Error>> {
    if !config.reference_feed.is_pyth_proxy_enabled() {
        return Ok(());
    }

    let recv_wall_ts = unix_time_ms();
    let recv_mono_ns = monotonic_like_ns();
    let client = PythHermesClient::new(
        &config.reference_feed.pyth_hermes_url,
        config.polymarket.request_timeout_ms,
    )?;
    let batch = client
        .fetch_latest(&config.reference_feed, recv_wall_ts)
        .await?;

    storage.append_raw_message(RawMessage {
        run_id: run_id.to_string(),
        source: SOURCE_PYTH_PROXY.to_string(),
        recv_wall_ts,
        recv_mono_ns,
        ingest_seq: *ingest_seq,
        payload: batch.raw_payload,
    })?;
    counts.raw_messages += 1;

    let event_count = batch.events.len();
    for (index, event) in batch.events.into_iter().enumerate() {
        storage.append_normalized_event(EventEnvelope::new(
            run_id,
            format!("{SOURCE_PYTH_PROXY}-{}-{index}", *ingest_seq),
            SOURCE_PYTH_PROXY,
            recv_wall_ts,
            recv_mono_ns + index as u64,
            *ingest_seq + index as u64,
            event,
        ))?;
        counts.normalized_events += 1;
    }
    *ingest_seq += 1 + event_count as u64;

    println!(
        "paper_reference_feed_source={SOURCE_PYTH_PROXY},provider=pyth,normalized_events={event_count},matches_market_resolution_source=false,settlement_reference_evidence=false"
    );

    if event_count == 0 {
        return Err("pyth proxy reference feed produced no reference ticks".into());
    }

    Ok(())
}

async fn record_polymarket_rtds_chainlink_reference_ticks(
    config: &AppConfig,
    run_id: &str,
    storage: &FileSessionStorage,
    markets: &[Market],
    ingest_seq: &mut u64,
    counts: &mut PaperCaptureCounts,
    message_limit: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut event_count = 0usize;
    let mut assets = Vec::<Asset>::new();
    let client = ReadOnlyWebSocketClient;
    let snapshot_client = PolymarketBookSnapshotClient::new(
        &config.polymarket.clob_rest_url,
        config.polymarket.request_timeout_ms,
    )?;

    for subscribed_asset in [Asset::Btc, Asset::Eth, Asset::Sol] {
        let mut asset_event_count = 0usize;
        let probe = FeedConnectionConfig {
            source: SOURCE_POLYMARKET_RTDS_CHAINLINK.to_string(),
            ws_url: config.reference_feed.polymarket_rtds_url.clone(),
            subscribe_payload: Some(polymarket_rtds_chainlink_subscription_payload_for_asset(
                subscribed_asset,
            )),
            message_limit: message_limit.max(1),
            connect_timeout_ms: config.feeds.connect_timeout_ms,
            read_timeout_ms: config.feeds.read_timeout_ms,
        };
        let result = client.connect_and_capture(&probe).await?;

        for message in result.received_text_messages {
            let recv_wall_ts = unix_time_ms();
            let recv_mono_ns = monotonic_like_ns();
            storage.append_raw_message(RawMessage {
                run_id: run_id.to_string(),
                source: SOURCE_POLYMARKET_RTDS_CHAINLINK.to_string(),
                recv_wall_ts,
                recv_mono_ns,
                ingest_seq: *ingest_seq,
                payload: message.clone(),
            })?;
            counts.raw_messages += 1;

            let events = match parse_polymarket_rtds_chainlink_message(
                &message,
                recv_wall_ts,
                config.reference_feed.max_staleness_ms,
            ) {
                Ok(events) => events,
                Err(error) if should_skip_stale_polymarket_rtds_reference_error(&error) => {
                    println!(
                        "paper_reference_feed_source={SOURCE_POLYMARKET_RTDS_CHAINLINK},provider={PROVIDER_POLYMARKET_RTDS_CHAINLINK},skipped_stale_reference_update=true,error={error}"
                    );
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let message_event_count = events.len();
            for (index, event) in events.into_iter().enumerate() {
                if let Some(asset) = event.asset() {
                    if !assets.contains(&asset) {
                        assets.push(asset);
                    }
                }
                storage.append_normalized_event(EventEnvelope::new(
                    run_id,
                    format!("{SOURCE_POLYMARKET_RTDS_CHAINLINK}-{}-{index}", *ingest_seq),
                    SOURCE_POLYMARKET_RTDS_CHAINLINK,
                    recv_wall_ts,
                    recv_mono_ns + index as u64,
                    *ingest_seq + index as u64,
                    event,
                ))?;
                counts.normalized_events += 1;
                event_count += 1;
                asset_event_count += 1;
            }
            *ingest_seq += 1 + message_event_count as u64;
        }

        if asset_event_count > 0 {
            let asset_markets = markets
                .iter()
                .filter(|market| market.asset == subscribed_asset)
                .cloned()
                .collect::<Vec<_>>();
            record_book_snapshots_for_markets(
                storage,
                run_id,
                &snapshot_client,
                &asset_markets,
                ingest_seq,
                counts,
            )
            .await?;
        }
    }

    let asset_list = assets
        .iter()
        .map(|asset| asset.symbol())
        .collect::<Vec<_>>()
        .join("|");
    println!(
        "paper_reference_feed_source={SOURCE_POLYMARKET_RTDS_CHAINLINK},provider={PROVIDER_POLYMARKET_RTDS_CHAINLINK},normalized_events={event_count},assets={asset_list},matches_market_resolution_source=true,settlement_reference_evidence=true,live_readiness_evidence=false"
    );

    if assets.len() != 3 {
        return Err(format!(
            "Polymarket RTDS Chainlink reference feed produced {} of 3 required BTC/ETH/SOL ticks",
            assets.len()
        )
        .into());
    }

    Ok(())
}

fn should_skip_stale_polymarket_rtds_reference_error(error: &ReferenceFeedError) -> bool {
    matches!(
        error,
        ReferenceFeedError::StalePrice { provider, .. }
            if *provider == PROVIDER_POLYMARKET_RTDS_CHAINLINK
    )
}

async fn record_book_snapshots_for_markets(
    storage: &FileSessionStorage,
    run_id: &str,
    snapshot_client: &PolymarketBookSnapshotClient,
    markets: &[Market],
    ingest_seq: &mut u64,
    counts: &mut PaperCaptureCounts,
) -> Result<(), Box<dyn std::error::Error>> {
    let recorder = FeedRecorder::new(storage, run_id, SOURCE_POLYMARKET_CLOB);
    for market in markets {
        for outcome in &market.outcomes {
            let payload = snapshot_client.fetch_book(&outcome.token_id).await?;
            let recorded = recorder.record_message(
                payload,
                unix_time_ms(),
                monotonic_like_ns(),
                *ingest_seq,
            )?;
            *ingest_seq += 1;
            counts.raw_messages += 1;
            counts.normalized_events += recorded.normalized_event_count;
            if recorded.normalized_event_count == 0 {
                return Err(format!(
                    "paper book snapshot for token_id={} produced no normalized event",
                    outcome.token_id
                )
                .into());
            }
        }
    }

    Ok(())
}

fn select_paper_markets(
    markets: &[Market],
    now_wall_ts: i64,
) -> Result<Vec<Market>, Box<dyn std::error::Error>> {
    let mut selected = Vec::new();
    for asset in [Asset::Btc, Asset::Eth, Asset::Sol] {
        let Some(market) = markets
            .iter()
            .filter(|market| {
                market.asset == asset
                    && market.ineligibility_reason.is_none()
                    && market.outcomes.len() == 2
                    && market.lifecycle_state == MarketLifecycleState::Active
                    && market.start_ts <= now_wall_ts
                    && now_wall_ts < market.end_ts
            })
            .min_by_key(|market| market.end_ts)
        else {
            let next_start_ts = markets
                .iter()
                .filter(|market| {
                    market.asset == asset
                        && market.ineligibility_reason.is_none()
                        && market.outcomes.len() == 2
                        && market.lifecycle_state == MarketLifecycleState::Active
                        && market.start_ts > now_wall_ts
                })
                .map(|market| market.start_ts)
                .min()
                .map(|start_ts| start_ts.to_string())
                .unwrap_or_else(|| "none".to_string());
            return Err(format!(
                "paper runtime requires one eligible in-window active {} 15m market at now_wall_ts={now_wall_ts}; next eligible start_ts={next_start_ts}",
                asset.symbol(),
            )
            .into());
        };
        selected.push(market.clone());
    }
    Ok(selected)
}

fn append_new_recorded_paper_events(
    storage: &FileSessionStorage,
    run_id: &str,
    result: &ReplayRunResult,
) -> Result<usize, Box<dyn std::error::Error>> {
    let recorded_count = result.recorded_paper_events.len();
    if recorded_count > result.generated_paper_events.len()
        || result.generated_paper_events[..recorded_count] != result.recorded_paper_events
    {
        return Err(format!(
            "recorded paper events diverged before append: generated_count={} recorded_count={recorded_count}",
            result.generated_paper_events.len()
        )
        .into());
    }

    let mut appended = 0usize;
    for (offset, event) in result
        .generated_paper_events
        .iter()
        .skip(recorded_count)
        .cloned()
        .enumerate()
    {
        let index = recorded_count + offset;
        storage.append_normalized_event(EventEnvelope::new(
            run_id,
            format!("paper-runtime-recorded-{index}"),
            "paper_runtime",
            unix_time_ms(),
            monotonic_like_ns() + index as u64,
            9_000_000 + index as u64,
            event,
        ))?;
        appended += 1;
    }

    Ok(appended)
}

fn append_recorded_paper_events_deterministic(
    storage: &FileSessionStorage,
    run_id: &str,
    result: &ReplayRunResult,
    source: &str,
    base_wall_ts: i64,
) -> Result<usize, Box<dyn std::error::Error>> {
    if !result.recorded_paper_events.is_empty() {
        return Err(format!(
            "deterministic fixture expected no pre-recorded paper events, found {}",
            result.recorded_paper_events.len()
        )
        .into());
    }

    for (index, event) in result.generated_paper_events.iter().cloned().enumerate() {
        let seq = 10_000 + index as u64;
        storage.append_normalized_event(EventEnvelope::new(
            run_id,
            format!("deterministic-fixture-recorded-paper-{index}"),
            source,
            base_wall_ts + index as i64,
            seq,
            seq,
            event,
        ))?;
    }

    Ok(result.generated_paper_events.len())
}

fn persist_paper_outputs(
    storage: &FileSessionStorage,
    run_id: &str,
    config: &AppConfig,
    result: &ReplayRunResult,
) -> Result<(), Box<dyn std::error::Error>> {
    persist_paper_outputs_at(storage, run_id, config, result, unix_time_ms())
}

fn persist_paper_outputs_at(
    storage: &FileSessionStorage,
    run_id: &str,
    config: &AppConfig,
    result: &ReplayRunResult,
    updated_ts: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    for order in &result.generated_orders {
        storage.insert_paper_order(order.clone())?;
    }
    if result.generated_orders.is_empty() {
        storage.write_session_artifact(run_id, "paper_orders.jsonl", b"")?;
    }
    for fill in &result.generated_fills {
        storage.insert_paper_fill(fill.clone())?;
    }
    if result.generated_fills.is_empty() {
        storage.write_session_artifact(run_id, "paper_fills.jsonl", b"")?;
    }
    for position in &result.position_snapshots {
        storage.upsert_paper_position(position.clone())?;
    }
    if result.position_snapshots.is_empty() {
        storage.write_session_artifact(run_id, "paper_positions.jsonl", b"")?;
    }
    let mut risk_event_count = 0usize;
    for (index, event) in result.generated_events.iter().enumerate() {
        if let NormalizedEvent::RiskHalt { risk_state, .. } = event {
            storage.insert_risk_event(RiskEvent {
                run_id: run_id.to_string(),
                event_id: format!("risk-runtime-{index}"),
                risk_state: risk_state.clone(),
            })?;
            risk_event_count += 1;
        }
    }
    if risk_event_count == 0 {
        storage.write_session_artifact(run_id, "risk_events.jsonl", b"")?;
    }
    storage.upsert_paper_balance(PaperBalanceSnapshot {
        run_id: run_id.to_string(),
        starting_balance: config.paper.starting_balance,
        cash_balance: config.paper.starting_balance + result.report.pnl.totals.realized_pnl,
        realized_pnl: result.report.pnl.totals.realized_pnl,
        unrealized_pnl: result.report.pnl.totals.unrealized_pnl,
        updated_ts,
    })?;
    Ok(())
}

fn write_runtime_artifacts(
    storage: &FileSessionStorage,
    run_id: &str,
    report_file: &str,
    metrics_file: &str,
    result: &ReplayRunResult,
    determinism_failed: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let report_path = storage.write_session_artifact(
        run_id,
        report_file,
        &deterministic_report_json(&result.report),
    )?;
    let metrics = metrics_from_replay_result(result, determinism_failed).render_prometheus();
    storage.write_session_artifact(run_id, metrics_file, metrics.as_bytes())?;
    Ok(report_path)
}

fn metrics_from_replay_result(
    result: &ReplayRunResult,
    determinism_failed: bool,
) -> MetricsSnapshot {
    let mut snapshot = MetricsSnapshot::new();
    for source in [SOURCE_POLYMARKET_CLOB, SOURCE_BINANCE, SOURCE_COINBASE] {
        snapshot.record_feed_message_rate(source, 0.0);
        snapshot.record_feed_latency_ms(source, 0.0);
        snapshot.record_websocket_reconnects(source, 0);
    }
    snapshot.record_book_staleness_ms("session", "all", 0.0);
    for asset in [Asset::Btc, Asset::Eth, Asset::Sol] {
        snapshot.record_reference_staleness_ms(asset, "resolution_source", 0.0);
    }
    snapshot.record_signal_decision("all", "evaluated", result.report.signals.evaluated_count);
    snapshot.record_signal_decision("all", "skipped", result.report.signals.skipped_count);
    snapshot.record_signal_decision(
        "all",
        "emitted_order_intent",
        result.report.signals.emitted_order_intent_count,
    );

    let mut saw_halt = false;
    for decision in &result.report.risk.decisions {
        for reason in &decision.halt_reasons {
            saw_halt = true;
            snapshot.record_risk_halt(reason.clone(), 1);
        }
    }
    if !saw_halt {
        snapshot.record_risk_halt(RiskHaltReason::Unknown, 0);
    }

    if result.generated_orders.is_empty() {
        snapshot.record_paper_order(PaperOrderStatus::Created, 0);
    } else {
        for order in &result.generated_orders {
            snapshot.record_paper_order(order.status, 1);
        }
    }
    if result.generated_fills.is_empty() {
        snapshot.record_paper_fill("none", 0);
    } else {
        for fill in &result.generated_fills {
            snapshot.record_paper_fill(&fill.market_id, 1);
        }
    }
    if result.position_snapshots.is_empty() {
        snapshot.record_paper_pnl("none", Asset::Btc, "realized", 0.0);
        snapshot.record_paper_pnl("none", Asset::Btc, "unrealized", 0.0);
    } else {
        for position in &result.position_snapshots {
            snapshot.record_paper_pnl(
                &position.market_id,
                position.asset,
                "realized",
                position.realized_pnl,
            );
            snapshot.record_paper_pnl(
                &position.market_id,
                position.asset,
                "unrealized",
                position.unrealized_pnl,
            );
        }
    }
    snapshot.record_storage_write_failure("none", 0);
    snapshot.record_replay_determinism_failure(
        &result.report.metadata.replay_run_id,
        u64::from(determinism_failed),
    );
    snapshot
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
) -> Result<GeoblockResponse, Box<dyn std::error::Error>> {
    let geoblock = run_geoblock_validation(config).await?;

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

    Ok(geoblock)
}

async fn run_geoblock_validation(
    config: &AppConfig,
) -> Result<GeoblockResponse, Box<dyn std::error::Error>> {
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
    Ok(geoblock)
}

fn compliance_client(config: &AppConfig) -> Result<ComplianceClient, Box<dyn std::error::Error>> {
    Ok(ComplianceClient::new(
        &config.polymarket.geoblock_url,
        config.polymarket.request_timeout_ms,
    )?)
}

fn run_lb2_secret_handle_validation(
    inventory: &secret_handling::SecretInventory,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = EnvSecretPresenceProvider;
    let report = secret_handling::validate_secret_presence(inventory, &provider)?;
    let status = if report.all_present() {
        "ok"
    } else {
        "missing"
    };
    println!("live_beta_secret_presence_status={status}");
    for check in &report.checks {
        println!(
            "live_beta_secret_handle=label={},backend={},handle={},present={}",
            check.label, report.backend, check.handle, check.present
        );
    }
    if !report.all_present() {
        return Err(format!(
            "LB2 secret handles missing from approved backend: {}",
            report.missing_handle_list()
        )
        .into());
    }

    Ok(())
}

fn run_lb3_signing_dry_run_validation(
    config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let artifact =
        live_beta_signing::sample_live_beta_signing_dry_run(&config.polymarket.clob_rest_url)?;
    println!("live_beta_signing_dry_run_status=ok");
    println!(
        "live_beta_signing_dry_run_not_submitted={}",
        artifact.not_submitted
    );
    println!(
        "live_beta_signing_dry_run_network_post_enabled={}",
        artifact.network_post_enabled
    );
    println!(
        "live_beta_signing_dry_run_fingerprint={}",
        artifact.fingerprint()?
    );
    println!(
        "live_beta_signing_dry_run_artifact={}",
        serde_json::to_string(&artifact)?
    );
    Ok(())
}

async fn run_lb4_readback_preflight_validation(
    config: &AppConfig,
    geoblock_gate_status: safety::GeoblockGateStatus,
    local_only: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let prerequisites = lb4_readback_prerequisites(config, geoblock_gate_status);
    println!(
        "live_beta_readback_preflight_lb3_hold_released={}",
        prerequisites.lb3_hold_released
    );
    println!(
        "live_beta_readback_preflight_legal_access_approved={}",
        prerequisites.legal_access_approved
    );
    println!(
        "live_beta_readback_preflight_deployment_geoblock_passed={}",
        prerequisites.deployment_geoblock_passed
    );
    let report = if local_only || !lb4_prerequisites_ready(prerequisites) {
        live_beta_readback::sample_readback_preflight(prerequisites)?
    } else {
        let secret_inventory = config.live_beta.secret_inventory();
        run_lb2_secret_handle_validation(&secret_inventory)?;
        let credentials = lb4_l2_credentials_from_env(&config.live_beta.secret_handles)?;
        let account = lb4_account_preflight(config)?;
        println!(
            "live_beta_readback_preflight_wallet_address={}",
            account.wallet_address
        );
        println!(
            "live_beta_readback_preflight_funder_address={}",
            account.funder_address
        );
        println!(
            "live_beta_readback_preflight_signature_type={}",
            account.signature_type.as_config_str()
        );
        live_beta_readback::authenticated_readback_preflight(AuthenticatedReadbackInput {
            prerequisites,
            account,
            credentials,
            required_collateral_allowance_units: config
                .live_beta
                .readback_account
                .required_collateral_allowance_units,
            request_timeout_ms: config.polymarket.request_timeout_ms,
        })
        .await?
    };
    println!("live_beta_readback_preflight_status={}", report.status);
    println!(
        "live_beta_readback_preflight_live_network_enabled={}",
        report.live_network_enabled
    );
    println!(
        "live_beta_readback_preflight_block_reasons={}",
        report.block_reasons.join(",")
    );
    println!(
        "live_beta_readback_preflight_open_order_count={}",
        report.open_order_count
    );
    println!(
        "live_beta_readback_preflight_trade_count={}",
        report.trade_count
    );
    println!(
        "live_beta_readback_preflight_reserved_pusd_units={}",
        report.reserved_pusd_units
    );
    println!(
        "live_beta_readback_preflight_available_pusd_units={}",
        report.available_pusd_units
    );
    println!(
        "live_beta_readback_preflight_venue_state={}",
        report.venue_state
    );
    println!(
        "live_beta_readback_preflight_heartbeat={}",
        report.heartbeat
    );
    println!(
        "live_beta_readback_preflight_report={}",
        serde_json::to_string(&report)?
    );
    if !report.passed() {
        return Err(format!(
            "LB4 readback/account preflight blocked: {}",
            report.block_reasons.join(",")
        )
        .into());
    }

    Ok(())
}

fn lb4_prerequisites_ready(prerequisites: ReadbackPrerequisites) -> bool {
    prerequisites.lb3_hold_released
        && prerequisites.legal_access_approved
        && prerequisites.deployment_geoblock_passed
}

fn lb4_account_preflight(
    config: &AppConfig,
) -> Result<AccountPreflight, Box<dyn std::error::Error>> {
    let account = &config.live_beta.readback_account;
    let Some(signature_type) = SignatureType::from_config(&account.signature_type) else {
        return Err(
            "LB4 readback account signature_type must be eoa, poly_proxy, or gnosis_safe".into(),
        );
    };
    Ok(AccountPreflight {
        clob_host: config.polymarket.clob_rest_url.clone(),
        chain_id: 137,
        wallet_address: account.wallet_address.clone(),
        funder_address: account.funder_address.clone(),
        signature_type,
    })
}

fn lb4_l2_credentials_from_env(
    handles: &LiveBetaSecretHandlesConfig,
) -> Result<L2ReadbackCredentials, Box<dyn std::error::Error>> {
    Ok(L2ReadbackCredentials {
        api_key: env::var(&handles.clob_l2_access)
            .map_err(|_| "LB4 clob_l2_access handle is not present")?,
        api_secret: env::var(&handles.clob_l2_credential)
            .map_err(|_| "LB4 clob_l2_credential handle is not present")?,
        api_passphrase: env::var(&handles.clob_l2_passphrase)
            .map_err(|_| "LB4 clob_l2_passphrase handle is not present")?,
    })
}

fn lb4_readback_prerequisites(
    config: &AppConfig,
    geoblock_gate_status: safety::GeoblockGateStatus,
) -> ReadbackPrerequisites {
    ReadbackPrerequisites {
        lb3_hold_released: config.live_beta.lb3_hold_released,
        legal_access_approved: config.live_beta.legal_access_approved,
        deployment_geoblock_passed: geoblock_gate_status == safety::GeoblockGateStatus::Passed,
    }
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

#[cfg(unix)]
async fn shutdown_signal() -> std::io::Result<()> {
    use tokio::signal::unix::{signal, SignalKind};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        signal = tokio::signal::ctrl_c() => signal,
        _ = terminate.recv() => Ok(()),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .ok()
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default()
}

fn format_utc_ms(timestamp_ms: i64) -> String {
    let Some(timestamp_ns) = i128::from(timestamp_ms).checked_mul(1_000_000) else {
        return format!("invalid:{timestamp_ms}");
    };
    let Ok(timestamp) = OffsetDateTime::from_unix_timestamp_nanos(timestamp_ns) else {
        return format!("invalid:{timestamp_ms}");
    };
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| format!("invalid:{timestamp_ms}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_capture_allows_quiet_clob_websocket_when_snapshots_are_recorded() {
        assert!(!paper_probe_requires_normalized_events(
            SOURCE_POLYMARKET_CLOB
        ));
        assert!(paper_probe_requires_normalized_events(SOURCE_BINANCE));
        assert!(paper_probe_requires_normalized_events(SOURCE_COINBASE));
    }

    #[test]
    fn stale_polymarket_rtds_updates_are_skipped_without_relaxing_other_errors() {
        let stale = ReferenceFeedError::StalePrice {
            provider: PROVIDER_POLYMARKET_RTDS_CHAINLINK,
            asset: Asset::Btc,
            age_ms: 6_000,
            max_staleness_ms: 5_000,
        };
        let protocol = ReferenceFeedError::Protocol("bad frame".to_string());

        assert!(should_skip_stale_polymarket_rtds_reference_error(&stale));
        assert!(!should_skip_stale_polymarket_rtds_reference_error(
            &protocol
        ));
    }

    #[test]
    fn paper_market_selection_requires_current_window() {
        let now = 1_777_000_000_000;
        let future_start = now + 86_400_000;
        let markets = vec![
            test_paper_market(Asset::Btc, future_start, future_start + 900_000),
            test_paper_market(Asset::Eth, future_start, future_start + 900_000),
            test_paper_market(Asset::Sol, future_start, future_start + 900_000),
            test_paper_market(Asset::Btc, now - 60_000, now + 840_000),
            test_paper_market(Asset::Eth, now - 60_000, now + 840_000),
            test_paper_market(Asset::Sol, now - 60_000, now + 840_000),
        ];

        let selected = select_paper_markets(&markets, now).expect("current markets selected");

        assert_eq!(selected.len(), 3);
        assert!(selected.iter().all(|market| market.start_ts <= now));
        assert!(selected.iter().all(|market| now < market.end_ts));
    }

    #[test]
    fn paper_market_selection_rejects_pre_start_only_markets() {
        let now = 1_777_000_000_000;
        let future_start = now + 86_400_000;
        let markets = vec![
            test_paper_market(Asset::Btc, future_start, future_start + 900_000),
            test_paper_market(Asset::Eth, future_start, future_start + 900_000),
            test_paper_market(Asset::Sol, future_start, future_start + 900_000),
        ];

        let error = select_paper_markets(&markets, now)
            .expect_err("pre-start markets must not be selected")
            .to_string();

        assert!(error.contains("in-window active BTC 15m market"));
        assert!(error.contains(&format!("next eligible start_ts={future_start}")));
    }

    #[test]
    fn utc_ms_formatter_outputs_rfc3339_utc() {
        assert_eq!(format_utc_ms(1_777_431_600_000), "2026-04-29T03:00:00Z");
    }

    #[test]
    fn lb4_readback_prerequisites_follow_runtime_config_and_geoblock_status() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        let local_prerequisites =
            lb4_readback_prerequisites(&config, safety::GeoblockGateStatus::Unknown);

        assert!(local_prerequisites.lb3_hold_released);
        assert!(!local_prerequisites.legal_access_approved);
        assert!(!local_prerequisites.deployment_geoblock_passed);

        config.live_beta.legal_access_approved = true;
        let approved_host_prerequisites =
            lb4_readback_prerequisites(&config, safety::GeoblockGateStatus::Passed);

        assert_eq!(
            approved_host_prerequisites,
            ReadbackPrerequisites {
                lb3_hold_released: true,
                legal_access_approved: true,
                deployment_geoblock_passed: true,
            }
        );
        let report = live_beta_readback::sample_readback_preflight(approved_host_prerequisites)
            .expect("approved runtime prerequisites can pass LB4 preflight");

        assert_eq!(report.status, "passed");
    }

    fn test_paper_market(asset: Asset, start_ts: i64, end_ts: i64) -> Market {
        let symbol = asset.symbol().to_ascii_lowercase();
        Market {
            market_id: format!("{symbol}-{start_ts}"),
            slug: format!("{symbol}-updown-15m-{}", start_ts / 1_000),
            title: format!("{} Up or Down", asset.symbol()),
            asset,
            condition_id: format!("{symbol}-condition-{start_ts}"),
            outcomes: vec![
                OutcomeToken {
                    token_id: format!("{symbol}-up-{start_ts}"),
                    outcome: "Up".to_string(),
                },
                OutcomeToken {
                    token_id: format!("{symbol}-down-{start_ts}"),
                    outcome: "Down".to_string(),
                },
            ],
            start_ts,
            end_ts,
            resolution_source: Some(asset.chainlink_resolution_source().to_string()),
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
}
