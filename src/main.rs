use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    live_alpha_config::LiveAlphaMode,
    live_alpha_gate::{self, LiveAlphaGateInput, LiveAlphaReadinessStatus},
    live_alpha_preflight::{
        self, LiveAlphaCurrentPreflight, LiveAlphaPreflightMode, LiveAlphaPreflightReport,
    },
    live_balance_tracker::LiveBalanceSnapshot,
    live_beta_canary::{
        self, CanaryApprovalContext, CanaryApprovalGuard, CanaryGateStatus, CanaryMode,
        CanaryOrderCapState, CanaryOrderPlan, CanaryRuntimeChecks, PreauthorizedEnvelopeBinding,
    },
    live_beta_cancel,
    live_beta_order_lifecycle::{
        self, ExactCancelInput, ExactCancelRuntimeChecks, ExactOrderReadbackInput,
        ExpectedCanaryOrder,
    },
    live_beta_readback::{
        self, AccountPreflight, AuthenticatedReadbackInput, AuthenticatedReadbackPreflightEvidence,
        BalanceAllowanceReadback, L2ReadbackCredentials, OpenOrderReadback,
        ReadbackPreflightReport, ReadbackPrerequisites, SignatureType, TradeReadback,
        TradeReadbackStatus,
    },
    live_beta_signing,
    live_executor::{
        ExecutionDecision, ExecutionSink, LiveMakerExecution, LiveMakerExecutionContext,
        ShadowLiveDecision, ShadowLiveReport,
    },
    live_fill_canary::{
        self, LiveAlphaApprovalArtifact, LiveAlphaFillCanaryCapState, LiveAlphaFillSubmitInput,
    },
    live_maker_micro::{
        cancel_exact_maker_order_with_official_sdk, post_maker_heartbeat_with_official_sdk,
        read_maker_order_with_official_sdk, submit_maker_order_with_official_sdk,
        LiveMakerOrderPlan, LiveMakerOrderReadbackReport, LiveMakerSubmissionReport,
        LiveMakerSubmitInput, GTD_SECURITY_BUFFER_SECONDS,
    },
    live_order_journal::{
        reduce_live_journal_events, LiveJournalEvent, LiveJournalEventType, LiveOrderJournal,
    },
    live_position_book::LivePositionBook,
    live_reconciliation::{
        reconcile_live_state, LiveReconciliationInput, LocalLiveState, VenueLiveState,
        VenueOrderState, VenueOrderStatus, VenueTradeState, VenueTradeStatus,
    },
    live_risk_engine::{LiveRiskContext, LiveRiskDecision, LiveRiskEngine},
    live_startup_recovery::{
        self, LiveStartupRecoveryBlockReason, LiveStartupRecoveryInput, LiveStartupRecoveryReport,
        LiveStartupRecoveryStatus, StartupRecoveryCheckStatus,
    },
    market_discovery::{
        emit_market_lifecycle_events, persist_discovered_markets, MarketDiscoveryClient,
    },
    metrics::{
        m8_smoke_metrics_snapshot, required_m8_metric_families, serve_prometheus_once,
        MetricsSnapshot,
    },
    module_names,
    normalization::{
        normalize_feed_message, SOURCE_BINANCE, SOURCE_COINBASE, SOURCE_POLYMARKET_CLOB,
    },
    reference_feed::{
        parse_polymarket_rtds_chainlink_message,
        polymarket_rtds_chainlink_subscription_payload_for_asset, PythHermesClient,
        ReferenceFeedError, PROVIDER_POLYMARKET_RTDS_CHAINLINK, SOURCE_POLYMARKET_RTDS_CHAINLINK,
        SOURCE_PYTH_PROXY,
    },
    replay::{
        compare_generated_to_recorded_paper_events, compare_replay_results, ReplayEngine,
        ReplayRunResult, ShadowLiveRuntimeReadiness,
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
use serde::Serialize;
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
#[allow(clippy::large_enum_variant)]
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
        #[arg(
            long,
            help = "Evaluate the LB5 single-order cancel readiness gate without live canceling"
        )]
        live_cancel_readiness: bool,
        #[arg(long, help = "Override feed smoke message limit")]
        feed_message_limit: Option<usize>,
    },
    /// Run read-only paper trading against captured market/reference feeds.
    Paper {
        #[arg(long, help = "Override generated paper run ID")]
        run_id: Option<String>,
        #[arg(
            long,
            help = "Record LA4 shadow-live decisions alongside paper execution without live order actions"
        )]
        shadow_live_alpha: bool,
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
    /// Evaluate or execute the exact one-order LB6 canary gate.
    LiveCanary {
        #[arg(long, help = "Evaluate the LB6 canary gate without submitting")]
        dry_run: bool,
        #[arg(
            long,
            help = "Final gated mode; may submit only after all exact gates pass"
        )]
        human_approved: bool,
        #[arg(
            long,
            help = "Use the reviewed LB6 pre-authorized ETH one-order canary envelope"
        )]
        preauthorized_envelope: bool,
        #[arg(
            long,
            help = "Required in final gated mode to enforce the one-order cap"
        )]
        one_order: bool,
        #[arg(long, help = "Exact approval text matching the generated LB6 prompt")]
        approval_text: Option<String>,
        #[arg(long, help = "Expected sha256:<hex> hash of the exact approval text")]
        approval_sha256: Option<String>,
        #[arg(long, help = "Unix timestamp after which the approval expires")]
        approval_expires_at_unix: Option<u64>,
        #[arg(long)]
        market_slug: String,
        #[arg(long)]
        condition_id: String,
        #[arg(long)]
        token_id: String,
        #[arg(long)]
        outcome: String,
        #[arg(long)]
        side: String,
        #[arg(long)]
        price: f64,
        #[arg(long)]
        size: f64,
        #[arg(long)]
        notional: f64,
        #[arg(long, default_value = "GTD")]
        order_type: String,
        #[arg(long, default_value_t = true)]
        post_only: bool,
        #[arg(long, default_value_t = true)]
        maker_only: bool,
        #[arg(long, default_value_t = 0.01)]
        tick_size: f64,
        #[arg(long)]
        gtd_expiry_unix: u64,
        #[arg(long)]
        market_end_unix: u64,
        #[arg(long)]
        best_bid: f64,
        #[arg(long)]
        best_ask: f64,
        #[arg(
            long,
            help = "Age in milliseconds of the fresh book snapshot used for best bid/ask"
        )]
        book_age_ms: u64,
        #[arg(
            long,
            help = "Age in milliseconds of the reference feed value used for final check"
        )]
        reference_age_ms: u64,
        #[arg(
            long,
            default_value = "reports/live-beta-lb6-one-order-canary-state.json",
            help = "Local non-secret sentinel used to prevent a second LB6 canary attempt"
        )]
        order_cap_state: PathBuf,
    },
    /// Read back and, with exact approval, cancel the one LB6 canary order.
    LiveCancel {
        #[arg(
            long,
            help = "Read back the exact order and evaluate cancel readiness only"
        )]
        dry_run: bool,
        #[arg(
            long,
            help = "Final gated mode; may cancel only the exact one canary order"
        )]
        human_approved: bool,
        #[arg(long, help = "Required in final gated mode to enforce one-order scope")]
        one_order: bool,
        #[arg(
            long,
            help = "Exact venue order ID written by the LB6 canary submission"
        )]
        order_id: String,
        #[arg(
            long,
            help = "LB6 canary approval hash recorded in the one-order cap state"
        )]
        canary_approval_sha256: String,
        #[arg(long, help = "Unix timestamp after which this cancel approval expires")]
        approval_expires_at_unix: Option<u64>,
        #[arg(long)]
        condition_id: String,
        #[arg(long)]
        token_id: String,
        #[arg(long)]
        side: String,
        #[arg(long)]
        price: f64,
        #[arg(long)]
        size: f64,
        #[arg(long, default_value = "GTD")]
        order_type: String,
        #[arg(
            long,
            default_value = "reports/live-beta-lb6-one-order-canary-state.json",
            help = "Local non-secret sentinel written by the LB6 canary submission"
        )]
        order_cap_state: PathBuf,
    },
    /// Run the LA3 controlled fill canary preflight without submitting.
    LiveAlphaPreflight {
        #[arg(long, help = "Required read-only mode; never submits orders")]
        read_only: bool,
        #[arg(
            long,
            default_value = "verification/2026-05-04-live-alpha-la3-approval.md",
            help = "Local LA3 approval artifact with exact host/account/market/order bounds"
        )]
        approval_artifact: PathBuf,
        #[arg(
            long,
            default_value = "reports/live-alpha-la3-fill-canary-state.json",
            help = "Local non-secret sentinel used to prevent a second LA3 fill attempt"
        )]
        order_cap_state: PathBuf,
    },
    /// Dry-run or submit the one approved LA3 controlled fill canary.
    LiveAlphaFillCanary {
        #[arg(
            long,
            help = "Validate the LA3 envelope and print approval prompt only"
        )]
        dry_run: bool,
        #[arg(
            long,
            help = "Final gated mode; may submit exactly one LA3 fill canary only if preflight passes"
        )]
        human_approved: bool,
        #[arg(long, help = "Approval ID from the LA3 approval artifact")]
        approval_id: Option<String>,
        #[arg(
            long,
            default_value = "verification/2026-05-04-live-alpha-la3-approval.md",
            help = "Local LA3 approval artifact with exact host/account/market/order bounds"
        )]
        approval_artifact: PathBuf,
        #[arg(
            long,
            default_value = "reports/live-alpha-la3-fill-canary-state.json",
            help = "Local non-secret sentinel used to prevent a second LA3 fill attempt"
        )]
        order_cap_state: PathBuf,
    },
    /// Dry-run or execute the gated LA5 maker-only micro path.
    LiveAlphaMakerMicro {
        #[arg(long, help = "Validate LA5 maker risk/order shape without submitting")]
        dry_run: bool,
        #[arg(
            long,
            help = "Final gated mode; may submit only after approval artifact and all gates pass"
        )]
        human_approved: bool,
        #[arg(long, help = "Approval ID from the LA5 approval artifact")]
        approval_id: Option<String>,
        #[arg(
            long,
            help = "Local LA5 approval artifact with exact account/risk/session bounds"
        )]
        approval_artifact: Option<PathBuf>,
        #[arg(long, default_value_t = 3, help = "Sequential LA5 order cap")]
        max_orders: u64,
        #[arg(
            long,
            default_value_t = 300,
            help = "LA5 session duration cap in seconds"
        )]
        max_duration_sec: u64,
    },
}

