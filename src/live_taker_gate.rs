use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::domain::{MarketLifecycleState, OrderKind, PaperFill, Side};
use crate::live_alpha_config::LiveAlphaMode;
use crate::live_beta_readback::SignatureType;
use crate::paper_executor::fee_paid;
use crate::signal_engine::{SignalEngine, SignalEngineConfig};
use crate::state::{BookFreshness, DecisionSnapshot, PriceLevelSnapshot, TokenBookSnapshot};

pub const MODULE: &str = "live_taker_gate";
pub const LA7_TAKER_APPROVAL_STATUS: &str = "Status: LA7 TAKER DRY RUN APPROVED";
pub const LA7_TAKER_LIVE_APPROVAL_STATUS: &str = "Status: LA7 TAKER LIVE CANARY APPROVED";
const PRICE_EPSILON: f64 = 1e-9;

pub const LA7_TAKER_APPROVAL_REQUIRED_FIELDS: &[&str] = &[
    "approval_id",
    "baseline_id",
    "baseline_capture_run_id",
    "baseline_hash",
    "wallet",
    "funder",
    "market_slug",
    "condition_id",
    "token_id",
    "outcome",
    "side",
    "max_size",
    "max_notional",
    "worst_price",
    "max_fee",
    "max_slippage_bps",
    "no_near_close_cutoff_seconds",
    "max_orders_per_day",
    "retry_after_ambiguous_submit",
    "batch_orders",
    "cancel_all",
];

pub const LA7_TAKER_LIVE_APPROVAL_REQUIRED_FIELDS: &[&str] = &[
    "approval_expires_at_unix",
    "dry_run_report_path",
    "dry_run_report_sha256",
    "dry_run_decision_path",
    "dry_run_decision_sha256",
];

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveTakerCanaryApprovalFields {
    pub approval_id: String,
    pub baseline_id: String,
    pub baseline_capture_run_id: String,
    pub baseline_hash: String,
    pub wallet: String,
    pub funder: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub max_size: f64,
    pub max_notional: f64,
    pub worst_price: f64,
    pub max_fee: f64,
    pub max_slippage_bps: u64,
    pub no_near_close_cutoff_seconds: u64,
    pub max_orders_per_day: u64,
    pub retry_after_ambiguous_submit: String,
    pub batch_orders: String,
    pub cancel_all: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveTakerCanaryLiveApprovalFields {
    pub approval: LiveTakerCanaryApprovalFields,
    pub approval_expires_at_unix: u64,
    pub dry_run_report_path: String,
    pub dry_run_report_sha256: String,
    pub dry_run_decision_path: String,
    pub dry_run_decision_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveTakerApprovalError {
    Approval(Vec<String>),
}

impl Display for LiveTakerApprovalError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approval(errors) => {
                write!(formatter, "LA7 taker approval artifact is not final: ")?;
                write!(formatter, "{}", errors.join(","))
            }
        }
    }
}

