use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MODULE: &str = "live_alpha_report";
pub const SCALE_REPORT_SCHEMA_VERSION: &str = "live_alpha_scale_report_v1";
pub const SUPPORTED_SCALE_REPORT_FROM: &str = "2026-04-29";
pub const SUPPORTED_SCALE_REPORT_TO: &str = "2026-05-09";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAlphaScaleReport {
    pub schema_version: String,
    pub period: ScaleReportPeriod,
    pub evidence_paths: Vec<String>,
    pub missing_evidence_paths: Vec<String>,
    pub paper: PaperScaleSummary,
    pub live: LiveScaleSummary,
    pub paper_live_comparison: PaperLiveComparison,
    pub recommendation: ScaleRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleReportPeriod {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaperScaleSummary {
    pub order_count: u64,
    pub fill_count: u64,
    pub cancel_count: u64,
    pub maker_fill_count: u64,
    pub taker_fill_count: u64,
    pub filled_notional: f64,
    pub fees_paid: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_pnl: f64,
    pub post_settlement_total_pnl: Option<f64>,
    pub missed_opportunity_count: u64,
    pub average_latency_ms: Option<f64>,
    pub per_asset_total_pnl: BTreeMap<String, f64>,
    pub per_market_total_pnl: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LiveScaleSummary {
    pub order_count: u64,
    pub fill_count: u64,
    pub matched_order_count: u64,
    pub cancel_count: u64,
    pub replacement_count: u64,
    pub maker_order_count: u64,
    pub maker_final_canceled_count: u64,
    pub maker_fill_count: u64,
    pub taker_order_count: u64,
    pub taker_fill_count: u64,
    pub known_fees_paid: f64,
    pub estimated_fees_paid: f64,
    pub realized_pnl: Option<f64>,
    pub settlement_pnl: Option<f64>,
    pub average_slippage_bps: Option<f64>,
    pub average_adverse_selection_buffer_bps: Option<f64>,
    pub average_edge_at_submit_bps: Option<f64>,
    pub average_edge_after_costs_bps: Option<f64>,
    pub open_order_mismatch_count: u64,
    pub reconciliation_mismatch_count: u64,
    pub halt_count: u64,
    pub unknown_state_count: u64,
    pub la7_cap_consumed: bool,
    pub shadow_taker_evaluation_count: u64,
    pub shadow_taker_would_take_count: u64,
    pub shadow_taker_live_allowed_count: u64,
    pub bugs_or_incidents: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaperLiveComparison {
    pub comparable: bool,
    pub divergence_status: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScaleRecommendation {
    pub decision: String,
    pub reasons: Vec<String>,
    pub next_hold_point: String,
}

#[derive(Debug)]
pub struct LiveAlphaReportError {
    message: String,
}

impl LiveAlphaReportError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for LiveAlphaReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for LiveAlphaReportError {}

impl From<std::io::Error> for LiveAlphaReportError {
    fn from(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }
}

impl From<serde_json::Error> for LiveAlphaReportError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string())
    }
}

pub fn build_live_alpha_scale_report(
    from: &str,
    to: &str,
    reports_root: &Path,
) -> Result<LiveAlphaScaleReport, LiveAlphaReportError> {
    validate_supported_period(from, to)?;

    let mut evidence_paths = Vec::new();
    let mut missing_evidence_paths = Vec::new();
    let mut paper = PaperScaleSummary::default();
    let mut live = LiveScaleSummary::default();

    let paper_report_path = reports_root
        .join("sessions")
        .join("m9-rtds-current-window-startuplog-20260429T035356Z")
        .join("paper_report.json");
    if let Some(report) = read_optional_json(
        &paper_report_path,
        &mut evidence_paths,
        &mut missing_evidence_paths,
    )? {
        apply_paper_report(&mut paper, &report);
    }

    let settlement_path = reports_root
        .join("sessions")
        .join("m9-rtds-current-window-startuplog-20260429T035356Z")
        .join("settlement_reconciliation.json");
    if let Some(report) = read_optional_json(
        &settlement_path,
        &mut evidence_paths,
        &mut missing_evidence_paths,
    )? {
        paper.post_settlement_total_pnl =
            json_f64(&report, &["totals", "post_settlement_total_pnl"]);
    }

    let la3_journal_path = reports_root.join("live-alpha-la3-live-order-journal.jsonl");
    if la3_journal_path.exists() {
        evidence_paths.push(path_string(&la3_journal_path));
        apply_la3_journal(&mut live, &la3_journal_path)?;
    } else {
        missing_evidence_paths.push(path_string(&la3_journal_path));
    }

    let la5_journal_path = reports_root.join("live-alpha-la5-maker-micro-journal.jsonl");
    if la5_journal_path.exists() {
        evidence_paths.push(path_string(&la5_journal_path));
        apply_la5_journal(&mut live, &la5_journal_path)?;
    } else {
        missing_evidence_paths.push(path_string(&la5_journal_path));
    }

    let la6_journal_path = reports_root.join("live-alpha-la6-quote-manager-journal.jsonl");
    if la6_journal_path.exists() {
        evidence_paths.push(path_string(&la6_journal_path));
        apply_la6_journal(&mut live, &la6_journal_path)?;
    } else {
        missing_evidence_paths.push(path_string(&la6_journal_path));
    }

    for live_report_path in
        find_session_artifacts(reports_root, "live_alpha_taker_canary_live_report.json")?
    {
        evidence_paths.push(path_string(&live_report_path));
        let report = read_json(&live_report_path)?;
        apply_la7_live_report(&mut live, &report);
    }

    for shadow_report_path in find_session_artifacts(reports_root, "shadow_taker_report.json")? {
        evidence_paths.push(path_string(&shadow_report_path));
        let report = read_json(&shadow_report_path)?;
        apply_shadow_taker_report(&mut live, &report);
    }

    let la7_cap_path = reports_root.join("live-alpha-la7-taker-canary-cap.json");
    if let Some(report) = read_optional_json(
        &la7_cap_path,
        &mut evidence_paths,
        &mut missing_evidence_paths,
    )? {
        live.la7_cap_consumed = json_bool(&report, &["consumed"]).unwrap_or(false);
    }

    if live.taker_fill_count > 0 && live.realized_pnl.is_none() {
        live.notes.push(
            "Live taker P&L is not fully machine-attributable from committed reports; use dated verification notes for LA3/LA7 settlement context."
                .to_string(),
        );
    }
    if live.maker_fill_count == 0 {
        live.notes
            .push("No live maker fills exist in the machine-readable evidence.".to_string());
    }

    let paper_live_comparison = PaperLiveComparison {
        comparable: false,
        divergence_status: "not_comparable".to_string(),
        reason:
            "Live Alpha used bounded live canaries and maker quote probes, while paper evidence came from separate M9 RTDS sessions."
                .to_string(),
    };
    let recommendation = recommend(&paper, &live, &missing_evidence_paths);

    Ok(LiveAlphaScaleReport {
        schema_version: SCALE_REPORT_SCHEMA_VERSION.to_string(),
        period: ScaleReportPeriod {
            from: from.to_string(),
            to: to.to_string(),
        },
        evidence_paths,
        missing_evidence_paths,
        paper,
        live,
        paper_live_comparison,
        recommendation,
    })
}

fn validate_supported_period(from: &str, to: &str) -> Result<(), LiveAlphaReportError> {
    if from == SUPPORTED_SCALE_REPORT_FROM && to == SUPPORTED_SCALE_REPORT_TO {
        return Ok(());
    }

    Err(LiveAlphaReportError::new(format!(
        "unsupported LA8 scale report period: from={from}, to={to}; supported period is {SUPPORTED_SCALE_REPORT_FROM} to {SUPPORTED_SCALE_REPORT_TO}"
    )))
}

fn recommend(
    paper: &PaperScaleSummary,
    live: &LiveScaleSummary,
    missing_evidence_paths: &[String],
) -> ScaleRecommendation {
    let mut reasons = Vec::new();
    let paper_pnl = paper.post_settlement_total_pnl.unwrap_or(paper.total_pnl);
    let lifecycle_unsafe = live.reconciliation_mismatch_count > 0
        || live.halt_count > 0
        || live.unknown_state_count > 0;

    if paper_pnl < 0.0 {
        reasons.push(format!(
            "paper/post-settlement P&L is negative ({paper_pnl:.6})"
        ));
    }
    if live.reconciliation_mismatch_count > 0 {
        reasons.push(format!(
            "live reconciliation mismatch count is {}",
            live.reconciliation_mismatch_count
        ));
    }
    if live.halt_count > 0 {
        reasons.push(format!(
            "fail-closed halt or blocked live status count is {}",
            live.halt_count
        ));
    }
    if live.maker_fill_count == 0 {
        reasons.push("maker-only live fill/P&L sample is absent".to_string());
    }
    if live.la7_cap_consumed {
        reasons.push(
            "LA7 one-order taker cap is consumed; any future approval cannot reuse it".to_string(),
        );
    }
    if !missing_evidence_paths.is_empty() {
        reasons.push(format!(
            "{} expected machine-readable evidence path(s) were missing",
            missing_evidence_paths.len()
        ));
    }

    let decision = if lifecycle_unsafe {
        "NO-GO: lifecycle unsafe"
    } else if paper_pnl < 0.0 {
        "NO-GO: negative expectancy"
    } else if !missing_evidence_paths.is_empty() || live.maker_fill_count == 0 {
        "HOLD: more maker-only data required"
    } else if live.shadow_taker_evaluation_count > 0
        && live.shadow_taker_would_take_count == 0
        && live.taker_fill_count == 0
    {
        "HOLD: taker shadow only"
    } else {
        reasons.push("evidence supports drafting a separate scale approval scope".to_string());
        "GO: propose next PRD for broader scaling"
    };

    ScaleRecommendation {
        decision: decision.to_string(),
        reasons,
        next_hold_point:
            "Stop at LA8. Any future scaling requires a new PRD/implementation plan and approval scope."
                .to_string(),
    }
}

fn apply_paper_report(summary: &mut PaperScaleSummary, report: &Value) {
    summary.order_count += json_u64(report, &["paper", "order_count"]).unwrap_or(0);
    summary.fill_count += json_u64(report, &["paper", "fill_count"]).unwrap_or(0);
    summary.cancel_count += json_u64(report, &["paper", "cancel_count"]).unwrap_or(0);
    summary.filled_notional += json_f64(report, &["paper", "total_filled_notional"]).unwrap_or(0.0);
    summary.fees_paid += json_f64(report, &["paper", "total_fees_paid"]).unwrap_or(0.0);
    summary.realized_pnl += json_f64(report, &["pnl", "totals", "realized_pnl"]).unwrap_or(0.0);
    summary.unrealized_pnl += json_f64(report, &["pnl", "totals", "unrealized_pnl"]).unwrap_or(0.0);
    summary.total_pnl += json_f64(report, &["pnl", "totals", "total_pnl"]).unwrap_or(0.0);
    summary.missed_opportunity_count += json_u64(
        report,
        &["diagnostics", "opportunities", "missed_opportunity_count"],
    )
    .unwrap_or(0);
    summary.average_latency_ms =
        json_f64(report, &["diagnostics", "latency", "average_latency_ms"]);

    if let Some(fills) = json_array(report, &["paper", "fills"]) {
        for fill in fills {
            match json_str(fill, &["liquidity"]) {
                Some("maker") => summary.maker_fill_count += 1,
                Some("taker") => summary.taker_fill_count += 1,
                _ => {}
            }
        }
    }
    collect_total_pnl_map(
        report,
        &["pnl", "by_asset"],
        &mut summary.per_asset_total_pnl,
    );
    collect_total_pnl_map(
        report,
        &["pnl", "by_market"],
        &mut summary.per_market_total_pnl,
    );
}

fn apply_la3_journal(
    summary: &mut LiveScaleSummary,
    path: &Path,
) -> Result<(), LiveAlphaReportError> {
    for event in read_jsonl(path)? {
        match json_str(&event, &["event_type"]) {
            Some("live_fill_attempted") => {
                summary.order_count += 1;
                summary.taker_order_count += 1;
            }
            Some("live_fill_succeeded") => {
                summary.fill_count += 1;
                summary.taker_fill_count += 1;
                if json_str(&event, &["payload", "venue_status"]) == Some("MATCHED") {
                    summary.matched_order_count += 1;
                }
            }
            Some("live_fill_reconciled") => {
                if json_str(&event, &["payload", "status"]) != Some("filled_and_reconciled") {
                    summary.halt_count += 1;
                }
                if let Some(reasons) = json_array(&event, &["payload", "block_reasons"]) {
                    summary.reconciliation_mismatch_count += reasons.len() as u64;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn apply_la5_journal(
    summary: &mut LiveScaleSummary,
    path: &Path,
) -> Result<(), LiveAlphaReportError> {
    for event in read_jsonl(path)? {
        match json_str(&event, &["event_type"]) {
            Some("maker_order_accepted") => {
                summary.order_count += 1;
                summary.maker_order_count += 1;
            }
            Some("maker_micro_stopped") => {
                if let Some(orders) = json_array(&event, &["payload", "orders"]) {
                    for order in orders {
                        if json_bool(order, &["filled"]).unwrap_or(false) {
                            summary.fill_count += 1;
                            summary.maker_fill_count += 1;
                        }
                        if json_str(order, &["final_status"]) == Some("CANCELED") {
                            summary.maker_final_canceled_count += 1;
                            summary.cancel_count += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn apply_la6_journal(
    summary: &mut LiveScaleSummary,
    path: &Path,
) -> Result<(), LiveAlphaReportError> {
    let mut canceled_order_ids = BTreeSet::new();
    for event in read_jsonl(path)? {
        match json_str(&event, &["event_type"]) {
            Some("quote_placed") => {
                summary.order_count += 1;
                summary.maker_order_count += 1;
            }
            Some("quote_cancel_confirmed") | Some("quote_expired") => {
                record_maker_cancel(
                    summary,
                    &mut canceled_order_ids,
                    json_str(&event, &["payload", "order_id"]),
                );
            }
            Some("quote_replacement_submitted") => {
                summary.replacement_count += 1;
            }
            Some("quote_reconciliation_result") => {
                if let Some(status) = json_str(&event, &["payload", "status"]) {
                    if status != "passed" {
                        summary.halt_count += 1;
                    }
                }
                summary.reconciliation_mismatch_count +=
                    json_mismatch_count(&event, &["payload", "mismatches"]);
            }
            Some("quote_manager_stopped") => {
                if let Some(status) = json_str(&event, &["payload", "status"]) {
                    if status != "completed" {
                        summary.halt_count += 1;
                    }
                }
                if let Some(outcome) = value_at(&event, &["payload", "outcome"]) {
                    if json_bool(outcome, &["filled"]).unwrap_or(false)
                        || json_array(outcome, &["trade_ids"])
                            .map(|trade_ids| !trade_ids.is_empty())
                            .unwrap_or(false)
                    {
                        summary.fill_count += 1;
                        summary.maker_fill_count += 1;
                    }
                    if json_str(outcome, &["final_status"]) == Some("CANCELED") {
                        record_maker_cancel(
                            summary,
                            &mut canceled_order_ids,
                            json_str(outcome, &["order_id"]),
                        );
                    }
                    if let Some(status) = json_str(outcome, &["reconciliation_status"]) {
                        if status != "passed" {
                            summary.halt_count += 1;
                            summary.reconciliation_mismatch_count +=
                                json_mismatch_count(outcome, &["reconciliation_mismatches"]);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn record_maker_cancel(
    summary: &mut LiveScaleSummary,
    canceled_order_ids: &mut BTreeSet<String>,
    order_id: Option<&str>,
) {
    if let Some(order_id) = order_id
        .map(str::trim)
        .filter(|order_id| !order_id.is_empty())
    {
        if !canceled_order_ids.insert(order_id.to_ascii_lowercase()) {
            return;
        }
    }
    summary.cancel_count += 1;
    summary.maker_final_canceled_count += 1;
}

fn apply_la7_live_report(summary: &mut LiveScaleSummary, report: &Value) {
    if report
        .get("submission")
        .and_then(Value::as_object)
        .is_some()
    {
        summary.order_count += 1;
        summary.taker_order_count += 1;
        if json_str(report, &["submission", "venue_status"]) == Some("MATCHED") {
            summary.matched_order_count += 1;
            summary.fill_count += 1;
            summary.taker_fill_count += 1;
        }
    }
    if let Some(decision) = value_at(report, &["pre_submit_report", "decision"]) {
        summary.estimated_fees_paid += json_f64(decision, &["taker_fee"]).unwrap_or(0.0);
        accumulate_average(
            &mut summary.average_slippage_bps,
            json_f64(decision, &["slippage_bps"]),
            1,
        );
        accumulate_average(
            &mut summary.average_adverse_selection_buffer_bps,
            json_f64(decision, &["adverse_selection_buffer_bps"]),
            1,
        );
        accumulate_average(
            &mut summary.average_edge_at_submit_bps,
            json_f64(decision, &["gross_edge_bps"]),
            1,
        );
        accumulate_average(
            &mut summary.average_edge_after_costs_bps,
            json_f64(decision, &["estimated_ev_after_costs_bps"]),
            1,
        );
    }
    if json_str(report, &["status"])
        .map(|status| status.contains("blocked") || status.contains("error"))
        .unwrap_or(false)
    {
        summary.halt_count += 1;
    }
    if json_str(report, &["post_submit_reconciliation_status"]) == Some("halt_required") {
        summary.halt_count += 1;
    }
    if let Some(mismatches) = json_array(report, &["post_submit_reconciliation_mismatches"]) {
        summary.reconciliation_mismatch_count += mismatches.len() as u64;
        for mismatch in mismatches {
            if mismatch.as_str() == Some("baseline:current_readback_not_passed") {
                summary.open_order_mismatch_count += 1;
            }
        }
    }
    if json_str(report, &["post_submit_readback_status"]) == Some("unknown") {
        summary.unknown_state_count += 1;
    }
    if json_str(report, &["status"]) == Some("submitted_post_check_blocked") {
        summary
            .bugs_or_incidents
            .push("LA7 immediate post-submit readback/reconciliation failed closed".to_string());
    }
}

fn apply_shadow_taker_report(summary: &mut LiveScaleSummary, report: &Value) {
    summary.shadow_taker_evaluation_count += json_u64(report, &["evaluation_count"]).unwrap_or(0);
    summary.shadow_taker_would_take_count += json_u64(report, &["would_take_count"]).unwrap_or(0);
    summary.shadow_taker_live_allowed_count +=
        json_u64(report, &["live_allowed_count"]).unwrap_or(0);
    summary.estimated_fees_paid += json_f64(report, &["estimated_taker_fee"]).unwrap_or(0.0);
}

fn collect_total_pnl_map(report: &Value, path: &[&str], output: &mut BTreeMap<String, f64>) {
    if let Some(object) = value_at(report, path).and_then(Value::as_object) {
        for (key, value) in object {
            if let Some(total_pnl) = json_f64(value, &["total_pnl"]) {
                *output.entry(key.clone()).or_insert(0.0) += total_pnl;
            }
        }
    }
}

fn find_session_artifacts(
    reports_root: &Path,
    artifact_name: &str,
) -> Result<Vec<PathBuf>, LiveAlphaReportError> {
    let sessions = reports_root.join("sessions");
    if !sessions.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(sessions)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join(artifact_name);
            if candidate.exists() {
                paths.push(candidate);
            }
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_optional_json(
    path: &Path,
    evidence_paths: &mut Vec<String>,
    missing_evidence_paths: &mut Vec<String>,
) -> Result<Option<Value>, LiveAlphaReportError> {
    if path.exists() {
        evidence_paths.push(path_string(path));
        Ok(Some(read_json(path)?))
    } else {
        missing_evidence_paths.push(path_string(path));
        Ok(None)
    }
}

fn read_json(path: &Path) -> Result<Value, LiveAlphaReportError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(Into::into)
}

fn read_jsonl(path: &Path) -> Result<Vec<Value>, LiveAlphaReportError> {
    let text = fs::read_to_string(path)?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn json_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_at(value, path).and_then(Value::as_str)
}

fn json_u64(value: &Value, path: &[&str]) -> Option<u64> {
    value_at(value, path).and_then(Value::as_u64)
}

fn json_f64(value: &Value, path: &[&str]) -> Option<f64> {
    value_at(value, path).and_then(Value::as_f64)
}

fn json_bool(value: &Value, path: &[&str]) -> Option<bool> {
    value_at(value, path).and_then(Value::as_bool)
}

fn json_array<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    value_at(value, path).and_then(Value::as_array)
}

fn json_mismatch_count(value: &Value, path: &[&str]) -> u64 {
    match value_at(value, path) {
        Some(Value::Array(mismatches)) => mismatches.len() as u64,
        Some(Value::String(mismatches)) => mismatches
            .split(',')
            .filter(|mismatch| !mismatch.trim().is_empty())
            .count() as u64,
        _ => 0,
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn accumulate_average(target: &mut Option<f64>, value: Option<f64>, existing_count: u64) {
    let Some(value) = value else {
        return;
    };
    *target = Some(match *target {
        Some(current) => (current * existing_count as f64 + value) / (existing_count as f64 + 1.0),
        None => value,
    });
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;

    #[test]
    fn live_alpha_report_summarizes_negative_paper_and_blocked_live_evidence() {
        let reports_root = unique_reports_root("negative_paper_blocked_live");
        let m9_dir = reports_root
            .join("sessions")
            .join("m9-rtds-current-window-startuplog-20260429T035356Z");
        fs::create_dir_all(&m9_dir).unwrap();
        write_json(
            &m9_dir.join("paper_report.json"),
            &json!({
                "paper": {
                    "order_count": 2,
                    "fill_count": 2,
                    "cancel_count": 0,
                    "total_filled_notional": 3.0,
                    "total_fees_paid": 0.2,
                    "fills": [
                        {"liquidity": "taker"},
                        {"liquidity": "maker"}
                    ]
                },
                "pnl": {
                    "totals": {
                        "realized_pnl": -0.2,
                        "unrealized_pnl": -0.3,
                        "total_pnl": -0.5
                    },
                    "by_asset": {
                        "BTC": {"total_pnl": -0.1}
                    },
                    "by_market": {
                        "m1": {"total_pnl": -0.1}
                    }
                },
                "diagnostics": {
                    "latency": {"average_latency_ms": 10.0},
                    "opportunities": {"missed_opportunity_count": 1}
                }
            }),
        );
        write_json(
            &m9_dir.join("settlement_reconciliation.json"),
            &json!({
                "totals": {"post_settlement_total_pnl": -3.5}
            }),
        );

        fs::create_dir_all(reports_root.join("sessions").join("la7-live")).unwrap();
        write_json(
            &reports_root
                .join("sessions")
                .join("la7-live")
                .join("live_alpha_taker_canary_live_report.json"),
            &json!({
                "status": "submitted_post_check_blocked",
                "post_submit_readback_status": "blocked",
                "post_submit_reconciliation_status": "halt_required",
                "post_submit_reconciliation_mismatches": [
                    "unexpected_fill",
                    "nonterminal_venue_trade_status"
                ],
                "submission": {"venue_status": "MATCHED"},
                "pre_submit_report": {
                    "decision": {
                        "taker_fee": 0.07,
                        "slippage_bps": 0.0,
                        "adverse_selection_buffer_bps": 25.0,
                        "gross_edge_bps": 1200.0,
                        "estimated_ev_after_costs_bps": 800.0
                    }
                }
            }),
        );
        write_json(
            &reports_root.join("live-alpha-la7-taker-canary-cap.json"),
            &json!({"consumed": true}),
        );
        fs::write(
            reports_root.join("live-alpha-la3-live-order-journal.jsonl"),
            r#"{"event_type":"live_fill_attempted"}
{"event_type":"live_fill_succeeded","payload":{"venue_status":"MATCHED"}}
{"event_type":"live_fill_reconciled","payload":{"status":"filled_and_reconciled","block_reasons":[]}}
"#,
        )
        .unwrap();
        fs::write(
            reports_root.join("live-alpha-la5-maker-micro-journal.jsonl"),
            r#"{"event_type":"maker_order_accepted"}
{"event_type":"maker_micro_stopped","payload":{"orders":[{"final_status":"CANCELED","filled":false}]}}
"#,
        )
        .unwrap();
        fs::write(
            reports_root.join("live-alpha-la6-quote-manager-journal.jsonl"),
            r#"{"event_type":"quote_placed","payload":{"order_id":"0xabc","status":"LIVE","trade_ids":[]}}
{"event_type":"quote_replacement_submitted","payload":{"order_id":"0xdef"}}
{"event_type":"quote_cancel_confirmed","payload":{"order_id":"0xabc"}}
{"event_type":"quote_manager_stopped","payload":{"status":"completed","outcome":{"order_id":"0xabc","final_status":"CANCELED","filled":false,"trade_ids":[],"reconciliation_status":"passed","reconciliation_mismatches":""}}}
"#,
        )
        .unwrap();

        let report =
            build_live_alpha_scale_report("2026-04-29", "2026-05-09", &reports_root).unwrap();

        assert!(report.missing_evidence_paths.is_empty());
        assert_eq!(report.paper.fill_count, 2);
        assert_eq!(report.paper.taker_fill_count, 1);
        assert_eq!(report.live.taker_fill_count, 2);
        assert_eq!(report.live.maker_order_count, 2);
        assert_eq!(report.live.cancel_count, 2);
        assert_eq!(report.live.maker_final_canceled_count, 2);
        assert_eq!(report.live.replacement_count, 1);
        assert_eq!(report.live.reconciliation_mismatch_count, 2);
        assert!(report.live.la7_cap_consumed);
        assert_eq!(report.recommendation.decision, "NO-GO: lifecycle unsafe");
        assert!(report
            .recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("negative")));

        let _ = fs::remove_dir_all(reports_root);
    }

    #[test]
    fn live_alpha_report_counts_la5_canceled_finals_in_live_cancel_total() {
        let reports_root = unique_reports_root("la5_canceled_finals");
        fs::create_dir_all(&reports_root).unwrap();
        let journal_path = reports_root.join("live-alpha-la5-maker-micro-journal.jsonl");
        fs::write(
            &journal_path,
            r#"{"event_type":"maker_micro_stopped","payload":{"orders":[{"final_status":"CANCELED","filled":false},{"final_status":"CANCELED","filled":false},{"final_status":"LIVE","filled":false}]}}
"#,
        )
        .unwrap();
        let mut live = LiveScaleSummary::default();

        apply_la5_journal(&mut live, &journal_path).unwrap();

        assert_eq!(live.cancel_count, 2);
        assert_eq!(live.maker_final_canceled_count, 2);

        let _ = fs::remove_dir_all(reports_root);
    }

    #[test]
    fn live_alpha_report_counts_la6_quote_manager_lifecycle() {
        let reports_root = unique_reports_root("la6_quote_manager");
        fs::create_dir_all(&reports_root).unwrap();
        let journal_path = reports_root.join("live-alpha-la6-quote-manager-journal.jsonl");
        fs::write(
            &journal_path,
            r#"{"event_type":"quote_placed","payload":{"order_id":"0xabc","status":"LIVE","trade_ids":[]}}
{"event_type":"quote_replacement_submitted","payload":{"order_id":"0xdef"}}
{"event_type":"quote_cancel_confirmed","payload":{"order_id":"0xabc"}}
{"event_type":"quote_reconciliation_result","payload":{"status":"failed","mismatches":"unexpected_fill,nonterminal_venue_trade_status"}}
{"event_type":"quote_manager_stopped","payload":{"status":"completed","outcome":{"order_id":"0xabc","final_status":"CANCELED","filled":false,"trade_ids":[],"reconciliation_status":"passed","reconciliation_mismatches":""}}}
"#,
        )
        .unwrap();
        let mut live = LiveScaleSummary::default();

        apply_la6_journal(&mut live, &journal_path).unwrap();

        assert_eq!(live.order_count, 1);
        assert_eq!(live.maker_order_count, 1);
        assert_eq!(live.cancel_count, 1);
        assert_eq!(live.maker_final_canceled_count, 1);
        assert_eq!(live.replacement_count, 1);
        assert_eq!(live.reconciliation_mismatch_count, 2);
        assert_eq!(live.halt_count, 1);

        let _ = fs::remove_dir_all(reports_root);
    }

    #[test]
    fn live_alpha_report_rejects_unsupported_period() {
        let error = build_live_alpha_scale_report("2026-05-03", "2026-05-09", Path::new("reports"))
            .expect_err("unsupported window must fail closed");

        assert!(error
            .to_string()
            .contains("unsupported LA8 scale report period"));
        assert!(error.to_string().contains("2026-04-29 to 2026-05-09"));
    }

    #[test]
    fn live_alpha_report_reads_shadow_taker_fee_field() {
        let mut live = LiveScaleSummary::default();

        apply_shadow_taker_report(
            &mut live,
            &json!({
                "evaluation_count": 3,
                "would_take_count": 1,
                "live_allowed_count": 0,
                "estimated_taker_fee": 0.42
            }),
        );

        assert_eq!(live.shadow_taker_evaluation_count, 3);
        assert_eq!(live.shadow_taker_would_take_count, 1);
        assert_eq!(live.estimated_fees_paid, 0.42);
    }

    #[test]
    fn live_alpha_report_holds_when_maker_live_fill_sample_is_absent() {
        let paper = PaperScaleSummary {
            total_pnl: 1.0,
            post_settlement_total_pnl: Some(1.0),
            ..PaperScaleSummary::default()
        };
        let live = LiveScaleSummary {
            maker_order_count: 3,
            maker_fill_count: 0,
            ..LiveScaleSummary::default()
        };

        let recommendation = recommend(&paper, &live, &[]);

        assert_eq!(
            recommendation.decision,
            "HOLD: more maker-only data required"
        );
        assert!(recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("maker-only live fill")));
    }

    #[test]
    fn live_alpha_report_can_recommend_go_for_clean_positive_evidence() {
        let paper = PaperScaleSummary {
            total_pnl: 1.0,
            post_settlement_total_pnl: Some(1.0),
            ..PaperScaleSummary::default()
        };
        let live = LiveScaleSummary {
            maker_order_count: 3,
            maker_fill_count: 3,
            fill_count: 3,
            ..LiveScaleSummary::default()
        };

        let recommendation = recommend(&paper, &live, &[]);

        assert_eq!(
            recommendation.decision,
            "GO: propose next PRD for broader scaling"
        );
        assert!(recommendation
            .reasons
            .iter()
            .any(|reason| reason.contains("separate scale approval")));
    }

    fn unique_reports_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "p15m-live-alpha-report-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
}