impl Commands {
    fn name(&self) -> &'static str {
        match self {
            Commands::Validate { .. } => "validate",
            Commands::Paper { .. } => "paper",
            Commands::Replay { .. } => "replay",
            Commands::LiveCanary { .. } => "live-canary",
            Commands::LiveCancel { .. } => "live-cancel",
            Commands::LiveAlphaPreflight { .. } => "live-alpha-preflight",
            Commands::LiveAlphaFillCanary { .. } => "live-alpha-fill-canary",
            Commands::LiveAlphaMakerMicro { .. } => "live-alpha-maker-micro",
        }
    }

    fn runtime_mode(&self) -> RuntimeMode {
        match self {
            Commands::Validate { .. } => RuntimeMode::Validate,
            Commands::Paper { .. } => RuntimeMode::Paper,
            Commands::Replay { .. } => RuntimeMode::Replay,
            Commands::LiveCanary { .. } => RuntimeMode::Validate,
            Commands::LiveCancel { .. } => RuntimeMode::Validate,
            Commands::LiveAlphaPreflight { .. } => RuntimeMode::Validate,
            Commands::LiveAlphaFillCanary { .. } => RuntimeMode::Validate,
            Commands::LiveAlphaMakerMicro { .. } => RuntimeMode::Validate,
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
                live_cancel_readiness,
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
                let live_alpha_summary = config.live_alpha.inert_summary();
                println!("live_alpha_enabled={}", live_alpha_summary.enabled);
                println!("live_alpha_mode={}", live_alpha_summary.mode.as_str());
                println!(
                    "live_alpha_fill_canary_enabled={}",
                    live_alpha_summary.fill_canary_enabled
                );
                println!(
                    "live_alpha_shadow_executor_enabled={}",
                    live_alpha_summary.shadow_executor_enabled
                );
                println!(
                    "live_alpha_maker_micro_enabled={}",
                    live_alpha_summary.maker_micro_enabled
                );
                println!(
                    "live_alpha_taker_enabled={}",
                    live_alpha_summary.taker_enabled
                );
                println!(
                    "live_alpha_scale_enabled={}",
                    live_alpha_summary.scale_enabled
                );
                println!(
                    "live_alpha_heartbeat_required={}",
                    config.live_alpha.heartbeat_required
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
                let live_readback_validation = if live_readback_preflight {
                    Some(
                        run_lb4_readback_preflight_validation(
                            &config,
                            geoblock_gate_status,
                            local_only,
                        )
                        .await?,
                    )
                } else {
                    None
                };
                let startup_recovery_input = live_alpha_startup_recovery_input_for_validate(
                    &config,
                    &run_id,
                    unix_time_ms(),
                    geoblock_gate_status,
                    live_readback_validation.as_ref(),
                );
                let startup_account_preflight_status =
                    readiness_from_startup_check(startup_recovery_input.account_preflight_status);
                let startup_recovery =
                    live_startup_recovery::evaluate_startup_recovery(startup_recovery_input);
                persist_startup_recovery_journal_events(&config, &startup_recovery)?;
                let startup_reconciliation_status =
                    reconciliation_readiness_from_startup_recovery(&startup_recovery);
                println!(
                    "live_alpha_startup_recovery_status={}",
                    startup_recovery.status_str()
                );
                println!(
                    "live_alpha_startup_recovery_block_reasons={}",
                    startup_recovery.block_reason_list()
                );
                println!(
                    "live_alpha_startup_recovery_journal_events={}",
                    live_journal_event_type_list(&startup_recovery.journal_event_types)
                );
                println!(
                    "live_alpha_startup_recovery_reconciliation_mismatches={}",
                    startup_recovery
                        .reconciliation_mismatches
                        .iter()
                        .map(|mismatch| mismatch.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                let live_alpha_gate =
                    live_alpha_gate::evaluate_live_alpha_gate(LiveAlphaGateInput {
                        live_alpha_enabled: config.live_alpha.enabled,
                        live_alpha_mode: config.live_alpha.mode,
                        fill_canary_enabled: config.live_alpha.fill_canary.enabled,
                        maker_enabled: config.live_alpha.maker.enabled,
                        taker_enabled: config.live_alpha.taker.enabled,
                        config_intent_enabled: config.live_alpha.enabled,
                        cli_intent_enabled: false,
                        kill_switch_active: config.live_beta.kill_switch_active,
                        geoblock_status: geoblock_gate_status,
                        account_preflight_status: startup_account_preflight_status,
                        heartbeat_required: config.live_alpha.heartbeat_required,
                        heartbeat_status: LiveAlphaReadinessStatus::Unknown,
                        reconciliation_status: startup_reconciliation_status,
                        approval_status: LiveAlphaReadinessStatus::Unknown,
                        phase_status: LiveAlphaReadinessStatus::Unknown,
                    });
                println!(
                    "live_alpha_compile_time_orders_enabled={}",
                    live_alpha_gate::LIVE_ALPHA_ORDER_FEATURE_ENABLED
                );
                println!("live_alpha_gate_status={}", live_alpha_gate.status());
                println!(
                    "live_alpha_gate_block_reasons={}",
                    live_alpha_gate.reason_list()
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
                if let Some(validation) = &live_readback_validation {
                    if !validation.report.passed() {
                        return Err(format!(
                            "LB4 readback/account preflight blocked: {}",
                            validation.report.block_reasons.join(",")
                        )
                        .into());
                    }
                }
                if live_cancel_readiness {
                    run_lb5_cancel_readiness_validation(&config)?;
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
                shadow_live_alpha,
                deterministic_fixture,
                feed_message_limit,
                cycles,
            } => {
                let paper_run_id = paper_run_id.unwrap_or(run_id.clone());
                if deterministic_fixture {
                    if shadow_live_alpha {
                        return Err(
                            "paper --shadow-live-alpha is supported for runtime paper mode, not deterministic fixture mode"
                                .into(),
                        );
                    }
                    run_deterministic_lifecycle_fixture(&config, &paper_run_id)?;
                } else {
                    run_paper_runtime(
                        &config,
                        &paper_run_id,
                        feed_message_limit,
                        cycles,
                        shadow_live_alpha,
                    )
                    .await?;
                }
            }
            Commands::Replay {
                run_id: replay_run_id,
            } => {
                let replay_run_id =
                    replay_run_id.ok_or("replay requires --run-id <stored paper run_id>")?;
                run_replay_runtime(&config, &run_id, &replay_run_id).await?;
            }
            Commands::LiveCanary {
                dry_run,
                human_approved,
                preauthorized_envelope,
                one_order,
                approval_text,
                approval_sha256,
                approval_expires_at_unix,
                market_slug,
                condition_id,
                token_id,
                outcome,
                side,
                price,
                size,
                notional,
                order_type,
                post_only,
                maker_only,
                tick_size,
                gtd_expiry_unix,
                market_end_unix,
                best_bid,
                best_ask,
                book_age_ms,
                reference_age_ms,
                order_cap_state,
            } => {
                run_lb6_live_canary(
                    &config,
                    dry_run,
                    human_approved,
                    preauthorized_envelope,
                    one_order,
                    approval_text,
                    approval_sha256,
                    approval_expires_at_unix,
                    market_slug,
                    condition_id,
                    token_id,
                    outcome,
                    side,
                    price,
                    size,
                    notional,
                    order_type,
                    post_only,
                    maker_only,
                    tick_size,
                    gtd_expiry_unix,
                    market_end_unix,
                    best_bid,
                    best_ask,
                    book_age_ms,
                    reference_age_ms,
                    &order_cap_state,
                    &run_id,
                )
                .await?;
            }
            Commands::LiveCancel {
                dry_run,
                human_approved,
                one_order,
                order_id,
                canary_approval_sha256,
                approval_expires_at_unix,
                condition_id,
                token_id,
                side,
                price,
                size,
                order_type,
                order_cap_state,
            } => {
                run_lb6_live_cancel(
                    &config,
                    dry_run,
                    human_approved,
                    one_order,
                    order_id,
                    canary_approval_sha256,
                    approval_expires_at_unix,
                    condition_id,
                    token_id,
                    side,
                    price,
                    size,
                    order_type,
                    &order_cap_state,
                )
                .await?;
            }
            Commands::LiveAlphaPreflight {
                read_only,
                approval_artifact,
                order_cap_state,
            } => {
                if !read_only {
                    return Err("live-alpha-preflight requires --read-only".into());
                }
                run_live_alpha_preflight_command(
                    &config,
                    &run_id,
                    &approval_artifact,
                    &order_cap_state,
                    LiveAlphaPreflightMode::ReadOnly,
                    false,
                    None,
                )
                .await?;
            }
            Commands::LiveAlphaFillCanary {
                dry_run,
                human_approved,
                approval_id,
                approval_artifact,
                order_cap_state,
            } => {
                run_live_alpha_fill_canary_command(
                    &config,
                    &run_id,
                    dry_run,
                    human_approved,
                    approval_id,
                    &approval_artifact,
                    &order_cap_state,
                )
                .await?;
            }
            Commands::LiveAlphaMakerMicro {
                dry_run,
                human_approved,
                approval_id,
                approval_artifact,
                max_orders,
                max_duration_sec,
            } => {
                run_live_alpha_maker_micro_command(
                    &config,
                    &run_id,
                    LiveAlphaMakerMicroCommandArgs {
                        dry_run,
                        human_approved,
                        approval_id,
                        approval_artifact,
                        max_orders,
                        max_duration_sec,
                    },
                )
                .await?;
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
    shadow_live_alpha: bool,
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
    let geoblock_gate_status = safety::GeoblockGateStatus::from_blocked(geoblock.blocked);
    let shadow_readback_validation = if shadow_live_alpha
        && config.live_alpha.enabled
        && config.live_alpha.mode == LiveAlphaMode::Shadow
    {
        shadow_live_readback_validation_for_paper(config, geoblock_gate_status).await?
    } else {
        None
    };
    let shadow_readiness = shadow_live_runtime_readiness_for_paper(
        config,
        run_id,
        unix_time_ms(),
        geoblock_gate_status,
        shadow_readback_validation.as_ref(),
    );
    let live_readiness_evidence = shadow_readback_validation
        .as_ref()
        .is_some_and(|validation| validation.report.live_network_enabled);
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
    println!("paper_shadow_live_alpha_enabled={shadow_live_alpha}");
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
    println!("live_readiness_evidence={live_readiness_evidence}");
    if shadow_live_alpha {
        println!(
            "shadow_live_runtime_geoblock_passed={}",
            shadow_readiness.geoblock_passed
        );
        println!(
            "shadow_live_runtime_heartbeat_healthy={}",
            shadow_readiness.heartbeat_healthy
        );
        println!(
            "shadow_live_runtime_reconciliation_clean={}",
            shadow_readiness.reconciliation_clean
        );
    }
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

        let cycle_replay = ReplayEngine::replay_from_storage_snapshot_with_shadow(
            &storage,
            run_id,
            shadow_live_alpha,
            shadow_readiness,
        )?;
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

    let final_result = ReplayEngine::replay_from_storage_snapshot_with_shadow(
        &storage,
        run_id,
        shadow_live_alpha,
        shadow_readiness,
    )?;
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
    let shadow_report_path = if shadow_live_alpha {
        Some(persist_shadow_live_outputs(
            &storage,
            run_id,
            config,
            &final_result.shadow_live_decisions,
            final_result.report.paper.order_count,
            final_result.report.paper.fill_count,
        )?)
    } else {
        None
    };
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
    if let Some(shadow_report_path) = shadow_report_path {
        let shadow_report = ShadowLiveReport::from_decisions(
            &final_result.shadow_live_decisions,
            final_result.report.paper.order_count,
            final_result.report.paper.fill_count,
        );
        println!("shadow_live_alpha_status=ok");
        println!("shadow_live_report_path={}", shadow_report_path.display());
        println!(
            "shadow_live_decision_count={}",
            shadow_report.decision_count
        );
        println!(
            "shadow_would_submit_count={}",
            shadow_report.shadow_would_submit_count
        );
        println!(
            "shadow_would_cancel_count={}",
            shadow_report.shadow_would_cancel_count
        );
        println!(
            "shadow_would_replace_count={}",
            shadow_report.shadow_would_replace_count
        );
        println!(
            "shadow_rejected_count={}",
            shadow_report.shadow_rejected_count
        );
        println!(
            "shadow_rejected_count_by_reason={}",
            format_counts(&shadow_report.shadow_rejected_count_by_reason)
        );
        println!(
            "paper_live_intent_divergence_count={}",
            shadow_report.paper_live_intent_divergence_count
        );
        println!(
            "shadow_estimated_fee_exposure={:.6}",
            shadow_report.estimated_fee_exposure
        );
        println!(
            "shadow_estimated_reserved_pusd_exposure={:.6}",
            shadow_report.estimated_reserved_pusd_exposure
        );
    }

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

fn persist_shadow_live_outputs(
    storage: &FileSessionStorage,
    run_id: &str,
    config: &AppConfig,
    decisions: &[ShadowLiveDecision],
    paper_order_count: u64,
    paper_fill_count: u64,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut decision_lines = Vec::new();
    for decision in decisions {
        serde_json::to_writer(&mut decision_lines, decision)?;
        decision_lines.push(b'\n');
    }
    storage.write_session_artifact(run_id, "shadow_live_decisions.jsonl", &decision_lines)?;

    let mut journal_lines = Vec::new();
    let journal_events = decisions
        .iter()
        .enumerate()
        .map(|(index, decision)| {
            decision.to_journal_event(
                run_id,
                format!("shadow-live-decision-{index}"),
                unix_time_ms(),
            )
        })
        .collect::<Vec<_>>();
    for event in &journal_events {
        serde_json::to_writer(&mut journal_lines, event)?;
        journal_lines.push(b'\n');
    }
    storage.write_session_artifact(run_id, "shadow_live_journal.jsonl", &journal_lines)?;

    if let Some(journal_path) = config.live_alpha.journal_path() {
        let journal = LiveOrderJournal::new(journal_path);
        for event in &journal_events {
            journal.append(event)?;
        }
    }

    let report = ShadowLiveReport::from_decisions(decisions, paper_order_count, paper_fill_count);
    let report_path = storage.write_session_artifact(
        run_id,
        "shadow_live_report.json",
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report_path)
}

fn format_counts(counts: &BTreeMap<String, u64>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(reason, count)| format!("{reason}={count}"))
        .collect::<Vec<_>>()
        .join(",")
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

fn run_lb5_cancel_readiness_validation(
    _config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = live_beta_cancel::evaluate_cancel_readiness(
        &live_beta_cancel::CancelReadinessInput::lb5_default(safety::LIVE_ORDER_PLACEMENT_ENABLED),
    );
    println!("live_beta_cancel_readiness_status={}", report.status);
    println!(
        "live_beta_cancel_readiness_live_network_enabled={}",
        report.live_cancel_network_enabled
    );
    println!(
        "live_beta_cancel_readiness_cancel_all_enabled={}",
        report.cancel_all_enabled
    );
    println!(
        "live_beta_cancel_readiness_request_constructable={}",
        report.cancel_request_constructable
    );
    println!(
        "live_beta_cancel_readiness_single_cancel_method={}",
        report.single_cancel_method
    );
    println!(
        "live_beta_cancel_readiness_single_cancel_path={}",
        report.single_cancel_path
    );
    println!(
        "live_beta_cancel_readiness_single_order_readback_path_prefix={}",
        report.single_order_readback_path_prefix
    );
    println!(
        "live_beta_cancel_readiness_block_reasons={}",
        report.block_reasons.join(",")
    );
    println!(
        "live_beta_cancel_readiness_report={}",
        serde_json::to_string(&report)?
    );
    Ok(())
}

struct LiveAlphaPreflightCommandResult {
    approval: LiveAlphaApprovalArtifact,
    report: LiveAlphaPreflightReport,
    envelope: live_fill_canary::LiveAlphaFillCanaryEnvelope,
    approval_prompt: String,
    approval_sha256: String,
}

async fn run_live_alpha_preflight_command(
    config: &AppConfig,
    run_id: &str,
    approval_artifact: &Path,
    order_cap_state: &Path,
    mode: LiveAlphaPreflightMode,
    human_approved: bool,
    approval_id: Option<&str>,
) -> Result<LiveAlphaPreflightCommandResult, Box<dyn std::error::Error>> {
    let result = build_live_alpha_preflight_command_result(
        config,
        run_id,
        approval_artifact,
        order_cap_state,
        mode,
        human_approved,
        approval_id,
    )
    .await?;
    print_live_alpha_preflight_result(&result)?;
    Ok(result)
}

async fn run_live_alpha_fill_canary_command(
    config: &AppConfig,
    run_id: &str,
    dry_run: bool,
    human_approved: bool,
    approval_id: Option<String>,
    approval_artifact: &Path,
    order_cap_state: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run == human_approved {
        return Err(
            "live-alpha-fill-canary requires exactly one of --dry-run or --human-approved".into(),
        );
    }
    if human_approved && approval_id.as_deref().unwrap_or_default().trim().is_empty() {
        return Err("live-alpha-fill-canary --human-approved requires --approval-id".into());
    }
    let mode = if human_approved {
        LiveAlphaPreflightMode::FinalSubmit
    } else {
        LiveAlphaPreflightMode::DryRun
    };
    let result = run_live_alpha_preflight_command(
        config,
        run_id,
        approval_artifact,
        order_cap_state,
        mode,
        human_approved,
        approval_id.as_deref(),
    )
    .await?;

    println!("live_alpha_fill_canary_mode={}", result.report.mode);
    println!("live_alpha_fill_canary_status={}", result.report.status);
    println!(
        "live_alpha_fill_canary_block_reasons={}",
        result.report.block_reasons.join(",")
    );
    println!(
        "live_alpha_fill_canary_approval_sha256={}",
        result.approval_sha256
    );
    if let Some(not_submitted) =
        live_alpha_fill_canary_pre_submit_not_submitted(dry_run, result.report.passed())
    {
        println!("live_alpha_fill_canary_not_submitted={not_submitted}");
    }

    if dry_run {
        return Ok(());
    }
    if !result.report.passed() {
        return Err(format!(
            "LA3 fill canary stopped before submit: {}",
            result.report.block_reasons.join(",")
        )
        .into());
    }

    let journal_path = config
        .live_alpha
        .journal_path()
        .ok_or("live_alpha.journal_path is required before LA3 final submit")?;
    let journal = LiveOrderJournal::new(Path::new(journal_path));
    append_la3_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::LiveFillCanaryStarted,
        serde_json::json!({
            "approval_id": &result.approval.approval_id,
            "approval_sha256": &result.approval_sha256,
            "market_slug": &result.approval.market_slug,
            "token_id": &result.approval.token_id,
            "order_type": &result.approval.order_type,
        }),
    )?;
    append_la3_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::LiveFillCanaryApproved,
        serde_json::json!({
            "approval_id": &result.approval.approval_id,
            "approval_sha256": &result.approval_sha256,
            "human_approved": true,
        }),
    )?;

    let submit_input = LiveAlphaFillSubmitInput {
        clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
        signer_handle: config.live_beta.secret_handles.canary_private_key.clone(),
        l2_access_handle: config.live_beta.secret_handles.clob_l2_access.clone(),
        l2_secret_handle: config.live_beta.secret_handles.clob_l2_credential.clone(),
        l2_passphrase_handle: config.live_beta.secret_handles.clob_l2_passphrase.clone(),
        wallet_address: result.report.wallet_id.clone(),
        funder_address: result.report.funder_id.clone(),
        signature_type: lb4_account_preflight(config)?.signature_type,
        approval: result.approval.clone(),
    };
    validate_and_reserve_la3_fill_cap(
        order_cap_state,
        &result.approval.approval_id,
        &submit_input,
    )?;
    append_la3_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::LiveFillAttempted,
        serde_json::json!({
            "approval_id": &result.approval.approval_id,
            "market_slug": &result.approval.market_slug,
            "token_id": &result.approval.token_id,
            "side": &result.approval.side,
            "order_type": &result.approval.order_type,
            "price": result.approval.worst_price,
            "amount_or_size": result.approval.amount_or_size,
        }),
    )?;

    let submission = match live_fill_canary::submit_one_fill_canary_with_official_sdk(submit_input)
        .await
    {
        Ok(submission) => submission,
        Err(error) => {
            append_la3_journal_event(
                &journal,
                run_id,
                LiveJournalEventType::LiveFillFailed,
                serde_json::json!({
                    "approval_id": &result.approval.approval_id,
                    "error": error.to_string(),
                    "incident_note": "submit failed after LA3 attempt cap reservation; no retry attempted",
                }),
            )?;
            return Err(error.into());
        }
    };
    println!(
        "live_alpha_fill_canary_not_submitted={}",
        submission.not_submitted
    );
    update_la3_fill_cap_with_order_id(
        order_cap_state,
        &result.approval.approval_id,
        &submission.order_id,
    )?;
    append_la3_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::LiveFillSucceeded,
        serde_json::json!({
            "approval_id": &result.approval.approval_id,
            "order_id": &submission.order_id,
            "trade_id": submission.trade_ids.first(),
            "venue_status": &submission.venue_status,
            "success": submission.success,
        }),
    )?;
    println!(
        "live_alpha_fill_canary_submission_report={}",
        serde_json::to_string(&submission)?
    );

    let after_readback = live_alpha_authenticated_readback(config).await?;
    let mut after_report = result.report.clone();
    after_report.available_pusd_units = after_readback.report.available_pusd_units;
    after_report.reserved_pusd_units = after_readback.report.reserved_pusd_units;
    after_report.open_order_count = after_readback.report.open_order_count;
    after_report.recent_trade_count = after_readback.report.trade_count;
    let reconciliation = live_fill_canary::reconcile_fill_submission(
        &submission,
        &result.approval,
        &after_report,
        &after_readback.open_orders,
        &after_readback.trades,
    );
    append_la3_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::LiveFillReconciled,
        serde_json::json!({
            "approval_id": &result.approval.approval_id,
            "order_id": &reconciliation.order_id,
            "trade_id": reconciliation.matching_trade_ids.first(),
            "status": reconciliation.status,
            "block_reasons": &reconciliation.block_reasons,
        }),
    )?;
    println!(
        "live_alpha_fill_canary_reconciliation_report={}",
        serde_json::to_string(&reconciliation)?
    );
    if reconciliation.status == "ambiguous_incident_required" {
        return Err(format!(
            "LA3 fill canary requires incident review: {}",
            reconciliation.block_reasons.join(",")
        )
        .into());
    }

    Ok(())
}

struct LiveAlphaMakerMicroCommandArgs {
    dry_run: bool,
    human_approved: bool,
    approval_id: Option<String>,
    approval_artifact: Option<PathBuf>,
    max_orders: u64,
    max_duration_sec: u64,
}

async fn run_live_alpha_maker_micro_command(
    config: &AppConfig,
    run_id: &str,
    args: LiveAlphaMakerMicroCommandArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let LiveAlphaMakerMicroCommandArgs {
        dry_run,
        human_approved,
        approval_id,
        approval_artifact,
        max_orders,
        max_duration_sec,
    } = args;
    if dry_run == human_approved {
        return Err(
            "live-alpha-maker-micro requires exactly one of --dry-run or --human-approved".into(),
        );
    }
    if max_orders == 0 || max_orders > 3 {
        return Err("LA5 max-orders must be between 1 and 3".into());
    }
    if max_duration_sec == 0 || max_duration_sec > 300 {
        return Err("LA5 max-duration-sec must be between 1 and 300".into());
    }
    if !config.live_alpha.enabled
        || config.live_alpha.mode != LiveAlphaMode::MakerMicro
        || !config.live_alpha.maker.enabled
    {
        return Err(
            "LA5 maker micro requires live_alpha.enabled=true, mode=maker_micro, and maker.enabled=true"
                .into(),
        );
    }

    if human_approved {
        validate_la5_live_submit_runtime_gates(config)?;
        let approval_id = approval_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("live-alpha-maker-micro --human-approved requires --approval-id")?;
        let approval_artifact = approval_artifact
            .as_deref()
            .ok_or("live-alpha-maker-micro --human-approved requires --approval-artifact")?;
        let approval_text = fs::read_to_string(approval_artifact)?;
        let approval = validate_la5_approval_artifact_text(&approval_text, approval_id)?;
        validate_la5_approval_against_cli_and_config(
            &approval,
            config,
            approval_id,
            max_orders,
            max_duration_sec,
        )?;
        let approval_artifact_sha256 = live_fill_canary::approval_hash(&approval_text);
        let approval_cap_path = la5_approval_cap_path(config, approval_id)?;
        reserve_la5_approval_cap(
            &approval_cap_path,
            &La5ApprovalCapReservation {
                approval_id: approval_id.to_string(),
                approval_artifact_sha256,
                approval_artifact_path: approval_artifact.display().to_string(),
                max_orders,
                max_duration_sec,
                reserved_at_unix: unix_time_secs(),
            },
        )?;

        println!("live_alpha_maker_micro_approval_id={approval_id}");
        println!(
            "live_alpha_maker_micro_approval_artifact={}",
            approval_artifact.display()
        );
        println!(
            "live_alpha_maker_micro_approval_cap_path={}",
            approval_cap_path.display()
        );
        return run_live_alpha_maker_micro_live_session(
            config,
            run_id,
            approval_id,
            &approval,
            max_orders,
            max_duration_sec,
        )
        .await;
    }

    let now_ms = unix_time_ms();
    let now_unix = unix_time_secs();
    let intent = sample_la5_maker_intent(now_ms);
    let risk_context = live_risk_context_for_la5_dry_run(config, now_ms);
    let risk_decision =
        LiveRiskEngine::new(config.live_alpha.risk.clone()).evaluate(&intent, &risk_context);
    let approval = match risk_decision {
        LiveRiskDecision::Approved(approval) => approval,
        LiveRiskDecision::Rejected(rejected) => {
            println!("live_alpha_maker_micro_status=blocked");
            println!(
                "live_alpha_maker_micro_block_reasons={}",
                rejected.reason_codes.join(",")
            );
            return Err(format!(
                "LA5 maker micro dry-run risk rejected: {}",
                rejected.reason_codes.join(",")
            )
            .into());
        }
        LiveRiskDecision::Halt(halt) => {
            println!("live_alpha_maker_micro_status=halted");
            println!("live_alpha_maker_micro_halt_reason={}", halt.reason);
            return Err(format!("LA5 maker micro dry-run halted: {}", halt.reason).into());
        }
    };

    let mut maker_execution = LiveMakerExecution::new(LiveMakerExecutionContext {
        risk_approval: approval.clone(),
        maker_config: config.live_alpha.maker.clone(),
        now_unix,
        human_approved,
    });
    let maker_decision = maker_execution.handle_intent(intent.clone());
    let ExecutionDecision::LiveMaker(maker_decision) = maker_decision else {
        return Err("LA5 maker execution returned a non-maker decision".into());
    };
    let plan = maker_decision
        .order_plan
        .clone()
        .ok_or("LA5 maker execution did not build an order plan")?;

    println!("live_alpha_maker_micro_status=ok");
    println!("run_id={run_id}");
    if dry_run {
        println!("live_alpha_maker_micro_not_submitted=true");
    }
    println!("live_alpha_maker_micro_max_orders={max_orders}");
    println!("live_alpha_maker_micro_max_duration_sec={max_duration_sec}");
    println!(
        "live_alpha_maker_micro_effective_quote_ttl_seconds={}",
        plan.effective_quote_ttl_seconds
    );
    println!(
        "live_alpha_maker_micro_gtd_expiration_unix={}",
        plan.gtd_expiration_unix
    );
    println!(
        "live_alpha_maker_micro_cancel_after_unix={}",
        plan.cancel_after_unix
    );
    println!(
        "live_alpha_maker_micro_order_plan={}",
        serde_json::to_string(&plan)?
    );

    if dry_run {
        return Ok(());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct La5ApprovalCapReservation {
    approval_id: String,
    approval_artifact_sha256: String,
    approval_artifact_path: String,
    max_orders: u64,
    max_duration_sec: u64,
    reserved_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct La5ApprovalArtifactFields {
    approved_wallet: String,
    approved_funder: String,
    max_single_order_notional: f64,
    max_total_live_notional: f64,
    max_available_pusd_usage: f64,
    max_reserved_pusd: f64,
    max_fee_spend: f64,
    max_orders: u64,
    max_open_orders: u64,
    max_duration_sec: u64,
    no_trade_seconds_before_close: u64,
    ttl_seconds: u64,
    venue_gtd_expiration_delta: u64,
    signature_type: SignatureType,
    available_pusd_units: u64,
    reserved_pusd_units: u64,
    open_order_count: usize,
    heartbeat_status: String,
    funder_allowance_units: u64,
    rollback_owner: String,
    monitoring_owner: String,
    approval_id: String,
    approval_date: String,
}

fn validate_la5_approval_artifact_text(
    text: &str,
    approval_id: &str,
) -> Result<La5ApprovalArtifactFields, Box<dyn std::error::Error>> {
    let mut errors = Vec::<String>::new();
    if !text.contains(approval_id) {
        errors.push("approval_id_missing".to_string());
    }
    if !text.contains("Status: LA5 APPROVED FOR THIS RUN ONLY") {
        errors.push("approval_status_missing".to_string());
    }
    if text.contains("[human name after reviewing PR]")
        || text.contains("[execution date]")
        || text.contains("PENDING")
        || text.contains("TBD")
    {
        errors.push("human_approval_or_live_readback_pending".to_string());
    }
    if la5_approval_artifact_indicates_consumed(text) {
        errors.push("approval_artifact_consumed".to_string());
    }
    for field in LA5_APPROVAL_REQUIRED_FIELDS {
        match la5_approval_table_value(text, field) {
            Some(value) if la5_approval_value_is_final(&value) => {}
            Some(_) => errors.push(format!("approval_field_pending:{field}")),
            None => errors.push(format!("approval_field_missing:{field}")),
        }
    }
    let parsed = if errors.is_empty() {
        parse_la5_approval_artifact_fields(text, &mut errors)
    } else {
        None
    };
    if let Some(approval) = parsed {
        Ok(approval)
    } else {
        errors.sort_unstable();
        errors.dedup();
        Err(format!("LA5 approval artifact is not final: {}", errors.join(",")).into())
    }
}

fn la5_approval_artifact_indicates_consumed(text: &str) -> bool {
    const CONSUMED_MARKERS: &[&str] = &[
        "EXECUTION GATE STATUS: LA5 RUN COMPLETED",
        "EXECUTION GATE STATUS: LA5 RUN CONSUMED",
        "EXECUTION GATE STATUS: LA5 APPROVAL CONSUMED",
        "EXECUTION RESULT: COMPLETED",
        "APPROVAL CONSUMED",
        "LA5 RUN COMPLETED",
        "AUTHORIZED SESSION COMPLETED",
    ];
    let upper = text.to_ascii_uppercase();
    CONSUMED_MARKERS.iter().any(|marker| upper.contains(marker))
}

const LA5_APPROVAL_REQUIRED_FIELDS: &[&str] = &[
    "approved_wallet",
    "approved_funder",
    "max_single_order_notional",
    "max_total_live_notional",
    "max_available_pusd_usage",
    "max_reserved_pusd",
    "max_fee_spend",
    "max_orders",
    "max_open_orders",
    "max_duration_sec",
    "no_trade_seconds_before_close",
    "ttl_seconds",
    "venue_gtd_expiration_delta",
    "signature_type",
    "available_pusd_units",
    "reserved_pusd_units",
    "open_order_count",
    "heartbeat_status",
    "funder_allowance_units",
    "rollback_owner",
    "monitoring_owner",
    "approval_id",
    "approval_date",
];

fn la5_approval_table_value(text: &str, field: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let cells = line.split('|').map(str::trim).collect::<Vec<_>>();
        if cells.len() >= 3 && cells[1] == field {
            Some(cells[2].trim_matches('`').trim().to_string())
        } else {
            None
        }
    })
}

fn parse_la5_approval_artifact_fields(
    text: &str,
    errors: &mut Vec<String>,
) -> Option<La5ApprovalArtifactFields> {
    Some(La5ApprovalArtifactFields {
        approved_wallet: la5_required_approval_string(text, "approved_wallet", errors)?,
        approved_funder: la5_required_approval_string(text, "approved_funder", errors)?,
        max_single_order_notional: la5_required_approval_f64(
            text,
            "max_single_order_notional",
            errors,
        )?,
        max_total_live_notional: la5_required_approval_f64(
            text,
            "max_total_live_notional",
            errors,
        )?,
        max_available_pusd_usage: la5_required_approval_f64(
            text,
            "max_available_pusd_usage",
            errors,
        )?,
        max_reserved_pusd: la5_required_approval_f64(text, "max_reserved_pusd", errors)?,
        max_fee_spend: la5_required_approval_f64(text, "max_fee_spend", errors)?,
        max_orders: la5_required_approval_u64(text, "max_orders", errors)?,
        max_open_orders: la5_required_approval_u64(text, "max_open_orders", errors)?,
        max_duration_sec: la5_required_approval_u64(text, "max_duration_sec", errors)?,
        no_trade_seconds_before_close: la5_required_approval_u64(
            text,
            "no_trade_seconds_before_close",
            errors,
        )?,
        ttl_seconds: la5_required_approval_u64(text, "ttl_seconds", errors)?,
        venue_gtd_expiration_delta: la5_required_approval_u64(
            text,
            "venue_gtd_expiration_delta",
            errors,
        )?,
        signature_type: la5_required_approval_signature_type(text, "signature_type", errors)?,
        available_pusd_units: la5_required_approval_u64(text, "available_pusd_units", errors)?,
        reserved_pusd_units: la5_required_approval_u64(text, "reserved_pusd_units", errors)?,
        open_order_count: usize::try_from(la5_required_approval_u64(
            text,
            "open_order_count",
            errors,
        )?)
        .ok()?,
        heartbeat_status: la5_required_approval_string(text, "heartbeat_status", errors)?,
        funder_allowance_units: la5_required_approval_u64(text, "funder_allowance_units", errors)?,
        rollback_owner: la5_required_approval_string(text, "rollback_owner", errors)?,
        monitoring_owner: la5_required_approval_string(text, "monitoring_owner", errors)?,
        approval_id: la5_required_approval_string(text, "approval_id", errors)?,
        approval_date: la5_required_approval_string(text, "approval_date", errors)?,
    })
}

fn la5_required_approval_string(
    text: &str,
    field: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    la5_approval_table_value(text, field).or_else(|| {
        errors.push(format!("approval_field_missing:{field}"));
        None
    })
}

fn la5_required_approval_u64(text: &str, field: &str, errors: &mut Vec<String>) -> Option<u64> {
    let value = la5_required_approval_string(text, field, errors)?;
    let Some(token) = la5_first_number_token(&value) else {
        errors.push(format!("approval_field_parse_error:{field}"));
        return None;
    };
    token.parse::<u64>().map_err(|_| ()).map_or_else(
        |_| {
            errors.push(format!("approval_field_parse_error:{field}"));
            None
        },
        Some,
    )
}

fn la5_required_approval_f64(text: &str, field: &str, errors: &mut Vec<String>) -> Option<f64> {
    let value = la5_required_approval_string(text, field, errors)?;
    let Some(token) = la5_first_number_token(&value) else {
        errors.push(format!("approval_field_parse_error:{field}"));
        return None;
    };
    token.parse::<f64>().map_err(|_| ()).map_or_else(
        |_| {
            errors.push(format!("approval_field_parse_error:{field}"));
            None
        },
        Some,
    )
}

fn la5_required_approval_signature_type(
    text: &str,
    field: &str,
    errors: &mut Vec<String>,
) -> Option<SignatureType> {
    let value = la5_required_approval_string(text, field, errors)?;
    SignatureType::from_config(&value).or_else(|| {
        errors.push(format!("approval_field_parse_error:{field}"));
        None
    })
}

fn la5_first_number_token(value: &str) -> Option<&str> {
    let start = value.find(|ch: char| ch.is_ascii_digit())?;
    let tail = &value[start..];
    let end = tail
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .unwrap_or(tail.len());
    Some(&tail[..end])
}

fn la5_approval_value_is_final(value: &str) -> bool {
    let trimmed = value.trim();
    let upper = trimmed.to_ascii_uppercase();
    !trimmed.is_empty()
        && !upper.contains("PENDING")
        && !upper.contains("TBD")
        && !upper.contains("TODO")
        && !upper.contains("BLOCKED")
        && !upper.contains("UNAVAILABLE")
        && !upper.contains("NOT RUN")
        && !upper.contains("UNKNOWN")
        && !upper.contains("MISSING")
        && !trimmed.starts_with('[')
        && !trimmed.ends_with(']')
}

fn validate_la5_approval_against_cli_and_config(
    approval: &La5ApprovalArtifactFields,
    config: &AppConfig,
    approval_id: &str,
    max_orders: u64,
    max_duration_sec: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mismatches = Vec::<String>::new();
    if approval.approval_id != approval_id {
        mismatches.push("approval_id_mismatch".to_string());
    }
    if approval.max_orders != max_orders {
        mismatches.push("approval_max_orders_mismatch".to_string());
    }
    if approval.max_duration_sec != max_duration_sec {
        mismatches.push("approval_max_duration_sec_mismatch".to_string());
    }
    if !la5_addresses_equal(
        &approval.approved_wallet,
        &config.live_beta.readback_account.wallet_address,
    ) {
        mismatches.push("approval_wallet_mismatch".to_string());
    }
    if !la5_addresses_equal(
        &approval.approved_funder,
        &config.live_beta.readback_account.funder_address,
    ) {
        mismatches.push("approval_funder_mismatch".to_string());
    }
    let config_signature_type =
        SignatureType::from_config(&config.live_beta.readback_account.signature_type);
    if config_signature_type != Some(approval.signature_type) {
        mismatches.push("approval_signature_type_mismatch".to_string());
    }
    la5_compare_f64(
        &mut mismatches,
        "approval_max_single_order_notional_mismatch",
        approval.max_single_order_notional,
        config.live_alpha.risk.max_single_order_notional,
    );
    la5_compare_f64(
        &mut mismatches,
        "approval_max_total_live_notional_mismatch",
        approval.max_total_live_notional,
        config.live_alpha.risk.max_total_live_notional,
    );
    la5_compare_f64(
        &mut mismatches,
        "approval_max_available_pusd_usage_mismatch",
        approval.max_available_pusd_usage,
        config.live_alpha.risk.max_available_pusd_usage,
    );
    la5_compare_f64(
        &mut mismatches,
        "approval_max_reserved_pusd_mismatch",
        approval.max_reserved_pusd,
        config.live_alpha.risk.max_reserved_pusd,
    );
    la5_compare_f64(
        &mut mismatches,
        "approval_max_fee_spend_mismatch",
        approval.max_fee_spend,
        config.live_alpha.risk.max_fee_spend,
    );
    if approval.max_open_orders != config.live_alpha.risk.max_open_orders {
        mismatches.push("approval_max_open_orders_mismatch".to_string());
    }
    if approval.no_trade_seconds_before_close
        != config.live_alpha.risk.no_trade_seconds_before_close
    {
        mismatches.push("approval_no_trade_seconds_before_close_mismatch".to_string());
    }
    if approval.ttl_seconds != config.live_alpha.maker.ttl_seconds {
        mismatches.push("approval_ttl_seconds_mismatch".to_string());
    }
    let configured_gtd_delta = config
        .live_alpha
        .maker
        .ttl_seconds
        .saturating_add(GTD_SECURITY_BUFFER_SECONDS);
    if approval.venue_gtd_expiration_delta != configured_gtd_delta {
        mismatches.push("approval_venue_gtd_expiration_delta_mismatch".to_string());
    }
    if !config.live_alpha.maker.post_only {
        mismatches.push("config_post_only_not_enabled".to_string());
    }
    if !config
        .live_alpha
        .maker
        .order_type
        .eq_ignore_ascii_case("GTD")
    {
        mismatches.push("config_order_type_not_gtd".to_string());
    }

    la5_fail_on_approval_mismatches(
        "LA5 approval artifact does not match CLI/config",
        mismatches,
    )
}

fn validate_la5_approval_against_account_readback(
    approval: &La5ApprovalArtifactFields,
    account: &AccountPreflight,
    readback: &ReadbackPreflightValidation,
    funder_allowance_units: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mismatches = Vec::<String>::new();
    if !la5_addresses_equal(&approval.approved_wallet, &account.wallet_address) {
        mismatches.push("approval_readback_wallet_mismatch".to_string());
    }
    if !la5_addresses_equal(&approval.approved_funder, &account.funder_address) {
        mismatches.push("approval_readback_funder_mismatch".to_string());
    }
    if approval.signature_type != account.signature_type {
        mismatches.push("approval_readback_signature_type_mismatch".to_string());
    }
    if approval.available_pusd_units != readback.report.available_pusd_units {
        mismatches.push("approval_available_pusd_units_mismatch".to_string());
    }
    if approval.reserved_pusd_units != readback.report.reserved_pusd_units {
        mismatches.push("approval_reserved_pusd_units_mismatch".to_string());
    }
    if approval.open_order_count != readback.report.open_order_count {
        mismatches.push("approval_open_order_count_mismatch".to_string());
    }
    if approval.heartbeat_status != readback.report.heartbeat {
        mismatches.push("approval_heartbeat_status_mismatch".to_string());
    }
    if approval.funder_allowance_units != funder_allowance_units {
        mismatches.push("approval_funder_allowance_units_mismatch".to_string());
    }

    la5_fail_on_approval_mismatches(
        "LA5 approval artifact does not match authenticated readback",
        mismatches,
    )
}

fn validate_la5_plan_against_approval(
    approval: &La5ApprovalArtifactFields,
    plan: &LiveMakerOrderPlan,
    cumulative_notional: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mismatches = Vec::<String>::new();
    if !plan.post_only {
        mismatches.push("approval_plan_post_only_mismatch".to_string());
    }
    if !plan.order_type.eq_ignore_ascii_case("GTD") {
        mismatches.push("approval_plan_order_type_mismatch".to_string());
    }
    if plan.effective_quote_ttl_seconds != approval.ttl_seconds {
        mismatches.push("approval_plan_ttl_seconds_mismatch".to_string());
    }
    let plan_start_unix = plan
        .cancel_after_unix
        .saturating_sub(plan.effective_quote_ttl_seconds);
    let plan_gtd_delta = plan.gtd_expiration_unix.saturating_sub(plan_start_unix);
    if plan_gtd_delta != approval.venue_gtd_expiration_delta {
        mismatches.push("approval_plan_gtd_delta_mismatch".to_string());
    }
    if plan.notional > approval.max_single_order_notional + LA5_FLOAT_EPSILON {
        mismatches.push("approval_plan_single_notional_exceeds_cap".to_string());
    }
    if cumulative_notional + plan.notional > approval.max_total_live_notional + LA5_FLOAT_EPSILON {
        mismatches.push("approval_plan_total_notional_exceeds_cap".to_string());
    }

    la5_fail_on_approval_mismatches(
        "LA5 approval artifact does not authorize submitted plan",
        mismatches,
    )
}

fn validate_la5_session_against_approval(
    approval: &La5ApprovalArtifactFields,
    outcomes: &[La5MakerOrderOutcome],
    cumulative_notional: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut mismatches = Vec::<String>::new();
    if outcomes.len() != approval.max_orders as usize {
        mismatches.push("approval_session_order_count_mismatch".to_string());
    }
    if cumulative_notional > approval.max_total_live_notional + LA5_FLOAT_EPSILON {
        mismatches.push("approval_session_total_notional_exceeds_cap".to_string());
    }
    if outcomes
        .iter()
        .any(|outcome| outcome.notional > approval.max_single_order_notional + LA5_FLOAT_EPSILON)
    {
        mismatches.push("approval_session_single_notional_exceeds_cap".to_string());
    }

    la5_fail_on_approval_mismatches(
        "LA5 approval artifact does not match completed session",
        mismatches,
    )
}

const LA5_FLOAT_EPSILON: f64 = 0.000_000_001;

fn la5_compare_f64(mismatches: &mut Vec<String>, label: &str, approved: f64, actual: f64) {
    if (approved - actual).abs() > LA5_FLOAT_EPSILON {
        mismatches.push(label.to_string());
    }
}

fn la5_addresses_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn la5_fail_on_approval_mismatches(
    prefix: &str,
    mut mismatches: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if mismatches.is_empty() {
        Ok(())
    } else {
        mismatches.sort_unstable();
        mismatches.dedup();
        Err(format!("{prefix}: {}", mismatches.join(",")).into())
    }
}

fn validate_la5_live_submit_runtime_gates(
    config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_la5_live_submit_runtime_gate_values(
        live_alpha_gate::LIVE_ALPHA_ORDER_FEATURE_ENABLED,
        safety::LIVE_ORDER_PLACEMENT_ENABLED,
        config.live_beta.kill_switch_active,
    )
}

fn validate_la5_live_submit_runtime_gate_values(
    compile_time_orders_enabled: bool,
    live_order_placement_enabled: bool,
    kill_switch_active: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut block_reasons = Vec::<&'static str>::new();
    if !compile_time_orders_enabled {
        block_reasons.push("compile_time_live_disabled");
    }
    if !live_order_placement_enabled {
        block_reasons.push("live_order_placement_disabled");
    }
    if kill_switch_active {
        block_reasons.push("kill_switch_active");
    }
    if block_reasons.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "LA5 live maker submit gate blocked: {}",
            block_reasons.join(",")
        )
        .into())
    }
}

