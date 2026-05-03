use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_beta_cancel";
pub const SINGLE_CANCEL_PATH: &str = "/order";
pub const SINGLE_ORDER_READBACK_PATH_PREFIX: &str = "/data/order/";
pub const HTTP_DELETE: &str = "DELETE";
pub const LIVE_CANCEL_NETWORK_ENABLED: bool = false;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelReadinessInput {
    pub lb4_preflight_passed: bool,
    pub lb5_operator_approved: bool,
    pub lb6_hold_released: bool,
    pub human_canary_order_approved: bool,
    pub human_cancel_approved: bool,
    pub approved_canary_order_id: Option<String>,
    pub single_open_order_verified: bool,
    pub heartbeat_ready: bool,
    pub cancel_plan_acknowledged: bool,
    pub service_stop_ready: bool,
    pub kill_switch_ready: bool,
    pub live_order_placement_enabled: bool,
}

impl CancelReadinessInput {
    pub fn lb5_default(live_order_placement_enabled: bool) -> Self {
        Self {
            lb4_preflight_passed: false,
            lb5_operator_approved: true,
            lb6_hold_released: false,
            human_canary_order_approved: false,
            human_cancel_approved: false,
            approved_canary_order_id: None,
            single_open_order_verified: false,
            heartbeat_ready: false,
            cancel_plan_acknowledged: true,
            service_stop_ready: true,
            kill_switch_ready: true,
            live_order_placement_enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelReadinessReport {
    pub status: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub single_cancel_method: &'static str,
    pub single_cancel_path: &'static str,
    pub single_order_readback_path_prefix: &'static str,
    pub cancel_request_constructable: bool,
    pub live_cancel_network_enabled: bool,
    pub cancel_all_enabled: bool,
}

impl CancelReadinessReport {
    pub fn passed_for_lb5(&self) -> bool {
        !self.live_cancel_network_enabled
            && !self.cancel_all_enabled
            && self.block_reasons.contains(&"lb6_hold_not_released")
            && self
                .block_reasons
                .contains(&"approved_canary_order_missing")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelRequestDraft {
    pub method: &'static str,
    pub path: &'static str,
    pub body_json: String,
    pub authenticated_l2_required: bool,
    pub network_enabled: bool,
    pub cancel_all: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelResponseReport {
    pub status: &'static str,
    pub canceled_count: usize,
    pub not_canceled_count: usize,
    pub block_reasons: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelEndpointError {
    pub status_code: u16,
    pub code: String,
    pub message_redacted: bool,
}

pub fn evaluate_cancel_readiness(input: &CancelReadinessInput) -> CancelReadinessReport {
    let mut block_reasons = Vec::new();

    if !input.live_order_placement_enabled {
        block_reasons.push("live_order_placement_disabled");
    }
    if !input.lb4_preflight_passed {
        block_reasons.push("lb4_preflight_not_recorded");
    }
    if !input.lb5_operator_approved {
        block_reasons.push("lb5_operator_approval_missing");
    }
    if !input.lb6_hold_released {
        block_reasons.push("lb6_hold_not_released");
    }
    if !input.human_canary_order_approved {
        block_reasons.push("human_canary_approval_missing");
    }
    if !input.human_cancel_approved {
        block_reasons.push("human_cancel_approval_missing");
    }
    if approved_canary_order_id(input).is_err() {
        block_reasons.push("approved_canary_order_missing");
    }
    if !input.single_open_order_verified {
        block_reasons.push("single_open_order_not_verified");
    }
    if !input.heartbeat_ready {
        block_reasons.push("heartbeat_not_ready");
    }
    if !input.cancel_plan_acknowledged {
        block_reasons.push("cancel_plan_not_acknowledged");
    }
    if !input.service_stop_ready {
        block_reasons.push("service_stop_not_ready");
    }
    if !input.kill_switch_ready {
        block_reasons.push("kill_switch_not_ready");
    }

    CancelReadinessReport {
        status: if block_reasons.is_empty() {
            "ready_for_lb6_manual_canary_cancel_only"
        } else {
            "blocked"
        },
        cancel_request_constructable: block_reasons.is_empty(),
        block_reasons,
        single_cancel_method: HTTP_DELETE,
        single_cancel_path: SINGLE_CANCEL_PATH,
        single_order_readback_path_prefix: SINGLE_ORDER_READBACK_PATH_PREFIX,
        live_cancel_network_enabled: LIVE_CANCEL_NETWORK_ENABLED,
        cancel_all_enabled: false,
    }
}

pub fn build_single_cancel_request_draft(
    input: &CancelReadinessInput,
) -> CancelReadinessResult<CancelRequestDraft> {
    let report = evaluate_cancel_readiness(input);
    if !report.block_reasons.is_empty() {
        return Err(CancelReadinessError::Blocked(report.block_reasons));
    }
    let order_id = approved_canary_order_id(input)?;
    let body_json = serde_json::to_string(&CancelRequestBody { order_id })
        .map_err(CancelReadinessError::Serialize)?;

    Ok(CancelRequestDraft {
        method: HTTP_DELETE,
        path: SINGLE_CANCEL_PATH,
        body_json,
        authenticated_l2_required: true,
        network_enabled: LIVE_CANCEL_NETWORK_ENABLED,
        cancel_all: false,
    })
}

pub fn parse_single_cancel_response(
    order_id: &str,
    response_json: &str,
    size_matched_units_before_cancel: u64,
) -> CancelReadinessResult<CancelResponseReport> {
    let response: CancelResponseWire =
        serde_json::from_str(response_json).map_err(CancelReadinessError::Parse)?;
    let mut block_reasons = Vec::new();

    let canceled = response
        .canceled
        .iter()
        .any(|canceled_id| canceled_id == order_id);
    let target_canceled_count = response
        .canceled
        .iter()
        .filter(|canceled_id| *canceled_id == order_id)
        .count();
    let unexpected_canceled_order = response
        .canceled
        .iter()
        .any(|canceled_id| canceled_id != order_id);
    let unexpected_not_canceled_order = response
        .not_canceled
        .keys()
        .any(|not_canceled_id| not_canceled_id != order_id);
    let not_canceled_reason = response.not_canceled.get(order_id);

    if unexpected_canceled_order {
        block_reasons.push("unexpected_canceled_order_ids");
    }
    if target_canceled_count > 1 {
        block_reasons.push("duplicate_canceled_order_ids");
    }
    if unexpected_not_canceled_order {
        block_reasons.push("unexpected_not_canceled_order_ids");
    }
    if canceled && not_canceled_reason.is_some() {
        block_reasons.push("cancel_response_conflict");
    } else if canceled {
        if size_matched_units_before_cancel > 0 {
            block_reasons.push("partial_fill_requires_trade_reconciliation");
        }
    } else if let Some(reason) = not_canceled_reason {
        block_reasons.push(classify_not_canceled_reason(
            reason,
            size_matched_units_before_cancel,
        ));
    } else {
        block_reasons.push("cancel_order_id_missing_from_response");
    }

    Ok(CancelResponseReport {
        status: if block_reasons.is_empty() {
            "canceled"
        } else {
            "blocked"
        },
        canceled_count: response.canceled.len(),
        not_canceled_count: response.not_canceled.len(),
        block_reasons,
    })
}

pub fn parse_cancel_endpoint_error(
    status_code: u16,
    body: &str,
) -> CancelReadinessResult<CancelEndpointError> {
    let parsed: CancelErrorWire = serde_json::from_str(body).unwrap_or_default();
    let code = if status_code == 401 || contains_any(&parsed.error_text(), &["auth", "unauthor"]) {
        "auth_error".to_string()
    } else if status_code == 429 || contains_any(&parsed.error_text(), &["rate", "throttle"]) {
        "rate_limited".to_string()
    } else if parsed.code.trim().is_empty() {
        format!("http_{status_code}")
    } else {
        sanitize_error_code(&parsed.code)
    };

    Ok(CancelEndpointError {
        status_code,
        code,
        message_redacted: true,
    })
}

pub fn cancel_path_catalog() -> Vec<&'static str> {
    vec![SINGLE_CANCEL_PATH, SINGLE_ORDER_READBACK_PATH_PREFIX]
}

fn approved_canary_order_id(input: &CancelReadinessInput) -> CancelReadinessResult<String> {
    let Some(order_id) = input
        .approved_canary_order_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Err(CancelReadinessError::Validation(
            "approved canary order ID is required".to_string(),
        ));
    };
    if !is_order_hash(order_id) {
        return Err(CancelReadinessError::Validation(
            "approved canary order ID must be an order hash".to_string(),
        ));
    }
    Ok(order_id.to_string())
}

fn classify_not_canceled_reason(
    reason: &str,
    size_matched_units_before_cancel: u64,
) -> &'static str {
    if size_matched_units_before_cancel > 0 || contains_any(reason, &["filled", "matched"]) {
        "already_filled_or_partially_filled"
    } else if contains_any(reason, &["already canceled", "cancelled", "canceled"]) {
        "already_canceled"
    } else if contains_any(reason, &["not found", "missing", "unknown order"]) {
        "missing_order"
    } else if contains_any(reason, &["auth", "unauthor"]) {
        "auth_error"
    } else if contains_any(reason, &["rate", "throttle"]) {
        "rate_limited"
    } else {
        "unknown_cancel_error"
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    let lower = value.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn sanitize_error_code(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('_').to_string()
}

fn is_order_hash(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[derive(Debug, Serialize)]
struct CancelRequestBody {
    #[serde(rename = "orderID")]
    order_id: String,
}

#[derive(Debug, Deserialize)]
struct CancelResponseWire {
    #[serde(default)]
    canceled: Vec<String>,
    #[serde(default)]
    not_canceled: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct CancelErrorWire {
    #[serde(default)]
    code: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    message: String,
}

impl CancelErrorWire {
    fn error_text(&self) -> String {
        format!("{} {} {}", self.code, self.error, self.message)
    }
}

pub type CancelReadinessResult<T> = Result<T, CancelReadinessError>;

#[derive(Debug)]
pub enum CancelReadinessError {
    Parse(serde_json::Error),
    Serialize(serde_json::Error),
    Validation(String),
    Blocked(Vec<&'static str>),
}

impl Display for CancelReadinessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(source) => write!(formatter, "failed to parse LB5 cancel JSON: {source}"),
            Self::Serialize(source) => {
                write!(formatter, "failed to serialize LB5 cancel draft: {source}")
            }
            Self::Validation(message) => {
                write!(formatter, "LB5 cancel validation failed: {message}")
            }
            Self::Blocked(reasons) => {
                write!(formatter, "LB5 cancel path blocked: {}", reasons.join(","))
            }
        }
    }
}

impl Error for CancelReadinessError {}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER_ID: &str = concat!(
        "0xabcdefabcdefabcdefabcdefabcdefabcdef",
        "abcdefabcdefabcdefabcdefabcd"
    );
    const OTHER_ORDER_ID: &str = concat!(
        "0x111111111111111111111111111111111111",
        "1111111111111111111111111111"
    );

    #[test]
    fn cancel_readiness_blocks_live_cancel_before_lb6_canary() {
        let report = evaluate_cancel_readiness(&CancelReadinessInput::lb5_default(false));

        assert_eq!(report.status, "blocked");
        assert!(report.passed_for_lb5());
        assert!(!report.cancel_request_constructable);
        assert!(!report.live_cancel_network_enabled);
        assert!(!report.cancel_all_enabled);
        assert!(report
            .block_reasons
            .contains(&"live_order_placement_disabled"));
    }

    #[test]
    fn single_cancel_draft_requires_lb6_gates_and_canary_order() {
        let input = lb6_ready_input();
        let draft = build_single_cancel_request_draft(&input).expect("draft builds");

        assert_eq!(draft.method, HTTP_DELETE);
        assert_eq!(draft.path, SINGLE_CANCEL_PATH);
        assert_eq!(draft.body_json, format!(r#"{{"orderID":"{ORDER_ID}"}}"#));
        assert!(draft.authenticated_l2_required);
        assert!(!draft.network_enabled);
        assert!(!draft.cancel_all);

        let mut missing_lb6 = input.clone();
        missing_lb6.lb6_hold_released = false;
        let error = build_single_cancel_request_draft(&missing_lb6)
            .expect_err("LB6 hold is required")
            .to_string();
        assert!(error.contains("lb6_hold_not_released"));

        let mut missing_canary = input;
        missing_canary.approved_canary_order_id = None;
        let error = build_single_cancel_request_draft(&missing_canary)
            .expect_err("canary order is required")
            .to_string();
        assert!(error.contains("approved_canary_order_missing"));
    }

    #[test]
    fn cancel_success_response_parses_for_the_approved_order() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(r#"{{"canceled":["{ORDER_ID}"],"not_canceled":{{}}}}"#),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "canceled");
        assert!(report.block_reasons.is_empty());
        assert_eq!(report.canceled_count, 1);
    }

    #[test]
    fn cancel_response_fails_closed_for_extra_canceled_order_ids() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(r#"{{"canceled":["{ORDER_ID}","{OTHER_ORDER_ID}"],"not_canceled":{{}}}}"#),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert_eq!(report.canceled_count, 2);
        assert!(report
            .block_reasons
            .contains(&"unexpected_canceled_order_ids"));
    }

    #[test]
    fn cancel_response_fails_closed_for_duplicate_canceled_order_ids() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(r#"{{"canceled":["{ORDER_ID}","{ORDER_ID}"],"not_canceled":{{}}}}"#),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert_eq!(report.canceled_count, 2);
        assert!(report
            .block_reasons
            .contains(&"duplicate_canceled_order_ids"));
    }

    #[test]
    fn cancel_response_fails_closed_for_extra_not_canceled_order_ids() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(
                r#"{{"canceled":["{ORDER_ID}"],"not_canceled":{{"{OTHER_ORDER_ID}":"not found"}}}}"#
            ),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert_eq!(report.canceled_count, 1);
        assert_eq!(report.not_canceled_count, 1);
        assert!(report
            .block_reasons
            .contains(&"unexpected_not_canceled_order_ids"));
    }

    #[test]
    fn cancel_response_fails_closed_for_partial_fill_ambiguity() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(r#"{{"canceled":["{ORDER_ID}"],"not_canceled":{{}}}}"#),
            1,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert!(report
            .block_reasons
            .contains(&"partial_fill_requires_trade_reconciliation"));
    }

    #[test]
    fn cancel_response_classifies_already_filled() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(r#"{{"canceled":[],"not_canceled":{{"{ORDER_ID}":"Order already filled"}}}}"#),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert!(report
            .block_reasons
            .contains(&"already_filled_or_partially_filled"));
    }

    #[test]
    fn cancel_response_classifies_already_canceled_and_missing_order() {
        for (message, expected) in [
            ("Order not found or already canceled", "already_canceled"),
            ("Order not found", "missing_order"),
        ] {
            let report = parse_single_cancel_response(
                ORDER_ID,
                &format!(r#"{{"canceled":[],"not_canceled":{{"{ORDER_ID}":"{message}"}}}}"#),
                0,
            )
            .expect("response parses");

            assert_eq!(report.status, "blocked");
            assert!(report.block_reasons.contains(&expected));
        }
    }

    #[test]
    fn cancel_response_blocks_unknown_not_canceled_reason() {
        let report = parse_single_cancel_response(
            ORDER_ID,
            &format!(
                r#"{{"canceled":[],"not_canceled":{{"{ORDER_ID}":"operator-specific detail"}}}}"#
            ),
            0,
        )
        .expect("response parses");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"unknown_cancel_error"));
    }

    #[test]
    fn cancel_endpoint_error_redacts_auth_and_rate_limit_messages() {
        let auth = parse_cancel_endpoint_error(
            401,
            r#"{"error":"Unauthorized/Invalid api key for operator account"}"#,
        )
        .expect("auth error parses");
        assert_eq!(auth.code, "auth_error");
        assert!(auth.message_redacted);

        let rate = parse_cancel_endpoint_error(429, r#"{"message":"rate limit exceeded"}"#)
            .expect("rate error parses");
        assert_eq!(rate.code, "rate_limited");
        assert!(rate.message_redacted);
    }

    #[test]
    fn cancel_path_catalog_is_single_order_only() {
        let joined = cancel_path_catalog().join(",");

        assert!(joined.contains(SINGLE_CANCEL_PATH));
        assert!(joined.contains(SINGLE_ORDER_READBACK_PATH_PREFIX));
        assert!(!joined.contains(concat!("cancel", "-all")));
        assert!(!joined.contains(concat!("cancel", "-market")));
    }

    #[test]
    fn cancel_module_has_no_network_dispatch_or_secret_loading() {
        let source = include_str!("live_beta_cancel.rs");

        for forbidden in [
            concat!("req", "west"),
            concat!(".se", "nd("),
            concat!("POLY", "_API", "_KEY"),
            concat!("POLY", "_SE", "CRET"),
            concat!("private", "_key"),
        ] {
            assert!(!source.contains(forbidden), "unexpected hit: {forbidden}");
        }
    }

    #[test]
    fn rollback_runbook_contains_lb5_minimums() {
        let runbook = include_str!("../runbooks/live-beta-lb5-rollback-runbook.md");
        for required in [
            "Kill Switch",
            "Service Stop",
            "Open-Order Readback",
            "Cancel Plan",
            "Incident Note Template",
            "Artifact Checklist",
        ] {
            assert!(
                runbook.contains(required),
                "runbook missing required section {required}"
            );
        }
    }

    #[test]
    fn rollback_runbook_contains_lb7_closeout_lessons() {
        let runbook = include_str!("../runbooks/live-beta-lb5-rollback-runbook.md");
        for required in [
            "GET /data/order/{orderID}",
            "DELETE /order",
            "Rust readback and official SDK readback disagree",
            "the local one-order cap is consumed",
            "This is lifecycle evidence only, not profitability evidence.",
        ] {
            assert!(
                runbook.contains(required),
                "runbook missing LB7 closeout lesson {required}"
            );
        }
    }

    fn lb6_ready_input() -> CancelReadinessInput {
        CancelReadinessInput {
            lb4_preflight_passed: true,
            lb5_operator_approved: true,
            lb6_hold_released: true,
            human_canary_order_approved: true,
            human_cancel_approved: true,
            approved_canary_order_id: Some(ORDER_ID.to_string()),
            single_open_order_verified: true,
            heartbeat_ready: true,
            cancel_plan_acknowledged: true,
            service_stop_ready: true,
            kill_switch_ready: true,
            live_order_placement_enabled: true,
        }
    }
}