impl Error for LiveTakerApprovalError {}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LiveTakerRuntimeState {
    pub geoblock_passed: bool,
    pub heartbeat_healthy: bool,
    pub reconciliation_clean: bool,
    pub inventory_clean: bool,
    pub baseline_ready: bool,
    pub live_risk_controls_passed: bool,
    pub existing_taker_orders_today: u64,
    pub existing_taker_fee_spend: f64,
    pub current_total_live_notional: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveTakerGateDecision {
    pub market_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub would_take: bool,
    pub live_allowed: bool,
    pub reason_codes: Vec<String>,
    pub fair_probability: Option<f64>,
    pub market_probability: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub average_price: Option<f64>,
    pub worst_price: Option<f64>,
    pub worst_price_limit: Option<f64>,
    pub size: f64,
    pub notional: f64,
    pub visible_depth: f64,
    pub gross_edge_bps: Option<f64>,
    pub spread_cost_bps: Option<f64>,
    pub taker_fee_bps: Option<f64>,
    pub taker_fee: Option<f64>,
    pub slippage_bps: Option<f64>,
    pub latency_buffer_bps: f64,
    pub adverse_selection_buffer_bps: f64,
    pub minimum_profit_buffer_bps: f64,
    pub estimated_ev_after_costs_bps: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTakerSubmissionReport {
    pub status: String,
    pub order_id: String,
    pub venue_status: String,
    pub success: bool,
    pub making_amount: String,
    pub taking_amount: String,
    pub trade_ids: Vec<String>,
    pub transaction_hashes: Vec<String>,
    pub approval_sha256: String,
    pub not_submitted: bool,
    pub submitted_order_count: u8,
    pub order_type: String,
    pub post_only: bool,
    pub fok_or_fak: bool,
    pub batch_orders: bool,
}

pub struct LiveTakerSubmitInput {
    pub clob_host: String,
    pub signer_handle: String,
    pub l2_access_handle: String,
    pub l2_secret_handle: String,
    pub l2_passphrase_handle: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: SignatureType,
    pub approval: LiveTakerCanaryApprovalFields,
    pub decision: LiveTakerGateDecision,
    pub approval_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, Default)]
pub struct LiveTakerShadowReport {
    pub evaluation_count: u64,
    pub would_take_count: u64,
    pub live_allowed_count: u64,
    pub rejected_by_fee_count: u64,
    pub rejected_by_depth_count: u64,
    pub rejected_by_slippage_count: u64,
    pub rejected_by_latency_buffer_count: u64,
    pub rejected_count_by_reason: BTreeMap<String, u64>,
    pub estimated_ev_after_costs_bps_sum: f64,
    pub estimated_ev_after_costs_bps_average: Option<f64>,
    pub estimated_ev_after_costs_bps_min: Option<f64>,
    pub estimated_ev_after_costs_bps_max: Option<f64>,
    pub estimated_taker_fee: f64,
    pub estimated_taker_notional: f64,
    pub paper_maker_fill_count: u64,
    pub paper_taker_fill_count: u64,
    pub paper_maker_filled_notional: f64,
    pub paper_taker_filled_notional: f64,
    pub paper_maker_fees_paid: f64,
    pub paper_taker_fees_paid: f64,
    pub paper_total_pnl: f64,
    pub taker_disabled_by_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TakerGateConfig {
    live_alpha_enabled: bool,
    live_alpha_mode: LiveAlphaMode,
    taker_enabled: bool,
    max_taker_notional: f64,
    max_slippage_bps: f64,
    max_orders_per_day: u64,
    max_fee_spend: f64,
    max_total_live_notional: f64,
    no_trade_seconds_before_close: u64,
    max_book_age_ms: u64,
    max_reference_age_ms: u64,
    latency_buffer_bps: f64,
    adverse_selection_buffer_bps: f64,
    minimum_profit_buffer_bps: f64,
}

impl TakerGateConfig {
    fn from_app(config: &AppConfig) -> Self {
        let signal = SignalEngineConfig::from(&config.strategy);
        Self {
            live_alpha_enabled: config.live_alpha.enabled,
            live_alpha_mode: config.live_alpha.mode,
            taker_enabled: config.live_alpha.taker.enabled,
            max_taker_notional: stricter_positive_f64(
                config.live_alpha.taker.max_notional,
                config.live_alpha.risk.max_single_order_notional,
            ),
            max_slippage_bps: config.live_alpha.taker.max_slippage_bps as f64,
            max_orders_per_day: config.live_alpha.taker.max_orders_per_day,
            max_fee_spend: config.live_alpha.risk.max_fee_spend,
            max_total_live_notional: config.live_alpha.risk.max_total_live_notional,
            no_trade_seconds_before_close: config.live_alpha.risk.no_trade_seconds_before_close,
            max_book_age_ms: stricter_positive_u64(
                config.live_alpha.risk.max_book_staleness_ms,
                config.risk.stale_book_ms,
            ),
            max_reference_age_ms: stricter_positive_u64(
                config.live_alpha.risk.max_reference_staleness_ms,
                config.risk.stale_reference_ms,
            ),
            latency_buffer_bps: signal.latency_buffer_bps,
            adverse_selection_buffer_bps: signal.adverse_selection_bps,
            minimum_profit_buffer_bps: config.live_alpha.taker.min_ev_after_all_costs_bps as f64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DepthCheck {
    average_price: f64,
    worst_price: f64,
    visible_depth: f64,
}

pub fn evaluate_shadow_taker_snapshot(
    config: &AppConfig,
    snapshot: &DecisionSnapshot,
    runtime: LiveTakerRuntimeState,
) -> Vec<LiveTakerGateDecision> {
    let taker_config = TakerGateConfig::from_app(config);
    let signal = SignalEngine::from_strategy_config(&config.strategy);
    let signal_evaluation = signal.evaluate(snapshot);
    let fair_probability_up = signal_evaluation.fair_probability.probability_up;

    snapshot
        .market
        .outcomes
        .iter()
        .map(|outcome| {
            let fair_probability = outcome_fair_probability(&outcome.outcome, fair_probability_up);
            let book = snapshot
                .token_books
                .iter()
                .find(|book| book.token_id == outcome.token_id);
            evaluate_buy_taker(
                &taker_config,
                snapshot,
                &outcome.token_id,
                &outcome.outcome,
                book,
                fair_probability,
                snapshot.market.min_order_size,
                runtime,
            )
        })
        .collect()
}

pub fn evaluate_taker_canary_snapshot(
    config: &AppConfig,
    snapshot: &DecisionSnapshot,
    runtime: LiveTakerRuntimeState,
    token_id: &str,
    outcome: &str,
    side: Side,
    size: f64,
) -> LiveTakerGateDecision {
    if side != Side::Buy {
        return empty_decision(
            snapshot,
            token_id,
            outcome,
            side,
            None,
            vec!["unsupported_taker_side"],
        );
    }

    let taker_config = TakerGateConfig::from_app(config);
    let signal = SignalEngine::from_strategy_config(&config.strategy);
    let signal_evaluation = signal.evaluate(snapshot);
    let fair_probability =
        outcome_fair_probability(outcome, signal_evaluation.fair_probability.probability_up);
    let book = snapshot
        .token_books
        .iter()
        .find(|book| book.token_id == token_id);

    evaluate_buy_taker(
        &taker_config,
        snapshot,
        token_id,
        outcome,
        book,
        fair_probability,
        size,
        runtime,
    )
}

pub fn validate_la7_taker_approval_artifact_text(
    text: &str,
    approval_id: &str,
) -> Result<LiveTakerCanaryApprovalFields, LiveTakerApprovalError> {
    validate_la7_taker_approval_artifact_text_for_status(
        text,
        approval_id,
        LA7_TAKER_APPROVAL_STATUS,
    )
}

pub fn validate_la7_taker_live_approval_artifact_text(
    text: &str,
    approval_id: &str,
) -> Result<LiveTakerCanaryLiveApprovalFields, LiveTakerApprovalError> {
    let approval = validate_la7_taker_approval_artifact_text_for_status(
        text,
        approval_id,
        LA7_TAKER_LIVE_APPROVAL_STATUS,
    )?;
    let mut errors = Vec::<String>::new();
    for field in LA7_TAKER_LIVE_APPROVAL_REQUIRED_FIELDS {
        match approval_table_value(text, field) {
            Some(value) if approval_value_is_final(&value) => {}
            Some(_) => errors.push(format!("approval_field_pending:{field}")),
            None => errors.push(format!("approval_field_missing:{field}")),
        }
    }
    if !errors.is_empty() {
        errors.sort_unstable();
        errors.dedup();
        return Err(LiveTakerApprovalError::Approval(errors));
    }

    let live = LiveTakerCanaryLiveApprovalFields {
        approval,
        approval_expires_at_unix: approval_u64(text, "approval_expires_at_unix")?,
        dry_run_report_path: approval_string(text, "dry_run_report_path")?,
        dry_run_report_sha256: approval_string(text, "dry_run_report_sha256")?,
        dry_run_decision_path: approval_string(text, "dry_run_decision_path")?,
        dry_run_decision_sha256: approval_string(text, "dry_run_decision_sha256")?,
    };
    validate_live_approval_fields(&live)?;
    Ok(live)
}

fn validate_la7_taker_approval_artifact_text_for_status(
    text: &str,
    approval_id: &str,
    required_status: &str,
) -> Result<LiveTakerCanaryApprovalFields, LiveTakerApprovalError> {
    let mut errors = Vec::<String>::new();
    if !text.contains(required_status) {
        errors.push("approval_status_missing".to_string());
    }
    if !text.contains(approval_id) {
        errors.push("approval_id_missing".to_string());
    }
    if approval_artifact_indicates_consumed_or_not_approved(text) {
        errors.push("approval_artifact_not_approved_or_consumed".to_string());
    }
    for field in LA7_TAKER_APPROVAL_REQUIRED_FIELDS {
        match approval_table_value(text, field) {
            Some(value) if approval_value_is_final(&value) => {}
            Some(_) => errors.push(format!("approval_field_pending:{field}")),
            None => errors.push(format!("approval_field_missing:{field}")),
        }
    }
    if !errors.is_empty() {
        errors.sort_unstable();
        errors.dedup();
        return Err(LiveTakerApprovalError::Approval(errors));
    }

    let fields = LiveTakerCanaryApprovalFields {
        approval_id: approval_string(text, "approval_id")?,
        baseline_id: approval_string(text, "baseline_id")?,
        baseline_capture_run_id: approval_string(text, "baseline_capture_run_id")?,
        baseline_hash: approval_string(text, "baseline_hash")?,
        wallet: approval_string(text, "wallet")?,
        funder: approval_string(text, "funder")?,
        market_slug: approval_string(text, "market_slug")?,
        condition_id: approval_string(text, "condition_id")?,
        token_id: approval_string(text, "token_id")?,
        outcome: approval_string(text, "outcome")?,
        side: approval_side(text, "side")?,
        max_size: approval_f64(text, "max_size")?,
        max_notional: approval_f64(text, "max_notional")?,
        worst_price: approval_f64(text, "worst_price")?,
        max_fee: approval_f64(text, "max_fee")?,
        max_slippage_bps: approval_u64(text, "max_slippage_bps")?,
        no_near_close_cutoff_seconds: approval_u64(text, "no_near_close_cutoff_seconds")?,
        max_orders_per_day: approval_u64(text, "max_orders_per_day")?,
        retry_after_ambiguous_submit: approval_string(text, "retry_after_ambiguous_submit")?,
        batch_orders: approval_string(text, "batch_orders")?,
        cancel_all: approval_string(text, "cancel_all")?,
    };

    validate_approval_fields(&fields)?;
    Ok(fields)
}

pub fn shadow_taker_report(
    decisions: &[LiveTakerGateDecision],
    fills: &[PaperFill],
    paper_total_pnl: f64,
    taker_disabled_by_default: bool,
) -> LiveTakerShadowReport {
    let mut report = LiveTakerShadowReport {
        evaluation_count: decisions.len() as u64,
        paper_total_pnl,
        taker_disabled_by_default,
        ..LiveTakerShadowReport::default()
    };
    let mut ev_count = 0_u64;

    for decision in decisions {
        if decision.would_take {
            report.would_take_count += 1;
            report.estimated_taker_fee += decision.taker_fee.unwrap_or_default();
            report.estimated_taker_notional += decision.notional;
        }
        if decision.live_allowed {
            report.live_allowed_count += 1;
        }
        for reason in &decision.reason_codes {
            *report
                .rejected_count_by_reason
                .entry(reason.clone())
                .or_default() += 1;
        }
        if has_reason(decision, "fee_reject") || has_reason(decision, "max_fee_spend_exceeded") {
            report.rejected_by_fee_count += 1;
        }
        if has_reason(decision, "insufficient_visible_depth") {
            report.rejected_by_depth_count += 1;
        }
        if has_reason(decision, "slippage_reject") || has_reason(decision, "max_slippage_exceeded")
        {
            report.rejected_by_slippage_count += 1;
        }
        if has_reason(decision, "latency_buffer_reject") {
            report.rejected_by_latency_buffer_count += 1;
        }
        if let Some(ev) = decision.estimated_ev_after_costs_bps {
            ev_count += 1;
            report.estimated_ev_after_costs_bps_sum += ev;
            report.estimated_ev_after_costs_bps_min = Some(
                report
                    .estimated_ev_after_costs_bps_min
                    .map_or(ev, |current| current.min(ev)),
            );
            report.estimated_ev_after_costs_bps_max = Some(
                report
                    .estimated_ev_after_costs_bps_max
                    .map_or(ev, |current| current.max(ev)),
            );
        }
    }

    if ev_count > 0 {
        report.estimated_ev_after_costs_bps_average =
            Some(report.estimated_ev_after_costs_bps_sum / ev_count as f64);
    }

    for fill in fills {
        match fill.liquidity {
            OrderKind::Maker => {
                report.paper_maker_fill_count += 1;
                report.paper_maker_filled_notional += fill.price * fill.size;
                report.paper_maker_fees_paid += fill.fee_paid;
            }
            OrderKind::Taker => {
                report.paper_taker_fill_count += 1;
                report.paper_taker_filled_notional += fill.price * fill.size;
                report.paper_taker_fees_paid += fill.fee_paid;
            }
        }
    }

    report
}

pub fn validate_taker_submit_input_without_network(
    input: &LiveTakerSubmitInput,
) -> LiveTakerSubmitResult<()> {
    validate_taker_submit_shape(input)?;
    let private_key = env_required_value(&input.signer_handle, "taker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let _l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let _l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    use polymarket_client_sdk_v2::auth::{LocalSigner, Uuid};

    LocalSigner::from_str(&private_key).map_err(|_| {
        LiveTakerSubmitError::Submit(
            "official SDK rejected the LA7 taker private-key handle value".to_string(),
        )
    })?;
    Uuid::parse_str(&l2_key).map_err(|_| {
        LiveTakerSubmitError::Submit(
            "official SDK rejected the LA7 clob_l2_access handle value".to_string(),
        )
    })?;
    parse_address(&input.wallet_address, "wallet_address")?;
    parse_address(&input.funder_address, "funder_address")?;
    parse_token_id(&input.approval.token_id)?;
    parse_decimal(taker_order_price(input)?, "price")?;
    parse_decimal(input.decision.size, "size")?;
    if input.signature_type == SignatureType::Eoa
        && !input
            .wallet_address
            .eq_ignore_ascii_case(&input.funder_address)
    {
        return Err(LiveTakerSubmitError::Submit(
            "EOA LA7 taker signer requires wallet_address and funder_address to match".to_string(),
        ));
    }
    Ok(())
}

pub async fn submit_taker_canary_with_official_sdk(
    input: LiveTakerSubmitInput,
) -> LiveTakerSubmitResult<LiveTakerSubmissionReport> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::types::{OrderType, Side as SdkSide};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::types::{Decimal, U256};
    use polymarket_client_sdk_v2::POLYGON;

    validate_taker_submit_input_without_network(&input)?;

    let private_key = env_required_value(&input.signer_handle, "taker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveTakerSubmitError::Submit(
                "official SDK rejected the LA7 taker private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveTakerSubmitError::Submit(
                "official SDK rejected the LA7 clob_l2_access handle value".to_string(),
            )
        })?,
        l2_secret,
        l2_passphrase,
    );