fn la5_approval_cap_path(
    config: &AppConfig,
    approval_id: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cap_root = config
        .live_alpha
        .journal_path()
        .and_then(|journal_path| {
            Path::new(journal_path)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
        })
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("reports"));
    Ok(cap_root
        .join("live-alpha-la5-approval-caps")
        .join(format!("{}.json", la5_cap_filename_fragment(approval_id)?)))
}

fn la5_cap_filename_fragment(approval_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let sanitized = approval_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('_').is_empty() {
        Err("LA5 approval ID cannot produce a cap filename".into())
    } else {
        Ok(sanitized)
    }
}

fn reserve_la5_approval_cap(
    path: &Path,
    reservation: &La5ApprovalCapReservation,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_canary_order_cap_parent(path)?;
    let contents = serde_json::to_string_pretty(reservation)?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err("LA5 approval cap is already reserved or consumed".into());
        }
        Err(error) => return Err(error.into()),
    };
    file.write_all(contents.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn sample_la5_maker_intent(
    now_ms: i64,
) -> polymarket_15m_arb_bot::execution_intent::ExecutionIntent {
    polymarket_15m_arb_bot::execution_intent::ExecutionIntent {
        intent_id: "la5-dry-run-intent-1".to_string(),
        strategy_snapshot_id: "la5-dry-run-snapshot".to_string(),
        market_slug: "btc-updown-15m-la5-dry-run".to_string(),
        condition_id: "la5-dry-run-condition".to_string(),
        token_id: "token-up".to_string(),
        asset_symbol: "BTC".to_string(),
        asset: Asset::Btc,
        outcome: "Up".to_string(),
        side: Side::Buy,
        price: 0.20,
        size: 1.0,
        notional: 0.20,
        order_type: "GTD".to_string(),
        time_in_force: "GTD".to_string(),
        post_only: true,
        expiry: None,
        fair_probability: 0.23,
        edge_bps: 300.0,
        reference_price: 100_000.0,
        reference_source_timestamp: Some(now_ms),
        book_snapshot_id: "la5-dry-run-book".to_string(),
        best_bid: Some(0.19),
        best_ask: Some(0.21),
        spread: Some(0.02),
        created_at: now_ms,
    }
}

fn live_risk_context_for_la5_dry_run(config: &AppConfig, now_ms: i64) -> LiveRiskContext {
    LiveRiskContext {
        now_ms: Some(now_ms),
        market_end_ms: Some(now_ms.saturating_add(900_000)),
        effective_quote_ttl_seconds: config.live_alpha.maker.ttl_seconds,
        available_pusd: config.live_alpha.risk.max_available_pusd_usage,
        reserved_pusd: 0.0,
        up_token_id: Some("token-up".to_string()),
        down_token_id: Some("token-down".to_string()),
        open_order_count: 0,
        open_orders_per_market: 0,
        open_orders_per_asset: 0,
        book_age_ms: Some(0),
        reference_age_ms: Some(0),
        geoblock_passed: true,
        heartbeat_healthy: true,
        reconciliation_clean: true,
        ..LiveRiskContext::default()
    }
}

#[derive(Debug, Clone)]
struct La5MakerMarketIntent {
    intent: polymarket_15m_arb_bot::execution_intent::ExecutionIntent,
    market: Market,
    best_bid: f64,
    best_ask: f64,
    best_bid_size: Option<f64>,
    best_ask_size: Option<f64>,
    tick_size: f64,
    min_order_size: f64,
    book_snapshot_id: String,
    book_age_ms: u64,
    reference_snapshot_id: String,
    reference_age_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct La5MakerOrderOutcome {
    sequence: u64,
    intent_id: String,
    market_slug: String,
    token_id: String,
    outcome: String,
    side: Side,
    price: f64,
    size: f64,
    notional: f64,
    gtd_expiration_unix: u64,
    cancel_after_unix: u64,
    order_id: String,
    accepted_status: String,
    final_status: String,
    canceled: bool,
    cancel_request_sent: bool,
    exact_cancel_confirmed: bool,
    venue_final_canceled: bool,
    filled: bool,
    trade_ids: Vec<String>,
    pre_submit_available_pusd_units: u64,
    post_order_available_pusd_units: u64,
    final_available_pusd_units: u64,
    final_reserved_pusd_units: u64,
    reconciliation_status: String,
    reconciliation_mismatches: String,
}

async fn run_live_alpha_maker_micro_live_session(
    config: &AppConfig,
    run_id: &str,
    approval_id: &str,
    approval_artifact: &La5ApprovalArtifactFields,
    max_orders: u64,
    max_duration_sec: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let journal_path = config
        .live_alpha
        .journal_path()
        .ok_or("LA5 requires live_alpha.journal_path for journal/replay evidence")?;
    let journal = LiveOrderJournal::new(journal_path);
    journal.replay()?;

    append_la5_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::MakerMicroStarted,
        serde_json::json!({
            "approval_id": approval_id,
            "max_orders": max_orders,
            "max_duration_sec": max_duration_sec,
        }),
    )?;
    append_la5_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::MakerMicroApprovalAccepted,
        serde_json::json!({
            "approval_id": approval_id,
            "scope": "exactly_3_maker_only_post_only_gtd_micro_orders",
        }),
    )?;

    let geoblock = run_geoblock_validation(config).await?;
    if geoblock.blocked {
        append_la5_journal_event(
            &journal,
            run_id,
            LiveJournalEventType::MakerMicroHalted,
            serde_json::json!({
                "status": "halted",
                "reason": "geoblock_not_passed",
                "geoblock_result": geoblock_result_label(&geoblock),
            }),
        )?;
        return Err(format!(
            "LA5 live maker blocked: {}",
            geoblock_result_label(&geoblock)
        )
        .into());
    }

    let account = lb4_account_preflight(config)?;
    let canary_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.canary_secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    if !canary_secret_report.all_present() {
        append_la5_journal_event(
            &journal,
            run_id,
            LiveJournalEventType::MakerMicroHalted,
            serde_json::json!({"status": "halted", "reason": "canary_private_key_handle_missing"}),
        )?;
        return Err("LA5 live maker blocked: canary_private_key_handle_missing".into());
    }

    let initial_readback = live_alpha_authenticated_readback(config).await?;
    require_la5_pre_submit_readback("initial", &initial_readback)?;
    let baseline_trade_ids = initial_readback
        .trades
        .iter()
        .map(|trade| trade.id.clone())
        .collect::<BTreeSet<_>>();
    let initial_allowance_units = initial_readback
        .collateral
        .as_ref()
        .map(|collateral| collateral.allowance_units)
        .ok_or("LA5 live readback missing collateral allowance evidence")?;
    validate_la5_approval_against_account_readback(
        approval_artifact,
        &account,
        &initial_readback,
        initial_allowance_units,
    )?;

    println!("live_alpha_maker_micro_live_readback_status=passed");
    println!(
        "live_alpha_maker_micro_available_pusd_units={}",
        initial_readback.report.available_pusd_units
    );
    println!(
        "live_alpha_maker_micro_reserved_pusd_units={}",
        initial_readback.report.reserved_pusd_units
    );
    println!(
        "live_alpha_maker_micro_open_order_count={}",
        initial_readback.report.open_order_count
    );
    println!("live_alpha_maker_micro_funder_allowance_units={initial_allowance_units}");
    println!(
        "live_alpha_maker_micro_heartbeat={}",
        initial_readback.report.heartbeat
    );
    println!(
        "live_alpha_maker_micro_baseline_trade_count={}",
        initial_readback.trades.len()
    );

    let started = Instant::now();
    let mut outcomes = Vec::<La5MakerOrderOutcome>::new();
    let mut submit_timestamps = Vec::<Instant>::new();
    let mut cancel_timestamps = Vec::<Instant>::new();
    let mut cumulative_notional = 0.0_f64;

    for sequence in 1..=max_orders {
        if started.elapsed() >= Duration::from_secs(max_duration_sec) {
            return Err(
                "LA5 live maker blocked: max_duration_elapsed_before_exact_order_count".into(),
            );
        }
        wait_for_la5_rate_slot(
            &submit_timestamps,
            config.live_alpha.risk.max_submit_rate_per_min,
            started,
            max_duration_sec,
        )
        .await?;

        let pre_submit_readback = live_alpha_authenticated_readback(config).await?;
        require_la5_pre_submit_readback("pre_submit", &pre_submit_readback)?;
        let market_intent = select_la5_maker_market_intent(
            config,
            sequence,
            max_orders,
            cumulative_notional,
            pre_submit_readback.report.available_pusd_units,
        )
        .await?;
        let risk_context = la5_live_risk_context(
            config,
            &market_intent,
            &pre_submit_readback,
            cumulative_notional,
            recent_count_last_min(&submit_timestamps),
        )?;
        let risk_decision = LiveRiskEngine::new(config.live_alpha.risk.clone())
            .evaluate(&market_intent.intent, &risk_context);
        let approval = match risk_decision {
            LiveRiskDecision::Approved(approval) => {
                append_la5_journal_event(
                    &journal,
                    run_id,
                    LiveJournalEventType::MakerRiskApproved,
                    serde_json::json!({
                        "intent_id": approval.intent_id,
                        "approved_notional": approval.approved_notional,
                        "approved_ttl_seconds": approval.approved_ttl_seconds,
                        "approved_side": approval.approved_side,
                        "reason_codes": approval.reason_codes,
                    }),
                )?;
                approval
            }
            LiveRiskDecision::Rejected(rejected) => {
                append_la5_journal_event(
                    &journal,
                    run_id,
                    LiveJournalEventType::MakerRiskRejected,
                    serde_json::json!({
                        "intent_id": rejected.intent_id,
                        "reason_codes": rejected.reason_codes,
                    }),
                )?;
                return Err(format!(
                    "LA5 live maker risk rejected: {}",
                    rejected.reason_codes.join(",")
                )
                .into());
            }
            LiveRiskDecision::Halt(halt) => {
                append_la5_journal_event(
                    &journal,
                    run_id,
                    LiveJournalEventType::MakerRiskHalt,
                    serde_json::json!({
                        "intent_id": halt.intent_id,
                        "reason": halt.reason,
                    }),
                )?;
                return Err(format!("LA5 live maker risk halted: {}", halt.reason).into());
            }
        };

        let mut maker_execution = LiveMakerExecution::new(LiveMakerExecutionContext {
            risk_approval: approval,
            maker_config: config.live_alpha.maker.clone(),
            now_unix: unix_time_secs(),
            human_approved: true,
        });
        let ExecutionDecision::LiveMaker(decision) =
            maker_execution.handle_intent(market_intent.intent.clone())
        else {
            return Err("LA5 maker execution returned a non-maker decision".into());
        };
        let plan = decision
            .order_plan
            .clone()
            .ok_or("LA5 maker execution did not build an order plan")?;
        validate_la5_plan_against_approval(approval_artifact, &plan, cumulative_notional)?;
        let submit_input = la5_maker_submit_input(config, &account, plan.clone());

        append_la5_journal_event(
            &journal,
            run_id,
            LiveJournalEventType::MakerOrderSubmitAttempted,
            serde_json::json!({
                "intent_id": plan.intent_id,
                "sequence": sequence,
                "market_slug": market_intent.market.slug,
                "condition_id": market_intent.market.condition_id,
                "token_id": plan.token_id,
                "outcome": plan.outcome,
                "side": plan.side,
                "price": plan.price,
                "size": plan.size,
                "notional": plan.notional,
                "order_type": plan.order_type,
                "post_only": plan.post_only,
                "best_bid": market_intent.best_bid,
                "best_ask": market_intent.best_ask,
                "best_bid_size": market_intent.best_bid_size,
                "best_ask_size": market_intent.best_ask_size,
                "tick_size": market_intent.tick_size,
                "min_order_size": market_intent.min_order_size,
                "book_snapshot_id": market_intent.book_snapshot_id,
                "book_age_ms": market_intent.book_age_ms,
                "reference_snapshot_id": market_intent.reference_snapshot_id,
                "reference_age_ms": market_intent.reference_age_ms,
                "effective_quote_ttl_seconds": plan.effective_quote_ttl_seconds,
                "gtd_expiration_unix": plan.gtd_expiration_unix,
                "cancel_after_unix": plan.cancel_after_unix,
            }),
        )?;

        let submitted_at = Instant::now();
        let submission = match submit_maker_order_with_official_sdk(submit_input.clone()).await {
            Ok(submission) => submission,
            Err(error) => {
                append_la5_journal_event(
                    &journal,
                    run_id,
                    LiveJournalEventType::MakerOrderRejected,
                    serde_json::json!({
                        "intent_id": plan.intent_id,
                        "sequence": sequence,
                        "status": "submit_error",
                        "error": error.to_string(),
                    }),
                )?;
                return Err(format!("LA5 maker submit failed before acceptance: {error}").into());
            }
        };
        submit_timestamps.push(submitted_at);
        println!(
            "live_alpha_maker_micro_order_{sequence}_submission={}",
            serde_json::to_string(&submission)?
        );
        if !submission.success || submission.order_id.trim().is_empty() {
            append_la5_journal_event(
                &journal,
                run_id,
                LiveJournalEventType::MakerOrderRejected,
                serde_json::json!({
                    "intent_id": plan.intent_id,
                    "sequence": sequence,
                    "status": submission.venue_status,
                    "order_id": submission.order_id,
                }),
            )?;
            return Err(format!(
                "LA5 maker submit rejected by venue: status={}",
                submission.venue_status
            )
            .into());
        }
        if let Err(error) = append_la5_journal_event(
            &journal,
            run_id,
            LiveJournalEventType::MakerOrderAccepted,
            serde_json::json!({
                "intent_id": plan.intent_id,
                "order_id": submission.order_id,
                "sequence": sequence,
                "status": submission.venue_status,
                "trade_ids": submission.trade_ids,
            }),
        ) {
            return Err(la5_cleanup_accepted_order_before_error(
                &journal,
                run_id,
                &submission.order_id,
                &plan.intent_id,
                sequence,
                "accepted_journal_append_failed",
                error.to_string(),
                || async {
                    cancel_exact_maker_order_with_official_sdk(&submit_input, &submission.order_id)
                        .await
                },
            )
            .await);
        }

        macro_rules! la5_try_after_accept {
            ($expr:expr, $reason:expr) => {
                match $expr {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(la5_cleanup_accepted_order_before_error(
                            &journal,
                            run_id,
                            &submission.order_id,
                            &plan.intent_id,
                            sequence,
                            $reason,
                            error.to_string(),
                            || async {
                                cancel_exact_maker_order_with_official_sdk(
                                    &submit_input,
                                    &submission.order_id,
                                )
                                .await
                            },
                        )
                        .await);
                    }
                }
            };
        }

        let heartbeat_id = la5_try_after_accept!(
            maintain_la5_heartbeat_until_cancel(&submit_input, plan.cancel_after_unix).await,
            "heartbeat_maintenance_failed"
        );
        println!("live_alpha_maker_micro_order_{sequence}_heartbeat_id={heartbeat_id}");

        let post_order_readback = la5_try_after_accept!(
            live_alpha_authenticated_readback(config).await,
            "post_order_account_readback_failed"
        );
        let order_readback = la5_try_after_accept!(
            read_maker_order_with_official_sdk(&submit_input, &submission.order_id).await,
            "post_order_exact_readback_failed"
        );
        let order_trade_ids = la5_order_trade_ids(
            &baseline_trade_ids,
            &post_order_readback.trades,
            &submission,
            &order_readback,
        );
        let open_reconciliation = la5_try_after_accept!(
            reconcile_la5_order_state(
                run_id,
                &submission.order_id,
                &order_readback,
                &post_order_readback,
                &order_trade_ids,
                false,
            ),
            "post_order_reconciliation_error"
        );
        la5_try_after_accept!(
            append_la5_reconciliation_event(
                &journal,
                run_id,
                &submission.order_id,
                &open_reconciliation,
            ),
            "post_order_reconciliation_journal_failed"
        );
        if open_reconciliation.status() != "passed" {
            return Err(la5_cleanup_accepted_order_before_error(
                &journal,
                run_id,
                &submission.order_id,
                &plan.intent_id,
                sequence,
                "post_order_reconciliation_failed",
                format!(
                    "LA5 post-submit reconciliation failed: {}",
                    open_reconciliation.mismatch_list()
                ),
                || async {
                    cancel_exact_maker_order_with_official_sdk(&submit_input, &submission.order_id)
                        .await
                },
            )
            .await);
        }

        la5_try_after_accept!(
            wait_for_la5_rate_slot(
                &cancel_timestamps,
                config.live_alpha.risk.max_cancel_rate_per_min,
                started,
                max_duration_sec,
            )
            .await,
            "cancel_rate_slot_unavailable"
        );
        let latest_order = la5_try_after_accept!(
            read_maker_order_with_official_sdk(&submit_input, &submission.order_id).await,
            "pre_cancel_exact_readback_failed"
        );
        let mut cancel_request_sent = false;
        let mut exact_cancel_confirmed = false;
        if la5_order_status_needs_cancel(&latest_order) {
            cancel_request_sent = true;
            let canceled_ids =
                cancel_exact_maker_order_with_official_sdk(&submit_input, &submission.order_id)
                    .await?;
            cancel_timestamps.push(Instant::now());
            exact_cancel_confirmed = canceled_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(&submission.order_id));
            if !exact_cancel_confirmed {
                append_la5_journal_event(
                    &journal,
                    run_id,
                    LiveJournalEventType::MakerReconciliationFailed,
                    serde_json::json!({
                        "status": "cancel_not_confirmed",
                        "order_id": submission.order_id,
                        "canceled_ids": canceled_ids,
                    }),
                )?;
                return Err("LA5 exact cancel did not return the submitted order ID".into());
            }
            append_la5_journal_event(
                &journal,
                run_id,
                LiveJournalEventType::MakerOrderCanceled,
                serde_json::json!({
                    "order_id": submission.order_id,
                    "intent_id": plan.intent_id,
                    "sequence": sequence,
                    "status": "cancel_requested",
                }),
            )?;
        }

        let final_order =
            read_maker_order_with_official_sdk(&submit_input, &submission.order_id).await?;
        let venue_final_canceled = la5_order_status_is_canceled(&final_order);
        let final_readback = live_alpha_authenticated_readback(config).await?;
        let final_trade_ids = la5_order_trade_ids(
            &baseline_trade_ids,
            &final_readback.trades,
            &submission,
            &final_order,
        );
        let filled = la5_order_status_is_filled(&final_order) || !final_trade_ids.is_empty();
        if filled {
            append_la5_journal_event(
                &journal,
                run_id,
                LiveJournalEventType::MakerOrderFilled,
                serde_json::json!({
                    "order_id": submission.order_id,
                    "intent_id": plan.intent_id,
                    "sequence": sequence,
                    "status": final_order.venue_status,
                    "trade_ids": final_trade_ids,
                }),
            )?;
        }
        let final_reconciliation = reconcile_la5_order_state(
            run_id,
            &submission.order_id,
            &final_order,
            &final_readback,
            &final_trade_ids,
            true,
        )?;
        append_la5_reconciliation_event(
            &journal,
            run_id,
            &submission.order_id,
            &final_reconciliation,
        )?;
        if final_reconciliation.status() != "passed" {
            return Err(format!(
                "LA5 final reconciliation failed: {}",
                final_reconciliation.mismatch_list()
            )
            .into());
        }
        if final_readback.report.open_order_count != 0
            || final_readback.report.reserved_pusd_units != 0
        {
            return Err(format!(
                "LA5 final readback not flat after order {}: open_orders={}, reserved_pusd_units={}",
                submission.order_id,
                final_readback.report.open_order_count,
                final_readback.report.reserved_pusd_units
            )
            .into());
        }

        cumulative_notional += plan.notional;
        outcomes.push(La5MakerOrderOutcome {
            sequence,
            intent_id: plan.intent_id.clone(),
            market_slug: market_intent.market.slug.clone(),
            token_id: plan.token_id.clone(),
            outcome: plan.outcome.clone(),
            side: plan.side,
            price: plan.price,
            size: plan.size,
            notional: plan.notional,
            gtd_expiration_unix: plan.gtd_expiration_unix,
            cancel_after_unix: plan.cancel_after_unix,
            order_id: submission.order_id.clone(),
            accepted_status: submission.venue_status.clone(),
            final_status: final_order.venue_status.clone(),
            canceled: venue_final_canceled,
            cancel_request_sent,
            exact_cancel_confirmed,
            venue_final_canceled,
            filled,
            trade_ids: final_trade_ids,
            pre_submit_available_pusd_units: pre_submit_readback.report.available_pusd_units,
            post_order_available_pusd_units: post_order_readback.report.available_pusd_units,
            final_available_pusd_units: final_readback.report.available_pusd_units,
            final_reserved_pusd_units: final_readback.report.reserved_pusd_units,
            reconciliation_status: final_reconciliation.status().to_string(),
            reconciliation_mismatches: final_reconciliation.mismatch_list(),
        });
    }

    if outcomes.len() != max_orders as usize {
        return Err(format!(
            "LA5 exact order count mismatch: expected {max_orders}, observed {}",
            outcomes.len()
        )
        .into());
    }
    validate_la5_session_against_approval(approval_artifact, &outcomes, cumulative_notional)?;
    append_la5_journal_event(
        &journal,
        run_id,
        LiveJournalEventType::MakerMicroStopped,
        serde_json::json!({
            "status": "completed",
            "orders": outcomes,
            "cumulative_notional": cumulative_notional,
        }),
    )?;
    let replay_state = journal.replay_state(run_id)?;
    if replay_state.reconciliation_mismatch_count != 0 || replay_state.risk_halted {
        return Err(
            "LA5 journal replay found mismatch or risk halt after completed session".into(),
        );
    }

    println!("live_alpha_maker_micro_status=completed");
    println!("run_id={run_id}");
    println!("live_alpha_maker_micro_orders_submitted={}", outcomes.len());
    println!("live_alpha_maker_micro_cumulative_notional={cumulative_notional:.6}");
    println!(
        "live_alpha_maker_micro_order_outcomes={}",
        serde_json::to_string(&outcomes)?
    );
    println!("live_alpha_maker_micro_journal_replay_status=passed");
    Ok(())
}

