use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::domain::Side;

pub const MODULE: &str = "live_trading_maker";
pub const LT4_SCHEMA_VERSION: &str = "lt4.live_trading_maker_shadow.v1";
pub const LT4_GTD_SECURITY_BUFFER_SECONDS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTradingMakerDryRunArtifact {
    #[serde(flatten)]
    pub body: LiveTradingMakerDryRunBody,
    pub artifact_hash: String,
}

impl LiveTradingMakerDryRunArtifact {
    pub fn new(body: LiveTradingMakerDryRunBody) -> Result<Self, LiveTradingMakerError> {
        let artifact_hash = artifact_hash(&body)?;
        Ok(Self {
            body,
            artifact_hash,
        })
    }

    pub fn validate(&self) -> Result<(), LiveTradingMakerError> {
        let expected = artifact_hash(&self.body)?;
        if self.artifact_hash != expected {
            return Err(LiveTradingMakerError::HashMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTradingMakerDryRunBody {
    pub schema_version: String,
    pub approval_id: String,
    pub run_id: String,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: String,
    pub docs_checked: Vec<String>,
    pub approval_artifact_path: String,
    pub dry_run_report_path: String,
    pub final_live_config_enabled: bool,
    pub deployment: MakerDeploymentSummary,
    pub account: MakerAccountSummary,
    pub baseline: MakerBaselineBinding,
    pub geoblock: MakerFreshnessStatus,
    pub heartbeat: MakerHeartbeatStatus,
    pub live_state: MakerLiveStateSummary,
    pub candidate: MakerOrderCandidate,
    pub caps: MakerCapSummary,
    pub shadow_comparison: MakerShadowComparison,
    pub no_submit_proof: MakerNoSubmitProof,
    pub status: String,
    pub block_reasons: Vec<String>,
    pub approval_review_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerDeploymentSummary {
    pub host: String,
    pub approved_host: String,
    pub approved_country: String,
    pub approved_region: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerAccountSummary {
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerBaselineBinding {
    pub baseline_id: String,
    pub baseline_capture_run_id: String,
    pub baseline_hash: String,
    pub baseline_artifact_path: String,
}

impl MakerBaselineBinding {
    fn is_complete(&self) -> bool {
        !self.baseline_id.trim().is_empty()
            && !self.baseline_capture_run_id.trim().is_empty()
            && !self.baseline_hash.trim().is_empty()
            && !self.baseline_artifact_path.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerFreshnessStatus {
    pub status: String,
    pub age_ms: Option<u64>,
    pub max_age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerHeartbeatStatus {
    pub required: bool,
    pub fresh: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerLiveStateSummary {
    pub unresolved_live_order_count: Option<u64>,
    pub unreviewed_incident_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MakerOrderCandidate {
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub order_type: String,
    pub post_only: bool,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub expiry_unix: u64,
    pub maker_fee_bps: Option<f64>,
    pub estimated_fee_pusd: Option<f64>,
    pub tick_size: Option<f64>,
    pub min_order_size: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub fair_probability: Option<f64>,
    pub edge_bps_at_submit: Option<f64>,
    pub min_edge_bps: f64,
    pub market_end_unix: u64,
    pub no_trade_seconds_before_close: u64,
    pub book_age_ms: Option<u64>,
    pub max_book_age_ms: u64,
    pub reference_age_ms: Option<u64>,
    pub max_reference_age_ms: u64,
    pub predictive_age_ms: Option<u64>,
    pub max_predictive_age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MakerCapSummary {
    pub max_orders: u64,
    pub max_open_orders: u64,
    pub max_single_order_notional_pusd: f64,
    pub required_collateral_allowance_units: u64,
    pub cap_writes: bool,
    pub cap_state_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerShadowComparison {
    pub status: String,
    pub paper_decision: String,
    pub live_decision: String,
    pub comparison: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerNoSubmitProof {
    pub not_submitted: bool,
    pub network_post_enabled: bool,
    pub network_cancel_enabled: bool,
    pub signed_order_for_submission: bool,
    pub raw_signature_generated: bool,
    pub order_submit_auth_headers_generated: bool,
    pub taker_submission_enabled: bool,
    pub batch_order_path_enabled: bool,
    pub cancel_all_path_enabled: bool,
    pub cap_writes: bool,
}

impl MakerNoSubmitProof {
    fn clean_lt4_dry_run() -> Self {
        Self {
            not_submitted: true,
            network_post_enabled: false,
            network_cancel_enabled: false,
            signed_order_for_submission: false,
            raw_signature_generated: false,
            order_submit_auth_headers_generated: false,
            taker_submission_enabled: false,
            batch_order_path_enabled: false,
            cancel_all_path_enabled: false,
            cap_writes: false,
        }
    }

    fn violated(&self) -> bool {
        !self.not_submitted
            || self.network_post_enabled
            || self.network_cancel_enabled
            || self.signed_order_for_submission
            || self.raw_signature_generated
            || self.order_submit_auth_headers_generated
            || self.taker_submission_enabled
            || self.batch_order_path_enabled
            || self.cancel_all_path_enabled
            || self.cap_writes
    }
}

#[derive(Debug, Clone)]
pub struct LiveTradingMakerDryRunInput<'a> {
    pub approval_id: &'a str,
    pub run_id: &'a str,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: &'a str,
    pub docs_checked: Vec<String>,
    pub approval_artifact_path: &'a str,
    pub dry_run_report_path: &'a str,
    pub final_live_config_enabled: bool,
    pub deployment: MakerDeploymentSummary,
    pub account: MakerAccountSummary,
    pub baseline: MakerBaselineBinding,
    pub geoblock: MakerFreshnessStatus,
    pub heartbeat: MakerHeartbeatStatus,
    pub live_state: MakerLiveStateSummary,
    pub candidate: MakerOrderCandidate,
    pub caps: MakerCapSummary,
    pub shadow_comparison: MakerShadowComparison,
}

#[derive(Debug)]
pub enum LiveTradingMakerError {
    Serialize(serde_json::Error),
    HashMismatch,
}

impl Display for LiveTradingMakerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(source) => {
                write!(
                    formatter,
                    "live trading maker dry-run serialize failed: {source}"
                )
            }
            Self::HashMismatch => write!(formatter, "live trading maker dry-run hash mismatch"),
        }
    }
}

impl Error for LiveTradingMakerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize(source) => Some(source),
            Self::HashMismatch => None,
        }
    }
}

pub fn build_live_trading_maker_dry_run(
    input: LiveTradingMakerDryRunInput<'_>,
) -> Result<LiveTradingMakerDryRunArtifact, LiveTradingMakerError> {
    let no_submit_proof = MakerNoSubmitProof::clean_lt4_dry_run();
    let mut block_reasons = evaluate_maker_candidate_blocks(&input);
    if no_submit_proof.violated() {
        block_reasons.push("live_write_observed".to_string());
    }
    block_reasons.sort();
    block_reasons.dedup();

    let status = if block_reasons.is_empty() {
        "passed"
    } else {
        "blocked"
    };

    LiveTradingMakerDryRunArtifact::new(LiveTradingMakerDryRunBody {
        schema_version: LT4_SCHEMA_VERSION.to_string(),
        approval_id: input.approval_id.to_string(),
        run_id: input.run_id.to_string(),
        captured_at_ms: input.captured_at_ms,
        captured_at_rfc3339: input.captured_at_rfc3339.to_string(),
        docs_checked: input.docs_checked,
        approval_artifact_path: input.approval_artifact_path.to_string(),
        dry_run_report_path: input.dry_run_report_path.to_string(),
        final_live_config_enabled: input.final_live_config_enabled,
        deployment: input.deployment,
        account: input.account,
        baseline: input.baseline,
        geoblock: input.geoblock,
        heartbeat: input.heartbeat,
        live_state: input.live_state,
        candidate: input.candidate,
        caps: input.caps,
        shadow_comparison: input.shadow_comparison,
        no_submit_proof,
        status: status.to_string(),
        block_reasons,
        approval_review_required: true,
    })
}

fn evaluate_maker_candidate_blocks(input: &LiveTradingMakerDryRunInput<'_>) -> Vec<String> {
    let mut reasons = Vec::new();
    let candidate = &input.candidate;

    if !input.final_live_config_enabled {
        reasons.push("final_live_config_disabled".to_string());
    }
    if input.deployment.approved_host.trim().is_empty()
        || input.deployment.host != input.deployment.approved_host
    {
        reasons.push("approved_host_not_matched".to_string());
    }
    if input.account.wallet_address.trim().is_empty()
        || input.account.funder_address.trim().is_empty()
        || input.account.signature_type.trim().is_empty()
    {
        reasons.push("account_binding_missing".to_string());
    }
    if !input.baseline.is_complete() {
        reasons.push("missing_baseline_binding".to_string());
    }
    if input.geoblock.status != "passed" {
        reasons.push("geoblock_not_passed".to_string());
    }
    if input
        .geoblock
        .age_ms
        .is_none_or(|age| age > input.geoblock.max_age_ms)
    {
        reasons.push("geoblock_stale_or_not_checked".to_string());
    }
    if input.heartbeat.required && !input.heartbeat.fresh {
        reasons.push("heartbeat_required_not_fresh".to_string());
    }
    match input.live_state.unresolved_live_order_count {
        Some(0) => {}
        Some(_) => reasons.push("existing_unresolved_live_order".to_string()),
        None => reasons.push("unresolved_live_order_state_unknown".to_string()),
    }
    if input.live_state.unreviewed_incident_count > 0 {
        reasons.push("unreviewed_incident".to_string());
    }
    if input.caps.max_orders != 1 {
        reasons.push("one_order_cap_not_bound".to_string());
    }
    if input.caps.max_open_orders != 1 {
        reasons.push("max_open_orders_not_one".to_string());
    }
    if !positive_finite(input.caps.max_single_order_notional_pusd) {
        reasons.push("max_single_order_notional_missing".to_string());
    }
    if input.caps.required_collateral_allowance_units == 0 {
        reasons.push("required_collateral_allowance_missing".to_string());
    }
    if input.caps.cap_writes {
        reasons.push("cap_write_not_allowed_in_lt4".to_string());
    }
    if !candidate.post_only {
        reasons.push("post_only_required".to_string());
    }
    if !order_type_supports_post_only(&candidate.order_type) {
        reasons.push("post_only_order_type_invalid".to_string());
    }
    if candidate.order_type.eq_ignore_ascii_case("GTD")
        && candidate.expiry_unix
            <= input
                .captured_at_ms
                .max(0)
                .unsigned_abs()
                .saturating_div(1_000)
                .saturating_add(LT4_GTD_SECURITY_BUFFER_SECONDS)
    {
        reasons.push("gtd_expiry_inside_security_threshold".to_string());
    }
    if !positive_finite(candidate.price) || candidate.price >= 1.0 {
        reasons.push("price_invalid".to_string());
    }
    if !positive_finite(candidate.size) {
        reasons.push("size_invalid".to_string());
    }
    if !positive_finite(candidate.notional) {
        reasons.push("notional_invalid".to_string());
    }
    match candidate.tick_size.filter(|tick| positive_finite(*tick)) {
        Some(tick_size) if !tick_aligned(candidate.price, tick_size) => {
            reasons.push("price_not_tick_aligned".to_string());
        }
        Some(_) => {}
        None => reasons.push("unknown_tick_size".to_string()),
    }
    let min_order_size = candidate
        .min_order_size
        .filter(|min_size| positive_finite(*min_size));
    match min_order_size {
        Some(min_order_size) if candidate.size + f64::EPSILON < min_order_size => {
            reasons.push("size_below_min_size".to_string());
        }
        Some(_) => {}
        None => reasons.push("unknown_min_size".to_string()),
    }
    if candidate
        .maker_fee_bps
        .is_none_or(|fee| !fee.is_finite() || fee < 0.0)
    {
        reasons.push("fee_not_known".to_string());
    }
    if candidate
        .edge_bps_at_submit
        .is_none_or(|edge| !edge.is_finite() || edge < candidate.min_edge_bps)
    {
        reasons.push("edge_at_submit_below_threshold".to_string());
    }
    if post_only_would_cross(
        candidate.side,
        candidate.price,
        candidate.best_bid,
        candidate.best_ask,
    ) {
        reasons.push("marketable_post_only_order".to_string());
    }
    if marketability_unknown(candidate.side, candidate.best_bid, candidate.best_ask) {
        reasons.push("post_only_marketability_unknown".to_string());
    }
    if candidate.market_end_unix
        <= input
            .captured_at_ms
            .max(0)
            .unsigned_abs()
            .saturating_div(1_000)
            .saturating_add(candidate.no_trade_seconds_before_close)
    {
        reasons.push("near_close_market".to_string());
    }
    if candidate
        .book_age_ms
        .is_none_or(|age| age > candidate.max_book_age_ms)
    {
        reasons.push("stale_book".to_string());
    }
    if candidate
        .reference_age_ms
        .is_none_or(|age| age > candidate.max_reference_age_ms)
    {
        reasons.push("stale_reference".to_string());
    }
    if candidate
        .predictive_age_ms
        .is_none_or(|age| age > candidate.max_predictive_age_ms)
    {
        reasons.push("stale_predictive".to_string());
    }
    if input.shadow_comparison.status != "comparable" {
        reasons.push("paper_shadow_comparison_not_feasible".to_string());
    }

    reasons
}

pub fn live_trading_maker_dry_run_json(
    artifact: &LiveTradingMakerDryRunArtifact,
) -> Result<String, LiveTradingMakerError> {
    serde_json::to_string_pretty(artifact).map_err(LiveTradingMakerError::Serialize)
}

pub fn live_trading_maker_approval_markdown(artifact: &LiveTradingMakerDryRunArtifact) -> String {
    let body = &artifact.body;
    let candidate = &body.candidate;
    let status = if body.status == "passed" {
        "LT4 DRY-RUN CANDIDATE - HUMAN REVIEW REQUIRED"
    } else {
        "LT4 BLOCKED - NOT APPROVED FOR SUBMISSION"
    };
    let block_reasons = if body.block_reasons.is_empty() {
        "none".to_string()
    } else {
        body.block_reasons.join(",")
    };
    format!(
        "\
# LT4 Maker Approval Candidate

Status: {status}

This artifact is for LT4 review only. It does not authorize LT5, does not submit orders, does not cancel orders, does not sign for submit, and does not write caps.

| field | value |
| --- | --- |
| approval_id | `{approval_id}` |
| artifact_hash | `{artifact_hash}` |
| run_id | `{run_id}` |
| captured_at_rfc3339 | `{captured_at_rfc3339}` |
| host | `{host}` |
| approved_host | `{approved_host}` |
| approved_country | `{approved_country}` |
| approved_region | `{approved_region}` |
| wallet | `{wallet}` |
| funder | `{funder}` |
| signature_type | `{signature_type}` |
| baseline_id | `{baseline_id}` |
| baseline_capture_run_id | `{baseline_capture_run_id}` |
| baseline_hash | `{baseline_hash}` |
| baseline_artifact_path | `{baseline_artifact_path}` |
| market_slug | `{market_slug}` |
| condition_id | `{condition_id}` |
| token_id | `{token_id}` |
| outcome | `{outcome}` |
| side | `{side}` |
| order_type | `{order_type}` |
| post_only | `{post_only}` |
| price | `{price}` |
| size | `{size}` |
| notional | `{notional}` |
| expiry_unix | `{expiry_unix}` |
| tick_size | `{tick_size}` |
| min_order_size | `{min_order_size}` |
| maker_fee_bps | `{maker_fee_bps}` |
| estimated_fee_pusd | `{estimated_fee_pusd}` |
| max_orders | `{max_orders}` |
| max_open_orders | `{max_open_orders}` |
| max_single_order_notional_pusd | `{max_single_order_notional}` |
| required_collateral_allowance_units | `{required_collateral_allowance_units}` |
| cap_writes | `{cap_writes}` |
| cap_state_path | `{cap_state_path}` |
| geoblock_status | `{geoblock_status}` |
| heartbeat_status | `{heartbeat_status}` |
| book_age_ms | `{book_age_ms}` |
| reference_age_ms | `{reference_age_ms}` |
| predictive_age_ms | `{predictive_age_ms}` |
| maker_status | `{maker_status}` |
| taker_status | `disabled_in_lt4` |
| shadow_comparison | `{shadow_comparison}` |
| not_submitted | `{not_submitted}` |
| network_post_enabled | `{network_post_enabled}` |
| network_cancel_enabled | `{network_cancel_enabled}` |
| signed_order_for_submission | `{signed_order_for_submission}` |
| raw_signature_generated | `{raw_signature_generated}` |
| order_submit_auth_headers_generated | `{order_submit_auth_headers_generated}` |
| batch_order_path_enabled | `{batch_order_path_enabled}` |
| cancel_all_path_enabled | `{cancel_all_path_enabled}` |
| block_reasons | `{block_reasons}` |
",
        approval_id = body.approval_id,
        artifact_hash = artifact.artifact_hash,
        run_id = body.run_id,
        captured_at_rfc3339 = body.captured_at_rfc3339,
        host = body.deployment.host,
        approved_host = field_or_blocked(&body.deployment.approved_host),
        approved_country = field_or_blocked(&body.deployment.approved_country),
        approved_region = field_or_blocked(&body.deployment.approved_region),
        wallet = field_or_blocked(&body.account.wallet_address),
        funder = field_or_blocked(&body.account.funder_address),
        signature_type = field_or_blocked(&body.account.signature_type),
        baseline_id = field_or_blocked(&body.baseline.baseline_id),
        baseline_capture_run_id = field_or_blocked(&body.baseline.baseline_capture_run_id),
        baseline_hash = field_or_blocked(&body.baseline.baseline_hash),
        baseline_artifact_path = field_or_blocked(&body.baseline.baseline_artifact_path),
        market_slug = field_or_blocked(&candidate.market_slug),
        condition_id = field_or_blocked(&candidate.condition_id),
        token_id = field_or_blocked(&candidate.token_id),
        outcome = field_or_blocked(&candidate.outcome),
        side = side_label(candidate.side),
        order_type = candidate.order_type,
        post_only = candidate.post_only,
        price = decimal_field(candidate.price),
        size = decimal_field(candidate.size),
        notional = decimal_field(candidate.notional),
        expiry_unix = candidate.expiry_unix,
        tick_size = option_decimal_field(candidate.tick_size),
        min_order_size = option_decimal_field(candidate.min_order_size),
        maker_fee_bps = option_decimal_field(candidate.maker_fee_bps),
        estimated_fee_pusd = option_decimal_field(candidate.estimated_fee_pusd),
        max_orders = body.caps.max_orders,
        max_open_orders = body.caps.max_open_orders,
        max_single_order_notional = decimal_field(body.caps.max_single_order_notional_pusd),
        required_collateral_allowance_units = body.caps.required_collateral_allowance_units,
        cap_writes = body.caps.cap_writes,
        cap_state_path = body.caps.cap_state_path,
        geoblock_status = body.geoblock.status,
        heartbeat_status = body.heartbeat.status,
        book_age_ms = option_u64_field(candidate.book_age_ms),
        reference_age_ms = option_u64_field(candidate.reference_age_ms),
        predictive_age_ms = option_u64_field(candidate.predictive_age_ms),
        maker_status = body.status,
        shadow_comparison = body.shadow_comparison.status,
        not_submitted = body.no_submit_proof.not_submitted,
        network_post_enabled = body.no_submit_proof.network_post_enabled,
        network_cancel_enabled = body.no_submit_proof.network_cancel_enabled,
        signed_order_for_submission = body.no_submit_proof.signed_order_for_submission,
        raw_signature_generated = body.no_submit_proof.raw_signature_generated,
        order_submit_auth_headers_generated = body.no_submit_proof.order_submit_auth_headers_generated,
        batch_order_path_enabled = body.no_submit_proof.batch_order_path_enabled,
        cancel_all_path_enabled = body.no_submit_proof.cancel_all_path_enabled,
        block_reasons = block_reasons,
    )
}

fn artifact_hash(body: &LiveTradingMakerDryRunBody) -> Result<String, LiveTradingMakerError> {
    let bytes = serde_json::to_vec(body).map_err(LiveTradingMakerError::Serialize)?;
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

fn positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn order_type_supports_post_only(order_type: &str) -> bool {
    order_type.eq_ignore_ascii_case("GTC") || order_type.eq_ignore_ascii_case("GTD")
}

fn post_only_would_cross(
    side: Side,
    price: f64,
    best_bid: Option<f64>,
    best_ask: Option<f64>,
) -> bool {
    match side {
        Side::Buy => best_ask.is_some_and(|ask| price >= ask),
        Side::Sell => best_bid.is_some_and(|bid| price <= bid),
    }
}

fn marketability_unknown(side: Side, best_bid: Option<f64>, best_ask: Option<f64>) -> bool {
    match side {
        Side::Buy => best_ask.is_none(),
        Side::Sell => best_bid.is_none(),
    }
}

fn tick_aligned(price: f64, tick_size: f64) -> bool {
    if !positive_finite(price) || !positive_finite(tick_size) {
        return false;
    }
    let ticks = price / tick_size;
    (ticks - ticks.round()).abs() < 1e-9
}

fn field_or_blocked(value: &str) -> String {
    if value.trim().is_empty() {
        "BLOCKED: missing".to_string()
    } else {
        value.to_string()
    }
}

fn decimal_field(value: f64) -> String {
    if value.is_finite() && value > 0.0 {
        trim_decimal(value)
    } else {
        "BLOCKED: unavailable".to_string()
    }
}

fn option_decimal_field(value: Option<f64>) -> String {
    value
        .filter(|inner| inner.is_finite() && *inner >= 0.0)
        .map(trim_decimal)
        .unwrap_or_else(|| "BLOCKED: unavailable".to_string())
}

fn option_u64_field(value: Option<u64>) -> String {
    value
        .map(|inner| inner.to_string())
        .unwrap_or_else(|| "BLOCKED: unavailable".to_string())
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

fn trim_decimal(value: f64) -> String {
    let mut rendered = format!("{value:.6}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.pop();
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_trading_maker_passes_complete_post_only_candidate_without_writes() {
        let artifact = build_live_trading_maker_dry_run(passing_input()).expect("artifact builds");

        artifact.validate().expect("hash validates");
        assert_eq!(artifact.body.status, "passed");
        assert!(artifact.body.block_reasons.is_empty());
        assert!(artifact.body.no_submit_proof.not_submitted);
        assert!(!artifact.body.no_submit_proof.network_post_enabled);
        assert!(!artifact.body.no_submit_proof.network_cancel_enabled);
        assert!(!artifact.body.no_submit_proof.signed_order_for_submission);
        assert!(!artifact.body.no_submit_proof.cap_writes);
    }

    #[test]
    fn live_trading_maker_blocks_marketable_post_only_order() {
        let mut input = passing_input();
        input.candidate.price = 0.51;
        input.candidate.best_ask = Some(0.50);

        let artifact = build_live_trading_maker_dry_run(input).expect("artifact builds");

        assert_eq!(artifact.body.status, "blocked");
        assert!(artifact
            .body
            .block_reasons
            .contains(&"marketable_post_only_order".to_string()));
    }

    #[test]
    fn live_trading_maker_blocks_missing_baseline_unknown_tick_and_stale_state() {
        let mut input = passing_input();
        input.baseline.baseline_id.clear();
        input.candidate.tick_size = None;
        input.candidate.min_order_size = None;
        input.candidate.book_age_ms = Some(6_000);
        input.candidate.reference_age_ms = None;
        input.candidate.predictive_age_ms = Some(6_000);
        input.live_state.unresolved_live_order_count = None;

        let artifact = build_live_trading_maker_dry_run(input).expect("artifact builds");

        for expected in [
            "missing_baseline_binding",
            "unknown_tick_size",
            "unknown_min_size",
            "stale_book",
            "stale_reference",
            "stale_predictive",
            "unresolved_live_order_state_unknown",
        ] {
            assert!(
                artifact.body.block_reasons.contains(&expected.to_string()),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn live_trading_maker_approval_markdown_contains_required_envelope_fields() {
        let artifact = build_live_trading_maker_dry_run(passing_input()).expect("artifact builds");
        let markdown = live_trading_maker_approval_markdown(&artifact);

        for field in [
            "host",
            "wallet",
            "funder",
            "baseline_id",
            "market_slug",
            "side",
            "order_type",
            "price",
            "size",
            "expiry_unix",
            "maker_fee_bps",
            "max_orders",
            "not_submitted",
            "network_post_enabled",
        ] {
            assert!(markdown.contains(field), "missing field {field}");
        }
    }

    fn passing_input<'a>() -> LiveTradingMakerDryRunInput<'a> {
        LiveTradingMakerDryRunInput {
            approval_id: "LT4-LOCAL-DRY-RUN",
            run_id: "run-1",
            captured_at_ms: 1_777_909_000_000,
            captured_at_rfc3339: "2026-05-05T00:00:00Z",
            docs_checked: vec!["https://docs.polymarket.com/trading/orders/overview".to_string()],
            approval_artifact_path: "verification/lt4-approval.md",
            dry_run_report_path:
                "artifacts/live_trading/LT4-LOCAL-DRY-RUN/maker_dry_run.redacted.json",
            final_live_config_enabled: true,
            deployment: MakerDeploymentSummary {
                host: "approved-host".to_string(),
                approved_host: "approved-host".to_string(),
                approved_country: "BR".to_string(),
                approved_region: "SP".to_string(),
            },
            account: MakerAccountSummary {
                wallet_address: "0x1111111111111111111111111111111111111111".to_string(),
                funder_address: "0x2222222222222222222222222222222222222222".to_string(),
                signature_type: "poly_proxy".to_string(),
            },
            baseline: MakerBaselineBinding {
                baseline_id: "LT1-BASELINE-001".to_string(),
                baseline_capture_run_id: "baseline-run-1".to_string(),
                baseline_hash: "sha256:abc123".to_string(),
                baseline_artifact_path: "artifacts/live_trading/LT1/account_baseline.redacted.json"
                    .to_string(),
            },
            geoblock: MakerFreshnessStatus {
                status: "passed".to_string(),
                age_ms: Some(1_000),
                max_age_ms: 30_000,
            },
            heartbeat: MakerHeartbeatStatus {
                required: true,
                fresh: true,
                status: "fresh".to_string(),
            },
            live_state: MakerLiveStateSummary {
                unresolved_live_order_count: Some(0),
                unreviewed_incident_count: 0,
            },
            candidate: MakerOrderCandidate {
                market_slug: "btc-updown-15m-1777909200".to_string(),
                condition_id: "0xcondition".to_string(),
                token_id: "123456".to_string(),
                outcome: "Up".to_string(),
                side: Side::Buy,
                order_type: "GTD".to_string(),
                post_only: true,
                price: 0.49,
                size: 5.0,
                notional: 2.45,
                expiry_unix: 1_777_909_090,
                maker_fee_bps: Some(0.0),
                estimated_fee_pusd: Some(0.0),
                tick_size: Some(0.01),
                min_order_size: Some(5.0),
                best_bid: Some(0.48),
                best_ask: Some(0.50),
                fair_probability: Some(0.52),
                edge_bps_at_submit: Some(300.0),
                min_edge_bps: 100.0,
                market_end_unix: 1_777_909_900,
                no_trade_seconds_before_close: 600,
                book_age_ms: Some(1_000),
                max_book_age_ms: 5_000,
                reference_age_ms: Some(1_000),
                max_reference_age_ms: 5_000,
                predictive_age_ms: Some(1_000),
                max_predictive_age_ms: 5_000,
            },
            caps: MakerCapSummary {
                max_orders: 1,
                max_open_orders: 1,
                max_single_order_notional_pusd: 5.0,
                required_collateral_allowance_units: 1_000_000,
                cap_writes: false,
                cap_state_path: "not_written_in_lt4".to_string(),
            },
            shadow_comparison: MakerShadowComparison {
                status: "comparable".to_string(),
                paper_decision: "maker_place_quote".to_string(),
                live_decision: "maker_place_quote".to_string(),
                comparison: "matched".to_string(),
            },
        }
    }
}
