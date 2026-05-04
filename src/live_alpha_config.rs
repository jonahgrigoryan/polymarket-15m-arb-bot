use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_alpha_config";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveAlphaMode {
    #[default]
    Disabled,
    FillCanary,
    Shadow,
    MakerMicro,
    QuoteManager,
    TakerGate,
    Scale,
}

impl LiveAlphaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::FillCanary => "fill_canary",
            Self::Shadow => "shadow",
            Self::MakerMicro => "maker_micro",
            Self::QuoteManager => "quote_manager",
            Self::TakerGate => "taker_gate",
            Self::Scale => "scale",
        }
    }

    pub fn can_place_live_orders(self) -> bool {
        matches!(
            self,
            Self::FillCanary | Self::MakerMicro | Self::QuoteManager | Self::TakerGate
        )
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveAlphaConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: LiveAlphaMode,
    #[serde(default = "default_true")]
    pub approved_host_required: bool,
    #[serde(default = "default_true")]
    pub approved_wallet_required: bool,
    #[serde(default = "default_true")]
    pub geoblock_required: bool,
    #[serde(default = "default_true")]
    pub heartbeat_required: bool,
    #[serde(default)]
    pub risk: LiveAlphaRiskConfig,
    #[serde(default)]
    pub fill_canary: LiveAlphaFillCanaryConfig,
    #[serde(default)]
    pub maker: LiveAlphaMakerConfig,
    #[serde(default)]
    pub taker: LiveAlphaTakerConfig,
}

impl Default for LiveAlphaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: LiveAlphaMode::Disabled,
            approved_host_required: true,
            approved_wallet_required: true,
            geoblock_required: true,
            heartbeat_required: true,
            risk: LiveAlphaRiskConfig::default(),
            fill_canary: LiveAlphaFillCanaryConfig::default(),
            maker: LiveAlphaMakerConfig::default(),
            taker: LiveAlphaTakerConfig::default(),
        }
    }
}

impl LiveAlphaConfig {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        self.risk.validate(&mut errors);
        self.fill_canary.validate(&mut errors);
        self.maker.validate(&mut errors);
        self.taker.validate(&mut errors);