    let client = Client::new(&input.clob_host, Config::default()).map_err(sdk_error)?;
    let mut auth = client
        .authentication_builder(&signer)
        .credentials(credentials)
        .signature_type(sdk_signature_type(input.signature_type));
    if input.signature_type != SignatureType::Eoa {
        auth = auth.funder(parse_address(&input.funder_address, "funder_address")?);
    }
    let client = auth.authenticate().await.map_err(sdk_error)?;

    let token_id = U256::from_str(&input.approval.token_id)
        .map_err(|_| LiveTakerSubmitError::Submit("invalid LA7 token id".to_string()))?;
    let price = Decimal::from_str(&decimal_label(taker_order_price(&input)?))
        .map_err(|_| LiveTakerSubmitError::Submit("invalid LA7 taker price".to_string()))?;
    let size = Decimal::from_str(&decimal_label(input.decision.size))
        .map_err(|_| LiveTakerSubmitError::Submit("invalid LA7 taker size".to_string()))?;
    let side = match input.approval.side {
        Side::Buy => SdkSide::Buy,
        Side::Sell => SdkSide::Sell,
    };

    let signable_order = client
        .limit_order()
        .token_id(token_id)
        .price(price)
        .size(size)
        .side(side)
        .order_type(OrderType::GTC)
        .build()
        .await
        .map_err(sdk_error)?;
    let signed_order = client
        .sign(&signer, signable_order)
        .await
        .map_err(sdk_error)?;
    let response = client.post_order(signed_order).await.map_err(sdk_error)?;

