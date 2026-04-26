use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use polymarket_15m_arb_bot::{config::AppConfig, module_names, safety};
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
    Validate,
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
            Commands::Validate => "validate",
            Commands::Paper => "paper",
            Commands::Replay { .. } => "replay",
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

    info!(
        %run_id,
        mode,
        config_path = %cli.config.display(),
        assets = %config.asset_list(),
        module_count = modules.len(),
        "startup validation complete"
    );

    match cli.command {
        Commands::Validate => {
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
        }
        Commands::Paper => {
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

fn generate_run_id() -> String {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id();
    let sequence = RUN_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{now_ns:x}-{pid:x}-{sequence:x}")
}