fn require_la5_pre_submit_readback(
    label: &str,
    readback: &ReadbackPreflightValidation,
) -> Result<(), Box<dyn std::error::Error>> {
    if !readback.report.live_network_enabled {
        return Err(format!("LA5 {label} readback blocked: live_network_disabled").into());
    }
    if !readback.report.passed() {
        return Err(format!(
            "LA5 {label} readback blocked: {}",
            readback.report.block_reasons.join(",")
        )
        .into());
    }
    if readback.report.open_order_count != 0 {
        return Err(format!("LA5 {label} readback blocked: open_orders_nonzero").into());
    }
    if readback.report.reserved_pusd_units != 0 {
        return Err(format!("LA5 {label} readback blocked: reserved_pusd_nonzero").into());
    }
    if !matches!(
        readback.report.heartbeat,
        "not_started_no_open_orders" | "healthy"
    ) {
        return Err(format!(
            "LA5 {label} readback blocked: heartbeat_status={}",
            readback.report.heartbeat
        )
        .into());
    }
    Ok(())
}

async fn select_la5_maker_market_intent(
    config: &AppConfig,
    sequence: u64,
    max_orders: u64,
    cumulative_notional: f64,
    available_pusd_units: u64,
) -> Result<La5MakerMarketIntent, Box<dyn std::error::Error>> {
    let now_ms = unix_time_ms();
    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let discovery_run = discovery.discover_crypto_15m_markets().await?;
    let markets = select_paper_markets(&discovery_run.markets, now_ms)?;
    let mut blockers = Vec::new();
    for market in markets {
        match build_la5_market_intent_from_market(
            config,
            sequence,
            max_orders,
            cumulative_notional,
            available_pusd_units,
            now_ms,
            market,
        )
        .await
        {
            Ok(intent) => return Ok(intent),
            Err(error) => blockers.push(error.to_string()),
        }
    }
    Err(format!(
        "LA5 could not select a current BTC/ETH/SOL maker market: {}",
        blockers.join(";")
    )
    .into())
}

#[allow(clippy::too_many_arguments)]
async fn build_la5_market_intent_from_market(
    config: &AppConfig,
    sequence: u64,
    max_orders: u64,
    cumulative_notional: f64,
    available_pusd_units: u64,
    now_ms: i64,
    market: Market,
) -> Result<La5MakerMarketIntent, Box<dyn std::error::Error>> {
    if market.end_ts
        <= now_ms.saturating_add(
            (config.live_alpha.risk.no_trade_seconds_before_close as i64).saturating_mul(1_000),
        )
    {
        return Err(format!("{} past_or_inside_no_trade_window", market.slug).into());
    }
    let outcome = market
        .outcomes
        .iter()
        .find(|outcome| outcome.outcome.eq_ignore_ascii_case("Up"))
        .or_else(|| market.outcomes.first())
        .ok_or_else(|| format!("{} missing outcome token", market.slug))?
        .clone();
    let book = fetch_live_alpha_book(config, &outcome.token_id)
        .await?
        .ok_or_else(|| format!("{} missing CLOB book", market.slug))?;
    let (best_bid, best_bid_size) =
        best_bid(&book).ok_or_else(|| format!("{} missing best bid", market.slug))?;
    let (best_ask, best_ask_size) =
        best_ask(&book).ok_or_else(|| format!("{} missing best ask", market.slug))?;
    if best_bid >= best_ask {
        return Err(format!("{} crossed_or_locked_book", market.slug).into());
    }
    let book_snapshot_id = book
        .hash
        .clone()
        .unwrap_or_else(|| format!("{}:{}", book.market_id, book.token_id));
    let book_age_ms = book
        .source_ts
        .and_then(|source_ts| age_ms(now_ms, source_ts))
        .ok_or_else(|| format!("{} missing book timestamp", market.slug))?;
    if book_age_ms > config.live_alpha.risk.max_book_staleness_ms {
        return Err(format!("{} book_stale age_ms={book_age_ms}", market.slug).into());
    }
    let reference = live_alpha_reference_evidence(config, market.asset).await?;
    let reference_snapshot_id = reference
        .snapshot_id
        .clone()
        .ok_or_else(|| format!("{} missing reference snapshot", market.slug))?;
    let reference_age_ms = reference
        .age_ms
        .ok_or_else(|| format!("{} missing reference age", market.slug))?;
    if reference_age_ms > config.live_alpha.risk.max_reference_staleness_ms {
        return Err(format!("{} reference_stale age_ms={reference_age_ms}", market.slug).into());
    }
    let reference_price = reference
        .price
        .ok_or_else(|| format!("{} missing reference price", market.slug))?;
    let max_order_notional = la5_max_order_notional(config, max_orders, cumulative_notional);
    let available_pusd = fixed6_units_to_decimal(available_pusd_units);
    let max_order_notional = max_order_notional.min(available_pusd);
    let price = la5_post_only_buy_price(
        best_bid,
        best_ask,
        market.tick_size,
        market.min_order_size,
        max_order_notional,
    )
    .ok_or_else(|| format!("{} no_safe_post_only_price_under_caps", market.slug))?;
    let size = market.min_order_size;
    let notional = round_decimal(price * size);
    if notional > max_order_notional + 1e-9 {
        return Err(format!("{} notional_exceeds_la5_cap", market.slug).into());
    }
    let fair_probability = (price + (config.live_alpha.maker.min_edge_bps as f64 / 10_000.0))
        .max(price)
        .min(0.99);
    let intent = polymarket_15m_arb_bot::execution_intent::ExecutionIntent {
        intent_id: format!(
            "la5-{sequence}-{}-{}",
            market.asset.symbol().to_ascii_lowercase(),
            now_ms
        ),
        strategy_snapshot_id: format!("la5-live-{sequence}-{reference_snapshot_id}"),
        market_slug: market.slug.clone(),
        condition_id: market.condition_id.clone(),
        token_id: outcome.token_id.clone(),
        asset_symbol: asset_symbol(market.asset).to_string(),
        asset: market.asset,
        outcome: outcome.outcome.clone(),
        side: Side::Buy,
        price,
        size,
        notional,
        order_type: "GTD".to_string(),
        time_in_force: "GTD".to_string(),
        post_only: true,
        expiry: None,
        fair_probability,
        edge_bps: ((fair_probability - price) * 10_000.0).max(0.0),
        reference_price,
        reference_source_timestamp: Some(now_ms.saturating_sub(reference_age_ms as i64)),
        book_snapshot_id: book_snapshot_id.clone(),
        best_bid: Some(best_bid),
        best_ask: Some(best_ask),
        spread: Some(best_ask - best_bid),
        created_at: now_ms,
    };
    let tick_size = market.tick_size;
    let min_order_size = market.min_order_size;
    Ok(La5MakerMarketIntent {
        intent,
        market,
        best_bid,
        best_ask,
        best_bid_size,
        best_ask_size,
        tick_size,
        min_order_size,
        book_snapshot_id,
        book_age_ms,
        reference_snapshot_id,
        reference_age_ms,
    })
}

fn la5_live_risk_context(
    config: &AppConfig,
    market: &La5MakerMarketIntent,
    readback: &ReadbackPreflightValidation,
    cumulative_notional: f64,
    submit_count_last_min: u64,
) -> Result<LiveRiskContext, Box<dyn std::error::Error>> {
    Ok(LiveRiskContext {
        now_ms: Some(unix_time_ms()),
        market_end_ms: Some(market.market.end_ts),
        effective_quote_ttl_seconds: config.live_alpha.maker.ttl_seconds,
        available_pusd: fixed6_units_to_decimal(readback.report.available_pusd_units),
        reserved_pusd: fixed6_units_to_decimal(readback.report.reserved_pusd_units),
        up_token_id: market
            .market
            .outcomes
            .iter()
            .find(|outcome| outcome.outcome.eq_ignore_ascii_case("Up"))
            .map(|outcome| outcome.token_id.clone()),
        down_token_id: market
            .market
            .outcomes
            .iter()
            .find(|outcome| outcome.outcome.eq_ignore_ascii_case("Down"))
            .map(|outcome| outcome.token_id.clone()),
        open_order_count: readback.report.open_order_count as u64,
        open_orders_per_market: readback
            .open_orders
            .iter()
            .filter(|order| {
                order
                    .market
                    .eq_ignore_ascii_case(&market.market.condition_id)
            })
            .count() as u64,
        open_orders_per_asset: readback.report.open_order_count as u64,
        current_market_notional: cumulative_notional,
        current_asset_notional: cumulative_notional,
        current_total_live_notional: cumulative_notional,
        submit_count_last_min,
        book_age_ms: Some(market.book_age_ms),
        reference_age_ms: Some(market.reference_age_ms),
        geoblock_passed: true,
        heartbeat_healthy: matches!(
            readback.report.heartbeat,
            "not_started_no_open_orders" | "healthy"
        ),
        reconciliation_clean: true,
        ..LiveRiskContext::default()
    })
}

fn la5_maker_submit_input(
    config: &AppConfig,
    account: &AccountPreflight,
    plan: LiveMakerOrderPlan,
) -> LiveMakerSubmitInput {
    LiveMakerSubmitInput {
        clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
        signer_handle: config.live_beta.secret_handles.canary_private_key.clone(),
        l2_access_handle: config.live_beta.secret_handles.clob_l2_access.clone(),
        l2_secret_handle: config.live_beta.secret_handles.clob_l2_credential.clone(),
        l2_passphrase_handle: config.live_beta.secret_handles.clob_l2_passphrase.clone(),
        wallet_address: account.wallet_address.clone(),
        funder_address: account.funder_address.clone(),
        signature_type: account.signature_type,
        plan,
    }
}

async fn maintain_la5_heartbeat_until_cancel(
    input: &LiveMakerSubmitInput,
    cancel_after_unix: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut heartbeat_id: Option<String> = None;
    loop {
        heartbeat_id =
            Some(post_maker_heartbeat_with_official_sdk(input, heartbeat_id.as_deref()).await?);
        let now = unix_time_secs();
        if now >= cancel_after_unix {
            break;
        }
        let remaining = cancel_after_unix.saturating_sub(now);
        tokio::time::sleep(Duration::from_secs(remaining.min(5))).await;
    }
    heartbeat_id.ok_or_else(|| "LA5 heartbeat did not return an id".into())
}