    Ok(LiveTakerSubmissionReport {
        status: "submitted".to_string(),
        order_id: response.order_id,
        venue_status: response.status.to_string(),
        success: response.success,
        making_amount: response.making_amount.to_string(),
        taking_amount: response.taking_amount.to_string(),
        trade_ids: response.trade_ids,
        transaction_hashes: response
            .transaction_hashes
            .into_iter()
            .map(|hash| hash.to_string())
            .collect(),
        approval_sha256: input.approval_sha256,
        not_submitted: false,
        submitted_order_count: 1,
        order_type: "GTC".to_string(),
        post_only: false,
        fok_or_fak: false,
        batch_orders: false,
    })
}

fn validate_taker_submit_shape(input: &LiveTakerSubmitInput) -> LiveTakerSubmitResult<()> {
    let mut errors = Vec::<String>::new();
    if input.approval.side != Side::Buy {
        errors.push("approval_side_must_be_buy".to_string());
    }
    if input.decision.side != Side::Buy {
        errors.push("decision_side_must_be_buy".to_string());
    }
    if !input.decision.would_take {
        errors.push("decision_would_take_false".to_string());
    }
    if !input.decision.live_allowed {
        errors.push("decision_live_allowed_false".to_string());
    }
    if !input.decision.reason_codes.is_empty() {
        errors.push("decision_reason_codes_nonempty".to_string());
    }
    if input.decision.token_id != input.approval.token_id {
        errors.push("decision_token_mismatch".to_string());
    }
    if !input
        .decision
        .outcome
        .eq_ignore_ascii_case(&input.approval.outcome)
    {
        errors.push("decision_outcome_mismatch".to_string());
    }
    if input.decision.size <= 0.0 || !input.decision.size.is_finite() {
        errors.push("decision_size_invalid".to_string());
    } else if input.decision.size > input.approval.max_size + PRICE_EPSILON {
        errors.push("decision_size_exceeds_approval".to_string());
    }
    if input.decision.notional <= 0.0 || !input.decision.notional.is_finite() {
        errors.push("decision_notional_invalid".to_string());
    } else if input.decision.notional > input.approval.max_notional + PRICE_EPSILON {
        errors.push("decision_notional_exceeds_approval".to_string());
    }
    match taker_order_price(input) {
        Ok(price) if price <= input.approval.worst_price + PRICE_EPSILON => {}
        Ok(_) => errors.push("decision_worst_price_limit_exceeds_approval".to_string()),
        Err(error) => errors.push(error.to_string()),
    }
    if input
        .decision
        .taker_fee
        .is_some_and(|fee| fee > input.approval.max_fee + PRICE_EPSILON)
    {
        errors.push("decision_fee_exceeds_approval".to_string());
    }
    if input
        .decision
        .slippage_bps
        .is_some_and(|slippage| slippage > input.approval.max_slippage_bps as f64 + PRICE_EPSILON)
    {
        errors.push("decision_slippage_exceeds_approval".to_string());
    }
    if !input
        .approval
        .retry_after_ambiguous_submit
        .trim()
        .eq_ignore_ascii_case("forbidden")
    {
        errors.push("retry_after_ambiguous_submit_not_forbidden".to_string());
    }
    if !input
        .approval
        .batch_orders
        .trim()
        .eq_ignore_ascii_case("forbidden")
    {
        errors.push("batch_orders_not_forbidden".to_string());
    }
    if !input
        .approval
        .cancel_all
        .trim()
        .eq_ignore_ascii_case("forbidden")
    {
        errors.push("cancel_all_not_forbidden".to_string());
    }
    if input.approval.max_orders_per_day != 1 {
        errors.push("max_orders_per_day_must_equal_1".to_string());
    }
    if !is_sha256_label(&input.approval_sha256) {
        errors.push("approval_sha256_invalid".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        errors.sort_unstable();
        errors.dedup();
        Err(LiveTakerSubmitError::Validation(errors))
    }
}

fn taker_order_price(input: &LiveTakerSubmitInput) -> LiveTakerSubmitResult<f64> {
    let price = input
        .decision
        .worst_price_limit
        .or(input.decision.worst_price)
        .ok_or_else(|| LiveTakerSubmitError::shape("decision_worst_price_limit_missing"))?;
    if probability_price(price) {
        Ok(price)
    } else {
        Err(LiveTakerSubmitError::shape(
            "decision_worst_price_limit_invalid",
        ))
    }
}

fn validate_approval_fields(
    fields: &LiveTakerCanaryApprovalFields,
) -> Result<(), LiveTakerApprovalError> {
    let mut errors = Vec::<String>::new();
    if fields.side != Side::Buy {
        errors.push("approval_side_must_be_buy".to_string());
    }
    if !positive_finite(fields.max_size) {
        errors.push("approval_field_parse_error:max_size".to_string());
    }
    if !positive_finite(fields.max_notional) {
        errors.push("approval_field_parse_error:max_notional".to_string());
    }
    if !probability_price(fields.worst_price) {
        errors.push("approval_field_parse_error:worst_price".to_string());
    }
    if !fields.max_fee.is_finite() || fields.max_fee < 0.0 {
        errors.push("approval_field_parse_error:max_fee".to_string());
    }
    if fields.no_near_close_cutoff_seconds == 0 {
        errors.push("approval_field_parse_error:no_near_close_cutoff_seconds".to_string());
    }
    if fields.max_orders_per_day != 1 {
        errors.push("approval_max_orders_per_day_must_equal_1".to_string());
    }
    for (field, value) in [
        (
            "retry_after_ambiguous_submit",
            fields.retry_after_ambiguous_submit.as_str(),
        ),
        ("batch_orders", fields.batch_orders.as_str()),
        ("cancel_all", fields.cancel_all.as_str()),
    ] {
        if !value.trim().eq_ignore_ascii_case("forbidden") {
            errors.push(format!("approval_field_must_be_forbidden:{field}"));
        }
    }
    if !errors.is_empty() {
        errors.sort_unstable();
        errors.dedup();
        return Err(LiveTakerApprovalError::Approval(errors));
    }
    Ok(())
}

fn validate_live_approval_fields(
    fields: &LiveTakerCanaryLiveApprovalFields,
) -> Result<(), LiveTakerApprovalError> {
    let mut errors = Vec::<String>::new();
    if fields.approval_expires_at_unix == 0 {
        errors.push("approval_field_parse_error:approval_expires_at_unix".to_string());
    }
    for (field, value) in [
        ("dry_run_report_path", fields.dry_run_report_path.as_str()),
        (
            "dry_run_report_sha256",
            fields.dry_run_report_sha256.as_str(),
        ),
        (
            "dry_run_decision_path",
            fields.dry_run_decision_path.as_str(),
        ),
        (
            "dry_run_decision_sha256",
            fields.dry_run_decision_sha256.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            errors.push(format!("approval_field_missing:{field}"));
        }
    }
    for (field, value) in [
        (
            "dry_run_report_sha256",
            fields.dry_run_report_sha256.as_str(),
        ),
        (
            "dry_run_decision_sha256",
            fields.dry_run_decision_sha256.as_str(),
        ),
    ] {
        if !is_sha256_label(value) {
            errors.push(format!("approval_field_parse_error:{field}"));
        }
    }
    if !errors.is_empty() {
        errors.sort_unstable();
        errors.dedup();
        return Err(LiveTakerApprovalError::Approval(errors));
    }
    Ok(())
}

fn is_sha256_label(value: &str) -> bool {
    value
        .trim()
        .strip_prefix("sha256:")
        .is_some_and(|hash| hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn approval_artifact_indicates_consumed_or_not_approved(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    [
        "NOT APPROVED",
        "NOT EXECUTABLE",
        "APPROVAL CONSUMED",
        "AUTHORIZED SESSION COMPLETED",
        "EXECUTION GATE STATUS: LA7 RUN COMPLETE",
        "EXECUTION GATE STATUS: LA7 RUN COMPLETED",
        "EXECUTION GATE STATUS: LA7 RUN CONSUMED",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn approval_table_value(text: &str, field: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let cells = line.split('|').map(str::trim).collect::<Vec<_>>();
        if cells.len() >= 3 && cells[1] == field {
            Some(cells[2].trim_matches('`').trim().to_string())
        } else {
            None
        }
    })
}

fn approval_value_is_final(value: &str) -> bool {
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

fn approval_string(text: &str, field: &'static str) -> Result<String, LiveTakerApprovalError> {
    approval_table_value(text, field).ok_or_else(|| {
        LiveTakerApprovalError::Approval(vec![format!("approval_field_missing:{field}")])
    })
}

fn approval_u64(text: &str, field: &'static str) -> Result<u64, LiveTakerApprovalError> {
    let value = approval_string(text, field)?;
    let trimmed = value.trim();
    if approval_numeric_value_is_negated(trimmed) {
        return Err(approval_parse_error(field));
    }
    let end = trimmed
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if end == 0 {
        return Err(approval_parse_error(field));
    }
    let suffix = trimmed[end..].strip_prefix('`').unwrap_or(&trimmed[end..]);
    if suffix.chars().next().is_some_and(|ch| !ch.is_whitespace()) {
        return Err(approval_parse_error(field));
    }
    trimmed[..end].parse::<u64>().map_err(|_| {
        LiveTakerApprovalError::Approval(vec![format!("approval_field_parse_error:{field}")])
    })
}

fn approval_f64(text: &str, field: &'static str) -> Result<f64, LiveTakerApprovalError> {
    let value = approval_string(text, field)?;
    let trimmed = value.trim();
    if approval_numeric_value_is_negated(trimmed) {
        return Err(approval_parse_error(field));
    }
    let token = trimmed
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(',');
    token
        .parse::<f64>()
        .map_err(|_| approval_parse_error(field))
}

fn approval_side(text: &str, field: &'static str) -> Result<Side, LiveTakerApprovalError> {
    match approval_string(text, field)?
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "BUY" => Ok(Side::Buy),
        "SELL" => Ok(Side::Sell),
        _ => Err(approval_parse_error(field)),
    }
}

fn approval_parse_error(field: &'static str) -> LiveTakerApprovalError {
    LiveTakerApprovalError::Approval(vec![format!("approval_field_parse_error:{field}")])
}

fn approval_numeric_value_is_negated(value: &str) -> bool {
    value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|ch| ch == '-' || ch == '−')
}

#[allow(clippy::too_many_arguments)]
fn evaluate_buy_taker(
    config: &TakerGateConfig,
    snapshot: &DecisionSnapshot,
    token_id: &str,
    outcome: &str,
    book: Option<&TokenBookSnapshot>,
    fair_probability: Option<f64>,
    target_size: f64,
    runtime: LiveTakerRuntimeState,
) -> LiveTakerGateDecision {
    let mut reasons = Vec::<&'static str>::new();
    let side = Side::Buy;
    let book_fresh = book_is_fresh(snapshot, token_id, config.max_book_age_ms);
    let reference_fresh = references_are_fresh(snapshot, config.max_reference_age_ms);
    let market_status_valid = snapshot.lifecycle_state == MarketLifecycleState::Active
        && snapshot.market.lifecycle_state == MarketLifecycleState::Active;
    let market_time_valid = market_time_valid(
        snapshot.snapshot_wall_ts,
        snapshot.market.end_ts,
        config.no_trade_seconds_before_close,
    );

    if !market_status_valid {
        reasons.push("market_status_invalid");
    }
    if !book_fresh {
        reasons.push("book_stale");
    }
    if !reference_fresh {
        reasons.push("reference_stale");
    }
    if !market_time_valid {
        reasons.push("market_too_close_to_close");
    }
    if !config.live_alpha_enabled {
        reasons.push("live_alpha_disabled");
    }
    if config.live_alpha_mode != LiveAlphaMode::TakerGate {
        reasons.push("taker_gate_mode_not_enabled");
    }
    if !config.taker_enabled {
        reasons.push("taker_disabled");
    }
    if !runtime.geoblock_passed {
        reasons.push("geoblock_not_passed");
    }
    if !runtime.heartbeat_healthy {
        reasons.push("heartbeat_not_healthy");
    }
    if !runtime.reconciliation_clean {
        reasons.push("reconciliation_not_clean");
    }
    if !runtime.inventory_clean {
        reasons.push("inventory_not_clean");
    }
    if !runtime.baseline_ready {
        reasons.push("baseline_not_ready");
    }
    if !runtime.live_risk_controls_passed {
        reasons.push("live_risk_controls_not_passed");
    }
    if runtime.existing_taker_orders_today >= config.max_orders_per_day {
        reasons.push("max_orders_per_day_exceeded");
    }
    if !positive_finite(target_size) {
        reasons.push("invalid_taker_size");
        return empty_decision(snapshot, token_id, outcome, side, fair_probability, reasons);
    }

    let Some(book) = book else {
        reasons.push("missing_book");
        return empty_decision(snapshot, token_id, outcome, side, fair_probability, reasons);
    };
    let Some(best_bid) = book.best_bid else {
        reasons.push("missing_best_bid");
        return empty_decision(snapshot, token_id, outcome, side, fair_probability, reasons);
    };
    let Some(best_ask) = book.best_ask else {
        reasons.push("missing_best_ask");
        return empty_decision(snapshot, token_id, outcome, side, fair_probability, reasons);
    };
    let Some(fair_probability) = fair_probability.filter(|value| probability_price(*value)) else {
        reasons.push("missing_fair_probability");
        return empty_decision(snapshot, token_id, outcome, side, None, reasons);
    };

    let depth = consume_buy_depth(&book.asks.levels, target_size);
    let Some(depth) = depth else {
        reasons.push("insufficient_visible_depth");
        return LiveTakerGateDecision {
            market_id: snapshot.market.market_id.clone(),
            token_id: token_id.to_string(),
            outcome: outcome.to_string(),
            side,
            would_take: false,
            live_allowed: false,
            reason_codes: reason_strings(reasons),
            fair_probability: Some(fair_probability),
            market_probability: Some((best_bid + best_ask) / 2.0),
            best_bid: Some(best_bid),
            best_ask: Some(best_ask),
            average_price: None,
            worst_price: None,
            worst_price_limit: worst_price_limit(best_ask, config.max_slippage_bps),
            size: target_size,
            notional: 0.0,
            visible_depth: book.asks.visible_depth,
            gross_edge_bps: None,
            spread_cost_bps: None,
            taker_fee_bps: None,
            taker_fee: None,
            slippage_bps: None,
            latency_buffer_bps: config.latency_buffer_bps,
            adverse_selection_buffer_bps: config.adverse_selection_buffer_bps,
            minimum_profit_buffer_bps: config.minimum_profit_buffer_bps,
            estimated_ev_after_costs_bps: None,
        };
    };

    let notional = target_size * depth.average_price;
    let taker_fee = fee_paid(
        target_size,
        depth.average_price,
        OrderKind::Taker,
        &snapshot.market.fee_parameters,
    );
    let taker_fee_bps = if target_size > 0.0 {
        Some((taker_fee / target_size) * 10_000.0)
    } else {
        None
    };
    let market_probability = (best_bid + best_ask) / 2.0;
    let gross_edge_bps = (fair_probability - market_probability) * 10_000.0;
    let spread_cost_bps = (best_ask - market_probability).max(0.0) * 10_000.0;
    let slippage_bps = (depth.average_price - best_ask).max(0.0) * 10_000.0;
    let fee_cost_bps = taker_fee_bps.unwrap_or_default();
    let estimated_ev_after_costs_bps = gross_edge_bps
        - spread_cost_bps
        - fee_cost_bps
        - slippage_bps
        - config.latency_buffer_bps
        - config.adverse_selection_buffer_bps
        - config.minimum_profit_buffer_bps;

    let ev_without_fee = estimated_ev_after_costs_bps + fee_cost_bps;
    let ev_without_slippage = estimated_ev_after_costs_bps + slippage_bps;
    let ev_without_latency = estimated_ev_after_costs_bps + config.latency_buffer_bps;

    if config.max_taker_notional <= 0.0 || notional > config.max_taker_notional {
        reasons.push("max_taker_notional_exceeded");
    }
    if config.max_total_live_notional <= 0.0
        || runtime.current_total_live_notional + notional > config.max_total_live_notional
    {
        reasons.push("max_total_live_notional_exceeded");
    }
    if config.max_fee_spend <= 0.0
        || runtime.existing_taker_fee_spend + taker_fee > config.max_fee_spend
    {
        reasons.push("max_fee_spend_exceeded");
    }
    if depth.worst_price > worst_price_limit(best_ask, config.max_slippage_bps).unwrap_or(best_ask)
    {
        reasons.push("worst_price_limit_exceeded");
    }
    if slippage_bps > config.max_slippage_bps {
        reasons.push("max_slippage_exceeded");
    }
    if ev_without_fee > 0.0 && estimated_ev_after_costs_bps <= 0.0 {
        reasons.push("fee_reject");
    }
    if ev_without_slippage > 0.0 && estimated_ev_after_costs_bps <= 0.0 {
        reasons.push("slippage_reject");
    }
    if ev_without_latency > 0.0 && estimated_ev_after_costs_bps <= 0.0 {
        reasons.push("latency_buffer_reject");
    }
    if estimated_ev_after_costs_bps <= 0.0 {
        reasons.push("expected_value_below_costs");
    }

    let reason_codes = reason_strings(reasons);
    let would_take = !reason_codes
        .iter()
        .any(|reason| is_shadow_rejection(reason.as_str()));
    let live_allowed = would_take
        && config.live_alpha_enabled
        && config.live_alpha_mode == LiveAlphaMode::TakerGate
        && config.taker_enabled
        && runtime.geoblock_passed
        && runtime.heartbeat_healthy
        && runtime.reconciliation_clean
        && runtime.inventory_clean
        && runtime.baseline_ready
        && runtime.live_risk_controls_passed;

    LiveTakerGateDecision {
        market_id: snapshot.market.market_id.clone(),
        token_id: token_id.to_string(),
        outcome: outcome.to_string(),
        side,
        would_take,
        live_allowed,
        reason_codes,
        fair_probability: Some(fair_probability),
        market_probability: Some(market_probability),
        best_bid: Some(best_bid),
        best_ask: Some(best_ask),
        average_price: Some(depth.average_price),
        worst_price: Some(depth.worst_price),
        worst_price_limit: worst_price_limit(best_ask, config.max_slippage_bps),
        size: target_size,
        notional,
        visible_depth: depth.visible_depth,
        gross_edge_bps: Some(gross_edge_bps),
        spread_cost_bps: Some(spread_cost_bps),
        taker_fee_bps,
        taker_fee: Some(taker_fee),
        slippage_bps: Some(slippage_bps),
        latency_buffer_bps: config.latency_buffer_bps,
        adverse_selection_buffer_bps: config.adverse_selection_buffer_bps,
        minimum_profit_buffer_bps: config.minimum_profit_buffer_bps,
        estimated_ev_after_costs_bps: Some(estimated_ev_after_costs_bps),
    }
}

fn empty_decision(
    snapshot: &DecisionSnapshot,
    token_id: &str,
    outcome: &str,
    side: Side,
    fair_probability: Option<f64>,
    reasons: Vec<&'static str>,
) -> LiveTakerGateDecision {
    LiveTakerGateDecision {
        market_id: snapshot.market.market_id.clone(),
        token_id: token_id.to_string(),
        outcome: outcome.to_string(),
        side,
        would_take: false,
        live_allowed: false,
        reason_codes: reason_strings(reasons),
        fair_probability,
        market_probability: None,
        best_bid: None,
        best_ask: None,
        average_price: None,
        worst_price: None,
        worst_price_limit: None,
        size: snapshot.market.min_order_size.max(0.0),
        notional: 0.0,
        visible_depth: 0.0,
        gross_edge_bps: None,
        spread_cost_bps: None,
        taker_fee_bps: None,
        taker_fee: None,
        slippage_bps: None,
        latency_buffer_bps: 0.0,
        adverse_selection_buffer_bps: 0.0,
        minimum_profit_buffer_bps: 0.0,
        estimated_ev_after_costs_bps: None,
    }
}

fn consume_buy_depth(levels: &[PriceLevelSnapshot], target_size: f64) -> Option<DepthCheck> {
    if !positive_finite(target_size) {
        return None;
    }

    let mut remaining = target_size;
    let mut notional = 0.0;
    let mut worst_price = 0.0_f64;
    let mut visible_depth = 0.0_f64;

    for level in levels {
        if !probability_price(level.price) || !positive_finite(level.size) {
            continue;
        }
        visible_depth += level.size;
        if remaining <= PRICE_EPSILON {
            continue;
        }
        let fill_size = remaining.min(level.size);
        notional += fill_size * level.price;
        worst_price = worst_price.max(level.price);
        remaining -= fill_size;
    }

    if remaining > PRICE_EPSILON {
        None
    } else {
        Some(DepthCheck {
            average_price: notional / target_size,
            worst_price,
            visible_depth,
        })
    }
}

fn worst_price_limit(best_ask: f64, max_slippage_bps: f64) -> Option<f64> {
    if !probability_price(best_ask) || !max_slippage_bps.is_finite() || max_slippage_bps < 0.0 {
        return None;
    }
    Some((best_ask + max_slippage_bps / 10_000.0).min(1.0))
}

fn outcome_fair_probability(outcome: &str, probability_up: Option<f64>) -> Option<f64> {
    let up = probability_up?;
    match outcome.trim().to_ascii_lowercase().as_str() {
        "up" | "yes" => Some(up),
        "down" | "no" => Some(1.0 - up),
        _ => None,
    }
}

fn book_is_fresh(snapshot: &DecisionSnapshot, token_id: &str, max_age_ms: u64) -> bool {
    matching_book_freshness(snapshot, token_id).is_some_and(|freshness| {
        !freshness.is_stale && freshness.age_ms.unwrap_or(i64::MAX) <= max_age_ms as i64
    })
}

fn matching_book_freshness<'a>(
    snapshot: &'a DecisionSnapshot,
    token_id: &str,
) -> Option<&'a BookFreshness> {
    snapshot.book_freshness.iter().find(|freshness| {
        freshness.token_id == token_id
            && (freshness.market_id == snapshot.market.market_id
                || freshness.market_id == snapshot.market.condition_id)
    })
}

fn references_are_fresh(snapshot: &DecisionSnapshot, max_age_ms: u64) -> bool {
    !snapshot.reference_freshness.is_empty()
        && snapshot.reference_freshness.iter().all(|freshness| {
            !freshness.is_stale && freshness.age_ms.unwrap_or(i64::MAX) <= max_age_ms as i64
        })
}

fn market_time_valid(now_ms: i64, end_ms: i64, no_trade_seconds_before_close: u64) -> bool {
    let cutoff_ms = (no_trade_seconds_before_close as i64).saturating_mul(1_000);
    now_ms.saturating_add(cutoff_ms) < end_ms
}

fn reason_strings(mut reasons: Vec<&'static str>) -> Vec<String> {
    reasons.sort_unstable();
    reasons.dedup();
    reasons.into_iter().map(str::to_string).collect()
}

fn is_shadow_rejection(reason: &str) -> bool {
    !matches!(
        reason,
        "live_alpha_disabled"
            | "taker_gate_mode_not_enabled"
            | "taker_disabled"
            | "geoblock_not_passed"
            | "heartbeat_not_healthy"
            | "reconciliation_not_clean"
            | "inventory_not_clean"
            | "baseline_not_ready"
            | "live_risk_controls_not_passed"
    )
}

fn has_reason(decision: &LiveTakerGateDecision, reason: &str) -> bool {
    decision
        .reason_codes
        .iter()
        .any(|candidate| candidate == reason)
}

fn positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn probability_price(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value < 1.0
}

fn env_required_value(handle: &str, label: &'static str) -> LiveTakerSubmitResult<String> {
    let value = env::var(handle).map_err(|_| LiveTakerSubmitError::MissingSecretHandle {
        label,
        handle: handle.to_string(),
    })?;
    if value.trim().is_empty() {
        return Err(LiveTakerSubmitError::MissingSecretHandle {
            label,
            handle: handle.to_string(),
        });
    }
    Ok(value)
}

fn parse_address(
    value: &str,
    label: &'static str,
) -> LiveTakerSubmitResult<polymarket_client_sdk_v2::types::Address> {
    polymarket_client_sdk_v2::types::Address::from_str(value)
        .map_err(|_| LiveTakerSubmitError::Submit(format!("official SDK rejected {label}")))
}

fn parse_token_id(value: &str) -> LiveTakerSubmitResult<polymarket_client_sdk_v2::types::U256> {
    polymarket_client_sdk_v2::types::U256::from_str(value)
        .map_err(|_| LiveTakerSubmitError::Submit("official SDK rejected token id".to_string()))
}

fn parse_decimal(value: f64, label: &'static str) -> LiveTakerSubmitResult<()> {
    polymarket_client_sdk_v2::types::Decimal::from_str(&decimal_label(value))
        .map(|_| ())
        .map_err(|_| LiveTakerSubmitError::Submit(format!("official SDK rejected {label}")))
}

fn sdk_signature_type(
    value: SignatureType,
) -> polymarket_client_sdk_v2::clob::types::SignatureType {
    match value {
        SignatureType::Eoa => polymarket_client_sdk_v2::clob::types::SignatureType::Eoa,
        SignatureType::PolyProxy => polymarket_client_sdk_v2::clob::types::SignatureType::Proxy,
        SignatureType::GnosisSafe => {
            polymarket_client_sdk_v2::clob::types::SignatureType::GnosisSafe
        }
    }
}

fn sdk_error(source: polymarket_client_sdk_v2::error::Error) -> LiveTakerSubmitError {
    LiveTakerSubmitError::Submit(format!("official SDK LA7 taker path failed: {source}"))
}

fn decimal_label(value: f64) -> String {
    let rounded = format!("{value:.6}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub type LiveTakerSubmitResult<T> = Result<T, LiveTakerSubmitError>;

#[derive(Debug)]
pub enum LiveTakerSubmitError {
    Validation(Vec<String>),
    MissingSecretHandle { label: &'static str, handle: String },
    Submit(String),
    Shape { reason: String },
}

impl LiveTakerSubmitError {
    fn shape(reason: &'static str) -> Self {
        Self::Shape {
            reason: reason.to_string(),
        }
    }
}

impl Display for LiveTakerSubmitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(errors) => {
                writeln!(formatter, "live taker validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
            Self::MissingSecretHandle { label, handle } => {
                write!(formatter, "missing LA7 {label} env handle {handle}")
            }
            Self::Submit(message) => write!(formatter, "{message}"),
            Self::Shape { reason } => write!(formatter, "{reason}"),
        }
    }
}

impl Error for LiveTakerSubmitError {}

fn stricter_positive_f64(primary: f64, fallback: f64) -> f64 {
    match (positive_finite(primary), positive_finite(fallback)) {
        (true, true) => primary.min(fallback),
        (true, false) => primary,
        (false, true) => fallback,
        (false, false) => 0.0,
    }
}

fn stricter_positive_u64(primary: u64, fallback: u64) -> u64 {
    match (primary, fallback) {
        (0, 0) => 0,
        (0, fallback) => fallback,
        (primary, 0) => primary,
        (primary, fallback) => primary.min(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::domain::{
        Asset, FeeParameters, Market, MarketLifecycleState, OrderBookLevel, OutcomeToken,
        ReferencePrice,
    };
    use crate::events::{EventEnvelope, NormalizedEvent};
    use crate::state::StateStore;

    #[test]
    fn live_taker_gate_accepts_only_when_edge_clears_all_costs() {
        let mut config = config();
        config.live_alpha.enabled = true;
        config.live_alpha.mode = LiveAlphaMode::TakerGate;
        config.live_alpha.taker.enabled = true;
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.min_ev_after_all_costs_bps = 50;
        config.live_alpha.taker.max_slippage_bps = 100;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.no_trade_seconds_before_close = 30;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        config.strategy.latency_buffer_ms = 50;
        config.strategy.adverse_selection_bps = 10;
        let snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);

        let decisions = evaluate_shadow_taker_snapshot(
            &config,
            &snapshot,
            LiveTakerRuntimeState {
                geoblock_passed: true,
                heartbeat_healthy: true,
                reconciliation_clean: true,
                inventory_clean: true,
                baseline_ready: true,
                live_risk_controls_passed: true,
                ..LiveTakerRuntimeState::default()
            },
        );

        let up = decisions
            .iter()
            .find(|decision| decision.outcome == "Up")
            .expect("up decision");
        assert!(up.would_take);
        assert!(up.live_allowed);
        assert!(up.reason_codes.is_empty());
        assert!(up.estimated_ev_after_costs_bps.unwrap() > 0.0);
    }

    #[test]
    fn fee_model_rejects_when_taker_fee_removes_required_edge() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.min_ev_after_all_costs_bps = 50;
        config.live_alpha.taker.max_slippage_bps = 100;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        config.strategy.latency_buffer_ms = 1;
        config.strategy.adverse_selection_bps = 0;
        let snapshot = snapshot(0.532, vec![(0.51, 10.0)], 1_777_000_000_000);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(!up.would_take);
        assert!(up.reason_codes.contains(&"fee_reject".to_string()));
        assert!(up
            .reason_codes
            .contains(&"expected_value_below_costs".to_string()));
    }

    #[test]
    fn depth_check_rejects_insufficient_visible_size() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        let snapshot = snapshot(0.60, vec![(0.51, 2.0)], 1_777_000_000_000);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(!up.would_take);
        assert!(up
            .reason_codes
            .contains(&"insufficient_visible_depth".to_string()));
    }

    #[test]
    fn depth_check_enforces_worst_price_and_slippage_limits() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_slippage_bps = 10;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        let snapshot = snapshot(0.70, vec![(0.51, 3.0), (0.54, 3.0)], 1_777_000_000_000);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(!up.would_take);
        assert!(up
            .reason_codes
            .contains(&"worst_price_limit_exceeded".to_string()));
        assert!(up
            .reason_codes
            .contains(&"max_slippage_exceeded".to_string()));
    }

    #[test]
    fn live_taker_gate_keeps_live_blocked_when_default_gates_are_not_ready() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        let snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(up.would_take);
        assert!(!up.live_allowed);
        assert!(up.reason_codes.contains(&"taker_disabled".to_string()));
        assert!(up.reason_codes.contains(&"baseline_not_ready".to_string()));
    }

