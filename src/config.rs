use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "config";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub runtime: RuntimeConfig,
    pub assets: AssetsConfig,
    pub polymarket: PolymarketConfig,
    pub feeds: FeedsConfig,
    #[serde(default)]
    pub reference_feed: ReferenceFeedConfig,
    pub storage: StorageConfig,
    pub strategy: StrategyConfig,
    pub risk: RiskConfig,
    pub paper: PaperConfig,
    pub metrics: MetricsConfig,
    pub replay: ReplayConfig,
}

impl AppConfig {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let path_display = path.display().to_string();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path_display.clone(),
            source,
        })?;

        let mut config: AppConfig =
            toml::from_str(&contents).map_err(|source| ConfigError::Parse {
                path: path_display,
                source,
            })?;

        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }

    pub fn asset_list(&self) -> String {
        self.assets.symbols.join(",")
    }

    fn apply_env_overrides(&mut self) {
        apply_string_override("P15M_LOG_LEVEL", &mut self.runtime.log_level);
        apply_string_override("P15M_CLICKHOUSE_URL", &mut self.storage.clickhouse_url);
        apply_string_override("P15M_POSTGRES_URL", &mut self.storage.postgres_url);
        apply_string_override("P15M_METRICS_BIND_ADDR", &mut self.metrics.bind_addr);
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let mut errors = Vec::new();

        require_non_empty(&mut errors, "runtime.log_level", &self.runtime.log_level);
        require_supported_mode(&mut errors, &self.runtime.mode);

        require_exact_assets(&mut errors, &self.assets.symbols);

        require_url(
            &mut errors,
            "polymarket.clob_rest_url",
            &self.polymarket.clob_rest_url,
            &["https://", "http://"],
        );
        require_url(
            &mut errors,
            "polymarket.market_ws_url",
            &self.polymarket.market_ws_url,
            &["wss://", "ws://"],
        );
        require_url(
            &mut errors,
            "polymarket.gamma_markets_url",
            &self.polymarket.gamma_markets_url,
            &["https://", "http://"],
        );
        require_url(
            &mut errors,
            "polymarket.geoblock_url",
            &self.polymarket.geoblock_url,
            &["https://", "http://"],
        );
        require_positive_u64(
            &mut errors,
            "polymarket.request_timeout_ms",
            self.polymarket.request_timeout_ms,
        );
        require_range_u16(
            &mut errors,
            "polymarket.market_discovery_page_limit",
            self.polymarket.market_discovery_page_limit,
            1,
            1_000,
        );
        require_positive_u16(
            &mut errors,
            "polymarket.market_discovery_max_pages",
            self.polymarket.market_discovery_max_pages,
        );
        require_positive_u64(
            &mut errors,
            "polymarket.market_discovery_poll_ms",
            self.polymarket.market_discovery_poll_ms,
        );
        require_range_u16(
            &mut errors,
            "polymarket.gamma_markets_request_limit_per_10s",
            self.polymarket.gamma_markets_request_limit_per_10s,
            1,
            300,
        );
        require_range_u16(
            &mut errors,
            "polymarket.clob_market_info_request_limit_per_10s",
            self.polymarket.clob_market_info_request_limit_per_10s,
            1,
            9_000,
        );
        require_url(
            &mut errors,
            "feeds.resolution_source_url",
            &self.feeds.resolution_source_url,
            &["https://", "http://", "wss://", "ws://"],
        );
        require_url(
            &mut errors,
            "feeds.binance_ws_url",
            &self.feeds.binance_ws_url,
            &["wss://", "ws://"],
        );
        require_url(
            &mut errors,
            "feeds.coinbase_ws_url",
            &self.feeds.coinbase_ws_url,
            &["wss://", "ws://"],
        );
        require_positive_u64(
            &mut errors,
            "feeds.connect_timeout_ms",
            self.feeds.connect_timeout_ms,
        );
        require_positive_u64(
            &mut errors,
            "feeds.read_timeout_ms",
            self.feeds.read_timeout_ms,
        );
        require_positive_u64(
            &mut errors,
            "feeds.stale_after_ms",
            self.feeds.stale_after_ms,
        );
        require_positive_u64(
            &mut errors,
            "feeds.reconnect_initial_backoff_ms",
            self.feeds.reconnect_initial_backoff_ms,
        );
        require_positive_u64(
            &mut errors,
            "feeds.reconnect_max_backoff_ms",
            self.feeds.reconnect_max_backoff_ms,
        );
        if self.feeds.reconnect_initial_backoff_ms > self.feeds.reconnect_max_backoff_ms {
            errors.push(
                "feeds.reconnect_initial_backoff_ms must be less than or equal to feeds.reconnect_max_backoff_ms"
                    .to_string(),
            );
        }
        require_positive_u16(
            &mut errors,
            "feeds.reconnect_max_attempts",
            self.feeds.reconnect_max_attempts,
        );
        require_positive_u16(
            &mut errors,
            "feeds.feed_smoke_message_limit",
            self.feeds.feed_smoke_message_limit,
        );
        require_reference_feed_config(&mut errors, &self.reference_feed);
        require_url(
            &mut errors,
            "storage.clickhouse_url",
            &self.storage.clickhouse_url,
            &["https://", "http://"],
        );
        require_url(
            &mut errors,
            "storage.postgres_url",
            &self.storage.postgres_url,
            &["postgres://", "postgresql://"],
        );

        require_positive_u64(
            &mut errors,
            "strategy.min_edge_bps",
            self.strategy.min_edge_bps,
        );
        require_positive_u64(
            &mut errors,
            "strategy.latency_buffer_ms",
            self.strategy.latency_buffer_ms,
        );
        require_positive_u64(
            &mut errors,
            "strategy.adverse_selection_bps",
            self.strategy.adverse_selection_bps,
        );
        require_positive_u64(
            &mut errors,
            "strategy.final_seconds_no_trade",
            self.strategy.final_seconds_no_trade,
        );

        require_positive_f64(
            &mut errors,
            "risk.max_loss_per_market",
            self.risk.max_loss_per_market,
        );
        require_positive_f64(
            &mut errors,
            "risk.max_notional_per_market",
            self.risk.max_notional_per_market,
        );
        require_positive_f64(
            &mut errors,
            "risk.max_notional_per_asset",
            self.risk.max_notional_per_asset,
        );
        require_positive_f64(
            &mut errors,
            "risk.max_total_notional",
            self.risk.max_total_notional,
        );
        require_positive_f64(
            &mut errors,
            "risk.max_correlated_notional",
            self.risk.max_correlated_notional,
        );
        require_positive_u64(
            &mut errors,
            "risk.stale_reference_ms",
            self.risk.stale_reference_ms,
        );
        require_positive_u64(&mut errors, "risk.stale_book_ms", self.risk.stale_book_ms);
        require_positive_u64(
            &mut errors,
            "risk.max_orders_per_minute",
            self.risk.max_orders_per_minute,
        );
        require_positive_f64(
            &mut errors,
            "risk.daily_drawdown_limit",
            self.risk.daily_drawdown_limit,
        );

        require_positive_f64(
            &mut errors,
            "paper.starting_balance",
            self.paper.starting_balance,
        );
        require_positive_u64(
            &mut errors,
            "paper.max_orders_per_market",
            self.paper.max_orders_per_market,
        );

        require_non_empty(&mut errors, "metrics.bind_addr", &self.metrics.bind_addr);
        require_non_empty(&mut errors, "replay.output_dir", &self.replay.output_dir);
        if !self.replay.deterministic {
            errors.push("replay.deterministic must remain true for the MVP".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(errors))
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeConfig {
    pub mode: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetsConfig {
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PolymarketConfig {
    pub clob_rest_url: String,
    pub market_ws_url: String,
    pub gamma_markets_url: String,
    pub geoblock_url: String,
    pub request_timeout_ms: u64,
    pub market_discovery_page_limit: u16,
    pub market_discovery_max_pages: u16,
    pub market_discovery_poll_ms: u64,
    pub gamma_markets_request_limit_per_10s: u16,
    pub clob_market_info_request_limit_per_10s: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeedsConfig {
    pub resolution_source_url: String,
    pub binance_ws_url: String,
    pub coinbase_ws_url: String,
    pub connect_timeout_ms: u64,
    pub read_timeout_ms: u64,
    pub stale_after_ms: u64,
    pub reconnect_initial_backoff_ms: u64,
    pub reconnect_max_backoff_ms: u64,
    pub reconnect_max_attempts: u16,
    pub feed_smoke_message_limit: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferenceFeedConfig {
    pub provider: String,
    pub polymarket_rtds_url: String,
    pub pyth_enabled: bool,
    pub pyth_hermes_url: String,
    pub pyth_btc_usd_price_id: String,
    pub pyth_eth_usd_price_id: String,
    pub pyth_sol_usd_price_id: String,
    pub max_staleness_ms: u64,
}

impl ReferenceFeedConfig {
    pub fn is_pyth_proxy_enabled(&self) -> bool {
        self.provider == "pyth_proxy" && self.pyth_enabled
    }

    pub fn is_polymarket_rtds_chainlink_enabled(&self) -> bool {
        self.provider == "polymarket_rtds_chainlink"
    }
}

impl Default for ReferenceFeedConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            polymarket_rtds_url: "wss://ws-live-data.polymarket.com".to_string(),
            pyth_enabled: false,
            pyth_hermes_url: "https://hermes.pyth.network".to_string(),
            pyth_btc_usd_price_id:
                "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43".to_string(),
            pyth_eth_usd_price_id:
                "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace".to_string(),
            pyth_sol_usd_price_id:
                "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d".to_string(),
            max_staleness_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub clickhouse_url: String,
    pub postgres_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategyConfig {
    pub min_edge_bps: u64,
    pub latency_buffer_ms: u64,
    pub adverse_selection_bps: u64,
    pub final_seconds_no_trade: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskConfig {
    pub max_loss_per_market: f64,
    pub max_notional_per_market: f64,
    pub max_notional_per_asset: f64,
    pub max_total_notional: f64,
    pub max_correlated_notional: f64,
    pub stale_reference_ms: u64,
    pub stale_book_ms: u64,
    pub max_orders_per_minute: u64,
    pub daily_drawdown_limit: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaperConfig {
    pub starting_balance: f64,
    pub max_orders_per_market: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    pub bind_addr: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReplayConfig {
    pub output_dir: String,
    pub deterministic: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: toml::de::Error,
    },
    Validation(Vec<String>),
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read { path, source } => {
                write!(formatter, "failed to read config {path}: {source}")
            }
            ConfigError::Parse { path, source } => {
                write!(formatter, "failed to parse config {path}: {source}")
            }
            ConfigError::Validation(errors) => {
                writeln!(formatter, "configuration validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ConfigError::Read { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
            ConfigError::Validation(_) => None,
        }
    }
}

fn apply_string_override(env_key: &str, target: &mut String) {
    if let Ok(value) = env::var(env_key) {
        if !value.trim().is_empty() {
            *target = value;
        }
    }
}

fn require_supported_mode(errors: &mut Vec<String>, mode: &str) {
    match mode {
        "validate" | "paper" | "replay" => {}
        _ => errors.push(format!(
            "runtime.mode must be one of validate, paper, replay; got {mode}"
        )),
    }
}

fn require_exact_assets(errors: &mut Vec<String>, symbols: &[String]) {
    let expected = ["BTC", "ETH", "SOL"];
    for symbol in expected {
        if !symbols.iter().any(|item| item == symbol) {
            errors.push(format!("assets.symbols is missing required asset {symbol}"));
        }
    }

    for symbol in symbols {
        if !expected.contains(&symbol.as_str()) {
            errors.push(format!(
                "assets.symbols contains unsupported MVP asset {symbol}"
            ));
        }
    }
}

fn require_non_empty(errors: &mut Vec<String>, name: &str, value: &str) {
    if value.trim().is_empty() {
        errors.push(format!("{name} must not be empty"));
    }
}

fn require_url(errors: &mut Vec<String>, name: &str, value: &str, schemes: &[&str]) {
    require_non_empty(errors, name, value);
    if !schemes.iter().any(|scheme| value.starts_with(scheme)) {
        errors.push(format!(
            "{name} must start with one of {}; got {value}",
            schemes.join(", ")
        ));
    }
}

fn require_positive_u64(errors: &mut Vec<String>, name: &str, value: u64) {
    if value == 0 {
        errors.push(format!("{name} must be greater than zero"));
    }
}

fn require_positive_u16(errors: &mut Vec<String>, name: &str, value: u16) {
    if value == 0 {
        errors.push(format!("{name} must be greater than zero"));
    }
}

fn require_range_u16(errors: &mut Vec<String>, name: &str, value: u16, min: u16, max: u16) {
    if value < min || value > max {
        errors.push(format!(
            "{name} must be between {min} and {max}; got {value}"
        ));
    }
}

fn require_reference_feed_config(errors: &mut Vec<String>, config: &ReferenceFeedConfig) {
    match config.provider.as_str() {
        "none" | "pyth_proxy" | "chainlink" | "polymarket_rtds_chainlink" => {}
        provider => errors.push(format!(
            "reference_feed.provider must be one of none, pyth_proxy, chainlink, polymarket_rtds_chainlink; got {provider}"
        )),
    }

    if config.pyth_enabled && config.provider != "pyth_proxy" {
        errors.push(
            "reference_feed.pyth_enabled can be true only when provider is pyth_proxy".to_string(),
        );
    }
    if config.provider == "pyth_proxy" && !config.pyth_enabled {
        errors.push(
            "reference_feed.provider=pyth_proxy requires reference_feed.pyth_enabled=true"
                .to_string(),
        );
    }

    require_url(
        errors,
        "reference_feed.polymarket_rtds_url",
        &config.polymarket_rtds_url,
        &["wss://", "ws://"],
    );
    require_url(
        errors,
        "reference_feed.pyth_hermes_url",
        &config.pyth_hermes_url,
        &["https://", "http://"],
    );
    require_price_id(
        errors,
        "reference_feed.pyth_btc_usd_price_id",
        &config.pyth_btc_usd_price_id,
    );
    require_price_id(
        errors,
        "reference_feed.pyth_eth_usd_price_id",
        &config.pyth_eth_usd_price_id,
    );
    require_price_id(
        errors,
        "reference_feed.pyth_sol_usd_price_id",
        &config.pyth_sol_usd_price_id,
    );
    require_positive_u64(
        errors,
        "reference_feed.max_staleness_ms",
        config.max_staleness_ms,
    );
}

fn require_price_id(errors: &mut Vec<String>, name: &str, value: &str) {
    let Some(stripped) = value.strip_prefix("0x") else {
        errors.push(format!("{name} must start with 0x"));
        return;
    };
    if stripped.len() != 64 || !stripped.chars().all(|ch| ch.is_ascii_hexdigit()) {
        errors.push(format!("{name} must be a 32-byte hex price id"));
    }
}

fn require_positive_f64(errors: &mut Vec<String>, name: &str, value: f64) {
    if !value.is_finite() || value <= 0.0 {
        errors.push(format!("{name} must be a finite value greater than zero"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG: &str = include_str!("../config/default.toml");

    #[test]
    fn default_config_is_valid() {
        let config: AppConfig = toml::from_str(VALID_CONFIG).expect("default config parses");
        config.validate().expect("default config validates");
        assert_eq!(config.reference_feed.provider, "none");
        assert!(!config.reference_feed.pyth_enabled);
        assert_eq!(
            config.reference_feed.polymarket_rtds_url,
            "wss://ws-live-data.polymarket.com"
        );
    }

    #[test]
    fn config_rejects_missing_required_asset() {
        let mut config: AppConfig = toml::from_str(VALID_CONFIG).expect("default config parses");
        config.assets.symbols = vec!["BTC".to_string(), "ETH".to_string()];

        let error = config.validate().expect_err("missing SOL fails validation");

        assert!(error
            .to_string()
            .contains("assets.symbols is missing required asset SOL"));
    }

    #[test]
    fn config_rejects_empty_endpoint() {
        let mut config: AppConfig = toml::from_str(VALID_CONFIG).expect("default config parses");
        config.polymarket.clob_rest_url.clear();

        let error = config
            .validate()
            .expect_err("empty endpoint fails validation");

        assert!(error
            .to_string()
            .contains("polymarket.clob_rest_url must not be empty"));
    }

    #[test]
    fn pyth_proxy_mode_requires_explicit_opt_in_and_default_ids() {
        let mut config: AppConfig = toml::from_str(VALID_CONFIG).expect("default config parses");
        config.reference_feed.provider = "pyth_proxy".to_string();

        let error = config
            .validate()
            .expect_err("pyth proxy without enabled flag fails");
        assert!(error
            .to_string()
            .contains("provider=pyth_proxy requires reference_feed.pyth_enabled=true"));

        config.reference_feed.pyth_enabled = true;
        config.validate().expect("explicit pyth proxy validates");
        assert_eq!(
            config.reference_feed.pyth_btc_usd_price_id,
            "0xe62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43"
        );
        assert_eq!(
            config.reference_feed.pyth_eth_usd_price_id,
            "0xff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace"
        );
        assert_eq!(
            config.reference_feed.pyth_sol_usd_price_id,
            "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d"
        );
    }

    #[test]
    fn polymarket_rtds_chainlink_mode_uses_documented_url_without_credentials() {
        let mut config: AppConfig = toml::from_str(VALID_CONFIG).expect("default config parses");
        config.reference_feed.provider = "polymarket_rtds_chainlink".to_string();

        config
            .validate()
            .expect("polymarket rtds chainlink validates");
        assert!(config.reference_feed.is_polymarket_rtds_chainlink_enabled());
        assert_eq!(
            config.reference_feed.polymarket_rtds_url,
            "wss://ws-live-data.polymarket.com"
        );
    }
}