async fn wait_for_la5_rate_slot(
    attempts: &[Instant],
    max_per_min: u64,
    started: Instant,
    max_duration_sec: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if max_per_min == 0 {
        return Err("LA5 rate limit configured as zero".into());
    }
    loop {
        let recent = recent_count_last_min(attempts);
        if recent < max_per_min {
            return Ok(());
        }
        if started.elapsed() >= Duration::from_secs(max_duration_sec) {
            return Err("LA5 max duration elapsed while waiting for rate limit slot".into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn recent_count_last_min(attempts: &[Instant]) -> u64 {
    attempts
        .iter()
        .filter(|attempt| attempt.elapsed() < Duration::from_secs(60))
        .count() as u64
}

fn la5_max_order_notional(config: &AppConfig, max_orders: u64, cumulative_notional: f64) -> f64 {
    let risk = &config.live_alpha.risk;
    let per_order_session_cap = risk.max_total_live_notional / max_orders as f64;
    [
        risk.max_single_order_notional,
        risk.max_available_pusd_usage,
        risk.max_reserved_pusd,
        per_order_session_cap,
        risk.max_total_live_notional
            .saturating_sub_f64(cumulative_notional),
    ]
    .into_iter()
    .filter(|value| value.is_finite() && *value > 0.0)
    .fold(f64::INFINITY, f64::min)
}

trait SaturatingSubF64 {
    fn saturating_sub_f64(self, rhs: f64) -> f64;
}

impl SaturatingSubF64 for f64 {
    fn saturating_sub_f64(self, rhs: f64) -> f64 {
        if self > rhs && self.is_finite() && rhs.is_finite() {
            self - rhs
        } else {
            0.0
        }
    }
}

fn la5_post_only_buy_price(
    best_bid: f64,
    best_ask: f64,
    tick_size: f64,
    min_order_size: f64,
    max_notional: f64,
) -> Option<f64> {
    if !la5_valid_price(best_bid)
        || !la5_valid_price(best_ask)
        || best_bid >= best_ask
        || tick_size <= 0.0
        || min_order_size <= 0.0
        || max_notional <= 0.0
    {
        return None;
    }
    let book_cap = if best_bid > tick_size {
        best_bid - tick_size
    } else {
        best_bid
    };
    let cap_price = (max_notional / min_order_size)
        .min(book_cap)
        .min(best_ask - tick_size);
    let price = floor_to_tick(cap_price, tick_size);
    (price > 0.0 && price < best_ask).then_some(price)
}

fn la5_valid_price(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value < 1.0
}

fn floor_to_tick(value: f64, tick_size: f64) -> f64 {
    round_decimal((value / tick_size).floor() * tick_size)
}

fn round_decimal(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn best_bid(book: &OrderBookSnapshot) -> Option<(f64, Option<f64>)> {
    book.bids
        .iter()
        .max_by(|left, right| left.price.total_cmp(&right.price))
        .map(|level| (level.price, Some(level.size)))
}

fn best_ask(book: &OrderBookSnapshot) -> Option<(f64, Option<f64>)> {
    book.asks
        .iter()
        .min_by(|left, right| left.price.total_cmp(&right.price))
        .map(|level| (level.price, Some(level.size)))
}

fn la5_order_trade_ids(
    baseline_trade_ids: &BTreeSet<String>,
    trades: &[TradeReadback],
    submission: &LiveMakerSubmissionReport,
    order: &LiveMakerOrderReadbackReport,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for id in submission
        .trade_ids
        .iter()
        .chain(order.associate_trades.iter())
    {
        if !id.trim().is_empty() {
            ids.insert(id.clone());
        }
    }
    for trade in trades {
        if baseline_trade_ids.contains(&trade.id) {
            continue;
        }
        if trade
            .order_id
            .as_deref()
            .is_some_and(|order_id| order_id.eq_ignore_ascii_case(&order.order_id))
        {
            ids.insert(trade.id.clone());
        }
    }
    ids.into_iter().collect()
}

fn reconcile_la5_order_state(
    run_id: &str,
    order_id: &str,
    order: &LiveMakerOrderReadbackReport,
    readback: &ReadbackPreflightValidation,
    trade_ids: &[String],
    expect_flat: bool,
) -> Result<
    polymarket_15m_arb_bot::live_reconciliation::LiveReconciliationResult,
    Box<dyn std::error::Error>,
> {
    let checked_at_ms = unix_time_ms();
    let collateral = readback
        .collateral
        .as_ref()
        .ok_or("LA5 reconciliation missing collateral readback")?;
    let balance = balance_snapshot_from_readback(&readback.report, collateral, checked_at_ms);
    let mut local = LocalLiveState {
        balance: Some(balance.clone()),
        ..LocalLiveState::default()
    };
    local.known_orders.insert(order_id.to_string());
    if expect_flat {
        local.canceled_orders.insert(order_id.to_string());
    }
    for trade_id in trade_ids {
        local.known_trades.insert(trade_id.clone());
        local.trade_order_ids.insert(order_id.to_string());
        local
            .trade_order_ids_by_trade
            .insert(trade_id.clone(), order_id.to_string());
    }

    let mut venue = VenueLiveState {
        balance: Some(balance),
        ..VenueLiveState::default()
    };
    venue.orders.insert(
        order_id.to_string(),
        VenueOrderState {
            order_id: order_id.to_string(),
            status: venue_order_status_from_la5_order(order),
            matched_size: order.size_matched,
            remaining_size: order.remaining_size,
        },
    );
    for trade in readback
        .trades
        .iter()
        .filter(|trade| trade_ids.contains(&trade.id))
    {
        venue.trades.insert(
            trade.id.clone(),
            VenueTradeState {
                trade_id: trade.id.clone(),
                order_id: trade
                    .order_id
                    .clone()
                    .unwrap_or_else(|| order_id.to_string()),
                status: venue_trade_status_from_readback(trade.status),
            },
        );
    }

    Ok(reconcile_live_state(LiveReconciliationInput {
        run_id: run_id.to_string(),
        checked_at_ms,
        local,
        venue,
        venue_position_evidence_complete: false,
    }))
}

fn venue_order_status_from_la5_order(order: &LiveMakerOrderReadbackReport) -> VenueOrderStatus {
    match order.venue_status.to_ascii_lowercase().as_str() {
        "live" => {
            if order.size_matched > 0.0 {
                VenueOrderStatus::PartiallyFilled
            } else {
                VenueOrderStatus::Live
            }
        }
        "matched" => {
            if order.remaining_size <= 0.0 {
                VenueOrderStatus::Filled
            } else {
                VenueOrderStatus::PartiallyFilled
            }
        }
        "canceled" => VenueOrderStatus::Canceled,
        _ => VenueOrderStatus::Unknown,
    }
}

fn venue_trade_status_from_readback(status: TradeReadbackStatus) -> VenueTradeStatus {
    match status {
        TradeReadbackStatus::Matched => VenueTradeStatus::Matched,
        TradeReadbackStatus::Mined => VenueTradeStatus::Mined,
        TradeReadbackStatus::Confirmed => VenueTradeStatus::Confirmed,
        TradeReadbackStatus::Retrying => VenueTradeStatus::Retrying,
        TradeReadbackStatus::Failed => VenueTradeStatus::Failed,
        TradeReadbackStatus::Unknown => VenueTradeStatus::Unknown,
    }
}

fn la5_order_status_needs_cancel(order: &LiveMakerOrderReadbackReport) -> bool {
    !matches!(
        venue_order_status_from_la5_order(order),
        VenueOrderStatus::Canceled | VenueOrderStatus::Filled
    )
}

fn la5_order_status_is_filled(order: &LiveMakerOrderReadbackReport) -> bool {
    venue_order_status_from_la5_order(order) == VenueOrderStatus::Filled
}

fn la5_order_status_is_canceled(order: &LiveMakerOrderReadbackReport) -> bool {
    venue_order_status_from_la5_order(order) == VenueOrderStatus::Canceled
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct La5PostAcceptanceCleanupReport {
    reason: String,
    order_id: String,
    attempted: bool,
    confirmed: bool,
    canceled_ids: Vec<String>,
    cleanup_error: Option<String>,
    journal_error: Option<String>,
}

#[allow(clippy::too_many_arguments)]
async fn la5_cleanup_accepted_order_before_error<F, Fut, E>(
    journal: &LiveOrderJournal,
    run_id: &str,
    order_id: &str,
    intent_id: &str,
    sequence: u64,
    reason: &str,
    original_error: String,
    cancel_exact: F,
) -> Box<dyn std::error::Error>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Vec<String>, E>>,
    E: std::fmt::Display,
{
    let mut report = La5PostAcceptanceCleanupReport {
        reason: reason.to_string(),
        order_id: order_id.to_string(),
        attempted: true,
        confirmed: false,
        canceled_ids: Vec::new(),
        cleanup_error: None,
        journal_error: None,
    };

    match cancel_exact().await {
        Ok(canceled_ids) => {
            report.confirmed = canceled_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(order_id));
            report.canceled_ids = canceled_ids;
            let event_type = if report.confirmed {
                LiveJournalEventType::MakerOrderCanceled
            } else {
                LiveJournalEventType::MakerReconciliationFailed
            };
            let status = if report.confirmed {
                "cleanup_cancel_confirmed"
            } else {
                "cleanup_cancel_not_confirmed"
            };
            if let Err(error) = append_la5_journal_event(
                journal,
                run_id,
                event_type,
                serde_json::json!({
                    "status": status,
                    "order_id": order_id,
                    "intent_id": intent_id,
                    "sequence": sequence,
                    "cleanup_reason": reason,
                    "canceled_ids": report.canceled_ids.clone(),
                }),
            ) {
                report.journal_error = Some(error.to_string());
            }
        }
        Err(error) => {
            report.cleanup_error = Some(error.to_string());
            if let Err(journal_error) = append_la5_journal_event(
                journal,
                run_id,
                LiveJournalEventType::MakerReconciliationFailed,
                serde_json::json!({
                    "status": "cleanup_cancel_failed",
                    "order_id": order_id,
                    "intent_id": intent_id,
                    "sequence": sequence,
                    "cleanup_reason": reason,
                    "cleanup_error": report.cleanup_error.clone(),
                }),
            ) {
                report.journal_error = Some(journal_error.to_string());
            }
        }
    }

    let mut message = format!(
        "{original_error}; cleanup_cancel_attempted={}; cleanup_cancel_confirmed={}",
        report.attempted, report.confirmed
    );
    if let Some(error) = &report.cleanup_error {
        message.push_str("; cleanup_cancel_error=");
        message.push_str(error);
    }
    if let Some(error) = &report.journal_error {
        message.push_str("; cleanup_journal_error=");
        message.push_str(error);
    }
    message.into()
}

fn append_la5_reconciliation_event(
    journal: &LiveOrderJournal,
    run_id: &str,
    order_id: &str,
    result: &polymarket_15m_arb_bot::live_reconciliation::LiveReconciliationResult,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_type = if result.status() == "passed" {
        LiveJournalEventType::MakerReconciliationPassed
    } else {
        LiveJournalEventType::MakerReconciliationFailed
    };
    append_la5_journal_event(
        journal,
        run_id,
        event_type,
        serde_json::json!({
            "status": result.status(),
            "order_id": order_id,
            "mismatches": result.mismatch_list(),
        }),
    )
}

fn append_la5_journal_event(
    journal: &LiveOrderJournal,
    run_id: &str,
    event_type: LiveJournalEventType,
    payload: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let event = LiveJournalEvent::new(
        run_id.to_string(),
        format!(
            "{}-la5-{}-{}",
            run_id,
            unix_time_ms(),
            event_type_label(event_type)
        ),
        event_type,
        unix_time_ms(),
        payload,
    );
    journal.append(&event)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn build_live_alpha_preflight_command_result(
    config: &AppConfig,
    run_id: &str,
    approval_artifact: &Path,
    order_cap_state: &Path,
    mode: LiveAlphaPreflightMode,
    human_approved: bool,
    approval_id: Option<&str>,
) -> Result<LiveAlphaPreflightCommandResult, Box<dyn std::error::Error>> {
    let markdown = fs::read_to_string(approval_artifact)?;
    let approval = live_fill_canary::parse_la3_approval_artifact(&markdown)?;
    if let Some(approval_id) = approval_id {
        if approval_id != approval.approval_id {
            return Err(format!(
                "LA3 approval id mismatch: command requested {approval_id}, artifact contains {}",
                approval.approval_id
            )
            .into());
        }
    }

    let geoblock = run_geoblock_validation(config).await?;
    let account = lb4_account_preflight(config)?;
    let l2_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    let canary_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.canary_secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    let readback = if !geoblock.blocked && l2_secret_report.all_present() {
        live_alpha_authenticated_readback(config).await?
    } else {
        ReadbackPreflightValidation::from_report(live_beta_readback::sample_readback_preflight(
            lb4_readback_prerequisites(
                config,
                safety::GeoblockGateStatus::from_blocked(geoblock.blocked),
            ),
        )?)
    };
    let public = live_alpha_public_market_evidence(config, &approval).await?;
    let reference_asset = live_alpha_asset_from_symbol(&approval.asset_symbol)?;
    let reference = live_alpha_reference_evidence(config, reference_asset).await?;
    let prior_attempt_consumed = read_la3_fill_cap_state(order_cap_state)?
        .map(|state| state.submission_attempted)
        .unwrap_or(false);
    let journal_path_present = config.live_alpha.journal_path().is_some();
    let journal_replay_passed = config
        .live_alpha
        .journal_path()
        .map(|path| {
            let path = Path::new(path);
            !path.exists() || LiveOrderJournal::new(path).replay().is_ok()
        })
        .unwrap_or(false);

    let current = LiveAlphaCurrentPreflight {
        run_id: run_id.to_string(),
        host_id: current_host_label(),
        geoblock_passed: !geoblock.blocked,
        geoblock_result: geoblock_result_label(&geoblock),
        wallet_id: account.wallet_address.clone(),
        funder_id: account.funder_address.clone(),
        signature_type: account.signature_type.as_config_str().to_string(),
        live_alpha_enabled: config.live_alpha.enabled,
        live_alpha_mode: config.live_alpha.mode.as_str().to_string(),
        fill_canary_enabled: config.live_alpha.fill_canary.enabled,
        allow_fak: config.live_alpha.fill_canary.allow_fak,
        allow_fok: config.live_alpha.fill_canary.allow_fok,
        allow_marketable_limit: config.live_alpha.fill_canary.allow_marketable_limit,
        compile_time_orders_enabled: live_alpha_gate::LIVE_ALPHA_ORDER_FEATURE_ENABLED,
        cli_intent_enabled: matches!(
            mode,
            LiveAlphaPreflightMode::DryRun | LiveAlphaPreflightMode::FinalSubmit
        ),
        human_approved,
        kill_switch_active: config.live_beta.kill_switch_active,
        account_preflight_passed: readback.report.passed() && readback.report.live_network_enabled,
        account_preflight_live_network_enabled: readback.report.live_network_enabled,
        available_pusd_units: readback.report.available_pusd_units,
        allowance_pusd_units: readback
            .collateral
            .as_ref()
            .map(|collateral| collateral.allowance_units)
            .unwrap_or_default(),
        reserved_pusd_units: readback.report.reserved_pusd_units,
        open_order_count: readback.report.open_order_count,
        recent_trade_count: readback.report.trade_count,
        heartbeat_status: readback.report.heartbeat.to_string(),
        market_found: public.market_found,
        market_active: public.market_active,
        market_closed: public.market_closed,
        market_accepting_orders: public.market_accepting_orders,
        current_market_slug: public.market_slug,
        current_condition_id: public.condition_id,
        current_token_id: public.token_id,
        current_asset_symbol: public.asset_symbol,
        current_outcome: public.outcome,
        current_market_end_unix: public.market_end_unix,
        current_min_order_size: public.min_order_size,
        current_tick_size: public.tick_size,
        best_bid: public.best_bid,
        best_bid_size: public.best_bid_size,
        best_ask: public.best_ask,
        best_ask_size: public.best_ask_size,
        book_snapshot_id: public.book_snapshot_id,
        book_age_ms: public.book_age_ms,
        max_book_age_ms: config
            .live_alpha
            .risk
            .max_book_staleness_ms
            .max(config.risk.stale_book_ms),
        reference_snapshot_id: reference.snapshot_id,
        reference_age_ms: reference.age_ms,
        max_reference_age_ms: config
            .live_alpha
            .risk
            .max_reference_staleness_ms
            .max(config.reference_feed.max_staleness_ms)
            .max(config.risk.stale_reference_ms),
        journal_path_present,
        journal_replay_passed,
        prior_attempt_consumed,
        now_unix: unix_time_secs(),
        no_trade_seconds_before_close: config.live_alpha.risk.no_trade_seconds_before_close,
        canary_secret_handles_present: canary_secret_report.all_present(),
        l2_secret_handles_present: l2_secret_report.all_present(),
    };
    let report = live_alpha_preflight::evaluate_live_alpha_preflight(
        mode,
        &approval.approved_bounds(),
        &current,
    );
    let envelope = live_fill_canary::build_fill_canary_envelope(&report, unix_time_ms());
    let approval_prompt = live_fill_canary::canonical_fill_canary_prompt(&envelope, &report);
    let approval_sha256 = live_fill_canary::approval_hash(&approval_prompt);

    Ok(LiveAlphaPreflightCommandResult {
        approval,
        report,
        envelope,
        approval_prompt,
        approval_sha256,
    })
}

fn print_live_alpha_preflight_result(
    result: &LiveAlphaPreflightCommandResult,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("live_alpha_preflight_status={}", result.report.status);
    println!("live_alpha_preflight_mode={}", result.report.mode);
    println!("run_id={}", result.report.run_id);
    println!(
        "live_alpha_preflight_approval_id={}",
        result.report.approval_id
    );
    println!(
        "live_alpha_preflight_block_reasons={}",
        result.report.block_reasons.join(",")
    );
    println!(
        "live_alpha_preflight_geoblock_result={}",
        result.report.geoblock_result
    );
    println!("live_alpha_preflight_host_id={}", result.report.host_id);
    println!("live_alpha_preflight_wallet_id={}", result.report.wallet_id);
    println!("live_alpha_preflight_funder_id={}", result.report.funder_id);
    println!(
        "live_alpha_preflight_account_passed={}",
        result.report.account_preflight_passed
    );
    println!(
        "live_alpha_preflight_account_live_network={}",
        result.report.account_preflight_live_network_enabled
    );
    println!(
        "live_alpha_preflight_available_pusd_units={}",
        result.report.available_pusd_units
    );
    println!(
        "live_alpha_preflight_reserved_pusd_units={}",
        result.report.reserved_pusd_units
    );
    println!(
        "live_alpha_preflight_open_order_count={}",
        result.report.open_order_count
    );
    println!(
        "live_alpha_preflight_recent_trade_count={}",
        result.report.recent_trade_count
    );
    println!(
        "live_alpha_preflight_heartbeat={}",
        result.report.heartbeat_status
    );
    println!(
        "live_alpha_preflight_market_order_intent={}/{}/{}/{}/{}/{}@{} amount_or_size={}",
        result.report.asset_symbol,
        result.report.market_slug,
        result.report.condition_id,
        result.report.token_id,
        result.report.outcome,
        result.report.side,
        result.report.price,
        result.report.amount_or_size
    );
    println!(
        "live_alpha_preflight_book_snapshot_id={}",
        result.report.book_snapshot_id
    );
    println!(
        "live_alpha_preflight_book_age_ms={}",
        result
            .report
            .book_age_ms
            .map(|age| age.to_string())
            .unwrap_or_else(|| "missing".to_string())
    );
    println!(
        "live_alpha_preflight_reference_snapshot_id={}",
        result.report.reference_snapshot_id
    );
    println!(
        "live_alpha_preflight_reference_age_ms={}",
        result
            .report
            .reference_age_ms
            .map(|age| age.to_string())
            .unwrap_or_else(|| "missing".to_string())
    );
    println!(
        "live_alpha_preflight_compile_time_orders_enabled={}",
        result.report.compile_time_orders_enabled
    );
    println!(
        "live_alpha_preflight_official_taker_fee_estimate={}",
        result
            .report
            .official_taker_fee_estimate
            .map(|fee| format!("{fee:.6}"))
            .unwrap_or_else(|| "missing".to_string())
    );
    println!(
        "live_alpha_fill_canary_approval_prompt=\n{}",
        result.approval_prompt
    );
    println!(
        "live_alpha_fill_canary_approval_sha256={}",
        result.approval_sha256
    );
    println!(
        "live_alpha_fill_canary_envelope={}",
        serde_json::to_string(&result.envelope)?
    );
    println!(
        "live_alpha_preflight_report={}",
        serde_json::to_string(&result.report)?
    );
    Ok(())
}

struct LiveAlphaPublicMarketEvidence {
    market_found: bool,
    market_active: bool,
    market_closed: bool,
    market_accepting_orders: bool,
    market_slug: Option<String>,
    condition_id: Option<String>,
    token_id: Option<String>,
    asset_symbol: Option<String>,
    outcome: Option<String>,
    market_end_unix: Option<u64>,
    min_order_size: Option<f64>,
    tick_size: Option<f64>,
    best_bid: Option<f64>,
    best_bid_size: Option<f64>,
    best_ask: Option<f64>,
    best_ask_size: Option<f64>,
    book_snapshot_id: Option<String>,
    book_age_ms: Option<u64>,
}

async fn live_alpha_public_market_evidence(
    config: &AppConfig,
    approval: &LiveAlphaApprovalArtifact,
) -> Result<LiveAlphaPublicMarketEvidence, Box<dyn std::error::Error>> {
    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let market = discovery
        .discover_crypto_15m_market_by_slug(&approval.market_slug)
        .await?;
    let Some(market) = market else {
        return Ok(LiveAlphaPublicMarketEvidence::missing());
    };
    let outcome = market
        .outcomes
        .iter()
        .find(|outcome| {
            outcome.token_id == approval.token_id
                || outcome.outcome.eq_ignore_ascii_case(&approval.outcome)
        })
        .cloned();
    let book: Option<OrderBookSnapshot> =
        (fetch_live_alpha_book(config, &approval.token_id).await).unwrap_or_default();
    let (best_bid, best_bid_size) = book
        .as_ref()
        .and_then(|book| {
            book.bids
                .iter()
                .max_by(|left, right| left.price.total_cmp(&right.price))
                .map(|level| (Some(level.price), Some(level.size)))
        })
        .unwrap_or((None, None));
    let (best_ask, best_ask_size) = book
        .as_ref()
        .and_then(|book| {
            book.asks
                .iter()
                .min_by(|left, right| left.price.total_cmp(&right.price))
                .map(|level| (Some(level.price), Some(level.size)))
        })
        .unwrap_or((None, None));
    let book_snapshot_id = book.as_ref().and_then(|book| book.hash.clone());
    let book_age_ms = book
        .as_ref()
        .and_then(|book| book.source_ts)
        .and_then(|source_ts| age_ms(unix_time_ms(), source_ts));

    Ok(LiveAlphaPublicMarketEvidence {
        market_found: true,
        market_active: market.lifecycle_state == MarketLifecycleState::Active,
        market_closed: market.lifecycle_state == MarketLifecycleState::Closed,
        market_accepting_orders: market.lifecycle_state == MarketLifecycleState::Active
            && market.ineligibility_reason.is_none(),
        market_slug: Some(market.slug),
        condition_id: Some(market.condition_id),
        token_id: outcome.as_ref().map(|outcome| outcome.token_id.clone()),
        asset_symbol: Some(asset_symbol(market.asset).to_string()),
        outcome: outcome.map(|outcome| outcome.outcome),
        market_end_unix: u64::try_from(market.end_ts / 1_000).ok(),
        min_order_size: Some(market.min_order_size),
        tick_size: Some(market.tick_size),
        best_bid,
        best_bid_size,
        best_ask,
        best_ask_size,
        book_snapshot_id,
        book_age_ms,
    })
}

impl LiveAlphaPublicMarketEvidence {
    fn missing() -> Self {
        Self {
            market_found: false,
            market_active: false,
            market_closed: true,
            market_accepting_orders: false,
            market_slug: None,
            condition_id: None,
            token_id: None,
            asset_symbol: None,
            outcome: None,
            market_end_unix: None,
            min_order_size: None,
            tick_size: None,
            best_bid: None,
            best_bid_size: None,
            best_ask: None,
            best_ask_size: None,
            book_snapshot_id: None,
            book_age_ms: None,
        }
    }
}

async fn fetch_live_alpha_book(
    config: &AppConfig,
    token_id: &str,
) -> Result<Option<OrderBookSnapshot>, Box<dyn std::error::Error>> {
    let snapshot_client = PolymarketBookSnapshotClient::new(
        &config.polymarket.clob_rest_url,
        config.polymarket.request_timeout_ms,
    )?;
    let payload = snapshot_client.fetch_book(token_id).await?;
    let batch = normalize_feed_message(SOURCE_POLYMARKET_CLOB, &payload, unix_time_ms())?;
    Ok(batch.events.into_iter().find_map(|event| match event {
        NormalizedEvent::BookSnapshot { book } if book.token_id == token_id => Some(book),
        _ => None,
    }))
}

struct LiveAlphaReferenceEvidence {
    snapshot_id: Option<String>,
    age_ms: Option<u64>,
    price: Option<f64>,
}

async fn live_alpha_reference_evidence(
    config: &AppConfig,
    asset: Asset,
) -> Result<LiveAlphaReferenceEvidence, Box<dyn std::error::Error>> {
    if config.reference_feed.is_polymarket_rtds_chainlink_enabled() {
        return live_alpha_rtds_chainlink_reference_evidence(config, asset).await;
    }

    if !config.reference_feed.is_pyth_proxy_enabled() {
        return Ok(LiveAlphaReferenceEvidence {
            snapshot_id: None,
            age_ms: None,
            price: None,
        });
    }
    let recv_wall_ts = unix_time_ms();
    let client = PythHermesClient::new(
        &config.reference_feed.pyth_hermes_url,
        config.polymarket.request_timeout_ms,
    )?;
    let batch = client
        .fetch_latest(&config.reference_feed, recv_wall_ts)
        .await?;
    Ok(live_alpha_reference_evidence_from_events(
        batch.events,
        asset,
    ))
}

async fn live_alpha_rtds_chainlink_reference_evidence(
    config: &AppConfig,
    asset: Asset,
) -> Result<LiveAlphaReferenceEvidence, Box<dyn std::error::Error>> {
    let client = ReadOnlyWebSocketClient;
    let probe = FeedConnectionConfig {
        source: SOURCE_POLYMARKET_RTDS_CHAINLINK.to_string(),
        ws_url: config.reference_feed.polymarket_rtds_url.clone(),
        subscribe_payload: Some(polymarket_rtds_chainlink_subscription_payload_for_asset(
            asset,
        )),
        message_limit: 8,
        connect_timeout_ms: config.feeds.connect_timeout_ms,
        read_timeout_ms: config.feeds.read_timeout_ms,
    };
    let result = client.connect_and_capture(&probe).await?;

    for message in result.received_text_messages {
        let recv_wall_ts = unix_time_ms();
        let events = match parse_polymarket_rtds_chainlink_message(
            &message,
            recv_wall_ts,
            config.reference_feed.max_staleness_ms,
        ) {
            Ok(events) => events,
            Err(error) if should_skip_stale_polymarket_rtds_reference_error(&error) => continue,
            Err(error) => return Err(error.into()),
        };
        let evidence = live_alpha_reference_evidence_from_events(events, asset);
        if evidence.snapshot_id.is_some() {
            return Ok(evidence);
        }
    }

    Ok(LiveAlphaReferenceEvidence {
        snapshot_id: None,
        age_ms: None,
        price: None,
    })
}

fn live_alpha_reference_evidence_from_events(
    events: Vec<NormalizedEvent>,
    asset: Asset,
) -> LiveAlphaReferenceEvidence {
    let Some(price) = events.into_iter().find_map(|event| match event {
        NormalizedEvent::ReferenceTick { price } if price.asset == asset => Some(price),
        _ => None,
    }) else {
        return LiveAlphaReferenceEvidence {
            snapshot_id: None,
            age_ms: None,
            price: None,
        };
    };
    let source_ts = price.source_ts.unwrap_or(price.recv_wall_ts);
    LiveAlphaReferenceEvidence {
        snapshot_id: Some(format!(
            "{}:{}:{}",
            price.source,
            price.provider.unwrap_or_else(|| "unknown".to_string()),
            source_ts
        )),
        age_ms: age_ms(price.recv_wall_ts, source_ts),
        price: Some(price.price),
    }
}

fn live_alpha_asset_from_symbol(value: &str) -> Result<Asset, String> {
    match value.trim().to_ascii_uppercase().as_str() {
        "BTC" => Ok(Asset::Btc),
        "ETH" => Ok(Asset::Eth),
        "SOL" => Ok(Asset::Sol),
        _ => Err(format!("unsupported LA3 approval asset_symbol {value:?}")),
    }
}

fn age_ms(now_ms: i64, source_ts: i64) -> Option<u64> {
    if source_ts <= 0 {
        return None;
    }
    if now_ms >= source_ts {
        u64::try_from(now_ms - source_ts).ok()
    } else {
        Some(0)
    }
}

async fn live_alpha_authenticated_readback(
    config: &AppConfig,
) -> Result<ReadbackPreflightValidation, Box<dyn std::error::Error>> {
    let credentials = lb4_l2_credentials_from_env(&config.live_beta.secret_handles)?;
    let account = lb4_account_preflight(config)?;
    let evidence =
        live_beta_readback::authenticated_readback_preflight_evidence(AuthenticatedReadbackInput {
            prerequisites: ReadbackPrerequisites {
                lb3_hold_released: config.live_beta.lb3_hold_released,
                legal_access_approved: config.live_beta.legal_access_approved,
                deployment_geoblock_passed: true,
            },
            account,
            credentials,
            required_collateral_allowance_units: config
                .live_beta
                .readback_account
                .required_collateral_allowance_units,
            request_timeout_ms: config.polymarket.request_timeout_ms,
        })
        .await?;
    Ok(ReadbackPreflightValidation::from_authenticated_evidence(
        evidence,
    ))
}

fn current_host_label() -> String {
    for key in ["HOSTNAME", "HOST"] {
        if let Ok(value) = env::var(key) {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn append_la3_journal_event(
    journal: &LiveOrderJournal,
    run_id: &str,
    event_type: LiveJournalEventType,
    payload: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let event = LiveJournalEvent::new(
        run_id.to_string(),
        format!(
            "{}-la3-{}-{}",
            run_id,
            unix_time_ms(),
            event_type_label(event_type)
        ),
        event_type,
        unix_time_ms(),
        payload,
    );
    journal.append(&event)?;
    Ok(())
}

fn event_type_label(event_type: LiveJournalEventType) -> String {
    serde_json::to_string(&event_type)
        .map(|encoded| encoded.trim_matches('"').to_string())
        .unwrap_or_else(|_| format!("{event_type:?}"))
}

fn read_la3_fill_cap_state(
    path: &Path,
) -> Result<Option<LiveAlphaFillCanaryCapState>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)?;
    Ok(Some(live_fill_canary::fill_canary_cap_state_from_json(
        &contents,
    )?))
}

fn reserve_la3_fill_cap(path: &Path, approval_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let state = LiveAlphaFillCanaryCapState {
        approval_id: approval_id.to_string(),
        submission_attempted: true,
        reserved_at_unix: unix_time_secs(),
        venue_order_id: None,
    };
    write_new_la3_fill_cap_state(path, &state)
}

fn validate_and_reserve_la3_fill_cap(
    path: &Path,
    approval_id: &str,
    submit_input: &LiveAlphaFillSubmitInput,
) -> Result<(), Box<dyn std::error::Error>> {
    live_fill_canary::validate_fill_submit_input_without_network(submit_input)?;
    reserve_la3_fill_cap(path, approval_id)
}

fn live_alpha_fill_canary_pre_submit_not_submitted(
    dry_run: bool,
    preflight_passed: bool,
) -> Option<bool> {
    if dry_run || !preflight_passed {
        Some(true)
    } else {
        None
    }
}

fn update_la3_fill_cap_with_order_id(
    path: &Path,
    approval_id: &str,
    venue_order_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_la3_fill_cap_state(
        path,
        &LiveAlphaFillCanaryCapState {
            approval_id: approval_id.to_string(),
            submission_attempted: true,
            reserved_at_unix: unix_time_secs(),
            venue_order_id: Some(venue_order_id.to_string()),
        },
    )
}

fn write_la3_fill_cap_state(
    path: &Path,
    state: &LiveAlphaFillCanaryCapState,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_canary_order_cap_parent(path)?;
    fs::write(path, live_fill_canary::fill_canary_cap_state_json(state)?)?;
    Ok(())
}

fn write_new_la3_fill_cap_state(
    path: &Path,
    state: &LiveAlphaFillCanaryCapState,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_canary_order_cap_parent(path)?;
    let contents = live_fill_canary::fill_canary_cap_state_json(state)?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err("LA3 fill canary cap is already reserved or consumed".into());
        }
        Err(error) => return Err(error.into()),
    };
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_lb6_live_canary(
    config: &AppConfig,
    dry_run: bool,
    human_approved: bool,
    preauthorized_envelope: bool,
    one_order: bool,
    approval_text: Option<String>,
    approval_sha256: Option<String>,
    approval_expires_at_unix: Option<u64>,
    market_slug: String,
    condition_id: String,
    token_id: String,
    outcome: String,
    side: String,
    price: f64,
    size: f64,
    notional: f64,
    order_type: String,
    post_only: bool,
    maker_only: bool,
    tick_size: f64,
    gtd_expiry_unix: u64,
    market_end_unix: u64,
    best_bid: f64,
    best_ask: f64,
    book_age_ms: u64,
    reference_age_ms: u64,
    order_cap_state: &Path,
    run_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let selected_mode_count = [dry_run, human_approved, preauthorized_envelope]
        .iter()
        .filter(|selected| **selected)
        .count();
    if selected_mode_count != 1 {
        return Err(
            "live-canary requires exactly one of --dry-run, --human-approved, or --preauthorized-envelope"
                .into(),
        );
    }
    if (human_approved || preauthorized_envelope) && !one_order {
        return Err("live-canary final modes require --one-order to enforce the canary cap".into());
    }
    if preauthorized_envelope
        && (approval_text.is_some()
            || approval_sha256.is_some()
            || approval_expires_at_unix.is_some())
    {
        return Err(
            "live-canary --preauthorized-envelope does not accept exact approval flags".into(),
        );
    }

    let mode = if human_approved {
        CanaryMode::FinalGated
    } else if preauthorized_envelope {
        CanaryMode::PreauthorizedEnvelope
    } else {
        CanaryMode::DryRun
    };
    let side = parse_canary_side(&side)?;
    let plan = CanaryOrderPlan {
        market_slug,
        condition_id,
        token_id,
        outcome,
        side,
        price,
        size,
        notional,
        order_type,
        post_only,
        maker_only,
        tick_size,
        gtd_expiry_unix,
        market_end_unix,
        best_bid,
        best_ask,
    };
    let geoblock = run_geoblock_validation(config).await?;
    let geoblock_status = if geoblock.blocked {
        CanaryGateStatus::Blocked
    } else {
        CanaryGateStatus::Passed
    };
    let preauthorized_envelope_binding = if preauthorized_envelope && !geoblock.blocked {
        discover_preauthorized_envelope_binding(config, &plan).await?
    } else {
        None
    };
    let l2_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    let canary_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.canary_secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;

    let prerequisites = lb4_readback_prerequisites(
        config,
        safety::GeoblockGateStatus::from_blocked(geoblock.blocked),
    );
    let account = lb4_account_preflight(config)?;
    let lb4_report = if prerequisites.lb3_hold_released
        && prerequisites.legal_access_approved
        && prerequisites.deployment_geoblock_passed
        && l2_secret_report.all_present()
    {
        let credentials = lb4_l2_credentials_from_env(&config.live_beta.secret_handles)?;
        live_beta_readback::authenticated_readback_preflight(AuthenticatedReadbackInput {
            prerequisites,
            account: account.clone(),
            credentials,
            required_collateral_allowance_units: config
                .live_beta
                .readback_account
                .required_collateral_allowance_units,
            request_timeout_ms: config.polymarket.request_timeout_ms,
        })
        .await?
    } else {
        live_beta_readback::sample_readback_preflight(prerequisites)?
    };
    let approval_context = CanaryApprovalContext {
        run_id: run_id.to_string(),
        host: lb6_host_label(),
        geoblock_result: geoblock_result_label(&geoblock),
        wallet_address: account.wallet_address.clone(),
        funder_address: account.funder_address.clone(),
        signature_type: account.signature_type.as_config_str().to_string(),
        available_pusd_units: lb4_report.available_pusd_units,
        reserved_pusd_units: lb4_report.reserved_pusd_units,
        fee_estimate: "0.000000 pUSD maker-only estimate; reconcile if matched".to_string(),
        book_age_ms,
        reference_age_ms,
        max_book_age_ms: config.risk.stale_book_ms,
        max_reference_age_ms: config.risk.stale_reference_ms,
        heartbeat: lb4_report.heartbeat.to_string(),
        cancel_plan: "if still open after readback, cancel only this exact order ID; no cancel-all"
            .to_string(),
        rollback_command: "LIVE_ORDER_PLACEMENT_ENABLED=false; stop service if running".to_string(),
        preauthorized_envelope_binding,
    };
    let cancel_report = live_beta_cancel::evaluate_cancel_readiness(
        &live_beta_cancel::CancelReadinessInput::lb5_default(true),
    );
    let prior_canary_submission_attempted = read_canary_order_cap_consumed(order_cap_state)?;
    let checks = CanaryRuntimeChecks {
        canary_submission_enabled: live_beta_canary::LB6_ONE_ORDER_CANARY_SUBMISSION_ENABLED,
        geoblock_status,
        lb4_account_preflight_passed: lb4_report.passed() && lb4_report.live_network_enabled,
        open_order_count: lb4_report.open_order_count,
        canary_secret_handles_present: canary_secret_report.all_present(),
        l2_secret_handles_present: l2_secret_report.all_present(),
        lb5_rollback_ready: !cancel_report.block_reasons.iter().any(|reason| {
            matches!(
                *reason,
                "cancel_plan_not_acknowledged" | "service_stop_not_ready" | "kill_switch_not_ready"
            )
        }),
        lb5_cancel_readiness_blocks_until_canary_exists: !cancel_report
            .cancel_request_constructable
            && !cancel_report.live_cancel_network_enabled
            && cancel_report
                .block_reasons
                .contains(&"approved_canary_order_missing"),
        lb6_exact_single_cancel_path_available:
            live_beta_order_lifecycle::LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED
                && !live_beta_order_lifecycle::LB6_CANCEL_ALL_ENABLED,
        official_sdk_available: true,
        previous_canary_submission_attempted: prior_canary_submission_attempted,
    };

    let approval = CanaryApprovalGuard {
        approval_text,
        expected_approval_sha256: approval_sha256,
        approval_expires_at_unix,
        now_unix: unix_time_secs(),
    };
    let report = live_beta_canary::evaluate_canary_readiness(
        mode,
        &plan,
        &approval_context,
        &approval,
        &checks,
    );

    println!("live_beta_canary_status={}", report.status);
    println!("live_beta_canary_mode={}", report.mode);
    println!(
        "live_beta_canary_submission_enabled={}",
        report.canary_submission_enabled
    );
    println!(
        "live_beta_canary_official_signing_client={}@{}",
        report.official_signing_client, report.official_signing_client_version
    );
    println!(
        "live_beta_canary_block_reasons={}",
        report.block_reasons.join(",")
    );
    println!(
        "live_beta_canary_approval_sha256={}",
        report.approval_sha256
    );
    println!("live_beta_canary_not_submitted={}", report.not_submitted);
    println!(
        "live_beta_canary_one_order_cap_remaining={}",
        report.one_order_cap_remaining
    );
    println!(
        "live_beta_canary_lb4_preflight_passed={}",
        checks.lb4_account_preflight_passed
    );
    println!(
        "live_beta_canary_open_order_count={}",
        checks.open_order_count
    );
    println!(
        "live_beta_canary_lb5_cancel_blocks_until_canary={}",
        checks.lb5_cancel_readiness_blocks_until_canary_exists
    );
    println!(
        "live_beta_canary_canary_secret_handles_present={}",
        checks.canary_secret_handles_present
    );
    println!(
        "live_beta_canary_l2_secret_handles_present={}",
        checks.l2_secret_handles_present
    );
    match mode {
        CanaryMode::DryRun | CanaryMode::FinalGated => {
            println!(
                "live_beta_canary_final_approval_prompt=\n{}",
                report.canonical_approval_text
            );
        }
        CanaryMode::PreauthorizedEnvelope => {
            println!("live_beta_canary_preauthorized_envelope_enabled=true");
        }
    }
    println!(
        "live_beta_canary_report={}",
        serde_json::to_string(&report)?
    );

    if mode == CanaryMode::DryRun {
        return Ok(());
    }
    if !report.ready_for_final_submission() {
        return Err(format!(
            "LB6 canary gate blocked: {}",
            report.block_reasons.join(",")
        )
        .into());
    }

    let submit_input = live_beta_canary::CanarySubmitInput {
        clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
        signer_handle: config.live_beta.secret_handles.canary_private_key.clone(),
        l2_access_handle: config.live_beta.secret_handles.clob_l2_access.clone(),
        l2_secret_handle: config.live_beta.secret_handles.clob_l2_credential.clone(),
        l2_passphrase_handle: config.live_beta.secret_handles.clob_l2_passphrase.clone(),
        wallet_address: account.wallet_address,
        funder_address: account.funder_address,
        signature_type: account.signature_type,
        plan,
        approval_sha256: report.approval_sha256.clone(),
    };
    live_beta_canary::validate_canary_submit_input_without_network(&submit_input)?;
    reserve_canary_order_cap(order_cap_state, &report.approval_sha256)?;
    let submission = live_beta_canary::submit_one_canary_with_official_sdk(submit_input).await?;
    update_canary_order_cap_with_order_id(
        order_cap_state,
        &report.approval_sha256,
        &submission.order_id,
    )?;
    println!(
        "live_beta_canary_submission_report={}",
        serde_json::to_string(&submission)?
    );

    Ok(())
}

async fn discover_preauthorized_envelope_binding(
    config: &AppConfig,
    plan: &CanaryOrderPlan,
) -> Result<Option<PreauthorizedEnvelopeBinding>, Box<dyn std::error::Error>> {
    let discovery = MarketDiscoveryClient::new(
        &config.polymarket.gamma_markets_url,
        &config.polymarket.clob_rest_url,
        config.polymarket.market_discovery_page_limit,
        config.polymarket.market_discovery_max_pages,
        config.polymarket.request_timeout_ms,
    )?;
    let Some(market) = discovery
        .discover_crypto_15m_market_by_slug(&plan.market_slug)
        .await?
    else {
        return Ok(None);
    };
    if market.asset != Asset::Eth
        || market.lifecycle_state != MarketLifecycleState::Active
        || market.ineligibility_reason.is_some()
    {
        return Ok(None);
    }
    let Some(up_token) = market
        .outcomes
        .iter()
        .find(|outcome| outcome.outcome.eq_ignore_ascii_case("Up"))
    else {
        return Ok(None);
    };

    Ok(Some(PreauthorizedEnvelopeBinding {
        market_slug: market.slug.clone(),
        condition_id: market.condition_id.clone(),
        up_token_id: up_token.token_id.clone(),
    }))
}

#[allow(clippy::too_many_arguments)]
async fn run_lb6_live_cancel(
    config: &AppConfig,
    dry_run: bool,
    human_approved: bool,
    one_order: bool,
    order_id: String,
    canary_approval_sha256: String,
    approval_expires_at_unix: Option<u64>,
    condition_id: String,
    token_id: String,
    side: String,
    price: f64,
    size: f64,
    order_type: String,
    order_cap_state: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if dry_run == human_approved {
        return Err("live-cancel requires exactly one of --dry-run or --human-approved".into());
    }
    if human_approved && !one_order {
        return Err("live-cancel --human-approved requires --one-order".into());
    }
    if human_approved && approval_expires_at_unix.is_none() {
        return Err("live-cancel --human-approved requires --approval-expires-at-unix".into());
    }
    if let Some(expires_at) = approval_expires_at_unix {
        if expires_at <= unix_time_secs() {
            return Err("live-cancel approval has expired".into());
        }
    }

    let side = parse_canary_side(&side)?;
    let size_units = decimal_to_fixed6_units(size, "size")?;
    let geoblock = run_geoblock_validation(config).await?;
    let l2_secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    let account = lb4_account_preflight(config)?;
    let cap_state = read_canary_order_cap_state(order_cap_state)?;
    let cap_matches =
        canary_order_cap_matches(cap_state.as_ref(), &order_id, &canary_approval_sha256);

    if human_approved {
        let state = cap_state
            .as_ref()
            .ok_or("LB6 one-order canary cap state is missing; refusing live cancel")?;
        if !state.submission_attempted {
            return Err("LB6 one-order canary cap state has no submission attempt".into());
        }
        let recorded_order_id = state
            .venue_order_id
            .as_deref()
            .ok_or("LB6 one-order canary cap state has no venue order ID")?;
        if !recorded_order_id.eq_ignore_ascii_case(&order_id) {
            return Err("live-cancel order ID does not match the recorded canary order ID".into());
        }
        if state.approval_sha256 != canary_approval_sha256 {
            return Err("live-cancel approval hash does not match canary cap state".into());
        }
    }

    let authenticated_readback_available = !geoblock.blocked && l2_secret_report.all_present();
    if !authenticated_readback_available {
        return Err("live-cancel requires geoblock PASS and all L2 secret handles present".into());
    }

    let expected = ExpectedCanaryOrder {
        order_id: order_id.clone(),
        approval_sha256: canary_approval_sha256,
        funder_address: account.funder_address.clone(),
        condition_id,
        token_id,
        side,
        price: decimal_arg_label(price),
        size_units,
        order_type,
    };
    let checks = ExactCancelRuntimeChecks {
        geoblock_passed: !geoblock.blocked,
        authenticated_readback_available,
        l2_secret_handles_present: l2_secret_report.all_present(),
        human_cancel_approved: human_approved && one_order && cap_matches,
        cancel_plan_acknowledged: true,
        kill_switch_ready: config.live_beta.kill_switch_active
            && !safety::LIVE_ORDER_PLACEMENT_ENABLED,
        service_stop_ready: true,
    };
    let credentials = lb4_l2_credentials_from_env(&config.live_beta.secret_handles)?;

    if dry_run {
        let order = live_beta_order_lifecycle::read_exact_order(ExactOrderReadbackInput {
            clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
            account,
            credentials,
            order_id: expected.order_id.clone(),
            request_timeout_ms: config.polymarket.request_timeout_ms,
        })
        .await?;
        let report =
            live_beta_order_lifecycle::evaluate_exact_cancel_readiness(&order, &expected, &checks);
        println!("live_beta_exact_cancel_status={}", report.status);
        println!("live_beta_exact_cancel_mode=dry_run");
        println!(
            "live_beta_exact_cancel_live_network_enabled={}",
            report.live_cancel_network_enabled
        );
        println!(
            "live_beta_exact_cancel_cancel_all_enabled={}",
            report.cancel_all_enabled
        );
        println!(
            "live_beta_exact_cancel_block_reasons={}",
            report.block_reasons.join(",")
        );
        println!(
            "live_beta_exact_cancel_order_status={}",
            report.pre_cancel_order_status
        );
        println!(
            "live_beta_exact_cancel_report={}",
            serde_json::to_string(&report)?
        );
        return Ok(());
    }

    let report = live_beta_order_lifecycle::cancel_exact_single_order(ExactCancelInput {
        clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
        account,
        credentials,
        expected,
        checks,
        request_timeout_ms: config.polymarket.request_timeout_ms,
    })
    .await?;
    println!("live_beta_exact_cancel_status={}", report.status);
    println!("live_beta_exact_cancel_mode=final_gated");
    println!(
        "live_beta_exact_cancel_cancel_attempted={}",
        report.cancel_attempted
    );
    println!(
        "live_beta_exact_cancel_block_reasons={}",
        report.block_reasons.join(",")
    );
    println!(
        "live_beta_exact_cancel_report={}",
        serde_json::to_string(&report)?
    );
    if report.status != "canceled" {
        return Err(format!(
            "LB6 exact single-order cancel blocked: {}",
            report.block_reasons.join(",")
        )
        .into());
    }

    Ok(())
}

async fn run_lb4_readback_preflight_validation(
    config: &AppConfig,
    geoblock_gate_status: safety::GeoblockGateStatus,
    local_only: bool,
) -> Result<ReadbackPreflightValidation, Box<dyn std::error::Error>> {
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
    let validation = if local_only || !lb4_prerequisites_ready(prerequisites) {
        ReadbackPreflightValidation::from_report(live_beta_readback::sample_readback_preflight(
            prerequisites,
        )?)
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
        ReadbackPreflightValidation::from_authenticated_evidence(
            live_beta_readback::authenticated_readback_preflight_evidence(
                AuthenticatedReadbackInput {
                    prerequisites,
                    account,
                    credentials,
                    required_collateral_allowance_units: config
                        .live_beta
                        .readback_account
                        .required_collateral_allowance_units,
                    request_timeout_ms: config.polymarket.request_timeout_ms,
                },
            )
            .await?,
        )
    };
    let report = &validation.report;
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
    if let Some(collateral) = &validation.collateral {
        println!(
            "live_beta_readback_preflight_funder_allowance_units={}",
            collateral.allowance_units
        );
    }
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

    Ok(validation)
}

#[derive(Debug, Clone)]
struct ReadbackPreflightValidation {
    report: ReadbackPreflightReport,
    collateral: Option<BalanceAllowanceReadback>,
    open_orders: Vec<OpenOrderReadback>,
    trades: Vec<TradeReadback>,
}

impl ReadbackPreflightValidation {
    fn from_report(report: ReadbackPreflightReport) -> Self {
        Self {
            report,
            collateral: None,
            open_orders: Vec::new(),
            trades: Vec::new(),
        }
    }

    fn from_authenticated_evidence(evidence: AuthenticatedReadbackPreflightEvidence) -> Self {
        Self {
            report: evidence.report,
            collateral: Some(evidence.collateral),
            open_orders: evidence.open_orders,
            trades: evidence.trades,
        }
    }
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
        clob_host: normalize_lb4_clob_host(&config.polymarket.clob_rest_url),
        chain_id: 137,
        wallet_address: account.wallet_address.clone(),
        funder_address: account.funder_address.clone(),
        signature_type,
    })
}

fn normalize_lb4_clob_host(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(parsed) = url::Url::parse(trimmed) else {
        return trimmed.trim_end_matches('/').to_string();
    };
    if parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return trimmed.trim_end_matches('/').to_string();
    }
    parsed.to_string().trim_end_matches('/').to_string()
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

fn parse_canary_side(value: &str) -> Result<Side, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_uppercase().as_str() {
        "BUY" => Ok(Side::Buy),
        "SELL" => Ok(Side::Sell),
        _ => Err("LB6 canary side must be BUY or SELL".into()),
    }
}

fn geoblock_result_label(geoblock: &GeoblockResponse) -> String {
    let status = if geoblock.blocked {
        "blocked"
    } else {
        "passed"
    };
    format!(
        "status={},country={},region={}",
        status,
        geoblock.country.as_deref().unwrap_or("unknown"),
        geoblock.region.as_deref().unwrap_or("unknown")
    )
}

fn lb6_host_label() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn read_canary_order_cap_consumed(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(read_canary_order_cap_state(path)?
        .map(|state| state.submission_attempted)
        .unwrap_or(false))
}

fn read_canary_order_cap_state(
    path: &Path,
) -> Result<Option<CanaryOrderCapState>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path)?;
    Ok(Some(live_beta_canary::canary_order_cap_state_from_json(
        &contents,
    )?))
}

fn canary_order_cap_matches(
    state: Option<&CanaryOrderCapState>,
    order_id: &str,
    approval_sha256: &str,
) -> bool {
    let Some(state) = state else {
        return false;
    };
    state.submission_attempted
        && state.approval_sha256 == approval_sha256
        && state
            .venue_order_id
            .as_deref()
            .is_some_and(|recorded| recorded.eq_ignore_ascii_case(order_id))
}

fn reserve_canary_order_cap(
    path: &Path,
    approval_sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = CanaryOrderCapState {
        submission_attempted: true,
        approval_sha256: approval_sha256.to_string(),
        reserved_at_unix: unix_time_secs(),
        venue_order_id: None,
    };
    write_new_canary_order_cap_state(path, &state)
}

fn update_canary_order_cap_with_order_id(
    path: &Path,
    approval_sha256: &str,
    venue_order_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_canary_order_cap_state(
        path,
        &CanaryOrderCapState {
            submission_attempted: true,
            approval_sha256: approval_sha256.to_string(),
            reserved_at_unix: unix_time_secs(),
            venue_order_id: Some(venue_order_id.to_string()),
        },
    )
}

fn write_canary_order_cap_state(
    path: &Path,
    state: &CanaryOrderCapState,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_canary_order_cap_parent(path)?;
    fs::write(path, live_beta_canary::canary_order_cap_state_json(state)?)?;
    Ok(())
}

fn write_new_canary_order_cap_state(
    path: &Path,
    state: &CanaryOrderCapState,
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_canary_order_cap_parent(path)?;
    let contents = live_beta_canary::canary_order_cap_state_json(state)?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err("LB6 one-order canary cap is already reserved or consumed".into());
        }
        Err(error) => return Err(error.into()),
    };
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn ensure_canary_order_cap_parent(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn decimal_to_fixed6_units(value: f64, label: &str) -> Result<u64, Box<dyn std::error::Error>> {
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("LB6 {label} must be positive and finite").into());
    }
    let units = (value * 1_000_000.0).round();
    if units <= 0.0 || units > u64::MAX as f64 {
        return Err(format!("LB6 {label} fixed-unit conversion overflowed").into());
    }
    Ok(units as u64)
}