    #[test]
    fn live_taker_gate_uses_stricter_live_alpha_freshness_limits() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        config.risk.stale_book_ms = 1_000;
        config.risk.stale_reference_ms = 1_000;
        let mut snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);
        set_snapshot_ages(&mut snapshot, 750);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(!up.would_take);
        assert!(up.reason_codes.contains(&"book_stale".to_string()));
        assert!(up.reason_codes.contains(&"reference_stale".to_string()));
    }

    #[test]
    fn live_taker_gate_falls_back_to_global_freshness_when_la7_limit_is_zero() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 0;
        config.live_alpha.risk.max_reference_staleness_ms = 0;
        config.risk.stale_book_ms = 1_000;
        config.risk.stale_reference_ms = 1_000;
        let mut snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);
        set_snapshot_ages(&mut snapshot, 750);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(up.would_take);
        assert!(!up.reason_codes.contains(&"book_stale".to_string()));
        assert!(!up.reason_codes.contains(&"reference_stale".to_string()));
    }

    #[test]
    fn live_taker_gate_uses_stricter_single_order_notional_cap() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_single_order_notional = 2.0;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        let snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);

        let up =
            evaluate_shadow_taker_snapshot(&config, &snapshot, LiveTakerRuntimeState::default())
                .into_iter()
                .find(|decision| decision.outcome == "Up")
                .expect("up decision");

        assert!(!up.would_take);
        assert!(up
            .reason_codes
            .contains(&"max_taker_notional_exceeded".to_string()));
    }

    #[test]
    fn shadow_taker_report_splits_maker_and_taker_fill_costs() {
        let maker = fill("maker-fill", OrderKind::Maker, 0.50, 2.0, 0.0);
        let taker = fill("taker-fill", OrderKind::Taker, 0.52, 3.0, 0.01);
        let report = shadow_taker_report(&[], &[maker, taker], -0.25, true);

        assert_eq!(report.paper_maker_fill_count, 1);
        assert_eq!(report.paper_taker_fill_count, 1);
        assert_close(report.paper_maker_filled_notional, 1.0);
        assert_close(report.paper_taker_filled_notional, 1.56);
        assert_close(report.paper_taker_fees_paid, 0.01);
        assert_close(report.paper_total_pnl, -0.25);
        assert!(report.taker_disabled_by_default);
    }

    #[test]
    fn la7_taker_approval_artifact_parses_required_fields() {
        let fields = validate_la7_taker_approval_artifact_text(
            valid_la7_taker_approval_artifact(),
            "LA7-2026-05-08-taker-dry-run-001",
        )
        .expect("approval parses");

        assert_eq!(fields.approval_id, "LA7-2026-05-08-taker-dry-run-001");
        assert_eq!(fields.baseline_id, "LA7-2026-05-08-wallet-baseline-003");
        assert_eq!(fields.baseline_capture_run_id, "baseline-run-003");
        assert_eq!(fields.baseline_hash, "sha256:abc123");
        assert_eq!(fields.wallet, "0x280ca8b14386Fe4203670538CCdE636C295d74E9");
        assert_eq!(fields.funder, "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159");
        assert_eq!(fields.market_slug, "btc-up-down-15m-1777000000");
        assert_eq!(fields.condition_id, "condition-1");
        assert_eq!(fields.token_id, "token-up");
        assert_eq!(fields.outcome, "Up");
        assert_eq!(fields.side, Side::Buy);
        assert_close(fields.max_size, 5.0);
        assert_close(fields.max_notional, 2.75);
        assert_close(fields.worst_price, 0.55);
        assert_close(fields.max_fee, 0.02);
        assert_eq!(fields.max_slippage_bps, 25);
        assert_eq!(fields.no_near_close_cutoff_seconds, 600);
        assert_eq!(fields.max_orders_per_day, 1);
        assert_eq!(fields.retry_after_ambiguous_submit, "forbidden");
        assert_eq!(fields.batch_orders, "forbidden");
        assert_eq!(fields.cancel_all, "forbidden");
    }

    #[test]
    fn la7_taker_approval_rejects_not_approved_candidate() {
        let text = valid_la7_taker_approval_artifact().replace(
            LA7_TAKER_APPROVAL_STATUS,
            "Status: NOT APPROVED; NOT EXECUTABLE",
        );

        let error =
            validate_la7_taker_approval_artifact_text(&text, "LA7-2026-05-08-taker-dry-run-001")
                .expect_err("approval is not executable");

        match error {
            LiveTakerApprovalError::Approval(errors) => {
                assert!(errors.contains(&"approval_status_missing".to_string()));
                assert!(errors.contains(&"approval_artifact_not_approved_or_consumed".to_string()));
            }
        }
    }

    #[test]
    fn la7_taker_approval_requires_buy_and_forbidden_policies() {
        let text = valid_la7_taker_approval_artifact()
            .replace("| side | BUY |", "| side | SELL |")
            .replace(
                "| retry_after_ambiguous_submit | forbidden |",
                "| retry_after_ambiguous_submit | allowed |",
            )
            .replace("| max_orders_per_day | 1 |", "| max_orders_per_day | 2 |");

        let error =
            validate_la7_taker_approval_artifact_text(&text, "LA7-2026-05-08-taker-dry-run-001")
                .expect_err("approval has unsafe fields");

        match error {
            LiveTakerApprovalError::Approval(errors) => {
                assert!(errors.contains(&"approval_side_must_be_buy".to_string()));
                assert!(errors.contains(
                    &"approval_field_must_be_forbidden:retry_after_ambiguous_submit".to_string()
                ));
                assert!(errors.contains(&"approval_max_orders_per_day_must_equal_1".to_string()));
            }
        }
    }

    #[test]
    fn live_taker_canary_uses_approval_size_instead_of_market_minimum() {
        let mut config = config();
        config.live_alpha.taker.max_notional = 10.0;
        config.live_alpha.taker.max_orders_per_day = 1;
        config.live_alpha.risk.max_fee_spend = 1.0;
        config.live_alpha.risk.max_total_live_notional = 10.0;
        config.live_alpha.risk.max_book_staleness_ms = 500;
        config.live_alpha.risk.max_reference_staleness_ms = 500;
        let snapshot = snapshot(0.60, vec![(0.51, 10.0)], 1_777_000_000_000);

        let decision = evaluate_taker_canary_snapshot(
            &config,
            &snapshot,
            LiveTakerRuntimeState::default(),
            "token-up",
            "Up",
            Side::Buy,
            6.0,
        );

        assert_close(decision.size, 6.0);
        assert_close(decision.notional, 3.06);
        assert_close(decision.visible_depth, 10.0);
    }

    #[test]
    fn la7_taker_live_approval_requires_live_status_and_evidence_hashes() {
        let live = validate_la7_taker_live_approval_artifact_text(
            valid_la7_taker_live_approval_artifact(),
            "LA7-2026-05-09-taker-live-001",
        )
        .expect("live approval parses");

        assert_eq!(live.approval.approval_id, "LA7-2026-05-09-taker-live-001");
        assert_eq!(
            live.approval.baseline_id,
            "LA7-2026-05-08-wallet-baseline-003"
        );
        assert_eq!(live.approval_expires_at_unix, 1_778_000_000);
        assert_eq!(
            live.dry_run_report_path,
            "reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_report.json"
        );
        assert!(is_sha256_label(&live.dry_run_report_sha256));

        let error = validate_la7_taker_live_approval_artifact_text(
            valid_la7_taker_approval_artifact(),
            "LA7-2026-05-08-taker-dry-run-001",
        )
        .expect_err("dry-run approval must not authorize live canary");

        match error {
            LiveTakerApprovalError::Approval(errors) => {
                assert!(errors.contains(&"approval_status_missing".to_string()));
            }
        }
    }

    #[test]
    fn la7_taker_submit_shape_is_one_gtc_buy_without_batch_or_fok_fak() {
        let mut input = sample_taker_submit_input();
        validate_taker_submit_shape(&input).expect("safe shape validates");

        input.decision.worst_price_limit = Some(0.49);
        let error = validate_taker_submit_shape(&input)
            .expect_err("price above exact approval worst price fails");
        assert!(error
            .to_string()
            .contains("decision_worst_price_limit_exceeds_approval"));

        input = sample_taker_submit_input();
        input.approval.batch_orders = "allowed".to_string();
        let error =
            validate_taker_submit_shape(&input).expect_err("batch orders must stay forbidden");
        assert!(error.to_string().contains("batch_orders_not_forbidden"));
    }

    fn valid_la7_taker_approval_artifact() -> &'static str {
        r#"# LA7 Taker Dry-Run Approval

