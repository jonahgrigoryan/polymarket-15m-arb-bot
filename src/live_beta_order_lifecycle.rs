use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::domain::Side;
use crate::live_beta_cancel::{
    parse_cancel_endpoint_error, parse_single_cancel_response, CancelResponseReport,
    SINGLE_CANCEL_PATH,
};
use crate::live_beta_readback::{
    build_l2_hmac_signature, parse_readback_error_response, parse_single_order, AccountPreflight,
    L2ReadbackCredentials, OpenOrderReadback, OrderReadbackStatus, ReadbackEndpointError,
    SINGLE_ORDER_PATH_PREFIX,
};

pub const MODULE: &str = "live_beta_order_lifecycle";
pub const LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED: bool = true;
pub const LB6_CANCEL_ALL_ENABLED: bool = false;
pub const HTTP_GET: &str = "GET";
pub const HTTP_DELETE: &str = "DELETE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedCanaryOrder {
    pub order_id: String,
    pub approval_sha256: String,
    pub funder_address: String,
    pub condition_id: String,
    pub token_id: String,
    pub side: Side,
    pub price: String,
    pub size_units: u64,
    pub order_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactCancelRuntimeChecks {
    pub geoblock_passed: bool,
    pub authenticated_readback_available: bool,
    pub l2_secret_handles_present: bool,
    pub human_cancel_approved: bool,
    pub cancel_plan_acknowledged: bool,
    pub kill_switch_ready: bool,
    pub service_stop_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExactCancelReadinessReport {
    pub status: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub order_id: String,
    pub pre_cancel_order_status: &'static str,
    pub pre_cancel_size_matched_units: u64,
    pub live_cancel_network_enabled: bool,
    pub cancel_all_enabled: bool,
    pub single_cancel_method: &'static str,
    pub single_cancel_path: &'static str,
    pub single_order_readback_path: String,
}

impl ExactCancelReadinessReport {
    pub fn ready_to_cancel(&self) -> bool {
        self.block_reasons.is_empty()
            && self.live_cancel_network_enabled
            && !self.cancel_all_enabled
            && self.pre_cancel_order_status == "live"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExactCancelExecutionReport {
    pub status: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub order_id: String,
    pub cancel_attempted: bool,
    pub pre_cancel_order_status: &'static str,
    pub post_cancel_order_status: Option<&'static str>,
    pub canceled_count: usize,
    pub not_canceled_count: usize,
    pub live_cancel_network_enabled: bool,
    pub cancel_all_enabled: bool,
}

pub struct ExactOrderReadbackInput {
    pub clob_host: String,
    pub account: AccountPreflight,
    pub credentials: L2ReadbackCredentials,
    pub order_id: String,
    pub request_timeout_ms: u64,
}

pub struct ExactCancelInput {
    pub clob_host: String,
    pub account: AccountPreflight,
    pub credentials: L2ReadbackCredentials,
    pub expected: ExpectedCanaryOrder,
    pub checks: ExactCancelRuntimeChecks,
    pub request_timeout_ms: u64,
}

pub fn evaluate_exact_cancel_readiness(
    order: &OpenOrderReadback,
    expected: &ExpectedCanaryOrder,
    checks: &ExactCancelRuntimeChecks,
) -> ExactCancelReadinessReport {
    let mut block_reasons = Vec::new();

    if !checks.geoblock_passed {
        block_reasons.push("geoblock_not_passed");
    }
    if !checks.authenticated_readback_available {
        block_reasons.push("authenticated_readback_not_available");
    }
    if !checks.l2_secret_handles_present {
        block_reasons.push("l2_secret_handles_missing");
    }
    if !checks.human_cancel_approved {
        block_reasons.push("human_cancel_approval_missing");
    }
    if !checks.cancel_plan_acknowledged {
        block_reasons.push("cancel_plan_not_acknowledged");
    }
    if !checks.kill_switch_ready {
        block_reasons.push("kill_switch_not_ready");
    }
    if !checks.service_stop_ready {
        block_reasons.push("service_stop_not_ready");
    }
    if !LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED {
        block_reasons.push("single_cancel_network_disabled");
    }
    if LB6_CANCEL_ALL_ENABLED {
        block_reasons.push("cancel_all_enabled");
    }
    if !is_order_hash(&expected.order_id) {
        block_reasons.push("approved_canary_order_invalid");
    }
    if !expected.approval_sha256.starts_with("sha256:") {
        block_reasons.push("canary_approval_hash_missing");
    }
    if !hex_strings_equal(&order.id, &expected.order_id) {
        block_reasons.push("order_id_mismatch");
    }
    if !evm_addresses_equal(&order.maker_address, &expected.funder_address) {
        block_reasons.push("funder_mismatch");
    }
    if !hex_strings_equal(&order.market, &expected.condition_id) {
        block_reasons.push("condition_id_mismatch");
    }
    if order.asset_id.trim() != expected.token_id.trim() {
        block_reasons.push("token_id_mismatch");
    }
    if order.side.to_ascii_uppercase() != side_label(expected.side) {
        block_reasons.push("side_mismatch");
    }
    if !decimal_strings_equal(&order.price, &expected.price) {
        block_reasons.push("price_mismatch");
    }
    if order.original_size_units != expected.size_units {
        block_reasons.push("size_mismatch");
    }
    if !order.order_type.eq_ignore_ascii_case(&expected.order_type) {
        block_reasons.push("order_type_mismatch");
    }
    match order.status {
        OrderReadbackStatus::Live => {}
        OrderReadbackStatus::Unknown => block_reasons.push("unknown_order_status"),
        OrderReadbackStatus::Matched => block_reasons.push("matched_order_requires_reconciliation"),
        OrderReadbackStatus::Canceled | OrderReadbackStatus::CanceledMarketResolved => {
            block_reasons.push("order_already_canceled")
        }
        OrderReadbackStatus::Invalid => block_reasons.push("order_invalid"),
    }
    if order.size_matched_units > 0 {
        block_reasons.push("order_partially_or_fully_matched");
    }
    if !order.associate_trades.is_empty() {
        block_reasons.push("associated_trades_present");
    }
    if order.remaining_size_units() == 0 {
        block_reasons.push("no_remaining_order_size");
    }

    dedupe_preserving_order(&mut block_reasons);

    ExactCancelReadinessReport {
        status: if block_reasons.is_empty() {
            "ready_for_exact_single_cancel"
        } else {
            "blocked"
        },
        block_reasons,
        order_id: expected.order_id.clone(),
        pre_cancel_order_status: order.status.as_str(),
        pre_cancel_size_matched_units: order.size_matched_units,
        live_cancel_network_enabled: LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED,
        cancel_all_enabled: LB6_CANCEL_ALL_ENABLED,
        single_cancel_method: HTTP_DELETE,
        single_cancel_path: SINGLE_CANCEL_PATH,
        single_order_readback_path: format!("{SINGLE_ORDER_PATH_PREFIX}{}", expected.order_id),
    }
}

pub async fn read_exact_order(
    input: ExactOrderReadbackInput,
) -> LiveBetaOrderLifecycleResult<OpenOrderReadback> {
    validate_order_id(&input.order_id)?;
    let client = LiveOrderLifecycleClient::new(
        input.clob_host,
        input.account.wallet_address,
        input.credentials,
        input.request_timeout_ms,
    )?;
    client.get_single_order(&input.order_id).await
}

pub async fn cancel_exact_single_order(
    input: ExactCancelInput,
) -> LiveBetaOrderLifecycleResult<ExactCancelExecutionReport> {
    validate_order_id(&input.expected.order_id)?;
    let client = LiveOrderLifecycleClient::new(
        input.clob_host,
        input.account.wallet_address,
        input.credentials,
        input.request_timeout_ms,
    )?;
    let pre_cancel_order = client.get_single_order(&input.expected.order_id).await?;
    let readiness =
        evaluate_exact_cancel_readiness(&pre_cancel_order, &input.expected, &input.checks);
    if !readiness.ready_to_cancel() {
        return Ok(ExactCancelExecutionReport {
            status: "blocked",
            block_reasons: readiness.block_reasons,
            order_id: input.expected.order_id,
            cancel_attempted: false,
            pre_cancel_order_status: pre_cancel_order.status.as_str(),
            post_cancel_order_status: None,
            canceled_count: 0,
            not_canceled_count: 0,
            live_cancel_network_enabled: LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED,
            cancel_all_enabled: LB6_CANCEL_ALL_ENABLED,
        });
    }

    let cancel_report = client
        .delete_single_order(
            &input.expected.order_id,
            pre_cancel_order.size_matched_units,
        )
        .await?;
    let post_cancel_order = client.get_single_order(&input.expected.order_id).await?;
    let mut block_reasons = cancel_report.block_reasons.clone();
    if post_cancel_order.status != OrderReadbackStatus::Canceled {
        block_reasons.push("post_cancel_readback_not_canceled");
    }
    dedupe_preserving_order(&mut block_reasons);

    Ok(ExactCancelExecutionReport {
        status: if block_reasons.is_empty() {
            "canceled"
        } else {
            "blocked"
        },
        block_reasons,
        order_id: input.expected.order_id,
        cancel_attempted: true,
        pre_cancel_order_status: pre_cancel_order.status.as_str(),
        post_cancel_order_status: Some(post_cancel_order.status.as_str()),
        canceled_count: cancel_report.canceled_count,
        not_canceled_count: cancel_report.not_canceled_count,
        live_cancel_network_enabled: LB6_SINGLE_ORDER_CANCEL_NETWORK_ENABLED,
        cancel_all_enabled: LB6_CANCEL_ALL_ENABLED,
    })
}

pub fn single_cancel_body_json(order_id: &str) -> LiveBetaOrderLifecycleResult<String> {
    validate_order_id(order_id)?;
    serde_json::to_string(&SingleCancelBody {
        order_id: order_id.to_string(),
    })
    .map_err(LiveBetaOrderLifecycleError::Serialize)
}

struct LiveOrderLifecycleClient {
    http: reqwest::Client,
    host: String,
    address: String,
    credentials: L2ReadbackCredentials,
}

impl LiveOrderLifecycleClient {
    fn new(
        host: String,
        address: String,
        credentials: L2ReadbackCredentials,
        timeout_ms: u64,
    ) -> LiveBetaOrderLifecycleResult<Self> {
        if timeout_ms == 0 {
            return Err(LiveBetaOrderLifecycleError::Validation(
                "request_timeout_ms must be positive".to_string(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| {
                LiveBetaOrderLifecycleError::Network(format!(
                    "failed to build LB6 lifecycle HTTP client: {source}"
                ))
            })?;
        Ok(Self {
            http,
            host: host.trim_end_matches('/').to_string(),
            address,
            credentials,
        })
    }

    async fn get_single_order(
        &self,
        order_id: &str,
    ) -> LiveBetaOrderLifecycleResult<OpenOrderReadback> {
        let path = format!("{SINGLE_ORDER_PATH_PREFIX}{order_id}");
        let body = self.authenticated_text(HTTP_GET, &path, None).await?;
        parse_single_order(&body).map_err(LiveBetaOrderLifecycleError::Readback)
    }

    async fn delete_single_order(
        &self,
        order_id: &str,
        size_matched_units_before_cancel: u64,
    ) -> LiveBetaOrderLifecycleResult<CancelResponseReport> {
        let body_json = single_cancel_body_json(order_id)?;
        let response = self
            .authenticated_text(HTTP_DELETE, SINGLE_CANCEL_PATH, Some(&body_json))
            .await?;
        parse_single_cancel_response(order_id, &response, size_matched_units_before_cancel)
            .map_err(|source| LiveBetaOrderLifecycleError::Cancel(source.to_string()))
    }

    async fn authenticated_text(
        &self,
        method: &str,
        path: &str,
        body_json: Option<&str>,
    ) -> LiveBetaOrderLifecycleResult<String> {
        let timestamp = current_unix_timestamp()?;
        let signature = build_l2_hmac_signature(
            &self.credentials.api_secret,
            timestamp,
            method,
            path,
            body_json,
        )
        .map_err(LiveBetaOrderLifecycleError::Readback)?;
        let url = format!("{}{}", self.host, path);
        let mut request = match method {
            HTTP_GET => self.http.get(url),
            HTTP_DELETE => self
                .http
                .delete(url)
                .header("Content-Type", "application/json"),
            _ => {
                return Err(LiveBetaOrderLifecycleError::Validation(
                    "unsupported LB6 lifecycle method".to_string(),
                ));
            }
        };
        request = request
            .header("POLY_ADDRESS", &self.address)
            .header("POLY_SIGNATURE", signature)
            .header("POLY_TIMESTAMP", timestamp.to_string())
            .header("POLY_API_KEY", &self.credentials.api_key)
            .header("POLY_PASSPHRASE", &self.credentials.api_passphrase);
        if let Some(body) = body_json {
            request = request.body(body.to_string());
        }
        let response = request.send().await.map_err(|source| {
            LiveBetaOrderLifecycleError::Network(format!(
                "LB6 lifecycle {method} failed for {path}: {source}"
            ))
        })?;
        let status = response.status();
        let status_code = status.as_u16();
        let body = response.text().await.map_err(|source| {
            LiveBetaOrderLifecycleError::Network(format!(
                "LB6 lifecycle response body failed for {path}: {source}"
            ))
        })?;
        if status.is_success() {
            if body.trim().is_empty() {
                return Err(LiveBetaOrderLifecycleError::Validation(format!(
                    "{path} returned an empty body"
                )));
            }
            return Ok(body);
        }
        if method == HTTP_DELETE {
            let endpoint = parse_cancel_endpoint_error(status_code, &body)
                .map_err(|source| LiveBetaOrderLifecycleError::Cancel(source.to_string()))?;
            return Err(LiveBetaOrderLifecycleError::Endpoint(
                LifecycleEndpointError {
                    status_code,
                    code: endpoint.code,
                    message_redacted: endpoint.message_redacted,
                },
            ));
        }
        let endpoint_error =
            parse_readback_error_response(status_code, &body).unwrap_or_else(|_| {
                ReadbackEndpointError {
                    status_code,
                    code: format!("http_{status_code}"),
                    message_redacted: true,
                }
            });
        Err(LiveBetaOrderLifecycleError::Endpoint(
            LifecycleEndpointError {
                status_code,
                code: endpoint_error.code,
                message_redacted: endpoint_error.message_redacted,
            },
        ))
    }
}

#[derive(Debug, Serialize)]
struct SingleCancelBody {
    #[serde(rename = "orderID")]
    order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleEndpointError {
    pub status_code: u16,
    pub code: String,
    pub message_redacted: bool,
}

pub type LiveBetaOrderLifecycleResult<T> = Result<T, LiveBetaOrderLifecycleError>;

#[derive(Debug)]
pub enum LiveBetaOrderLifecycleError {
    Readback(crate::live_beta_readback::LiveBetaReadbackError),
    Serialize(serde_json::Error),
    Validation(String),
    Network(String),
    Endpoint(LifecycleEndpointError),
    Cancel(String),
}

impl Display for LiveBetaOrderLifecycleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readback(source) => write!(formatter, "LB6 order readback failed: {source}"),
            Self::Serialize(source) => write!(formatter, "LB6 cancel body failed: {source}"),
            Self::Validation(message) => {
                write!(
                    formatter,
                    "LB6 order lifecycle validation failed: {message}"
                )
            }
            Self::Network(message) => {
                write!(formatter, "LB6 order lifecycle network failed: {message}")
            }
            Self::Endpoint(error) => write!(
                formatter,
                "LB6 order lifecycle endpoint returned status={} code={} message_redacted={}",
                error.status_code, error.code, error.message_redacted
            ),
            Self::Cancel(message) => write!(formatter, "LB6 exact single cancel failed: {message}"),
        }
    }
}

impl Error for LiveBetaOrderLifecycleError {}

fn current_unix_timestamp() -> LiveBetaOrderLifecycleResult<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            LiveBetaOrderLifecycleError::Validation("system clock is before Unix epoch".to_string())
        })?
        .as_secs())
}