fn decimal_arg_label(value: f64) -> String {
    let mut rendered = format!("{value:.6}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    rendered
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

async fn shadow_live_readback_validation_for_paper(
    config: &AppConfig,
    geoblock_gate_status: safety::GeoblockGateStatus,
) -> Result<Option<ReadbackPreflightValidation>, Box<dyn std::error::Error>> {
    if geoblock_gate_status != safety::GeoblockGateStatus::Passed {
        return Ok(None);
    }

    let prerequisites = lb4_readback_prerequisites(config, geoblock_gate_status);
    if !lb4_prerequisites_ready(prerequisites) {
        return Ok(Some(ReadbackPreflightValidation::from_report(
            live_beta_readback::sample_readback_preflight(prerequisites)?,
        )));
    }

    let secret_report = secret_handling::validate_secret_presence(
        &config.live_beta.secret_inventory(),
        &EnvSecretPresenceProvider,
    )?;
    if !secret_report.all_present() {
        return Ok(Some(ReadbackPreflightValidation::from_report(
            live_beta_readback::sample_readback_preflight(prerequisites)?,
        )));
    }

    Ok(Some(live_alpha_authenticated_readback(config).await?))
}

fn shadow_live_runtime_readiness_for_paper(
    config: &AppConfig,
    run_id: &str,
    checked_at_ms: i64,
    geoblock_status: safety::GeoblockGateStatus,
    readback_validation: Option<&ReadbackPreflightValidation>,
) -> ShadowLiveRuntimeReadiness {
    let startup_recovery_input = live_alpha_startup_recovery_input_for_validate(
        config,
        run_id,
        checked_at_ms,
        geoblock_status,
        readback_validation,
    );
    let startup_recovery = live_startup_recovery::evaluate_startup_recovery(startup_recovery_input);

    ShadowLiveRuntimeReadiness {
        geoblock_passed: geoblock_status == safety::GeoblockGateStatus::Passed,
        heartbeat_healthy: shadow_live_heartbeat_healthy_for_paper(config, readback_validation),
        reconciliation_clean: startup_recovery.status == LiveStartupRecoveryStatus::Passed,
    }
}

fn shadow_live_heartbeat_healthy_for_paper(
    config: &AppConfig,
    readback_validation: Option<&ReadbackPreflightValidation>,
) -> bool {
    if !config.live_alpha.heartbeat_required {
        return true;
    }

    let Some(validation) = readback_validation else {
        return false;
    };
    let report = &validation.report;
    if !report.live_network_enabled
        || report
            .block_reasons
            .iter()
            .any(|reason| reason.starts_with("heartbeat_"))
    {
        return false;
    }

    match report.heartbeat {
        value if value == live_beta_readback::HeartbeatReadiness::Healthy.as_str() => true,
        value
            if value == live_beta_readback::HeartbeatReadiness::NotStartedNoOpenOrders.as_str() =>
        {
            report.open_order_count == 0
        }
        _ => false,
    }
}

fn live_alpha_startup_recovery_input_for_validate(
    config: &AppConfig,
    run_id: &str,
    checked_at_ms: i64,
    geoblock_status: safety::GeoblockGateStatus,
    readback_validation: Option<&ReadbackPreflightValidation>,
) -> LiveStartupRecoveryInput {
    if !config.live_alpha.enabled || config.live_alpha.mode == LiveAlphaMode::Disabled {
        return LiveStartupRecoveryInput::disabled(run_id, checked_at_ms);
    }

    let readback_report = readback_validation.map(|validation| &validation.report);
    let readback_status = startup_check_from_readback_report(readback_report);
    let journal_recovery = live_alpha_journal_recovery_evidence(config);
    let reconciliation_input = live_alpha_reconciliation_input_for_validate(
        run_id,
        checked_at_ms,
        readback_validation,
        journal_recovery.local_state,
    );

    LiveStartupRecoveryInput {
        run_id: run_id.to_string(),
        checked_at_ms,
        live_alpha_enabled: config.live_alpha.enabled,
        live_alpha_mode: config.live_alpha.mode,
        geoblock_status,
        account_preflight_status: readback_status,
        balance_allowance_status: readback_status,
        open_orders_readback_status: readback_status,
        recent_trades_readback_status: readback_status,
        journal_replay_status: journal_recovery.journal_replay_status,
        position_reconstruction_status: journal_recovery.position_reconstruction_status,
        reconciliation_input,
    }
}

#[derive(Debug, Clone)]
struct LiveAlphaJournalRecoveryEvidence {
    journal_replay_status: StartupRecoveryCheckStatus,
    position_reconstruction_status: StartupRecoveryCheckStatus,
    local_state: Option<LocalLiveState>,
}

fn live_alpha_journal_recovery_evidence(config: &AppConfig) -> LiveAlphaJournalRecoveryEvidence {
    let Some(journal_path) = config.live_alpha.journal_path() else {
        return LiveAlphaJournalRecoveryEvidence {
            journal_replay_status: StartupRecoveryCheckStatus::Unknown,
            position_reconstruction_status: StartupRecoveryCheckStatus::Unknown,
            local_state: None,
        };
    };
    let path = Path::new(journal_path);
    if !path.exists() {
        return LiveAlphaJournalRecoveryEvidence {
            journal_replay_status: StartupRecoveryCheckStatus::Failed,
            position_reconstruction_status: StartupRecoveryCheckStatus::Failed,
            local_state: None,
        };
    }

    let journal = LiveOrderJournal::new(path);
    match journal
        .replay()
        .and_then(|events| reduce_live_journal_events(&events))
    {
        Ok(state) => LiveAlphaJournalRecoveryEvidence {
            journal_replay_status: StartupRecoveryCheckStatus::Passed,
            position_reconstruction_status: StartupRecoveryCheckStatus::Passed,
            local_state: Some(LocalLiveState::from(&state)),
        },
        Err(_) => LiveAlphaJournalRecoveryEvidence {
            journal_replay_status: StartupRecoveryCheckStatus::Failed,
            position_reconstruction_status: StartupRecoveryCheckStatus::Failed,
            local_state: None,
        },
    }
}

fn live_alpha_reconciliation_input_for_validate(
    run_id: &str,
    checked_at_ms: i64,
    readback_validation: Option<&ReadbackPreflightValidation>,
    local: Option<LocalLiveState>,
) -> Option<LiveReconciliationInput> {
    let validation = readback_validation?;
    let local = local?;
    let collateral = validation.collateral.as_ref()?;
    if !validation.report.live_network_enabled || !validation.report.passed() {
        return None;
    }
    if validation.report.open_order_count != validation.open_orders.len()
        || validation.report.trade_count != validation.trades.len()
    {
        return None;
    }
    let local = local_state_for_startup_reconciliation_scope(
        local,
        &validation.open_orders,
        &validation.trades,
    );

    let venue = live_startup_recovery::venue_state_from_readback(
        &validation.open_orders,
        &validation.trades,
        Some(balance_snapshot_from_readback(
            &validation.report,
            collateral,
            checked_at_ms,
        )),
        LivePositionBook::new(),
    );

    Some(LiveReconciliationInput {
        run_id: run_id.to_string(),
        checked_at_ms,
        local,
        venue,
        venue_position_evidence_complete: false,
    })
}

fn local_state_for_startup_reconciliation_scope(
    mut local: LocalLiveState,
    open_orders: &[OpenOrderReadback],
    trades: &[TradeReadback],
) -> LocalLiveState {
    let open_order_ids = open_orders
        .iter()
        .map(|order| order.id.clone())
        .collect::<BTreeSet<_>>();
    let venue_trade_ids = trades.iter().map(|t| t.id.clone()).collect::<BTreeSet<_>>();
    local
        .known_orders
        .retain(|order_id| open_order_ids.contains(order_id));
    local
        .canceled_orders
        .retain(|order_id| open_order_ids.contains(order_id));
    local
        .partially_filled_orders
        .retain(|order_id| open_order_ids.contains(order_id));

    // Preflight trade readback is a bounded window; journal may contain older confirmed trades.
    // Reconcile only against trade IDs present in this readback snapshot.
    local
        .trade_order_ids_by_trade
        .retain(|trade_id, _| venue_trade_ids.contains(trade_id));
    local.known_trades = local.trade_order_ids_by_trade.keys().cloned().collect();
    local.trade_order_ids = local.trade_order_ids_by_trade.values().cloned().collect();
    local
}

fn balance_snapshot_from_readback(
    report: &ReadbackPreflightReport,
    collateral: &BalanceAllowanceReadback,
    checked_at_ms: i64,
) -> LiveBalanceSnapshot {
    LiveBalanceSnapshot {
        p_usd_available: fixed6_units_to_decimal(report.available_pusd_units),
        p_usd_reserved: fixed6_units_to_decimal(report.reserved_pusd_units),
        p_usd_total: fixed6_units_to_decimal(collateral.balance_units),
        conditional_token_positions: BTreeMap::new(),
        conditional_token_positions_evidence_complete: false,
        balance_snapshot_at: checked_at_ms,
        source: "live_readback_preflight".to_string(),
    }
}

fn fixed6_units_to_decimal(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

fn startup_check_from_readback_report(
    readback_report: Option<&ReadbackPreflightReport>,
) -> StartupRecoveryCheckStatus {
    match readback_report {
        Some(report) if report.live_network_enabled && report.passed() => {
            StartupRecoveryCheckStatus::Passed
        }
        Some(report) if report.live_network_enabled => StartupRecoveryCheckStatus::Failed,
        _ => StartupRecoveryCheckStatus::Unknown,
    }
}

fn readiness_from_startup_check(status: StartupRecoveryCheckStatus) -> LiveAlphaReadinessStatus {
    match status {
        StartupRecoveryCheckStatus::Passed => LiveAlphaReadinessStatus::Passed,
        StartupRecoveryCheckStatus::Failed => LiveAlphaReadinessStatus::Failed,
        StartupRecoveryCheckStatus::Unknown => LiveAlphaReadinessStatus::Unknown,
    }
}

fn reconciliation_readiness_from_startup_recovery(
    report: &LiveStartupRecoveryReport,
) -> LiveAlphaReadinessStatus {
    match report.status {
        LiveStartupRecoveryStatus::Passed => LiveAlphaReadinessStatus::Passed,
        LiveStartupRecoveryStatus::Skipped => LiveAlphaReadinessStatus::Unknown,
        LiveStartupRecoveryStatus::HaltRequired
            if report
                .block_reasons
                .contains(&LiveStartupRecoveryBlockReason::ReconciliationFailed) =>
        {
            LiveAlphaReadinessStatus::Failed
        }
        LiveStartupRecoveryStatus::HaltRequired => LiveAlphaReadinessStatus::Unknown,
    }
}

fn persist_startup_recovery_journal_events(
    config: &AppConfig,
    report: &LiveStartupRecoveryReport,
) -> Result<(), Box<dyn std::error::Error>> {
    if report.journal_event_types.is_empty() {
        return Ok(());
    }

    let journal_path = config.live_alpha.journal_path().ok_or_else(|| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            "live_alpha.journal_path is required to persist startup recovery journal events",
        )
    })?;
    let journal = LiveOrderJournal::new(Path::new(journal_path));
    let block_reasons = report
        .block_reasons
        .iter()
        .map(|reason| reason.as_str())
        .collect::<Vec<_>>();
    let reconciliation_mismatches = report
        .reconciliation_mismatches
        .iter()
        .map(|mismatch| mismatch.as_str())
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "startup_recovery_status": report.status_str(),
        "block_reasons": block_reasons,
        "reconciliation_mismatches": reconciliation_mismatches,
    });

    for (index, event_type) in report.journal_event_types.iter().copied().enumerate() {
        let event = polymarket_15m_arb_bot::live_order_journal::LiveJournalEvent::new(
            report.run_id.clone(),
            format!("{}-startup-recovery-{index}", report.run_id),
            event_type,
            report.checked_at_ms,
            payload.clone(),
        );
        journal.append(&event)?;
    }

    Ok(())
}