Status: LA7 TAKER DRY RUN APPROVED

| Field | Value |
| --- | --- |
| approval_id | LA7-2026-05-08-taker-dry-run-001 |
| baseline_id | LA7-2026-05-08-wallet-baseline-003 |
| baseline_capture_run_id | baseline-run-003 |
| baseline_hash | sha256:abc123 |
| wallet | 0x280ca8b14386Fe4203670538CCdE636C295d74E9 |
| funder | 0xB06867f742290D25B7430fD35D7A8cE7bc3a1159 |
| market_slug | btc-up-down-15m-1777000000 |
| condition_id | condition-1 |
| token_id | token-up |
| outcome | Up |
| side | BUY |
| max_size | 5.0 |
| max_notional | 2.75 |
| worst_price | 0.55 |
| max_fee | 0.02 |
| max_slippage_bps | 25 |
| no_near_close_cutoff_seconds | 600 |
| max_orders_per_day | 1 |
| retry_after_ambiguous_submit | forbidden |
| batch_orders | forbidden |
| cancel_all | forbidden |
"#
    }

    fn valid_la7_taker_live_approval_artifact() -> &'static str {
        r#"# LA7 Taker Live Canary Approval

Status: LA7 TAKER LIVE CANARY APPROVED

| Field | Value |
| --- | --- |
| approval_id | LA7-2026-05-09-taker-live-001 |
| baseline_id | LA7-2026-05-08-wallet-baseline-003 |
| baseline_capture_run_id | baseline-run-003 |
| baseline_hash | sha256:abc123 |
| wallet | 0x280ca8b14386Fe4203670538CCdE636C295d74E9 |
| funder | 0xB06867f742290D25B7430fD35D7A8cE7bc3a1159 |
| market_slug | btc-up-down-15m-1777000000 |
| condition_id | condition-1 |
| token_id | token-up |
| outcome | Up |
| side | BUY |
| max_size | 5.0 |
| max_notional | 2.75 |
| worst_price | 0.55 |
| max_fee | 0.02 |
| max_slippage_bps | 25 |
| no_near_close_cutoff_seconds | 600 |
| max_orders_per_day | 1 |
| retry_after_ambiguous_submit | forbidden |
| batch_orders | forbidden |
| cancel_all | forbidden |
| approval_expires_at_unix | 1778000000 |
| dry_run_report_path | reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_report.json |
| dry_run_report_sha256 | sha256:1111111111111111111111111111111111111111111111111111111111111111 |
| dry_run_decision_path | reports/sessions/18adb209348a61e0-b004-0/live_alpha_taker_canary_dry_run_decision.json |
| dry_run_decision_sha256 | sha256:2222222222222222222222222222222222222222222222222222222222222222 |
"#
    }

    fn sample_taker_submit_input() -> LiveTakerSubmitInput {
        LiveTakerSubmitInput {
            clob_host: "https://clob.polymarket.com".to_string(),
            signer_handle: "P15M_LIVE_ALPHA_TEST_PRIVATE_KEY".to_string(),
            l2_access_handle: "P15M_LIVE_ALPHA_TEST_L2_ACCESS".to_string(),
            l2_secret_handle: "P15M_LIVE_ALPHA_TEST_L2_SECRET".to_string(),
            l2_passphrase_handle: "P15M_LIVE_ALPHA_TEST_L2_PASSPHRASE".to_string(),
            wallet_address: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_address: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            signature_type: SignatureType::PolyProxy,
            approval: LiveTakerCanaryApprovalFields {
                approval_id: "LA7-2026-05-09-taker-live-001".to_string(),
                baseline_id: "LA7-2026-05-08-wallet-baseline-003".to_string(),
                baseline_capture_run_id: "baseline-run-003".to_string(),
                baseline_hash: "sha256:abc123".to_string(),
                wallet: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
                funder: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
                market_slug: "btc-up-down-15m-1777000000".to_string(),
                condition_id: "condition-1".to_string(),
                token_id: "123456789".to_string(),
                outcome: "Up".to_string(),
                side: Side::Buy,
                max_size: 5.0,
                max_notional: 2.75,
                worst_price: 0.48,
                max_fee: 0.10,
                max_slippage_bps: 100,
                no_near_close_cutoff_seconds: 600,
                max_orders_per_day: 1,
                retry_after_ambiguous_submit: "forbidden".to_string(),
                batch_orders: "forbidden".to_string(),
                cancel_all: "forbidden".to_string(),
            },
            decision: LiveTakerGateDecision {
                market_id: "market-1".to_string(),
                token_id: "123456789".to_string(),
                outcome: "Up".to_string(),
                side: Side::Buy,
                would_take: true,
                live_allowed: true,
                reason_codes: Vec::new(),
                fair_probability: Some(0.60),
                market_probability: Some(0.34),
                best_bid: Some(0.33),
                best_ask: Some(0.34),
                average_price: Some(0.34),
                worst_price: Some(0.34),
                worst_price_limit: Some(0.35),
                size: 5.0,
                notional: 1.70,
                visible_depth: 10.0,
                gross_edge_bps: Some(2600.0),
                spread_cost_bps: Some(50.0),
                taker_fee_bps: Some(157.08),
                taker_fee: Some(0.07854),
                slippage_bps: Some(0.0),
                latency_buffer_bps: 5.0,
                adverse_selection_buffer_bps: 25.0,
                minimum_profit_buffer_bps: 0.0,
                estimated_ev_after_costs_bps: Some(2400.0),
            },
            approval_sha256:
                "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                    .to_string(),
        }
    }

    fn config() -> AppConfig {
        toml::from_str(include_str!("../config/default.toml")).expect("default config parses")
    }

    fn snapshot(probability_up: f64, asks: Vec<(f64, f64)>, now_ms: i64) -> DecisionSnapshot {
        let market = market(now_ms);
        let mut store = StateStore::new();
        for (seq, event) in [
            NormalizedEvent::MarketDiscovered {
                market: market.clone(),
            },
            NormalizedEvent::BookSnapshot {
                book: crate::domain::OrderBookSnapshot {
                    market_id: market.market_id.clone(),
                    token_id: "token-up".to_string(),
                    bids: vec![OrderBookLevel {
                        price: 0.49,
                        size: 10.0,
                    }],
                    asks: asks
                        .into_iter()
                        .map(|(price, size)| OrderBookLevel { price, size })
                        .collect(),
                    hash: Some("book-up".to_string()),
                    source_ts: Some(now_ms - 5),
                },
            },
            NormalizedEvent::BookSnapshot {
                book: crate::domain::OrderBookSnapshot {
                    market_id: market.market_id.clone(),
                    token_id: "token-down".to_string(),
                    bids: vec![OrderBookLevel {
                        price: 0.40,
                        size: 10.0,
                    }],
                    asks: vec![OrderBookLevel {
                        price: 0.42,
                        size: 10.0,
                    }],
                    hash: Some("book-down".to_string()),
                    source_ts: Some(now_ms - 5),
                },
            },
            NormalizedEvent::ReferenceTick {
                price: reference(100.0, now_ms - 4),
            },
            NormalizedEvent::PredictiveTick {
                price: reference(
                    100.0 + probability_to_price_delta(probability_up),
                    now_ms - 3,
                ),
            },
        ]
        .into_iter()
        .enumerate()
        {
            store
                .apply_event(&EventEnvelope::new(
                    "run-1",
                    format!("event-{seq}"),
                    "unit-test",
                    now_ms,
                    seq as u64,
                    seq as u64,
                    event,
                ))
                .expect("event applies");
        }
        store
            .decision_snapshot(&market.market_id, now_ms, 500, 500)
            .expect("snapshot")
    }

    fn set_snapshot_ages(snapshot: &mut DecisionSnapshot, age_ms: i64) {
        for freshness in &mut snapshot.book_freshness {
            freshness.age_ms = Some(age_ms);
            freshness.stale_after_ms = age_ms as u64 + 1;
            freshness.is_stale = false;
        }
        for freshness in &mut snapshot.reference_freshness {
            freshness.age_ms = Some(age_ms);
            freshness.stale_after_ms = age_ms as u64 + 1;
            freshness.is_stale = false;
        }
    }

    fn market(now_ms: i64) -> Market {
        Market {
            market_id: "market-1".to_string(),
            slug: "btc-up-down-15m-unit".to_string(),
            title: "BTC Up or Down Unit".to_string(),
            asset: Asset::Btc,
            condition_id: "condition-1".to_string(),
            outcomes: vec![
                OutcomeToken {
                    token_id: "token-up".to_string(),
                    outcome: "Up".to_string(),
                },
                OutcomeToken {
                    token_id: "token-down".to_string(),
                    outcome: "Down".to_string(),
                },
            ],
            start_ts: now_ms - 60_000,
            end_ts: now_ms + 600_000,
            resolution_source: Some(Asset::Btc.chainlink_resolution_source().to_string()),
            tick_size: 0.01,
            min_order_size: 5.0,
            fee_parameters: FeeParameters {
                fees_enabled: true,
                maker_fee_bps: 0.0,
                taker_fee_bps: 200.0,
                raw_fee_config: Some(serde_json::json!({"r": 0.07, "e": 1, "to": true})),
            },
            lifecycle_state: MarketLifecycleState::Active,
            ineligibility_reason: None,
        }
    }

    fn reference(price: f64, ts: i64) -> ReferencePrice {
        ReferencePrice {
            asset: Asset::Btc,
            source: Asset::Btc.chainlink_resolution_source().to_string(),
            price,
            confidence: None,
            provider: None,
            matches_market_resolution_source: None,
            source_ts: Some(ts),
            recv_wall_ts: ts,
        }
    }

    fn probability_to_price_delta(probability_up: f64) -> f64 {
        (probability_up - 0.5) * 10.0
    }

    fn fill(id: &str, liquidity: OrderKind, price: f64, size: f64, fee_paid: f64) -> PaperFill {
        PaperFill {
            fill_id: id.to_string(),
            order_id: format!("order-{id}"),
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            side: Side::Buy,
            price,
            size,
            fee_paid,
            liquidity,
            filled_ts: 1,
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9,
            "actual={actual} expected={expected}"
        );
    }
}
