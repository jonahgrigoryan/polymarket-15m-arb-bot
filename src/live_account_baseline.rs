use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::live_beta_readback::{
    AccountPreflight, AuthenticatedReadbackPreflightEvidence, BalanceAllowanceReadback,
    OpenOrderReadback, ReadbackPreflightReport, TradeReadback, TradeReadbackStatus,
};
use crate::live_reconciliation::{
    reconcile_live_state, LiveReconciliationInput, LiveReconciliationResult,
};

pub const MODULE: &str = "live_account_baseline";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBaselineArtifact {
    #[serde(flatten)]
    pub body: AccountBaselineBody,
    pub baseline_hash: String,
}

impl AccountBaselineArtifact {
    pub fn new(body: AccountBaselineBody) -> Result<Self, AccountBaselineError> {
        let baseline_hash = baseline_hash(&body)?;
        Ok(Self {
            body,
            baseline_hash,
        })
    }

    pub fn validate(&self) -> Result<(), AccountBaselineError> {
        validate_baseline_counts(self)?;
        let expected_hash = baseline_hash(&self.body)?;
        if self.baseline_hash != expected_hash {
            return Err(AccountBaselineError::HashMismatch);
        }
        Ok(())
    }

    pub fn trade_ids(&self) -> BTreeSet<String> {
        self.body
            .trades
            .iter()
            .map(|trade| trade.id.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBaselineBody {
    pub baseline_id: String,
    pub run_id: String,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: String,
    pub readback_report: BaselineReadbackReport,
    pub collateral: BaselineCollateral,
    pub open_orders: Vec<BaselineOpenOrder>,
    pub trades: Vec<BaselineTrade>,
    pub positions: BaselinePositions,
    pub no_secrets_guarantee: NoSecretsGuarantee,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineReadbackReport {
    pub status: String,
    pub block_reasons: Vec<String>,
    pub open_order_count: usize,
    pub trade_count: usize,
    pub reserved_pusd_units: u64,
    pub required_collateral_allowance_units: u64,
    pub available_pusd_units: u64,
    pub venue_state: String,
    pub heartbeat: String,
    pub live_network_enabled: bool,
}

impl From<&ReadbackPreflightReport> for BaselineReadbackReport {
    fn from(report: &ReadbackPreflightReport) -> Self {
        Self {
            status: report.status.to_string(),
            block_reasons: report
                .block_reasons
                .iter()
                .map(|reason| (*reason).to_string())
                .collect(),
            open_order_count: report.open_order_count,
            trade_count: report.trade_count,
            reserved_pusd_units: report.reserved_pusd_units,
            required_collateral_allowance_units: report.required_collateral_allowance_units,
            available_pusd_units: report.available_pusd_units,
            venue_state: report.venue_state.to_string(),
            heartbeat: report.heartbeat.to_string(),
            live_network_enabled: report.live_network_enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineCollateral {
    pub asset_type: String,
    pub token_id: Option<String>,
    pub balance_units: u64,
    pub allowance_units: u64,
}

impl From<&BalanceAllowanceReadback> for BaselineCollateral {
    fn from(collateral: &BalanceAllowanceReadback) -> Self {
        Self {
            asset_type: collateral.asset_type.as_str().to_string(),
            token_id: collateral.token_id.clone(),
            balance_units: collateral.balance_units,
            allowance_units: collateral.allowance_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineOpenOrder {
    pub id: String,
    pub status: String,
    pub maker_address: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub original_size_units: u64,
    pub size_matched_units: u64,
    pub remaining_size_units: u64,
    pub price: String,
    pub outcome: String,
    pub expiration: String,
    pub order_type: String,
    pub associate_trades: Vec<String>,
    pub created_at: i64,
}

impl From<&OpenOrderReadback> for BaselineOpenOrder {
    fn from(order: &OpenOrderReadback) -> Self {
        Self {
            id: order.id.clone(),
            status: order.status.as_str().to_string(),
            maker_address: order.maker_address.clone(),
            market: order.market.clone(),
            asset_id: order.asset_id.clone(),
            side: order.side.clone(),
            original_size_units: order.original_size_units,
            size_matched_units: order.size_matched_units,
            remaining_size_units: order.remaining_size_units(),
            price: order.price.clone(),
            outcome: order.outcome.clone(),
            expiration: order.expiration.clone(),
            order_type: order.order_type.clone(),
            associate_trades: order.associate_trades.clone(),
            created_at: order.created_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineTrade {
    pub id: String,
    pub market: String,
    pub asset_id: String,
    pub status: String,
    pub transaction_hash: Option<String>,
    pub maker_address: String,
    pub order_id: Option<String>,
}

impl From<&TradeReadback> for BaselineTrade {
    fn from(trade: &TradeReadback) -> Self {
        Self {
            id: trade.id.clone(),
            market: trade.market.clone(),
            asset_id: trade.asset_id.clone(),
            status: trade_status_label(trade.status).to_string(),
            transaction_hash: trade.transaction_hash.clone(),
            maker_address: trade.maker_address.clone(),
            order_id: trade.order_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselinePositions {
    pub evidence_complete: bool,
    pub positions: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoSecretsGuarantee {
    pub contains_auth_headers: bool,
    pub contains_l2_api_credentials: bool,
    pub contains_signed_payloads: bool,
    pub contains_private_keys: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct La7BaselineGateReport {
    pub status: &'static str,
    pub block_reasons: Vec<&'static str>,
}

impl La7BaselineGateReport {
    pub fn passed(&self) -> bool {
        self.block_reasons.is_empty()
    }
}

#[derive(Debug)]
pub enum AccountBaselineError {
    Read(std::io::Error),
    Serialize(serde_json::Error),
    Parse(serde_json::Error),
    HashMismatch,
    CountMismatch(&'static str),
}

impl Display for AccountBaselineError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(source) => write!(formatter, "baseline artifact read failed: {source}"),
            Self::Serialize(source) => {
                write!(formatter, "baseline artifact serialize failed: {source}")
            }
            Self::Parse(source) => write!(formatter, "baseline artifact parse failed: {source}"),
            Self::HashMismatch => write!(formatter, "baseline artifact hash mismatch"),
            Self::CountMismatch(field) => {
                write!(formatter, "baseline artifact count mismatch: {field}")
            }
        }
    }
}

impl Error for AccountBaselineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Serialize(source) | Self::Parse(source) => Some(source),
            Self::HashMismatch | Self::CountMismatch(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AccountBaselineBinding<'a> {
    pub expected_baseline_id: &'a str,
    pub expected_capture_run_id: &'a str,
    pub current_account: &'a AccountPreflight,
    pub current_evidence: &'a AuthenticatedReadbackPreflightEvidence,
}

pub fn build_account_baseline_artifact(
    baseline_id: String,
    run_id: String,
    captured_at_ms: i64,
    captured_at_rfc3339: String,
    account: &AccountPreflight,
    evidence: &AuthenticatedReadbackPreflightEvidence,
    position_evidence_complete: bool,
) -> Result<AccountBaselineArtifact, AccountBaselineError> {
    build_account_baseline_artifact_with_positions(
        baseline_id,
        run_id,
        captured_at_ms,
        captured_at_rfc3339,
        account,
        evidence,
        BaselinePositions {
            evidence_complete: position_evidence_complete,
            positions: Vec::new(),
        },
    )
}

pub fn build_account_baseline_artifact_with_positions(
    baseline_id: String,
    run_id: String,
    captured_at_ms: i64,
    captured_at_rfc3339: String,
    account: &AccountPreflight,
    evidence: &AuthenticatedReadbackPreflightEvidence,
    positions: BaselinePositions,
) -> Result<AccountBaselineArtifact, AccountBaselineError> {
    let mut open_orders = evidence
        .open_orders
        .iter()
        .map(BaselineOpenOrder::from)
        .collect::<Vec<_>>();
    open_orders.sort_by(|left, right| left.id.cmp(&right.id));
    let mut trades = evidence
        .trades
        .iter()
        .map(BaselineTrade::from)
        .collect::<Vec<_>>();
    trades.sort_by(|left, right| left.id.cmp(&right.id));

    AccountBaselineArtifact::new(AccountBaselineBody {
        baseline_id,
        run_id,
        captured_at_ms,
        captured_at_rfc3339,
        wallet_address: account.wallet_address.clone(),
        funder_address: account.funder_address.clone(),
        signature_type: account.signature_type.as_config_str().to_string(),
        readback_report: BaselineReadbackReport::from(&evidence.report),
        collateral: BaselineCollateral::from(&evidence.collateral),
        open_orders,
        trades,
        positions,
        no_secrets_guarantee: NoSecretsGuarantee::default(),
    })
}

pub fn account_baseline_json(
    artifact: &AccountBaselineArtifact,
) -> Result<String, AccountBaselineError> {
    serde_json::to_string_pretty(artifact).map_err(AccountBaselineError::Serialize)
}

pub fn parse_account_baseline_json(
    json: &str,
) -> Result<AccountBaselineArtifact, AccountBaselineError> {
    let artifact: AccountBaselineArtifact =
        serde_json::from_str(json).map_err(AccountBaselineError::Parse)?;
    artifact.validate()?;
    Ok(artifact)
}

pub fn load_account_baseline_artifact(
    path: impl AsRef<Path>,
) -> Result<AccountBaselineArtifact, AccountBaselineError> {
    let json = fs::read_to_string(path).map_err(AccountBaselineError::Read)?;
    parse_account_baseline_json(&json)
}

pub fn evaluate_la7_live_baseline_gate(
    current_report: &ReadbackPreflightReport,
    baseline: Option<&AccountBaselineArtifact>,
) -> Result<La7BaselineGateReport, AccountBaselineError> {
    let mut block_reasons = Vec::new();

    if current_report.trade_count > 0 && baseline.is_none() {
        block_reasons.push("baseline_artifact_required_for_history");
    }

    if let Some(baseline) = baseline {
        baseline.validate()?;
        if baseline.body.readback_report.status != "passed" {
            block_reasons.push("baseline_readback_not_passed");
        }
        if !baseline.body.readback_report.live_network_enabled {
            block_reasons.push("baseline_not_live_network");
        }
        if baseline.body.readback_report.open_order_count != 0 {
            block_reasons.push("baseline_open_orders_nonzero");
        }
        if baseline.body.readback_report.reserved_pusd_units != 0 {
            block_reasons.push("baseline_reserved_pusd_nonzero");
        }
        if !baseline.body.positions.evidence_complete {
            block_reasons.push("baseline_position_evidence_incomplete");
        }
        if !baseline.body.positions.positions.is_empty() {
            block_reasons.push("baseline_positions_nonzero");
        }
    }

    let status = if block_reasons.is_empty() {
        "passed"
    } else {
        "blocked"
    };
    Ok(La7BaselineGateReport {
        status,
        block_reasons,
    })
}

pub fn evaluate_la7_live_baseline_binding(
    binding: AccountBaselineBinding<'_>,
    baseline: Option<&AccountBaselineArtifact>,
) -> Result<La7BaselineGateReport, AccountBaselineError> {
    let mut report = evaluate_la7_live_baseline_gate(&binding.current_evidence.report, baseline)?;
    let Some(baseline) = baseline else {
        return Ok(report);
    };

    if !binding.expected_baseline_id.trim().is_empty()
        && baseline.body.baseline_id != binding.expected_baseline_id
    {
        report.block_reasons.push("baseline_id_mismatch");
    }
    if !binding.expected_capture_run_id.trim().is_empty()
        && baseline.body.run_id != binding.expected_capture_run_id
    {
        report
            .block_reasons
            .push("baseline_capture_run_id_mismatch");
    }
    if !eq_address(
        &baseline.body.wallet_address,
        &binding.current_account.wallet_address,
    ) {
        report.block_reasons.push("baseline_wallet_mismatch");
    }
    if !eq_address(
        &baseline.body.funder_address,
        &binding.current_account.funder_address,
    ) {
        report.block_reasons.push("baseline_funder_mismatch");
    }
    if baseline.body.signature_type != binding.current_account.signature_type.as_config_str() {
        report
            .block_reasons
            .push("baseline_signature_type_mismatch");
    }
    if binding.current_evidence.report.status != "passed" {
        report.block_reasons.push("current_readback_not_passed");
    }
    if !binding.current_evidence.report.live_network_enabled {
        report
            .block_reasons
            .push("current_readback_not_live_network");
    }
    if binding.current_evidence.report.open_order_count
        != binding.current_evidence.open_orders.len()
        || binding.current_evidence.report.trade_count != binding.current_evidence.trades.len()
    {
        report.block_reasons.push("current_readback_count_mismatch");
    }
    if binding.current_evidence.report.open_order_count != 0 {
        report.block_reasons.push("current_open_orders_nonzero");
    }
    if binding.current_evidence.report.reserved_pusd_units != 0 {
        report.block_reasons.push("current_reserved_pusd_nonzero");
    }

    let current_trade_ids = binding
        .current_evidence
        .trades
        .iter()
        .map(|trade| trade.id.clone())
        .collect::<BTreeSet<_>>();
    if !baseline
        .trade_ids()
        .iter()
        .all(|trade_id| current_trade_ids.contains(trade_id))
    {
        report
            .block_reasons
            .push("baseline_trade_missing_from_current_readback");
    }

    report.block_reasons.sort_unstable();
    report.block_reasons.dedup();
    report.status = if report.block_reasons.is_empty() {
        "passed"
    } else {
        "blocked"
    };
    Ok(report)
}

pub fn reconcile_live_state_with_account_baseline(
    mut input: LiveReconciliationInput,
    baseline: &AccountBaselineArtifact,
) -> Result<LiveReconciliationResult, AccountBaselineError> {
    baseline.validate()?;
    let baseline_trade_ids = baseline.trade_ids();
    input
        .venue
        .trades
        .retain(|trade_id, _| !baseline_trade_ids.contains(trade_id));
    input
        .local
        .known_trades
        .retain(|trade_id| !baseline_trade_ids.contains(trade_id));
    input
        .local
        .trade_order_ids_by_trade
        .retain(|trade_id, _| !baseline_trade_ids.contains(trade_id));
    input.local.trade_order_ids = input
        .local
        .trade_order_ids_by_trade
        .values()
        .cloned()
        .collect();
    Ok(reconcile_live_state(input))
}

fn validate_baseline_counts(
    artifact: &AccountBaselineArtifact,
) -> Result<(), AccountBaselineError> {
    if artifact.body.readback_report.open_order_count != artifact.body.open_orders.len() {
        return Err(AccountBaselineError::CountMismatch("open_orders"));
    }
    if artifact.body.readback_report.trade_count != artifact.body.trades.len() {
        return Err(AccountBaselineError::CountMismatch("trades"));
    }
    Ok(())
}

fn baseline_hash(body: &AccountBaselineBody) -> Result<String, AccountBaselineError> {
    let bytes = serde_json::to_vec(body).map_err(AccountBaselineError::Serialize)?;
    Ok(format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, &bytes).as_ref())
    ))
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn trade_status_label(status: TradeReadbackStatus) -> &'static str {
    match status {
        TradeReadbackStatus::Matched => "matched",
        TradeReadbackStatus::Mined => "mined",
        TradeReadbackStatus::Confirmed => "confirmed",
        TradeReadbackStatus::Retrying => "retrying",
        TradeReadbackStatus::Failed => "failed",
        TradeReadbackStatus::Unknown => "unknown",
    }
}

fn eq_address(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::live_beta_readback::{AssetType, SignatureType, TradeReadbackStatus};
    use crate::live_reconciliation::{
        LiveReconciliationInput, LiveReconciliationMismatch, LocalLiveState, VenueLiveState,
        VenueTradeState, VenueTradeStatus,
    };

    #[test]
    fn live_account_baseline_artifact_contains_ids_and_no_secret_values() {
        let artifact = sample_artifact(false);
        let json = account_baseline_json(&artifact).expect("baseline serializes");

        assert!(json.contains("trade-baseline-1"));
        assert!(json.contains("order-baseline-1"));
        assert!(json.contains("\"contains_auth_headers\": false"));
        assert!(!json.contains("super-secret-value"));
        assert!(!json.contains("signed-payload-value"));
        assert!(!json.contains("private-key-value"));
    }

    #[test]
    fn live_account_baseline_count_and_hash_validation_work() {
        let mut artifact = sample_artifact(false);
        artifact.validate().expect("valid baseline passes");

        artifact.body.readback_report.trade_count += 1;
        let count_error = artifact.validate().expect_err("count drift fails");
        assert!(matches!(
            count_error,
            AccountBaselineError::CountMismatch("trades")
        ));

        let mut artifact = sample_artifact(false);
        artifact.baseline_hash = "sha256:wrong".to_string();
        let hash_error = artifact.validate().expect_err("hash drift fails");
        assert!(matches!(hash_error, AccountBaselineError::HashMismatch));
    }

    #[test]
    fn live_account_baseline_round_trips_from_json() {
        let artifact = sample_artifact(false);
        let json = account_baseline_json(&artifact).expect("baseline serializes");
        let parsed = parse_account_baseline_json(&json).expect("baseline parses");

        assert_eq!(parsed, artifact);
        parsed.validate().expect("parsed baseline validates");
    }

    #[test]
    fn baseline_trade_ids_do_not_trigger_unexpected_fill_when_explicitly_baselined() {
        let artifact = sample_artifact(true);
        let result = reconcile_live_state_with_account_baseline(
            reconciliation_input_with_venue_trade("trade-baseline-1", "order-baseline-1"),
            &artifact,
        )
        .expect("baseline-aware reconciliation runs");

        assert!(matches!(result, LiveReconciliationResult::Passed { .. }));
    }

    #[test]
    fn new_unbaselined_trade_after_capture_still_triggers_unexpected_fill() {
        let artifact = sample_artifact(true);
        let result = reconcile_live_state_with_account_baseline(
            reconciliation_input_with_venue_trade("trade-new-1", "order-new-1"),
            &artifact,
        )
        .expect("baseline-aware reconciliation runs");

        assert_eq!(
            result.mismatches(),
            &[LiveReconciliationMismatch::UnexpectedFill]
        );
    }

    #[test]
    fn live_capable_la7_blocks_without_baseline_when_history_exists() {
        let report = sample_report(23, 0, 0);
        let gate = evaluate_la7_live_baseline_gate(&report, None).expect("baseline gate evaluates");

        assert_eq!(gate.status, "blocked");
        assert_eq!(
            gate.block_reasons,
            vec!["baseline_artifact_required_for_history"]
        );
    }

    #[test]
    fn live_capable_la7_blocks_when_position_evidence_is_incomplete() {
        let report = sample_report(23, 0, 0);
        let artifact = sample_artifact(false);
        let gate = evaluate_la7_live_baseline_gate(&report, Some(&artifact))
            .expect("baseline gate evaluates");

        assert_eq!(gate.status, "blocked");
        assert!(gate
            .block_reasons
            .contains(&"baseline_position_evidence_incomplete"));
    }

    #[test]
    fn live_capable_la7_blocks_when_positions_are_nonzero() {
        let report = sample_report(23, 0, 0);
        let mut artifact = sample_artifact(true);
        artifact
            .body
            .positions
            .positions
            .push(serde_json::json!({"asset": "token-1", "size": 1.0}));
        artifact = AccountBaselineArtifact::new(artifact.body).expect("baseline rehashes");

        let gate = evaluate_la7_live_baseline_gate(&report, Some(&artifact))
            .expect("baseline gate evaluates");

        assert_eq!(gate.status, "blocked");
        assert!(gate.block_reasons.contains(&"baseline_positions_nonzero"));
    }

    #[test]
    fn live_capable_la7_blocks_on_unsafe_baseline_report_fields() {
        let report = sample_report(23, 0, 0);
        let mut artifact = sample_artifact(true);
        artifact.body.readback_report.status = "blocked".to_string();
        artifact.body.readback_report.live_network_enabled = false;
        artifact.body.readback_report.open_order_count = 1;
        artifact.body.open_orders.push(BaselineOpenOrder {
            id: "order-open-1".to_string(),
            status: "live".to_string(),
            maker_address: artifact.body.funder_address.clone(),
            market: "market-1".to_string(),
            asset_id: "token-1".to_string(),
            side: "BUY".to_string(),
            original_size_units: 1,
            size_matched_units: 0,
            remaining_size_units: 1,
            price: "0.01".to_string(),
            outcome: "YES".to_string(),
            expiration: "1777000000".to_string(),
            order_type: "GTD".to_string(),
            associate_trades: Vec::new(),
            created_at: 1_777_000_000,
        });
        artifact.body.readback_report.reserved_pusd_units = 1;
        artifact = AccountBaselineArtifact::new(artifact.body).expect("baseline rehashes");

        let gate = evaluate_la7_live_baseline_gate(&report, Some(&artifact))
            .expect("baseline gate evaluates");

        assert_eq!(gate.status, "blocked");
        assert!(gate.block_reasons.contains(&"baseline_readback_not_passed"));
        assert!(gate.block_reasons.contains(&"baseline_not_live_network"));
        assert!(gate.block_reasons.contains(&"baseline_open_orders_nonzero"));
        assert!(gate
            .block_reasons
            .contains(&"baseline_reserved_pusd_nonzero"));
    }

    #[test]
    fn live_capable_la7_baseline_binding_requires_exact_config_and_account_match() {
        let account = sample_account();
        let evidence = sample_evidence(&account, 1);
        let mut artifact = build_account_baseline_artifact(
            "wrong-baseline".to_string(),
            "wrong-run".to_string(),
            1_777_000_000_000,
            "2026-05-08T00:00:00Z".to_string(),
            &account,
            &evidence,
            true,
        )
        .expect("baseline builds");
        artifact.body.wallet_address = "0x1111111111111111111111111111111111111111".to_string();
        artifact = AccountBaselineArtifact::new(artifact.body).expect("baseline rehashes");

        let gate = evaluate_la7_live_baseline_binding(
            AccountBaselineBinding {
                expected_baseline_id: "baseline-1",
                expected_capture_run_id: "run-1",
                current_account: &account,
                current_evidence: &evidence,
            },
            Some(&artifact),
        )
        .expect("baseline binding evaluates");

        assert_eq!(gate.status, "blocked");
        assert!(gate.block_reasons.contains(&"baseline_id_mismatch"));
        assert!(gate
            .block_reasons
            .contains(&"baseline_capture_run_id_mismatch"));
        assert!(gate.block_reasons.contains(&"baseline_wallet_mismatch"));
    }

    #[test]
    fn live_capable_la7_baseline_binding_passes_only_with_complete_position_evidence() {
        let account = sample_account();
        let evidence = sample_evidence(&account, 1);
        let artifact = build_account_baseline_artifact(
            "baseline-1".to_string(),
            "run-1".to_string(),
            1_777_000_000_000,
            "2026-05-08T00:00:00Z".to_string(),
            &account,
            &evidence,
            true,
        )
        .expect("baseline builds");

        let gate = evaluate_la7_live_baseline_binding(
            AccountBaselineBinding {
                expected_baseline_id: "baseline-1",
                expected_capture_run_id: "run-1",
                current_account: &account,
                current_evidence: &evidence,
            },
            Some(&artifact),
        )
        .expect("baseline binding evaluates");

        assert_eq!(gate.status, "passed");
        assert!(gate.block_reasons.is_empty());
    }

    #[test]
    fn live_capable_la7_baseline_binding_blocks_when_current_readback_omits_baseline_trade() {
        let account = sample_account();
        let baseline_evidence = sample_evidence(&account, 1);
        let current_evidence = sample_evidence(&account, 0);
        let artifact = build_account_baseline_artifact(
            "baseline-1".to_string(),
            "run-1".to_string(),
            1_777_000_000_000,
            "2026-05-08T00:00:00Z".to_string(),
            &account,
            &baseline_evidence,
            true,
        )
        .expect("baseline builds");

        let gate = evaluate_la7_live_baseline_binding(
            AccountBaselineBinding {
                expected_baseline_id: "baseline-1",
                expected_capture_run_id: "run-1",
                current_account: &account,
                current_evidence: &current_evidence,
            },
            Some(&artifact),
        )
        .expect("baseline binding evaluates");

        assert_eq!(gate.status, "blocked");
        assert!(gate
            .block_reasons
            .contains(&"baseline_trade_missing_from_current_readback"));
    }

    fn sample_artifact(position_evidence_complete: bool) -> AccountBaselineArtifact {
        let account = sample_account();
        let evidence = sample_evidence(&account, 1);

        build_account_baseline_artifact(
            "baseline-1".to_string(),
            "run-1".to_string(),
            1_777_000_000_000,
            "2026-05-08T00:00:00Z".to_string(),
            &account,
            &evidence,
            position_evidence_complete,
        )
        .expect("baseline builds")
    }

    fn sample_account() -> AccountPreflight {
        AccountPreflight {
            clob_host: "https://clob.polymarket.com".to_string(),
            chain_id: 137,
            wallet_address: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_address: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            signature_type: SignatureType::PolyProxy,
        }
    }

    fn sample_evidence(
        account: &AccountPreflight,
        trade_count: usize,
    ) -> AuthenticatedReadbackPreflightEvidence {
        let trades = if trade_count == 0 {
            Vec::new()
        } else {
            vec![TradeReadback {
                id: "trade-baseline-1".to_string(),
                market: "market-1".to_string(),
                asset_id: "token-1".to_string(),
                status: TradeReadbackStatus::Confirmed,
                transaction_hash: Some("0xabc".to_string()),
                maker_address: account.funder_address.clone(),
                order_id: Some("order-baseline-1".to_string()),
            }]
        };

        AuthenticatedReadbackPreflightEvidence {
            report: sample_report(trades.len(), 0, 0),
            collateral: BalanceAllowanceReadback {
                asset_type: AssetType::Collateral,
                token_id: None,
                balance_units: 6_314_318,
                allowance_units: u64::MAX,
            },
            open_orders: Vec::new(),
            trades,
        }
    }

    fn sample_report(
        trade_count: usize,
        open_order_count: usize,
        reserved_pusd_units: u64,
    ) -> ReadbackPreflightReport {
        ReadbackPreflightReport {
            status: "passed",
            block_reasons: Vec::new(),
            open_order_count,
            trade_count,
            reserved_pusd_units,
            required_collateral_allowance_units: 1_000_000,
            available_pusd_units: 6_314_318,
            venue_state: "trading_enabled",
            heartbeat: "not_started_no_open_orders",
            live_network_enabled: true,
        }
    }

    fn reconciliation_input_with_venue_trade(
        trade_id: &str,
        order_id: &str,
    ) -> LiveReconciliationInput {
        let mut venue_trades = BTreeMap::new();
        venue_trades.insert(
            trade_id.to_string(),
            VenueTradeState {
                trade_id: trade_id.to_string(),
                order_id: order_id.to_string(),
                status: VenueTradeStatus::Confirmed,
            },
        );

        LiveReconciliationInput {
            run_id: "run-1".to_string(),
            checked_at_ms: 1_777_000_000_000,
            local: LocalLiveState {
                known_orders: BTreeSet::new(),
                canceled_orders: BTreeSet::new(),
                partially_filled_orders: BTreeSet::new(),
                known_trades: BTreeSet::new(),
                trade_order_ids: BTreeSet::new(),
                trade_order_ids_by_trade: BTreeMap::new(),
                balance: None,
                positions: crate::live_position_book::LivePositionBook::new(),
                rust_readback_fingerprint: None,
                sdk_readback_fingerprint: None,
            },
            venue: VenueLiveState {
                orders: BTreeMap::new(),
                trades: venue_trades,
                balance: None,
                positions: crate::live_position_book::LivePositionBook::new(),
                rust_readback_fingerprint: None,
                sdk_readback_fingerprint: None,
            },
            venue_position_evidence_complete: false,
        }
    }
}