        if self.mode == LiveAlphaMode::Disabled
            && (self.fill_canary.enabled || self.maker.enabled || self.taker.enabled)
        {
            errors
                .push("live_alpha.mode=disabled requires submodes to remain disabled".to_string());
        }
        if self.taker.enabled {
            errors.push("live_alpha.taker.enabled must remain false during LA1".to_string());
        }
        if self.fill_canary.allow_fok {
            errors
                .push("live_alpha.fill_canary.allow_fok must remain false during LA1".to_string());
        }
        if self.fill_canary.allow_fak {
            errors
                .push("live_alpha.fill_canary.allow_fak must remain false during LA1".to_string());
        }
        if self.fill_canary.allow_marketable_limit {
            errors.push(
                "live_alpha.fill_canary.allow_marketable_limit must remain false during LA1"
                    .to_string(),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn inert_summary(&self) -> LiveAlphaInertSummary {
        LiveAlphaInertSummary {
            enabled: self.enabled,
            mode: self.mode,
            fill_canary_enabled: self.fill_canary.enabled,
            shadow_executor_enabled: self.mode == LiveAlphaMode::Shadow,
            maker_micro_enabled: self.maker.enabled,
            taker_enabled: self.taker.enabled,
            scale_enabled: self.mode == LiveAlphaMode::Scale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveAlphaInertSummary {
    pub enabled: bool,
    pub mode: LiveAlphaMode,
    pub fill_canary_enabled: bool,
    pub shadow_executor_enabled: bool,
    pub maker_micro_enabled: bool,
    pub taker_enabled: bool,
    pub scale_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveAlphaRiskConfig {
    #[serde(default)]
    pub max_wallet_funding_pusd: f64,
    #[serde(default)]
    pub max_available_pusd_usage: f64,
    #[serde(default)]
    pub max_reserved_pusd: f64,
    #[serde(default)]
    pub max_single_order_notional: f64,
    #[serde(default)]
    pub max_per_market_notional: f64,
    #[serde(default)]
    pub max_per_asset_notional: f64,
    #[serde(default)]
    pub max_total_live_notional: f64,
    #[serde(default)]
    pub max_open_orders: u64,
    #[serde(default)]
    pub max_open_orders_per_market: u64,
    #[serde(default)]
    pub max_open_orders_per_asset: u64,
    #[serde(default)]
    pub max_daily_realized_loss: f64,
    #[serde(default)]
    pub max_daily_unrealized_loss: f64,
    #[serde(default)]
    pub max_fee_spend: f64,
    #[serde(default)]
    pub max_submit_rate_per_min: u64,
    #[serde(default)]
    pub max_cancel_rate_per_min: u64,
    #[serde(default)]
    pub max_reconciliation_lag_ms: u64,
    #[serde(default)]
    pub max_book_staleness_ms: u64,
    #[serde(default)]
    pub max_reference_staleness_ms: u64,
    #[serde(default)]
    pub no_trade_seconds_before_close: u64,
}

impl Default for LiveAlphaRiskConfig {
    fn default() -> Self {
        Self {
            max_wallet_funding_pusd: 0.0,
            max_available_pusd_usage: 0.0,
            max_reserved_pusd: 0.0,
            max_single_order_notional: 0.0,
            max_per_market_notional: 0.0,
            max_per_asset_notional: 0.0,
            max_total_live_notional: 0.0,
            max_open_orders: 0,
            max_open_orders_per_market: 0,
            max_open_orders_per_asset: 0,
            max_daily_realized_loss: 0.0,
            max_daily_unrealized_loss: 0.0,
            max_fee_spend: 0.0,
            max_submit_rate_per_min: 0,
            max_cancel_rate_per_min: 0,
            max_reconciliation_lag_ms: 0,
            max_book_staleness_ms: 0,
            max_reference_staleness_ms: 0,
            no_trade_seconds_before_close: 0,
        }
    }
}

impl LiveAlphaRiskConfig {
    fn validate(&self, errors: &mut Vec<String>) {
        for (name, value) in [
            ("max_wallet_funding_pusd", self.max_wallet_funding_pusd),
            ("max_available_pusd_usage", self.max_available_pusd_usage),
            ("max_reserved_pusd", self.max_reserved_pusd),
            ("max_single_order_notional", self.max_single_order_notional),
            ("max_per_market_notional", self.max_per_market_notional),
            ("max_per_asset_notional", self.max_per_asset_notional),
            ("max_total_live_notional", self.max_total_live_notional),
            ("max_daily_realized_loss", self.max_daily_realized_loss),
            ("max_daily_unrealized_loss", self.max_daily_unrealized_loss),
            ("max_fee_spend", self.max_fee_spend),
        ] {
            require_non_negative_f64(errors, &format!("live_alpha.risk.{name}"), value);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveAlphaFillCanaryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub human_approval_required: bool,
    #[serde(default)]
    pub max_notional: f64,
    #[serde(default)]
    pub max_price: f64,
    #[serde(default)]
    pub allow_fok: bool,
    #[serde(default)]
    pub allow_fak: bool,
    #[serde(default)]
    pub allow_marketable_limit: bool,
}

impl Default for LiveAlphaFillCanaryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            human_approval_required: true,
            max_notional: 0.0,
            max_price: 0.0,
            allow_fok: false,
            allow_fak: false,
            allow_marketable_limit: false,
        }
    }
}

impl LiveAlphaFillCanaryConfig {
    fn validate(&self, errors: &mut Vec<String>) {
        require_non_negative_f64(
            errors,
            "live_alpha.fill_canary.max_notional",
            self.max_notional,
        );
        require_non_negative_f64(errors, "live_alpha.fill_canary.max_price", self.max_price);
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveAlphaMakerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub post_only: bool,
    #[serde(default = "default_gtd")]
    pub order_type: String,
    #[serde(default)]
    pub ttl_seconds: u64,
    #[serde(default)]
    pub min_edge_bps: u64,
    #[serde(default)]
    pub replace_tolerance_bps: u64,
    #[serde(default)]
    pub min_quote_lifetime_ms: u64,
}

impl Default for LiveAlphaMakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            post_only: true,
            order_type: default_gtd(),
            ttl_seconds: 0,
            min_edge_bps: 0,
            replace_tolerance_bps: 0,
            min_quote_lifetime_ms: 0,
        }
    }
}

impl LiveAlphaMakerConfig {
    fn validate(&self, errors: &mut Vec<String>) {
        if self.order_type != "GTD" {
            errors.push("live_alpha.maker.order_type must remain GTD during LA1".to_string());
        }
        if !self.post_only {
            errors.push("live_alpha.maker.post_only must remain true during LA1".to_string());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveAlphaTakerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub max_notional: f64,
    #[serde(default)]
    pub min_ev_after_all_costs_bps: u64,
    #[serde(default)]
    pub max_slippage_bps: u64,
    #[serde(default)]
    pub max_orders_per_day: u64,
}

impl Default for LiveAlphaTakerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_notional: 0.0,
            min_ev_after_all_costs_bps: 0,
            max_slippage_bps: 0,
            max_orders_per_day: 0,
        }
    }
}

impl LiveAlphaTakerConfig {
    fn validate(&self, errors: &mut Vec<String>) {
        require_non_negative_f64(errors, "live_alpha.taker.max_notional", self.max_notional);
    }
}

fn default_true() -> bool {
    true
}

fn default_gtd() -> String {
    "GTD".to_string()
}

fn require_non_negative_f64(errors: &mut Vec<String>, name: &str, value: f64) {
    if !value.is_finite() || value < 0.0 {
        errors.push(format!("{name} must be finite and non-negative"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_alpha_config_defaults_are_inert() {
        let config = LiveAlphaConfig::default();
        let summary = config.inert_summary();

        assert!(!summary.enabled);
        assert_eq!(summary.mode, LiveAlphaMode::Disabled);
        assert!(!summary.fill_canary_enabled);
        assert!(!summary.shadow_executor_enabled);
        assert!(!summary.maker_micro_enabled);
        assert!(!summary.taker_enabled);
        assert!(!summary.scale_enabled);
        assert_eq!(config.risk.max_open_orders, 0);
        assert_eq!(config.risk.max_single_order_notional, 0.0);
        config
            .validate()
            .expect("default live alpha config validates");
    }

    #[test]
    fn live_alpha_config_rejects_la1_taker_or_marketable_flags() {
        let mut config = LiveAlphaConfig::default();
        config.fill_canary.allow_fok = true;
        config.fill_canary.allow_fak = true;
        config.fill_canary.allow_marketable_limit = true;
        config.taker.enabled = true;

        let errors = config.validate().expect_err("LA1-disallowed flags fail");
        let rendered = errors.join(",");

        assert!(rendered.contains("allow_fok"));
        assert!(rendered.contains("allow_fak"));
        assert!(rendered.contains("allow_marketable_limit"));
        assert!(rendered.contains("taker.enabled"));
    }

    #[test]
    fn live_alpha_modes_report_order_capability_without_enabling_it() {
        assert!(!LiveAlphaMode::Disabled.can_place_live_orders());
        assert!(!LiveAlphaMode::Shadow.can_place_live_orders());
        assert!(LiveAlphaMode::FillCanary.can_place_live_orders());
        assert!(LiveAlphaMode::MakerMicro.can_place_live_orders());
    }
}