fn validate_order_id(order_id: &str) -> LiveBetaOrderLifecycleResult<()> {
    if is_order_hash(order_id) {
        Ok(())
    } else {
        Err(LiveBetaOrderLifecycleError::Validation(
            "order_id must be a 0x-prefixed 32-byte order hash".to_string(),
        ))
    }
}

fn is_order_hash(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn evm_addresses_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn hex_strings_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

fn decimal_strings_equal(left: &str, right: &str) -> bool {
    let Ok(left) = left.trim().parse::<f64>() else {
        return false;
    };
    let Ok(right) = right.trim().parse::<f64>() else {
        return false;
    };
    (left - right).abs() < 0.000_000_001
}

fn dedupe_preserving_order(values: &mut Vec<&'static str>) {
    let mut deduped = Vec::new();
    for value in values.drain(..) {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    *values = deduped;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_beta_readback::OrderReadbackStatus;

    const ORDER_ID: &str = concat!(
        "0xabcdefabcdefabcdefabcdefabcdefabcdef",
        "abcdefabcdefabcdefabcdefabcd"
    );
    const CONDITION_ID: &str = "0x0ec08b1e170fca8d967445849a3fabba49858911ab8c46dc36069aa1090718dd";
    const FUNDER: &str = "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159";
    const TOKEN_ID: &str =
        "32406149813503763845643545664364177107395801695160552332908724065335543321711";

    #[test]
    fn exact_cancel_readiness_passes_only_for_live_unmatched_canary_order() {
        let report =
            evaluate_exact_cancel_readiness(&live_order(), &expected_order(), &passing_checks());

        assert_eq!(report.status, "ready_for_exact_single_cancel");
        assert!(report.ready_to_cancel());
        assert!(report.block_reasons.is_empty());
        assert!(report.live_cancel_network_enabled);
        assert!(!report.cancel_all_enabled);
        assert_eq!(report.single_cancel_method, HTTP_DELETE);
        assert_eq!(report.single_cancel_path, SINGLE_CANCEL_PATH);
    }

    #[test]
    fn exact_cancel_readiness_blocks_partial_fill_and_associated_trades() {
        let mut order = live_order();
        order.size_matched_units = 1;
        order.associate_trades = vec!["trade-1".to_string()];

        let report = evaluate_exact_cancel_readiness(&order, &expected_order(), &passing_checks());

        assert_eq!(report.status, "blocked");
        assert!(report
            .block_reasons
            .contains(&"order_partially_or_fully_matched"));
        assert!(report.block_reasons.contains(&"associated_trades_present"));
    }

    #[test]
    fn exact_cancel_readiness_blocks_wrong_order_or_account() {
        let mut order = live_order();
        order.id = "0x1111111111111111111111111111111111111111111111111111111111111111".to_string();
        order.maker_address = "0x2222222222222222222222222222222222222222".to_string();

        let report = evaluate_exact_cancel_readiness(&order, &expected_order(), &passing_checks());

        assert!(report.block_reasons.contains(&"order_id_mismatch"));
        assert!(report.block_reasons.contains(&"funder_mismatch"));
    }

    #[test]
    fn exact_cancel_readiness_blocks_terminal_or_unknown_status() {
        for (status, expected) in [
            (
                OrderReadbackStatus::Matched,
                "matched_order_requires_reconciliation",
            ),
            (OrderReadbackStatus::Canceled, "order_already_canceled"),
            (OrderReadbackStatus::Invalid, "order_invalid"),
            (OrderReadbackStatus::Unknown, "unknown_order_status"),
        ] {
            let mut order = live_order();
            order.status = status;

            let report =
                evaluate_exact_cancel_readiness(&order, &expected_order(), &passing_checks());

            assert!(report.block_reasons.contains(&expected));
        }
    }

    #[test]
    fn exact_cancel_readiness_requires_runtime_gates() {
        let report = evaluate_exact_cancel_readiness(
            &live_order(),
            &expected_order(),
            &ExactCancelRuntimeChecks {
                geoblock_passed: false,
                authenticated_readback_available: false,
                l2_secret_handles_present: false,
                human_cancel_approved: false,
                cancel_plan_acknowledged: false,
                kill_switch_ready: false,
                service_stop_ready: false,
            },
        );

        for expected in [
            "geoblock_not_passed",
            "authenticated_readback_not_available",
            "l2_secret_handles_missing",
            "human_cancel_approval_missing",
            "cancel_plan_not_acknowledged",
            "kill_switch_not_ready",
            "service_stop_not_ready",
        ] {
            assert!(report.block_reasons.contains(&expected));
        }
    }

    #[test]
    fn single_cancel_body_is_exact_single_order_shape() {
        let body = single_cancel_body_json(ORDER_ID).expect("body builds");

        assert_eq!(body, format!(r#"{{"orderID":"{ORDER_ID}"}}"#));
    }

    #[test]
    fn lifecycle_module_has_no_cancel_all_or_bulk_cancel_surface() {
        let source = include_str!("live_beta_order_lifecycle.rs");

        for forbidden in [
            concat!("/", "cancel-all"),
            concat!("/", "cancel-market"),
            concat!("DELETE ", "/orders"),
        ] {
            assert!(!source.contains(forbidden), "unexpected hit: {forbidden}");
        }
    }

    fn expected_order() -> ExpectedCanaryOrder {
        ExpectedCanaryOrder {
            order_id: ORDER_ID.to_string(),
            approval_sha256: "sha256:0123456789abcdef".to_string(),
            funder_address: FUNDER.to_string(),
            condition_id: CONDITION_ID.to_string(),
            token_id: TOKEN_ID.to_string(),
            side: Side::Buy,
            price: "0.01".to_string(),
            size_units: 5_000_000,
            order_type: "GTD".to_string(),
        }
    }

    fn passing_checks() -> ExactCancelRuntimeChecks {
        ExactCancelRuntimeChecks {
            geoblock_passed: true,
            authenticated_readback_available: true,
            l2_secret_handles_present: true,
            human_cancel_approved: true,
            cancel_plan_acknowledged: true,
            kill_switch_ready: true,
            service_stop_ready: true,
        }
    }

    fn live_order() -> OpenOrderReadback {
        OpenOrderReadback {
            id: ORDER_ID.to_string(),
            status: OrderReadbackStatus::Live,
            maker_address: FUNDER.to_string(),
            market: CONDITION_ID.to_string(),
            asset_id: TOKEN_ID.to_string(),
            side: "BUY".to_string(),
            original_size_units: 5_000_000,
            size_matched_units: 0,
            price: "0.010000".to_string(),
            outcome: "Up".to_string(),
            expiration: "1777761720".to_string(),
            order_type: "GTD".to_string(),
            associate_trades: Vec::new(),
            created_at: 1_777_761_000,
        }
    }
}
