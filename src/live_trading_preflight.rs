use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::live_beta_readback::ReadbackPreflightReport;

pub const MODULE: &str = "live_trading_preflight";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingPreflightArtifact {
    #[serde(flatten)]
    pub body: LiveTradingPreflightBody,
    pub artifact_hash: String,
}

impl LiveTradingPreflightArtifact {
    pub fn new(body: LiveTradingPreflightBody) -> Result<Self, LiveTradingPreflightError> {
        let artifact_hash = artifact_hash(&body)?;
        Ok(Self {
            body,
            artifact_hash,
        })
    }

    pub fn validate(&self) -> Result<(), LiveTradingPreflightError> {
        let expected_hash = artifact_hash(&self.body)?;
        if self.artifact_hash != expected_hash {
            return Err(LiveTradingPreflightError::HashMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingPreflightBody {
    pub baseline_id: String,
    pub run_id: String,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: String,
    pub mode: String,
    pub final_live_config_enabled: bool,
    pub deployment_host: String,
    pub approved_host: String,
    pub approved_jurisdiction: ApprovedJurisdiction,
    pub geoblock: LiveTradingGeoblockReadback,
    pub account_readback_status: String,
    pub account_baseline_hash: String,
    pub account_baseline_path: String,
    pub account_open_order_count: usize,
    pub account_trade_count: usize,
    pub account_position_count: usize,
    pub reserved_pusd_units: u64,
    pub available_pusd_units: u64,
    pub l2_secret_handles_present: bool,
    pub read_only_freshness: ReadOnlyFreshnessStatus,
    pub no_live_actions: NoLiveActions,
    pub status: String,
    pub block_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedJurisdiction {
    pub country: String,
    pub region: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingGeoblockReadback {
    pub status: String,
    pub country: Option<String>,
    pub region: Option<String>,
    pub source: String,
    pub error_redacted: bool,
}

impl LiveTradingGeoblockReadback {
    pub fn not_checked() -> Self {
        Self {
            status: "not_checked".to_string(),
            country: None,
            region: None,
            source: "not_checked_final_live_config_disabled_or_unapproved".to_string(),
            error_redacted: false,
        }
    }

    pub fn passed(country: Option<String>, region: Option<String>) -> Self {
        Self {
            status: "passed".to_string(),
            country,
            region,
            source: "https://polymarket.com/api/geoblock".to_string(),
            error_redacted: false,
        }
    }

    pub fn blocked(country: Option<String>, region: Option<String>) -> Self {
        Self {
            status: "blocked".to_string(),
            country,
            region,
            source: "https://polymarket.com/api/geoblock".to_string(),
            error_redacted: false,
        }
    }

    pub fn error() -> Self {
        Self {
            status: "error".to_string(),
            country: None,
            region: None,
            source: "https://polymarket.com/api/geoblock".to_string(),
            error_redacted: true,
        }
    }

    pub fn passed_status(&self) -> bool {
        self.status == "passed"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadOnlyFreshnessStatus {
    pub market_discovery: String,
    pub book: String,
    pub reference: String,
    pub predictive: String,
}

impl ReadOnlyFreshnessStatus {
    pub fn not_checked() -> Self {
        Self {
            market_discovery: "not_checked".to_string(),
            book: "not_checked".to_string(),
            reference: "not_checked".to_string(),
            predictive: "not_checked".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoLiveActions {
    pub submitted_orders: bool,
    pub signed_orders_for_submission: bool,
    pub submitted_cancels: bool,
    pub heartbeat_posts: bool,
    pub cap_writes: bool,
}

impl NoLiveActions {
    fn any_live_action(&self) -> bool {
        self.submitted_orders
            || self.signed_orders_for_submission
            || self.submitted_cancels
            || self.heartbeat_posts
            || self.cap_writes
    }
}

#[derive(Debug, Clone)]
pub struct LiveTradingPreflightInput<'a> {
    pub baseline_id: &'a str,
    pub run_id: &'a str,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: &'a str,
    pub read_only_mode: bool,
    pub final_live_config_enabled: bool,
    pub deployment_host: &'a str,
    pub approved_host: &'a str,
    pub approved_country: &'a str,
    pub approved_region: &'a str,
    pub geoblock: LiveTradingGeoblockReadback,
    pub readback_report: &'a ReadbackPreflightReport,
    pub account_baseline_hash: &'a str,
    pub account_baseline_path: &'a str,
    pub account_position_count: usize,
    pub l2_secret_handles_present: bool,
    pub read_only_freshness: ReadOnlyFreshnessStatus,
    pub no_live_actions: NoLiveActions,
}

#[derive(Debug)]
pub enum LiveTradingPreflightError {
    Serialize(serde_json::Error),
    HashMismatch,
}

impl Display for LiveTradingPreflightError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(source) => {
                write!(
                    formatter,
                    "live trading preflight serialize failed: {source}"
                )
            }
            Self::HashMismatch => write!(formatter, "live trading preflight hash mismatch"),
        }
    }
}

impl Error for LiveTradingPreflightError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize(source) => Some(source),
            Self::HashMismatch => None,
        }
    }
}

pub fn evaluate_live_trading_preflight(
    input: LiveTradingPreflightInput<'_>,
) -> Result<LiveTradingPreflightArtifact, LiveTradingPreflightError> {
    let mut block_reasons = Vec::new();

    if !input.read_only_mode {
        block_reasons.push("read_only_mode_required".to_string());
    }
    if !input.final_live_config_enabled {
        block_reasons.push("final_live_config_disabled".to_string());
    }
    if input.approved_host.trim().is_empty() {
        block_reasons.push("approved_host_not_configured".to_string());
    } else if input.deployment_host != input.approved_host {
        block_reasons.push("approved_host_mismatch".to_string());
    }
    push_jurisdiction_blocks(
        &mut block_reasons,
        &input.geoblock,
        input.approved_country,
        input.approved_region,
    );
    if input.readback_report.status != "passed" {
        block_reasons.push("account_readback_not_passed".to_string());
    }
    if !input.readback_report.live_network_enabled {
        block_reasons.push("account_readback_not_live_network".to_string());
    }
    if !input.l2_secret_handles_present {
        block_reasons.push("l2_secret_handles_not_present".to_string());
    }
    push_freshness_block(
        &mut block_reasons,
        "market_discovery",
        &input.read_only_freshness.market_discovery,
    );
    push_freshness_block(&mut block_reasons, "book", &input.read_only_freshness.book);
    push_freshness_block(
        &mut block_reasons,
        "reference",
        &input.read_only_freshness.reference,
    );
    push_freshness_block(
        &mut block_reasons,
        "predictive",
        &input.read_only_freshness.predictive,
    );
    if input.no_live_actions.any_live_action() {
        block_reasons.push("live_action_observed".to_string());
    }

    block_reasons.sort();
    block_reasons.dedup();
    let status = if block_reasons.is_empty() {
        "passed"
    } else {
        "blocked"
    };

    LiveTradingPreflightArtifact::new(LiveTradingPreflightBody {
        baseline_id: input.baseline_id.to_string(),
        run_id: input.run_id.to_string(),
        captured_at_ms: input.captured_at_ms,
        captured_at_rfc3339: input.captured_at_rfc3339.to_string(),
        mode: "read_only".to_string(),
        final_live_config_enabled: input.final_live_config_enabled,
        deployment_host: input.deployment_host.to_string(),
        approved_host: input.approved_host.to_string(),
        approved_jurisdiction: ApprovedJurisdiction {
            country: input.approved_country.to_string(),
            region: input.approved_region.to_string(),
        },
        geoblock: input.geoblock,
        account_readback_status: input.readback_report.status.to_string(),
        account_baseline_hash: input.account_baseline_hash.to_string(),
        account_baseline_path: input.account_baseline_path.to_string(),
        account_open_order_count: input.readback_report.open_order_count,
        account_trade_count: input.readback_report.trade_count,
        account_position_count: input.account_position_count,
        reserved_pusd_units: input.readback_report.reserved_pusd_units,
        available_pusd_units: input.readback_report.available_pusd_units,
        l2_secret_handles_present: input.l2_secret_handles_present,
        read_only_freshness: input.read_only_freshness,
        no_live_actions: input.no_live_actions,
        status: status.to_string(),
        block_reasons,
    })
}

pub fn live_trading_preflight_json(
    artifact: &LiveTradingPreflightArtifact,
) -> Result<String, LiveTradingPreflightError> {
    serde_json::to_string_pretty(artifact).map_err(LiveTradingPreflightError::Serialize)
}

fn push_jurisdiction_blocks(
    block_reasons: &mut Vec<String>,
    geoblock: &LiveTradingGeoblockReadback,
    approved_country: &str,
    approved_region: &str,
) {
    if !geoblock.passed_status() {
        block_reasons.push("geoblock_not_passed".to_string());
    }
    if approved_country.trim().is_empty() {
        block_reasons.push("approved_country_not_configured".to_string());
    } else if geoblock.country.as_deref() != Some(approved_country) {
        block_reasons.push("approved_country_mismatch".to_string());
    }
    if approved_region.trim().is_empty() {
        block_reasons.push("approved_region_not_configured".to_string());
    } else if geoblock.region.as_deref() != Some(approved_region) {
        block_reasons.push("approved_region_mismatch".to_string());
    }
}

fn push_freshness_block(block_reasons: &mut Vec<String>, label: &str, status: &str) {
    if status != "passed" {
        block_reasons.push(format!("{label}_freshness_not_passed"));
    }
}

fn artifact_hash(body: &LiveTradingPreflightBody) -> Result<String, LiveTradingPreflightError> {
    let bytes = serde_json::to_vec(body).map_err(LiveTradingPreflightError::Serialize)?;
    let hash = digest(&SHA256, &bytes);
    Ok(format!("sha256:{}", to_hex(hash.as_ref())))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_beta_readback::{self, ReadbackPreflightReport, ReadbackPrerequisites};

    #[test]
    fn live_trading_preflight_blocks_default_disabled_local_dry_run() {
        let readback = live_beta_readback::sample_readback_preflight(ReadbackPrerequisites {
            lb3_hold_released: true,
            legal_access_approved: false,
            deployment_geoblock_passed: false,
        })
        .expect("sample readback builds");

        let artifact =
            evaluate_live_trading_preflight(sample_input(&readback)).expect("artifact builds");

        assert_eq!(artifact.body.status, "blocked");
        assert!(artifact
            .body
            .block_reasons
            .contains(&"final_live_config_disabled".to_string()));
        assert!(artifact
            .body
            .block_reasons
            .contains(&"account_readback_not_passed".to_string()));
        assert!(!artifact.body.no_live_actions.submitted_orders);
        artifact.validate().expect("hash validates");
    }

    #[test]
    fn live_trading_preflight_passes_only_when_all_read_only_gates_pass() {
        let mut readback = live_beta_readback::sample_readback_preflight(ReadbackPrerequisites {
            lb3_hold_released: true,
            legal_access_approved: true,
            deployment_geoblock_passed: true,
        })
        .expect("sample readback builds");
        readback.live_network_enabled = true;

        let mut input = sample_input(&readback);
        input.final_live_config_enabled = true;
        input.deployment_host = "approved-host";
        input.approved_host = "approved-host";
        input.approved_country = "BR";
        input.approved_region = "SP";
        input.geoblock =
            LiveTradingGeoblockReadback::passed(Some("BR".to_string()), Some("SP".to_string()));
        input.l2_secret_handles_present = true;
        input.read_only_freshness = ReadOnlyFreshnessStatus {
            market_discovery: "passed".to_string(),
            book: "passed".to_string(),
            reference: "passed".to_string(),
            predictive: "passed".to_string(),
        };

        let artifact = evaluate_live_trading_preflight(input).expect("passing artifact builds");

        assert_eq!(artifact.body.status, "passed");
        assert!(artifact.body.block_reasons.is_empty());
    }

    #[test]
    fn live_trading_preflight_blocks_any_live_action_observed() {
        let mut readback = passing_readback_report();
        readback.live_network_enabled = true;
        let mut input = sample_input(&readback);
        input.final_live_config_enabled = true;
        input.deployment_host = "approved-host";
        input.approved_host = "approved-host";
        input.approved_country = "BR";
        input.approved_region = "SP";
        input.geoblock =
            LiveTradingGeoblockReadback::passed(Some("BR".to_string()), Some("SP".to_string()));
        input.l2_secret_handles_present = true;
        input.read_only_freshness = ReadOnlyFreshnessStatus {
            market_discovery: "passed".to_string(),
            book: "passed".to_string(),
            reference: "passed".to_string(),
            predictive: "passed".to_string(),
        };
        input.no_live_actions.cap_writes = true;

        let artifact = evaluate_live_trading_preflight(input).expect("blocked artifact builds");

        assert_eq!(artifact.body.status, "blocked");
        assert!(artifact
            .body
            .block_reasons
            .contains(&"live_action_observed".to_string()));
    }

    fn passing_readback_report() -> ReadbackPreflightReport {
        live_beta_readback::sample_readback_preflight(ReadbackPrerequisites {
            lb3_hold_released: true,
            legal_access_approved: true,
            deployment_geoblock_passed: true,
        })
        .expect("sample readback builds")
    }

    fn sample_input<'a>(readback: &'a ReadbackPreflightReport) -> LiveTradingPreflightInput<'a> {
        LiveTradingPreflightInput {
            baseline_id: "LT1-LOCAL-DRY-RUN",
            run_id: "run-1",
            captured_at_ms: 1_778_000_000_000,
            captured_at_rfc3339: "2026-05-12T00:00:00Z",
            read_only_mode: true,
            final_live_config_enabled: false,
            deployment_host: "local-host",
            approved_host: "",
            approved_country: "",
            approved_region: "",
            geoblock: LiveTradingGeoblockReadback::not_checked(),
            readback_report: readback,
            account_baseline_hash: "sha256:baseline",
            account_baseline_path: "artifacts/live_trading/LT1/account_baseline.redacted.json",
            account_position_count: 0,
            l2_secret_handles_present: false,
            read_only_freshness: ReadOnlyFreshnessStatus::not_checked(),
            no_live_actions: NoLiveActions::default(),
        }
    }
}