fn live_journal_event_type_list(
    event_types: &[polymarket_15m_arb_bot::live_order_journal::LiveJournalEventType],
) -> String {
    event_types
        .iter()
        .map(|event_type| {
            serde_json::to_string(event_type)
                .map(|encoded| encoded.trim_matches('"').to_string())
                .unwrap_or_else(|_| format!("{event_type:?}"))
        })
        .collect::<Vec<_>>()
        .join(",")
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

fn unix_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
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
    use polymarket_15m_arb_bot::live_order_journal::{LiveJournalEvent, LiveJournalEventType};
    use polymarket_15m_arb_bot::live_reconciliation::LiveReconciliationMismatch;

    #[test]
    fn paper_capture_allows_quiet_clob_websocket_when_snapshots_are_recorded() {
        assert!(!paper_probe_requires_normalized_events(
            SOURCE_POLYMARKET_CLOB
        ));
        assert!(paper_probe_requires_normalized_events(SOURCE_BINANCE));
        assert!(paper_probe_requires_normalized_events(SOURCE_COINBASE));
    }

    #[test]
    fn paper_shadow_live_alpha_flag_parses_without_live_order_enablement() {
        let cli = Cli::try_parse_from([
            "polymarket-15m-arb-bot",
            "--config",
            "config/default.toml",
            "paper",
            "--shadow-live-alpha",
        ])
        .expect("shadow-live paper flag parses");

        match cli.command {
            Commands::Paper {
                shadow_live_alpha,
                deterministic_fixture,
                ..
            } => {
                assert!(shadow_live_alpha);
                assert!(!deterministic_fixture);
            }
            other => panic!("expected paper command, got {other:?}"),
        }
        assert!(!safety::LIVE_ORDER_PLACEMENT_ENABLED);
    }

    #[test]
    fn shadow_live_outputs_always_include_session_journal() {
        let run_id = "la4-shadow-journal-test";
        let unique = monotonic_like_ns();
        let root = std::env::temp_dir().join(format!("p15m-la4-shadow-{unique}"));
        let storage = FileSessionStorage::for_run(&root, run_id).expect("storage scopes to run");
        let config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");

        let report_path = persist_shadow_live_outputs(
            &storage,
            run_id,
            &config,
            &[sample_shadow_decision()],
            0,
            0,
        )
        .expect("shadow outputs persist");
        let journal_path = storage
            .session_dir(run_id)
            .expect("session dir resolves")
            .join("shadow_live_journal.jsonl");

        assert!(report_path.ends_with("shadow_live_report.json"));
        let journal = std::fs::read_to_string(&journal_path).expect("journal artifact reads");
        assert!(journal.contains("\"event_type\":\"live_shadow_decision_recorded\""));
        assert!(journal.contains("\"shadow_decision_id\":\"shadow-decision-1\""));

        let _ = std::fs::remove_dir_all(root);
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
    fn la3_reference_evidence_accepts_rtds_chainlink_for_requested_asset() {
        let recv_wall_ts = 1_777_911_000_010;
        let source_ts = 1_777_911_000_000;
        let events = vec![NormalizedEvent::ReferenceTick {
            price: ReferencePrice {
                asset: Asset::Eth,
                source: Asset::Eth.chainlink_resolution_source().to_string(),
                price: 3_240.12,
                confidence: None,
                provider: Some(PROVIDER_POLYMARKET_RTDS_CHAINLINK.to_string()),
                matches_market_resolution_source: Some(true),
                source_ts: Some(source_ts),
                recv_wall_ts,
            },
        }];

        let evidence = live_alpha_reference_evidence_from_events(events, Asset::Eth);

        assert_eq!(
            evidence.snapshot_id.as_deref(),
            Some("https://data.chain.link/streams/eth-usd:polymarket_rtds_chainlink:1777911000000")
        );
        assert_eq!(evidence.age_ms, Some(10));
    }

    #[test]
    fn la3_approval_asset_symbol_is_not_hardcoded_to_btc() {
        assert_eq!(
            live_alpha_asset_from_symbol("ETH").expect("ETH parses"),
            Asset::Eth
        );
        assert_eq!(
            live_alpha_asset_from_symbol("sol").expect("SOL parses"),
            Asset::Sol
        );
        assert!(live_alpha_asset_from_symbol("XRP").is_err());
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

    #[test]
    fn lb4_account_preflight_normalizes_clob_host_before_gate_evaluation() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.polymarket.clob_rest_url = " HTTPS://CLOB.POLYMARKET.COM:443/ ".to_string();
        config.live_beta.readback_account.wallet_address =
            "0x1111111111111111111111111111111111111111".to_string();
        config.live_beta.readback_account.funder_address =
            "0x1111111111111111111111111111111111111111".to_string();
        config.live_beta.readback_account.signature_type = "eoa".to_string();

        let account = lb4_account_preflight(&config).expect("account preflight builds");

        assert_eq!(account.clob_host, live_beta_readback::CLOB_HOST);
        let report = live_beta_readback::evaluate_readback_preflight(
            &live_beta_readback::ReadbackPreflightInput {
                prerequisites: ReadbackPrerequisites {
                    lb3_hold_released: true,
                    legal_access_approved: true,
                    deployment_geoblock_passed: true,
                },
                account,
                venue_state: live_beta_readback::VenueState::TradingEnabled,
                collateral: live_beta_readback::BalanceAllowanceReadback {
                    asset_type: live_beta_readback::AssetType::Collateral,
                    token_id: None,
                    balance_units: 25_000_000,
                    allowance_units: 25_000_000,
                },
                open_orders: Vec::new(),
                trades: Vec::new(),
                heartbeat: live_beta_readback::HeartbeatReadiness::NotStartedNoOpenOrders,
                required_collateral_allowance_units: 1_000_000,
            },
        )
        .expect("report builds");

        assert_eq!(report.status, "passed");
        assert!(!report.block_reasons.contains(&"clob_host_mismatch"));
    }

    #[test]
    fn startup_recovery_validate_path_halts_non_disabled_live_alpha_without_recovery_evidence() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            None,
        );
        assert_eq!(
            input.open_orders_readback_status,
            StartupRecoveryCheckStatus::Unknown
        );

        let report = live_startup_recovery::evaluate_startup_recovery(input);

        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::OpenOrdersReadbackUnknown));
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::JournalReplayUnknown));
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::ReconciliationUnknown));
        assert_eq!(
            reconciliation_readiness_from_startup_recovery(&report),
            LiveAlphaReadinessStatus::Unknown
        );
    }

    #[test]
    fn startup_recovery_validate_path_persists_journal_events() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-startup-recovery-events-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();
        let report = LiveStartupRecoveryReport {
            run_id: "test-run".to_string(),
            checked_at_ms: 1_000,
            status: LiveStartupRecoveryStatus::HaltRequired,
            block_reasons: vec![
                LiveStartupRecoveryBlockReason::JournalReplayFailed,
                LiveStartupRecoveryBlockReason::ReconciliationFailed,
            ],
            reconciliation_mismatches: vec![LiveReconciliationMismatch::UnknownOpenOrder],
            journal_event_types: vec![
                LiveJournalEventType::LiveStartupRecoveryStarted,
                LiveJournalEventType::LiveStartupRecoveryFailed,
                LiveJournalEventType::LiveRiskHalt,
            ],
        };

        persist_startup_recovery_journal_events(&config, &report)
            .expect("startup recovery journal events persist");

        let events = LiveOrderJournal::new(&path)
            .replay()
            .expect("journal replays");
        assert_eq!(events.len(), 3);
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![
                LiveJournalEventType::LiveStartupRecoveryStarted,
                LiveJournalEventType::LiveStartupRecoveryFailed,
                LiveJournalEventType::LiveRiskHalt,
            ]
        );
        assert!(events.iter().all(|event| event.run_id == "test-run"));
        assert!(events.iter().all(|event| event.created_at == 1_000));
        assert_eq!(
            events[2].payload["startup_recovery_status"].as_str(),
            Some("halt_required")
        );
        assert_eq!(
            events[2].payload["block_reasons"]
                .as_array()
                .expect("block reasons array")
                .iter()
                .filter_map(|reason| reason.as_str())
                .collect::<Vec<_>>(),
            vec!["journal_replay_failed", "reconciliation_failed"]
        );
        assert_eq!(
            events[2].payload["reconciliation_mismatches"]
                .as_array()
                .expect("mismatches array")
                .iter()
                .filter_map(|mismatch| mismatch.as_str())
                .collect::<Vec<_>>(),
            vec!["unknown_open_order"]
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_validate_path_does_not_treat_local_readback_sample_as_live_evidence() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let report = ReadbackPreflightReport {
            status: "passed",
            block_reasons: Vec::new(),
            open_order_count: 0,
            trade_count: 0,
            reserved_pusd_units: 0,
            required_collateral_allowance_units: 1_000_000,
            available_pusd_units: 1_000_000,
            venue_state: "trading_enabled",
            heartbeat: "not_started_no_open_orders",
            live_network_enabled: false,
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&ReadbackPreflightValidation::from_report(report)),
        );

        assert_eq!(
            input.account_preflight_status,
            StartupRecoveryCheckStatus::Unknown
        );
        assert_eq!(
            input.open_orders_readback_status,
            StartupRecoveryCheckStatus::Unknown
        );
    }

    #[test]
    fn startup_recovery_validate_path_uses_approved_live_readback_status_without_faking_reconcile()
    {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let report = ReadbackPreflightReport {
            status: "passed",
            block_reasons: Vec::new(),
            open_order_count: 0,
            trade_count: 0,
            reserved_pusd_units: 0,
            required_collateral_allowance_units: 1_000_000,
            available_pusd_units: 1_000_000,
            venue_state: "trading_enabled",
            heartbeat: "not_started_no_open_orders",
            live_network_enabled: true,
        };
        let validation = ReadbackPreflightValidation {
            report,
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        assert_eq!(
            input.account_preflight_status,
            StartupRecoveryCheckStatus::Passed
        );
        assert_eq!(
            input.open_orders_readback_status,
            StartupRecoveryCheckStatus::Passed
        );
        assert_eq!(
            input.journal_replay_status,
            StartupRecoveryCheckStatus::Unknown
        );
        assert!(input.reconciliation_input.is_none());
    }

    #[test]
    fn startup_recovery_validate_path_replays_journal_and_reconciles_live_readback() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-startup-recovery-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let journal = LiveOrderJournal::new(&path);
        journal
            .append(
                &polymarket_15m_arb_bot::live_order_journal::LiveJournalEvent::new(
                    "previous-run",
                    "balance-1",
                    polymarket_15m_arb_bot::live_order_journal::LiveJournalEventType::LiveBalanceSnapshot,
                    900,
                    serde_json::json!({
                        "p_usd_available": 1.0,
                        "p_usd_reserved": 0.0,
                        "p_usd_total": 1.0,
                        "conditional_token_positions": {},
                        "balance_snapshot_at": 900,
                        "source": "fixture"
                    }),
                ),
            )
            .expect("journal event appends");

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        assert_eq!(
            input.journal_replay_status,
            StartupRecoveryCheckStatus::Passed
        );
        assert_eq!(
            input.position_reconstruction_status,
            StartupRecoveryCheckStatus::Passed
        );
        assert!(input.reconciliation_input.is_some());

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::Passed);
        assert_eq!(
            reconciliation_readiness_from_startup_recovery(&report),
            LiveAlphaReadinessStatus::Passed
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn shadow_live_runtime_readiness_uses_live_readback_startup_and_heartbeat_state() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::Shadow;
        let path = std::env::temp_dir().join(format!(
            "p15m-la4-shadow-readiness-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        LiveOrderJournal::new(&path)
            .append(
                &polymarket_15m_arb_bot::live_order_journal::LiveJournalEvent::new(
                    "previous-run",
                    "balance-1",
                    polymarket_15m_arb_bot::live_order_journal::LiveJournalEventType::LiveBalanceSnapshot,
                    900,
                    serde_json::json!({
                        "p_usd_available": 1.0,
                        "p_usd_reserved": 0.0,
                        "p_usd_total": 1.0,
                        "conditional_token_positions": {},
                        "balance_snapshot_at": 900,
                        "source": "fixture"
                    }),
                ),
            )
            .expect("journal event appends");
        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let readiness = shadow_live_runtime_readiness_for_paper(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        assert_eq!(readiness, ShadowLiveRuntimeReadiness::passed());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn shadow_live_runtime_readiness_fails_closed_without_live_readback_evidence() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::Shadow;
        let validation = ReadbackPreflightValidation::from_report(ReadbackPreflightReport {
            status: "passed",
            block_reasons: Vec::new(),
            open_order_count: 0,
            trade_count: 0,
            reserved_pusd_units: 0,
            required_collateral_allowance_units: 1_000_000,
            available_pusd_units: 1_000_000,
            venue_state: "trading_enabled",
            heartbeat: "not_started_no_open_orders",
            live_network_enabled: false,
        });

        let readiness = shadow_live_runtime_readiness_for_paper(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        assert!(readiness.geoblock_passed);
        assert!(!readiness.heartbeat_healthy);
        assert!(!readiness.reconciliation_clean);
    }

    #[test]
    fn startup_recovery_validate_path_halts_missing_venue_position_evidence_not_spurious_position_mismatch(
    ) {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-missing-venue-pos-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let position = polymarket_15m_arb_bot::live_position_book::LivePosition {
            key: polymarket_15m_arb_bot::live_position_book::LivePositionKey {
                market_id: "market-1".to_string(),
                token_id: "token-up".to_string(),
                asset: Asset::Btc,
                outcome: "Up".to_string(),
            },
            size: 5.0,
            average_price: 0.42,
            fees_paid: 0.0,
            updated_at: 901,
        };

        let journal = LiveOrderJournal::new(&path);
        journal
            .append(&LiveJournalEvent::new(
                "previous-run",
                "balance-1",
                LiveJournalEventType::LiveBalanceSnapshot,
                900,
                serde_json::json!({
                    "p_usd_available": 1.0,
                    "p_usd_reserved": 0.0,
                    "p_usd_total": 1.0,
                    "conditional_token_positions": {},
                    "balance_snapshot_at": 900,
                    "source": "fixture"
                }),
            ))
            .expect("append balance");
        journal
            .append(&LiveJournalEvent::new(
                "previous-run",
                "pos-1",
                LiveJournalEventType::LivePositionOpened,
                901,
                serde_json::to_value(&position).expect("position json"),
            ))
            .expect("append position");

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::ReconciliationFailed));
        assert!(
            report
                .reconciliation_mismatches
                .contains(&LiveReconciliationMismatch::MissingVenuePositionEvidence),
            "expected missing position evidence, got {}",
            report
                .reconciliation_mismatches
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(!report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::PositionMismatch));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_validate_path_halts_missing_conditional_balance_evidence_not_spurious_drift(
    ) {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-missing-cond-bal-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let journal = LiveOrderJournal::new(&path);
        journal
            .append(&LiveJournalEvent::new(
                "previous-run",
                "balance-1",
                LiveJournalEventType::LiveBalanceSnapshot,
                900,
                serde_json::json!({
                    "p_usd_available": 1.0,
                    "p_usd_reserved": 0.0,
                    "p_usd_total": 1.0,
                    "conditional_token_positions": {"token-up": 2.5},
                    "balance_snapshot_at": 900,
                    "source": "fixture"
                }),
            ))
            .expect("append balance");

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        assert!(
            report
                .reconciliation_mismatches
                .contains(&LiveReconciliationMismatch::MissingVenueConditionalBalanceEvidence),
            "expected missing conditional balance evidence, got {}",
            report
                .reconciliation_mismatches
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(!report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::BalanceDeltaMismatch));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_validate_path_scopes_local_orders_to_open_order_readback() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-startup-recovery-terminal-orders-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let journal = LiveOrderJournal::new(&path);
        for event in [
            LiveJournalEvent::new(
                "previous-run",
                "balance-1",
                LiveJournalEventType::LiveBalanceSnapshot,
                900,
                serde_json::json!({
                    "p_usd_available": 1.0,
                    "p_usd_reserved": 0.0,
                    "p_usd_total": 1.0,
                    "conditional_token_positions": {},
                    "balance_snapshot_at": 900,
                    "source": "fixture"
                }),
            ),
            LiveJournalEvent::new(
                "previous-run",
                "closed-order-readback",
                LiveJournalEventType::LiveOrderReadbackObserved,
                901,
                serde_json::json!({"order_id":"closed-order"}),
            ),
            LiveJournalEvent::new(
                "previous-run",
                "closed-order-canceled",
                LiveJournalEventType::LiveOrderCanceled,
                902,
                serde_json::json!({"order_id":"closed-order"}),
            ),
            LiveJournalEvent::new(
                "previous-run",
                "filled-order",
                LiveJournalEventType::LiveOrderFilled,
                903,
                serde_json::json!({"order_id":"filled-order"}),
            ),
        ] {
            journal.append(&event).expect("journal event appends");
        }

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        let reconciliation_input = input
            .reconciliation_input
            .as_ref()
            .expect("reconciliation input");
        assert!(reconciliation_input.local.known_orders.is_empty());
        assert!(reconciliation_input.local.canceled_orders.is_empty());

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::Passed);
        assert!(!report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::MissingVenueOrder));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_validate_path_scopes_local_trades_to_readback_trade_window() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-startup-recovery-historical-trade-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let journal = LiveOrderJournal::new(&path);
        for event in [
            LiveJournalEvent::new(
                "previous-run",
                "balance-1",
                LiveJournalEventType::LiveBalanceSnapshot,
                900,
                serde_json::json!({
                    "p_usd_available": 1.0,
                    "p_usd_reserved": 0.0,
                    "p_usd_total": 1.0,
                    "conditional_token_positions": {},
                    "balance_snapshot_at": 900,
                    "source": "fixture"
                }),
            ),
            LiveJournalEvent::new(
                "previous-run",
                "legacy-trade",
                LiveJournalEventType::LiveTradeConfirmed,
                901,
                serde_json::json!({
                    "trade_id": "trade-outside-readback-window",
                    "order_id": "filled-long-ago"
                }),
            ),
        ] {
            journal.append(&event).expect("journal event appends");
        }

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 0,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        let reconciliation_input = input
            .reconciliation_input
            .as_ref()
            .expect("reconciliation input");
        assert!(reconciliation_input.local.known_trades.is_empty());
        assert!(reconciliation_input.local.trade_order_ids.is_empty());

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::Passed);
        assert!(!report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::MissingVenueTrade));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn startup_recovery_validate_path_preserves_in_window_trade_order_mismatch() {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::FillCanary;
        config.live_alpha.fill_canary.enabled = true;
        let path = std::env::temp_dir().join(format!(
            "p15m-la2-startup-recovery-trade-mismatch-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        config.live_alpha.journal_path = path.display().to_string();

        let journal = LiveOrderJournal::new(&path);
        for event in [
            LiveJournalEvent::new(
                "previous-run",
                "balance-1",
                LiveJournalEventType::LiveBalanceSnapshot,
                900,
                serde_json::json!({
                    "p_usd_available": 1.0,
                    "p_usd_reserved": 0.0,
                    "p_usd_total": 1.0,
                    "conditional_token_positions": {},
                    "balance_snapshot_at": 900,
                    "source": "fixture"
                }),
            ),
            LiveJournalEvent::new(
                "previous-run",
                "trade-1",
                LiveJournalEventType::LiveTradeConfirmed,
                901,
                serde_json::json!({
                    "trade_id": "trade-in-readback-window",
                    "order_id": "local-order-1"
                }),
            ),
        ] {
            journal.append(&event).expect("journal event appends");
        }

        let validation = ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 1,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 1_000_000,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 1_000_000,
                allowance_units: 1_000_000,
            }),
            open_orders: Vec::new(),
            trades: vec![TradeReadback {
                id: "trade-in-readback-window".to_string(),
                market: "market-1".to_string(),
                asset_id: "token-up".to_string(),
                status: live_beta_readback::TradeReadbackStatus::Confirmed,
                transaction_hash: Some(format!("0x{}", "1".repeat(64))),
                maker_address: "0x1111111111111111111111111111111111111111".to_string(),
                order_id: Some("venue-wrong-order".to_string()),
            }],
        };

        let input = live_alpha_startup_recovery_input_for_validate(
            &config,
            "test-run",
            1_000,
            safety::GeoblockGateStatus::Passed,
            Some(&validation),
        );

        let reconciliation_input = input
            .reconciliation_input
            .as_ref()
            .expect("reconciliation input");
        assert!(reconciliation_input
            .local
            .known_trades
            .contains("trade-in-readback-window"));
        assert!(reconciliation_input
            .local
            .trade_order_ids
            .contains("local-order-1"));

        let report = live_startup_recovery::evaluate_startup_recovery(input);
        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        assert!(
            report
                .reconciliation_mismatches
                .contains(&LiveReconciliationMismatch::TradeOrderMismatch),
            "expected trade order mismatch, got {}",
            report
                .reconciliation_mismatches
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(!report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::MissingVenueTrade));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn lb6_order_cap_reservation_fails_closed_when_state_exists() {
        let path = std::env::temp_dir().join(format!(
            "p15m-lb6-cap-{}-{}.json",
            std::process::id(),
            monotonic_like_ns()
        ));

        reserve_canary_order_cap(&path, "sha256:first").expect("first reservation succeeds");
        let first_state = live_beta_canary::canary_order_cap_state_from_json(
            &fs::read_to_string(&path).expect("reserved state reads"),
        )
        .expect("reserved state parses");
        let second_error = reserve_canary_order_cap(&path, "sha256:second")
            .expect_err("second reservation fails closed")
            .to_string();
        let final_state = live_beta_canary::canary_order_cap_state_from_json(
            &fs::read_to_string(&path).expect("final state reads"),
        )
        .expect("final state parses");

        assert!(second_error.contains("already reserved or consumed"));
        assert_eq!(first_state.approval_sha256, "sha256:first");
        assert_eq!(final_state, first_state);

        fs::remove_file(path).expect("test cap state removed");
    }

    #[test]
    fn la3_submit_validation_failure_does_not_reserve_fill_cap() {
        let unique = format!("{}_{}", std::process::id(), monotonic_like_ns());
        let signer_handle = format!("P15M_TEST_LA3_SIGNER_{unique}");
        let l2_access_handle = format!("P15M_TEST_LA3_L2_ACCESS_{unique}");
        let l2_secret_handle = format!("P15M_TEST_LA3_L2_SECRET_{unique}");
        let l2_passphrase_handle = format!("P15M_TEST_LA3_L2_PASSPHRASE_{unique}");
        env::set_var(&signer_handle, "not-a-private-key");
        env::set_var(&l2_access_handle, "not-a-uuid");
        env::set_var(&l2_secret_handle, "present");
        env::set_var(&l2_passphrase_handle, "present");

        let path = std::env::temp_dir().join(format!("p15m-la3-cap-{unique}.json"));
        let input = LiveAlphaFillSubmitInput {
            clob_host: live_beta_readback::CLOB_HOST.to_string(),
            signer_handle: signer_handle.clone(),
            l2_access_handle: l2_access_handle.clone(),
            l2_secret_handle: l2_secret_handle.clone(),
            l2_passphrase_handle: l2_passphrase_handle.clone(),
            wallet_address: "0x1111111111111111111111111111111111111111".to_string(),
            funder_address: "0x2222222222222222222222222222222222222222".to_string(),
            signature_type: SignatureType::PolyProxy,
            approval: LiveAlphaApprovalArtifact {
                approval_id: "LA3-test".to_string(),
                approved_host_ids: vec!["approved-host".to_string()],
                wallet_id: "0x1111111111111111111111111111111111111111".to_string(),
                funder_id: "0x2222222222222222222222222222222222222222".to_string(),
                signature_type: "poly_proxy".to_string(),
                asset_symbol: "BTC".to_string(),
                market_slug: "btc-updown-15m-1777909500".to_string(),
                market_question: "BTC Up or Down".to_string(),
                condition_id: "0x371c52ca5f8dbe256978e6d27f6a6d8cf64f3722b15e44ba3128685ccfbeee0c"
                    .to_string(),
                outcome: "Up".to_string(),
                token_id:
                    "91899612655270438973839203540142703788805338252926995927363610489118446263952"
                        .to_string(),
                side: "BUY".to_string(),
                order_type: "FAK".to_string(),
                amount_or_size: 2.56,
                max_notional: 2.56,
                max_fee_estimate: 0.10,
                worst_price: 0.51,
                max_slippage_bps: 200,
                max_open_orders_after_run: 0,
                retry_count: 0,
                min_order_size: 5.0,
                tick_size: 0.01,
                market_end_unix: 1_777_909_600,
                approved_best_bid: Some(0.49),
                approved_best_bid_size: Some(10.0),
                approved_best_ask: Some(0.50),
                approved_best_ask_size: Some(10.0),
                approved_book_hash: Some("book-hash".to_string()),
                approved_book_timestamp_ms: Some(1_777_909_000_000),
            },
        };

        let error = validate_and_reserve_la3_fill_cap(&path, "LA3-test", &input)
            .expect_err("invalid local submit input must fail before cap reservation")
            .to_string();

        assert!(error.contains("private-key"));
        assert!(!path.exists(), "cap state must not be reserved");

        env::remove_var(signer_handle);
        env::remove_var(l2_access_handle);
        env::remove_var(l2_secret_handle);
        env::remove_var(l2_passphrase_handle);
    }

    #[test]
    fn la3_not_submitted_flag_is_not_emitted_before_final_submit_path() {
        assert_eq!(
            live_alpha_fill_canary_pre_submit_not_submitted(true, true),
            Some(true)
        );
        assert_eq!(
            live_alpha_fill_canary_pre_submit_not_submitted(false, false),
            Some(true)
        );
        assert_eq!(
            live_alpha_fill_canary_pre_submit_not_submitted(false, true),
            None
        );
    }

    #[test]
    fn la5_approval_artifact_rejects_pending_live_readback_fields() {
        let artifact = r#"
Status: PENDING LIVE READBACK AND HUMAN APPROVAL

| Field | Value |
| --- | --- |
| approved_wallet | `0x1111111111111111111111111111111111111111` |
| approval_id | PENDING EXECUTION RUN ID |
"#;

        let error = validate_la5_approval_artifact_text(artifact, "LA5-approval-1")
            .expect_err("pending artifact must fail")
            .to_string();

        assert!(error.contains("approval_status_missing"));
        assert!(error.contains("human_approval_or_live_readback_pending"));
        assert!(error.contains("approval_field_pending:approval_id"));
    }

    #[test]
    fn la5_approval_artifact_requires_all_final_gate_fields() {
        let artifact = la5_valid_approval_artifact_text();

        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("complete final artifact validates");

        assert_eq!(approval.max_orders, 3);
        assert_eq!(approval.ttl_seconds, 30);
        assert_eq!(approval.venue_gtd_expiration_delta, 90);
    }

    #[test]
    fn la5_approval_artifact_rejects_blocked_live_readback_fields() {
        let artifact = r#"
Status: LA5 APPROVED FOR THIS RUN ONLY

| Field | Value |
| --- | --- |
| approved_wallet | `0x1111111111111111111111111111111111111111` |
| approved_funder | `0x2222222222222222222222222222222222222222` |
| max_single_order_notional | `2.56` |
| max_total_live_notional | `2.56` |
| max_available_pusd_usage | `1.0` |
| max_reserved_pusd | `1.0` |
| max_fee_spend | `0.06` |
| max_orders | `3` |
| max_open_orders | `1` |
| max_duration_sec | `300` |
| no_trade_seconds_before_close | `600` |
| ttl_seconds | `30` |
| venue_gtd_expiration_delta | `90` |
| signature_type | `1` |
| available_pusd_units | BLOCKED: authenticated REST units unavailable |
| reserved_pusd_units | `0` |
| open_order_count | `0` |
| heartbeat_status | `not_started_no_open_orders` |
| funder_allowance_units | BLOCKED: authenticated REST allowance unavailable |
| rollback_owner | `primary-agent` |
| monitoring_owner | `primary-agent` |
| approval_id | `LA5-approval-1` |
| approval_date | `2026-05-05` |
"#;

        let error = validate_la5_approval_artifact_text(artifact, "LA5-approval-1")
            .expect_err("blocked readback evidence is not final");
        let error = error.to_string();
        assert!(error.contains("approval_field_pending:available_pusd_units"));
        assert!(error.contains("approval_field_pending:funder_allowance_units"));
    }

    #[test]
    fn la5_approval_artifact_rejects_completed_or_consumed_status() {
        let artifact = format!(
            "{}\nExecution Gate Status: LA5 RUN COMPLETED\n",
            la5_valid_approval_artifact_text()
        );

        let error = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect_err("completed approval artifact must fail closed")
            .to_string();

        assert!(error.contains("approval_artifact_consumed"));
    }

    #[test]
    fn la5_approval_cap_reservation_fails_closed_on_duplicate() {
        let parent = std::env::temp_dir().join(format!(
            "p15m-la5-cap-{}-{}",
            std::process::id(),
            monotonic_like_ns()
        ));
        let path = parent.join("LA5-approval-1.json");
        let first_reservation = La5ApprovalCapReservation {
            approval_id: "LA5-approval-1".to_string(),
            approval_artifact_sha256: "sha256:first".to_string(),
            approval_artifact_path: "verification/la5-approval.md".to_string(),
            max_orders: 3,
            max_duration_sec: 300,
            reserved_at_unix: 1_234_567,
        };
        let second_reservation = La5ApprovalCapReservation {
            approval_id: "LA5-approval-1".to_string(),
            approval_artifact_sha256: "sha256:second".to_string(),
            approval_artifact_path: "verification/other-la5-approval.md".to_string(),
            max_orders: 1,
            max_duration_sec: 30,
            reserved_at_unix: 1_234_568,
        };

        reserve_la5_approval_cap(&path, &first_reservation).expect("first reservation succeeds");
        let first_state = fs::read_to_string(&path).expect("reserved cap reads");
        let parsed: serde_json::Value =
            serde_json::from_str(&first_state).expect("reserved cap parses");
        let second_error = reserve_la5_approval_cap(&path, &second_reservation)
            .expect_err("second reservation fails closed")
            .to_string();
        let final_state = fs::read_to_string(&path).expect("final cap reads");

        assert!(second_error.contains("already reserved or consumed"));
        assert_eq!(parsed["approval_id"], "LA5-approval-1");
        assert_eq!(parsed["approval_artifact_sha256"], "sha256:first");
        assert_eq!(
            parsed["approval_artifact_path"],
            "verification/la5-approval.md"
        );
        assert_eq!(parsed["max_orders"], 3);
        assert_eq!(parsed["max_duration_sec"], 300);
        assert_eq!(final_state, first_state);

        fs::remove_file(path).expect("test cap state removed");
        fs::remove_dir_all(parent).expect("test cap parent removed");
    }

    #[test]
    fn la5_human_approved_gate_rejects_kill_switch_active() {
        let error = validate_la5_live_submit_runtime_gate_values(true, true, true)
            .expect_err("active kill switch must fail closed")
            .to_string();

        assert!(error.contains("kill_switch_active"));
    }

    #[test]
    fn la5_human_approved_gate_rejects_live_placement_disabled() {
        let error = validate_la5_live_submit_runtime_gate_values(true, false, false)
            .expect_err("disabled live placement must fail closed")
            .to_string();

        assert!(error.contains("live_order_placement_disabled"));
    }

    #[tokio::test]
    async fn la5_post_acceptance_cleanup_confirms_exact_cancel_before_error() {
        let path = std::env::temp_dir().join(format!(
            "p15m-la5-cleanup-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        let journal = LiveOrderJournal::new(&path);
        let order_id =
            "0x1111111111111111111111111111111111111111111111111111111111111111".to_string();
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_for_cleanup = called.clone();
        let order_id_for_cleanup = order_id.clone();

        let error = la5_cleanup_accepted_order_before_error(
            &journal,
            "run-cleanup-ok",
            &order_id,
            "intent-cleanup-ok",
            1,
            "post_order_readback_failed",
            "readback transport failed".to_string(),
            || async move {
                called_for_cleanup.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok::<Vec<String>, &'static str>(vec![order_id_for_cleanup])
            },
        )
        .await
        .to_string();

        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(error.contains("readback transport failed"));
        assert!(error.contains("cleanup_cancel_attempted=true"));
        assert!(error.contains("cleanup_cancel_confirmed=true"));
        let contents = fs::read_to_string(&path).expect("cleanup journal reads");
        assert!(contents.contains("maker_order_canceled"));
        assert!(contents.contains("cleanup_cancel_confirmed"));
        let replay = journal
            .replay_state("run-cleanup-ok")
            .expect("cleanup journal replays");
        assert!(replay.canceled_orders.contains(&order_id));

        fs::remove_file(path).expect("test cleanup journal removed");
    }

    #[tokio::test]
    async fn la5_post_acceptance_cleanup_preserves_original_error_when_cancel_fails() {
        let path = std::env::temp_dir().join(format!(
            "p15m-la5-cleanup-fail-{}-{}.jsonl",
            std::process::id(),
            monotonic_like_ns()
        ));
        let journal = LiveOrderJournal::new(&path);
        let order_id =
            "0x2222222222222222222222222222222222222222222222222222222222222222".to_string();
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_for_cleanup = called.clone();

        let error = la5_cleanup_accepted_order_before_error(
            &journal,
            "run-cleanup-fail",
            &order_id,
            "intent-cleanup-fail",
            1,
            "cancel_rate_slot_unavailable",
            "LA5 rate limit configured as zero".to_string(),
            || async move {
                called_for_cleanup.store(true, std::sync::atomic::Ordering::SeqCst);
                Err::<Vec<String>, &'static str>("cancel transport failed")
            },
        )
        .await
        .to_string();

        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(error.contains("LA5 rate limit configured as zero"));
        assert!(error.contains("cleanup_cancel_attempted=true"));
        assert!(error.contains("cleanup_cancel_confirmed=false"));
        assert!(error.contains("cleanup_cancel_error=cancel transport failed"));
        let contents = fs::read_to_string(&path).expect("cleanup journal reads");
        assert!(contents.contains("maker_reconciliation_failed"));
        assert!(contents.contains("cleanup_cancel_failed"));
        let replay = journal
            .replay_state("run-cleanup-fail")
            .expect("cleanup journal replays");
        assert_eq!(replay.reconciliation_mismatch_count, 1);
        assert!(!replay.canceled_orders.contains(&order_id));

        fs::remove_file(path).expect("test cleanup journal removed");
    }

    #[test]
    fn la5_approval_binding_rejects_mismatched_max_orders() {
        let artifact = la5_approval_artifact_with_field("max_orders", "`1`");
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses before binding");
        let config = la5_test_config();

        let error = validate_la5_approval_against_cli_and_config(
            &approval,
            &config,
            "LA5-approval-1",
            3,
            300,
        )
        .expect_err("artifact max_orders must bind to CLI max_orders")
        .to_string();

        assert!(error.contains("approval_max_orders_mismatch"));
    }

    #[test]
    fn la5_approval_binding_rejects_mismatched_wallet_and_funder() {
        let artifact = la5_approval_artifact_with_field(
            "approved_wallet",
            "`0x3333333333333333333333333333333333333333`",
        );
        let artifact = la5_replace_approval_field(
            &artifact,
            "approved_funder",
            "`0x4444444444444444444444444444444444444444`",
        );
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses before binding");
        let config = la5_test_config();

        let error = validate_la5_approval_against_cli_and_config(
            &approval,
            &config,
            "LA5-approval-1",
            3,
            300,
        )
        .expect_err("artifact wallet/funder must bind to config account")
        .to_string();

        assert!(error.contains("approval_wallet_mismatch"));
        assert!(error.contains("approval_funder_mismatch"));
    }

    #[test]
    fn la5_approval_binding_rejects_unapproved_notional_cap() {
        let artifact = la5_approval_artifact_with_field("max_single_order_notional", "`0.50`");
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses before binding");
        let plan = la5_test_maker_plan();

        let error = validate_la5_plan_against_approval(&approval, &plan, 0.0)
            .expect_err("plan notional must stay inside artifact cap")
            .to_string();

        assert!(error.contains("approval_plan_single_notional_exceeds_cap"));
    }

    #[test]
    fn la5_approval_binding_rejects_mismatched_ttl_and_gtd_delta() {
        let artifact = la5_approval_artifact_with_field("ttl_seconds", "`31`");
        let artifact = la5_replace_approval_field(&artifact, "venue_gtd_expiration_delta", "`89`");
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses before binding");
        let config = la5_test_config();
        let plan = la5_test_maker_plan();

        let config_error = validate_la5_approval_against_cli_and_config(
            &approval,
            &config,
            "LA5-approval-1",
            3,
            300,
        )
        .expect_err("artifact TTL/GTD delta must bind to config")
        .to_string();
        let plan_error = validate_la5_plan_against_approval(&approval, &plan, 0.0)
            .expect_err("artifact TTL/GTD delta must bind to submitted plan")
            .to_string();

        assert!(config_error.contains("approval_ttl_seconds_mismatch"));
        assert!(config_error.contains("approval_venue_gtd_expiration_delta_mismatch"));
        assert!(plan_error.contains("approval_plan_ttl_seconds_mismatch"));
        assert!(plan_error.contains("approval_plan_gtd_delta_mismatch"));
    }

    #[test]
    fn la5_approval_binding_rejects_mismatched_readback_values() {
        let artifact = la5_approval_artifact_with_field("available_pusd_units", "`123`");
        let artifact = la5_replace_approval_field(&artifact, "open_order_count", "`1`");
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses before binding");
        let account = la5_test_account();
        let readback = la5_test_readback();

        let error = validate_la5_approval_against_account_readback(
            &approval,
            &account,
            &readback,
            18446744073709551615,
        )
        .expect_err("artifact readback fields must match authenticated readback")
        .to_string();

        assert!(error.contains("approval_available_pusd_units_mismatch"));
        assert!(error.contains("approval_open_order_count_mismatch"));
    }

    #[test]
    fn la5_approval_binding_accepts_matching_cli_config_readback_plan_and_session() {
        let artifact = la5_valid_approval_artifact_text();
        let approval = validate_la5_approval_artifact_text(&artifact, "LA5-approval-1")
            .expect("artifact parses");
        let config = la5_test_config();
        let account = la5_test_account();
        let readback = la5_test_readback();
        let plan = la5_test_maker_plan();
        let outcomes = vec![
            la5_test_outcome(1, 0.85),
            la5_test_outcome(2, 0.85),
            la5_test_outcome(3, 0.85),
        ];

        validate_la5_approval_against_cli_and_config(&approval, &config, "LA5-approval-1", 3, 300)
            .expect("matching CLI/config passes");
        validate_la5_approval_against_account_readback(
            &approval,
            &account,
            &readback,
            18446744073709551615,
        )
        .expect("matching readback passes");
        validate_la5_plan_against_approval(&approval, &plan, 0.0).expect("matching plan passes");
        validate_la5_session_against_approval(&approval, &outcomes, 2.55)
            .expect("matching session passes");
    }

    fn la5_valid_approval_artifact_text() -> String {
        r#"
Status: LA5 APPROVED FOR THIS RUN ONLY

| Field | Value |
| --- | --- |
| approved_wallet | `0x1111111111111111111111111111111111111111` |
| approved_funder | `0x2222222222222222222222222222222222222222` |
| max_single_order_notional | `2.56` |
| max_total_live_notional | `2.56` |
| max_available_pusd_usage | `1.0` |
| max_reserved_pusd | `1.0` |
| max_fee_spend | `0.06` |
| max_orders | `3` |
| max_open_orders | `1` |
| max_duration_sec | `300` |
| no_trade_seconds_before_close | `600` |
| ttl_seconds | `30` effective quote TTL |
| venue_gtd_expiration_delta | `90` seconds |
| signature_type | `1` |
| available_pusd_units | `6314318` |
| reserved_pusd_units | `0` |
| open_order_count | `0` |
| heartbeat_status | `not_started_no_open_orders` |
| funder_allowance_units | `18446744073709551615` |
| rollback_owner | `primary-agent` |
| monitoring_owner | `primary-agent` |
| approval_id | `LA5-approval-1` |
| approval_date | `2026-05-05` |

Approved: Operator authorized agent-run LA5; human action limited to PR merge
"#
        .to_string()
    }

    fn la5_approval_artifact_with_field(field: &str, value: &str) -> String {
        la5_replace_approval_field(&la5_valid_approval_artifact_text(), field, value)
    }

    fn la5_replace_approval_field(artifact: &str, field: &str, value: &str) -> String {
        artifact
            .lines()
            .map(|line| {
                let cells = line.split('|').map(str::trim).collect::<Vec<_>>();
                if cells.len() >= 3 && cells[1] == field {
                    format!("| {field} | {value} |")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn la5_test_config() -> AppConfig {
        let mut config: AppConfig =
            toml::from_str(include_str!("../config/default.toml")).expect("default config parses");
        config.live_beta.readback_account.wallet_address =
            "0x1111111111111111111111111111111111111111".to_string();
        config.live_beta.readback_account.funder_address =
            "0x2222222222222222222222222222222222222222".to_string();
        config.live_beta.readback_account.signature_type = "1".to_string();
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::MakerMicro;
        config.live_alpha.maker.enabled = true;
        config.live_alpha.maker.post_only = true;
        config.live_alpha.maker.order_type = "GTD".to_string();
        config.live_alpha.maker.ttl_seconds = 30;
        config.live_alpha.risk.max_single_order_notional = 2.56;
        config.live_alpha.risk.max_total_live_notional = 2.56;
        config.live_alpha.risk.max_available_pusd_usage = 1.0;
        config.live_alpha.risk.max_reserved_pusd = 1.0;
        config.live_alpha.risk.max_fee_spend = 0.06;
        config.live_alpha.risk.max_open_orders = 1;
        config.live_alpha.risk.no_trade_seconds_before_close = 600;
        config
    }

    fn la5_test_account() -> AccountPreflight {
        AccountPreflight {
            clob_host: live_beta_readback::CLOB_HOST.to_string(),
            chain_id: 137,
            wallet_address: "0x1111111111111111111111111111111111111111".to_string(),
            funder_address: "0x2222222222222222222222222222222222222222".to_string(),
            signature_type: SignatureType::PolyProxy,
        }
    }

    fn la5_test_readback() -> ReadbackPreflightValidation {
        ReadbackPreflightValidation {
            report: ReadbackPreflightReport {
                status: "passed",
                block_reasons: Vec::new(),
                open_order_count: 0,
                trade_count: 23,
                reserved_pusd_units: 0,
                required_collateral_allowance_units: 1_000_000,
                available_pusd_units: 6_314_318,
                venue_state: "trading_enabled",
                heartbeat: "not_started_no_open_orders",
                live_network_enabled: true,
            },
            collateral: Some(live_beta_readback::BalanceAllowanceReadback {
                asset_type: live_beta_readback::AssetType::Collateral,
                token_id: None,
                balance_units: 6_314_318,
                allowance_units: 18446744073709551615,
            }),
            open_orders: Vec::new(),
            trades: Vec::new(),
        }
    }

    fn la5_test_maker_plan() -> LiveMakerOrderPlan {
        LiveMakerOrderPlan {
            intent_id: "la5-test-intent".to_string(),
            token_id: "token-up".to_string(),
            outcome: "Up".to_string(),
            side: Side::Buy,
            price: 0.17,
            size: 5.0,
            notional: 0.85,
            post_only: true,
            order_type: "GTD".to_string(),
            effective_quote_ttl_seconds: 30,
            gtd_expiration_unix: 1_000_090,
            cancel_after_unix: 1_000_030,
            reason_codes: Vec::new(),
        }
    }

    fn la5_test_outcome(sequence: u64, notional: f64) -> La5MakerOrderOutcome {
        La5MakerOrderOutcome {
            sequence,
            intent_id: format!("la5-test-intent-{sequence}"),
            market_slug: "btc-updown-15m-test".to_string(),
            token_id: "token-up".to_string(),
            outcome: "Up".to_string(),
            side: Side::Buy,
            price: 0.17,
            size: 5.0,
            notional,
            gtd_expiration_unix: 1_000_090 + sequence,
            cancel_after_unix: 1_000_030 + sequence,
            order_id: format!("order-{sequence}"),
            accepted_status: "LIVE".to_string(),
            final_status: "CANCELED".to_string(),
            canceled: true,
            cancel_request_sent: false,
            exact_cancel_confirmed: false,
            venue_final_canceled: true,
            filled: false,
            trade_ids: Vec::new(),
            pre_submit_available_pusd_units: 6_314_318,
            post_order_available_pusd_units: 6_314_318,
            final_available_pusd_units: 6_314_318,
            final_reserved_pusd_units: 0,
            reconciliation_status: "passed".to_string(),
            reconciliation_mismatches: String::new(),
        }
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

    fn sample_shadow_decision() -> ShadowLiveDecision {
        ShadowLiveDecision {
            shadow_decision_id: "shadow-decision-1".to_string(),
            shadow_intent_id: "shadow-intent-1".to_string(),
            intent_id: "intent-1".to_string(),
            strategy_snapshot_id: Some("snapshot-1".to_string()),
            market_slug: "btc-updown-15m-test".to_string(),
            condition_id: "condition-1".to_string(),
            token_id: "token-up".to_string(),
            side: Side::Buy,
            would_submit: false,
            would_cancel: false,
            would_replace: false,
            live_eligible: false,
            risk_eligible: true,
            post_only_safe: true,
            inventory_valid: true,
            balance_valid: true,
            book_fresh: true,
            reference_fresh: true,
            market_time_valid: true,
            reason_codes: vec!["mode_not_approved".to_string()],
            expected_order_type: "GTD".to_string(),
            expected_price: 0.42,
            expected_size: 5.0,
            expected_notional: 2.1,
            expected_edge_bps: 500.0,
            expected_edge: 0.05,
            expected_fee: Some(0.0),
            expected_ttl: Some(60_000),
            book_snapshot_id: Some("book-1".to_string()),
            best_bid: Some(0.41),
            best_ask: Some(0.43),
            geoblock_passed: false,
            heartbeat_healthy: false,
            reconciliation_clean: false,
            available_pusd: 1_000.0,
            reserved_pusd: 0.0,
            open_order_count: 0,
        }
    }
}
