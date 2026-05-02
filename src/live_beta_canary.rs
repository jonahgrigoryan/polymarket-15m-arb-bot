use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::domain::Side;
use crate::live_beta_readback::SignatureType;

pub const MODULE: &str = "live_beta_canary";
pub const LB6_ONE_ORDER_CANARY_SUBMISSION_ENABLED: bool = true;
pub const MAX_LB6_CANARY_NOTIONAL: f64 = 1.0;
pub const MIN_GTD_SECURITY_BUFFER_SECS: u64 = 60;
pub const OFFICIAL_SIGNING_CLIENT: &str = "polymarket_client_sdk_v2";
pub const OFFICIAL_SIGNING_CLIENT_VERSION: &str = "0.6.0-canary.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanaryMode {
    DryRun,
    FinalGated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanaryOrderPlan {
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub order_type: String,
    pub post_only: bool,
    pub maker_only: bool,
    pub tick_size: f64,
    pub gtd_expiry_unix: u64,
    pub market_end_unix: u64,
    pub best_bid: f64,
    pub best_ask: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanaryApprovalContext {
    pub run_id: String,
    pub host: String,
    pub geoblock_result: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: String,
    pub available_pusd_units: u64,
    pub reserved_pusd_units: u64,
    pub fee_estimate: String,
    pub book_age_ms: u64,
    pub reference_age_ms: u64,
    pub max_book_age_ms: u64,
    pub max_reference_age_ms: u64,
    pub heartbeat: String,
    pub cancel_plan: String,
    pub rollback_command: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanaryApprovalGuard {
    pub approval_text: Option<String>,
    pub expected_approval_sha256: Option<String>,
    pub approval_expires_at_unix: Option<u64>,
    pub now_unix: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanaryGateStatus {
    Passed,
    Blocked,
    Unknown,
}

impl CanaryGateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanaryRuntimeChecks {
    pub canary_submission_enabled: bool,
    pub geoblock_status: CanaryGateStatus,
    pub lb4_account_preflight_passed: bool,
    pub open_order_count: usize,
    pub canary_secret_handles_present: bool,
    pub l2_secret_handles_present: bool,
    pub lb5_rollback_ready: bool,
    pub lb5_cancel_readiness_blocks_until_canary_exists: bool,
    pub official_sdk_available: bool,
    pub previous_canary_submission_attempted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanaryReadinessReport {
    pub status: &'static str,
    pub mode: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub approval_sha256: String,
    pub canonical_approval_text: String,
    pub order_type: String,
    pub post_only: bool,
    pub maker_only: bool,
    pub one_order_cap_remaining: bool,
    pub not_submitted: bool,
    pub canary_submission_enabled: bool,
    pub official_signing_client: &'static str,
    pub official_signing_client_version: &'static str,
}

impl CanaryReadinessReport {
    pub fn ready_for_final_submission(&self) -> bool {
        self.mode == "final_gated" && self.block_reasons.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanaryOrderCapState {
    pub submission_attempted: bool,
    pub approval_sha256: String,
    pub reserved_at_unix: u64,
    pub venue_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanarySubmissionReport {
    pub status: &'static str,
    pub order_id: String,
    pub venue_status: String,
    pub success: bool,
    pub approval_sha256: String,
    pub not_submitted: bool,
    pub submitted_order_count: u8,
}

pub struct CanarySubmitInput {
    pub clob_host: String,
    pub signer_handle: String,
    pub l2_access_handle: String,
    pub l2_secret_handle: String,
    pub l2_passphrase_handle: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: SignatureType,
    pub plan: CanaryOrderPlan,
    pub approval_sha256: String,
}

pub fn canonical_approval_text(plan: &CanaryOrderPlan, context: &CanaryApprovalContext) -> String {
    format!(
        concat!(
            "APPROVE LB6 CANARY:\n",
            "Run ID: {}\n",
            "Host: {}\n",
            "Geoblock: {}\n",
            "Wallet address: {}\n",
            "Funder address: {}\n",
            "Signature type: {}\n",
            "Available pUSD units: {}\n",
            "Reserved pUSD units: {}\n",
            "Market slug: {}\n",
            "Condition ID: {}\n",
            "Token ID: {}\n",
            "Outcome: {}\n",
            "Side: {}\n",
            "Order type: GTD, post-only maker-only\n",
            "Price: {}\n",
            "Size: {}\n",
            "Notional: {} pUSD\n",
            "GTD expiry unix: {}\n",
            "Market end unix: {}\n",
            "Best bid: {}\n",
            "Best ask: {}\n",
            "Fee estimate: {}\n",
            "Current book age ms: {}\n",
            "Reference age ms: {}\n",
            "Heartbeat state: {}\n",
            "Cancel plan: {}\n",
            "Rollback: {}"
        ),
        context.run_id,
        context.host,
        context.geoblock_result,
        context.wallet_address,
        context.funder_address,
        context.signature_type,
        context.available_pusd_units,
        context.reserved_pusd_units,
        plan.market_slug,
        plan.condition_id,
        plan.token_id,
        plan.outcome,
        side_label(plan.side),
        decimal_label(plan.price),
        decimal_label(plan.size),
        decimal_label(plan.notional),
        plan.gtd_expiry_unix,
        plan.market_end_unix,
        decimal_label(plan.best_bid),
        decimal_label(plan.best_ask),
        context.fee_estimate,
        context.book_age_ms,
        context.reference_age_ms,
        context.heartbeat,
        context.cancel_plan,
        context.rollback_command
    )
}

pub fn approval_hash(text: &str) -> String {
    format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, text.as_bytes()).as_ref())
    )
}

pub fn evaluate_canary_readiness(
    mode: CanaryMode,
    plan: &CanaryOrderPlan,
    context: &CanaryApprovalContext,
    approval: &CanaryApprovalGuard,
    checks: &CanaryRuntimeChecks,
) -> CanaryReadinessReport {
    let canonical_text = canonical_approval_text(plan, context);
    let canonical_hash = approval_hash(&canonical_text);
    let mut block_reasons = validate_plan(mode, plan, approval);
    validate_context(context, &mut block_reasons);

    if mode == CanaryMode::FinalGated {
        match approval.approval_text.as_deref() {
            Some(text) if text == canonical_text => {}
            Some(_) => block_reasons.push("approval_text_mismatch"),
            None => block_reasons.push("approval_text_missing"),
        }

        match approval.expected_approval_sha256.as_deref() {
            Some(expected) if expected == canonical_hash => {}
            Some(_) => block_reasons.push("approval_hash_mismatch"),
            None => block_reasons.push("approval_hash_missing"),
        }
    }

    if !checks.canary_submission_enabled {
        block_reasons.push("canary_submission_disabled");
    }
    match checks.geoblock_status {
        CanaryGateStatus::Passed => {}
        CanaryGateStatus::Blocked => block_reasons.push("geoblock_blocked"),
        CanaryGateStatus::Unknown => block_reasons.push("geoblock_unknown"),
    }
    if !checks.lb4_account_preflight_passed {
        block_reasons.push("lb4_account_preflight_not_passed");
    }
    if checks.open_order_count != 0 {
        block_reasons.push("open_orders_nonzero");
    }
    if !checks.canary_secret_handles_present {
        block_reasons.push("canary_secret_handles_missing");
    }
    if !checks.l2_secret_handles_present {
        block_reasons.push("l2_secret_handles_missing");
    }
    if !checks.lb5_rollback_ready {
        block_reasons.push("lb5_rollback_not_ready");
    }
    if !checks.lb5_cancel_readiness_blocks_until_canary_exists {
        block_reasons.push("lb5_cancel_readiness_not_fail_closed_before_canary");
    }
    if !checks.official_sdk_available {
        block_reasons.push("official_signing_sdk_unavailable");
    }
    if checks.previous_canary_submission_attempted {
        block_reasons.push("one_order_cap_already_consumed");
    }

    dedupe_preserving_order(&mut block_reasons);

    CanaryReadinessReport {
        status: if block_reasons.is_empty() {
            "ready_for_one_order_canary"
        } else {
            "blocked"
        },
        mode: match mode {
            CanaryMode::DryRun => "dry_run",
            CanaryMode::FinalGated => "final_gated",
        },
        block_reasons,
        approval_sha256: canonical_hash,
        canonical_approval_text: canonical_text,
        order_type: "GTD".to_string(),
        post_only: plan.post_only,
        maker_only: plan.maker_only,
        one_order_cap_remaining: !checks.previous_canary_submission_attempted,
        not_submitted: true,
        canary_submission_enabled: checks.canary_submission_enabled,
        official_signing_client: OFFICIAL_SIGNING_CLIENT,
        official_signing_client_version: OFFICIAL_SIGNING_CLIENT_VERSION,
    }
}

pub fn canary_order_cap_state_json(state: &CanaryOrderCapState) -> CanaryReadinessResult<String> {
    serde_json::to_string_pretty(state).map_err(CanaryReadinessError::Serialize)
}

pub fn canary_order_cap_state_from_json(json: &str) -> CanaryReadinessResult<CanaryOrderCapState> {
    serde_json::from_str(json).map_err(CanaryReadinessError::Parse)
}

pub async fn submit_one_canary_with_official_sdk(
    input: CanarySubmitInput,
) -> CanaryReadinessResult<CanarySubmissionReport> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::types::OrderType;
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::types::{DateTime, Decimal, Utc, U256};
    use polymarket_client_sdk_v2::POLYGON;

    validate_canary_submit_input_without_network(&input)?;

    let private_key = env_required_value(&input.signer_handle, "canary_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            CanaryReadinessError::Submit(
                "official SDK rejected the canary private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            CanaryReadinessError::Submit(
                "official SDK rejected the clob_l2_access handle value".to_string(),
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
    } else if !input
        .funder_address
        .eq_ignore_ascii_case(&input.wallet_address)
    {
        return Err(CanaryReadinessError::Submit(
            "EOA canary signer requires wallet_address and funder_address to match".to_string(),
        ));
    }
    let client = auth.authenticate().await.map_err(sdk_error)?;

    let token_id = U256::from_str(&input.plan.token_id).map_err(|_| {
        CanaryReadinessError::Submit("official SDK rejected the canary token id".to_string())
    })?;
    let price = Decimal::from_str(&decimal_label(input.plan.price)).map_err(|_| {
        CanaryReadinessError::Submit("official SDK rejected the canary price".to_string())
    })?;
    let size = Decimal::from_str(&decimal_label(input.plan.size)).map_err(|_| {
        CanaryReadinessError::Submit("official SDK rejected the canary size".to_string())
    })?;
    let expiration = DateTime::<Utc>::from_timestamp(input.plan.gtd_expiry_unix as i64, 0)
        .ok_or_else(|| {
            CanaryReadinessError::Submit("official SDK rejected the canary GTD expiry".to_string())
        })?;

    let signable_order = client
        .limit_order()
        .token_id(token_id)
        .order_type(OrderType::GTD)
        .post_only(true)
        .expiration(expiration)
        .price(price)
        .size(size)
        .side(sdk_side(input.plan.side))
        .build()
        .await
        .map_err(sdk_error)?;
    let signed_order = client
        .sign(&signer, signable_order)
        .await
        .map_err(sdk_error)?;
    let response = client.post_order(signed_order).await.map_err(sdk_error)?;

    Ok(CanarySubmissionReport {
        status: "submitted",
        order_id: response.order_id.to_string(),
        venue_status: response.status.to_string(),
        success: response.success,
        approval_sha256: input.approval_sha256,
        not_submitted: false,
        submitted_order_count: 1,
    })
}

pub fn validate_canary_submit_input_without_network(
    input: &CanarySubmitInput,
) -> CanaryReadinessResult<()> {
    let private_key = env_required_value(&input.signer_handle, "canary_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let _l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let _l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    LocalSignerLike::validate_private_key(&private_key)?;
    UuidLike::validate_l2_key(&l2_key)?;
    parse_address(&input.wallet_address, "wallet_address")?;
    parse_address(&input.funder_address, "funder_address")?;
    if input.signature_type == SignatureType::Eoa
        && !input
            .funder_address
            .eq_ignore_ascii_case(&input.wallet_address)
    {
        return Err(CanaryReadinessError::Submit(
            "EOA canary signer requires wallet_address and funder_address to match".to_string(),
        ));
    }
    parse_token_id(&input.plan.token_id)?;
    parse_decimal(input.plan.price, "price")?;
    parse_decimal(input.plan.size, "size")?;
    parse_gtd_expiry(input.plan.gtd_expiry_unix)?;

    Ok(())
}

fn validate_plan<'a>(
    mode: CanaryMode,
    plan: &CanaryOrderPlan,
    approval: &CanaryApprovalGuard,
) -> Vec<&'a str> {
    let mut block_reasons = Vec::new();

    if plan.market_slug.trim().is_empty() {
        block_reasons.push("market_slug_missing");
    }
    if !is_condition_id(&plan.condition_id) {
        block_reasons.push("condition_id_invalid");
    }
    if plan.token_id.trim().is_empty() || !plan.token_id.chars().all(|ch| ch.is_ascii_digit()) {
        block_reasons.push("token_id_invalid");
    }
    if plan.outcome.trim().is_empty() {
        block_reasons.push("outcome_missing");
    }
    if plan.order_type != "GTD" {
        block_reasons.push("order_type_not_gtd");
    }
    if !plan.post_only {
        block_reasons.push("post_only_missing");
    }
    if !plan.maker_only {
        block_reasons.push("maker_only_missing");
    }
    if !valid_price(plan.price) {
        block_reasons.push("price_invalid");
    } else if !tick_aligned(plan.price, plan.tick_size) {
        block_reasons.push("price_not_tick_aligned");
    }
    if !plan.size.is_finite() || plan.size <= 0.0 {
        block_reasons.push("size_invalid");
    }
    if !plan.notional.is_finite() || plan.notional <= 0.0 {
        block_reasons.push("notional_invalid");
    } else if (plan.price * plan.size - plan.notional).abs() > 0.000_001 {
        block_reasons.push("notional_mismatch");
    } else if plan.notional > MAX_LB6_CANARY_NOTIONAL {
        block_reasons.push("notional_exceeds_lb6_cap");
    }
    if !plan.tick_size.is_finite() || plan.tick_size <= 0.0 || plan.tick_size > 1.0 {
        block_reasons.push("tick_size_invalid");
    }
    if !valid_price(plan.best_bid) {
        block_reasons.push("best_bid_invalid");
    }
    if !valid_price(plan.best_ask) {
        block_reasons.push("best_ask_invalid");
    }
    match plan.side {
        Side::Buy if valid_price(plan.best_ask) && plan.best_ask <= plan.price => {
            block_reasons.push("best_ask_not_above_bid");
        }
        Side::Sell if valid_price(plan.best_bid) && plan.best_bid >= plan.price => {
            block_reasons.push("best_bid_not_below_sell_price");
        }
        _ => {}
    }
    if plan.gtd_expiry_unix <= approval.now_unix + MIN_GTD_SECURITY_BUFFER_SECS {
        block_reasons.push("gtd_expiry_missing_security_buffer");
    }
    if plan.market_end_unix == 0 || plan.gtd_expiry_unix >= plan.market_end_unix {
        block_reasons.push("gtd_expiry_not_before_market_end");
    }
    if mode == CanaryMode::FinalGated {
        match approval.approval_expires_at_unix {
            Some(expires_at) if expires_at > approval.now_unix => {}
            Some(_) => block_reasons.push("approval_expired"),
            None => block_reasons.push("approval_expiry_missing"),
        }
    }

    block_reasons
}

fn validate_context(context: &CanaryApprovalContext, block_reasons: &mut Vec<&str>) {
    if context.run_id.trim().is_empty() {
        block_reasons.push("run_id_missing");
    }
    if context.host.trim().is_empty() {
        block_reasons.push("host_missing");
    }
    if context.geoblock_result.trim().is_empty() {
        block_reasons.push("geoblock_result_missing");
    }
    if !is_valid_evm_address(&context.wallet_address) {
        block_reasons.push("wallet_address_invalid");
    }
    if !is_valid_evm_address(&context.funder_address) {
        block_reasons.push("funder_address_invalid");
    }
    if context.signature_type.trim().is_empty() {
        block_reasons.push("signature_type_missing");
    }
    if context.book_age_ms > context.max_book_age_ms {
        block_reasons.push("book_stale");
    }
    if context.reference_age_ms > context.max_reference_age_ms {
        block_reasons.push("reference_stale");
    }
    if context.heartbeat.trim().is_empty() {
        block_reasons.push("heartbeat_missing");
    }
    if context.cancel_plan.trim().is_empty() {
        block_reasons.push("cancel_plan_missing");
    }
    if context.rollback_command.trim().is_empty() {
        block_reasons.push("rollback_missing");
    }
}

fn env_required_value(handle: &str, label: &'static str) -> CanaryReadinessResult<String> {
    let value = env::var(handle).map_err(|_| CanaryReadinessError::MissingSecretHandle {
        label,
        handle: handle.to_string(),
    })?;
    if value.trim().is_empty() {
        return Err(CanaryReadinessError::MissingSecretHandle {
            label,
            handle: handle.to_string(),
        });
    }
    Ok(value)
}

fn parse_address(
    value: &str,
    label: &'static str,
) -> CanaryReadinessResult<polymarket_client_sdk_v2::types::Address> {
    polymarket_client_sdk_v2::types::Address::from_str(value)
        .map_err(|_| CanaryReadinessError::Submit(format!("official SDK rejected {label}")))
}

fn parse_token_id(value: &str) -> CanaryReadinessResult<polymarket_client_sdk_v2::types::U256> {
    polymarket_client_sdk_v2::types::U256::from_str(value).map_err(|_| {
        CanaryReadinessError::Submit("official SDK rejected the canary token id".to_string())
    })
}

fn parse_decimal(
    value: f64,
    label: &'static str,
) -> CanaryReadinessResult<polymarket_client_sdk_v2::types::Decimal> {
    polymarket_client_sdk_v2::types::Decimal::from_str(&decimal_label(value)).map_err(|_| {
        CanaryReadinessError::Submit(format!("official SDK rejected the canary {label}"))
    })
}

fn parse_gtd_expiry(
    value: u64,
) -> CanaryReadinessResult<
    polymarket_client_sdk_v2::types::DateTime<polymarket_client_sdk_v2::types::Utc>,
> {
    polymarket_client_sdk_v2::types::DateTime::<polymarket_client_sdk_v2::types::Utc>::from_timestamp(
        value as i64,
        0,
    )
    .ok_or_else(|| {
        CanaryReadinessError::Submit("official SDK rejected the canary GTD expiry".to_string())
    })
}

fn is_valid_evm_address(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 40
        && hex.chars().all(|ch| ch.is_ascii_hexdigit())
        && hex.chars().any(|ch| ch != '0')
}

struct LocalSignerLike;

impl LocalSignerLike {
    fn validate_private_key(value: &str) -> CanaryReadinessResult<()> {
        use polymarket_client_sdk_v2::auth::LocalSigner;

        LocalSigner::from_str(value).map(|_| ()).map_err(|_| {
            CanaryReadinessError::Submit(
                "official SDK rejected the canary private-key handle value".to_string(),
            )
        })
    }
}

struct UuidLike;

impl UuidLike {
    fn validate_l2_key(value: &str) -> CanaryReadinessResult<()> {
        use polymarket_client_sdk_v2::auth::Uuid;

        Uuid::parse_str(value).map(|_| ()).map_err(|_| {
            CanaryReadinessError::Submit(
                "official SDK rejected the clob_l2_access handle value".to_string(),
            )
        })
    }
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

fn sdk_side(value: Side) -> polymarket_client_sdk_v2::clob::types::Side {
    match value {
        Side::Buy => polymarket_client_sdk_v2::clob::types::Side::Buy,
        Side::Sell => polymarket_client_sdk_v2::clob::types::Side::Sell,
    }
}

fn sdk_error(source: polymarket_client_sdk_v2::error::Error) -> CanaryReadinessError {
    CanaryReadinessError::Submit(format!("official SDK canary path failed: {source}"))
}

fn is_condition_id(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("0x") else {
        return false;
    };
    hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn valid_price(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value < 1.0
}

fn tick_aligned(price: f64, tick_size: f64) -> bool {
    if !tick_size.is_finite() || tick_size <= 0.0 {
        return false;
    }
    let ticks = price / tick_size;
    (ticks - ticks.round()).abs() < 1e-9
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

fn decimal_label(value: f64) -> String {
    let rounded = format!("{value:.6}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn dedupe_preserving_order(values: &mut Vec<&'static str>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(*value));
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

pub type CanaryReadinessResult<T> = Result<T, CanaryReadinessError>;

#[derive(Debug)]
pub enum CanaryReadinessError {
    MissingSecretHandle { label: &'static str, handle: String },
    Parse(serde_json::Error),
    Serialize(serde_json::Error),
    Submit(String),
}

impl Display for CanaryReadinessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSecretHandle { label, handle } => {
                write!(formatter, "LB6 {label} handle is not present: {handle}")
            }
            Self::Parse(source) => {
                write!(formatter, "failed to parse LB6 canary state: {source}")
            }
            Self::Serialize(source) => {
                write!(formatter, "failed to serialize LB6 canary state: {source}")
            }
            Self::Submit(message) => formatter.write_str(message),
        }
    }
}

impl Error for CanaryReadinessError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canary_accepts_exact_approval_and_gates() {
        let plan = valid_plan();
        let context = valid_context();
        let approval_text = canonical_approval_text(&plan, &context);
        let approval_hash = approval_hash(&approval_text);
        let report = evaluate_canary_readiness(
            CanaryMode::FinalGated,
            &plan,
            &context,
            &CanaryApprovalGuard {
                approval_text: Some(approval_text),
                expected_approval_sha256: Some(approval_hash),
                approval_expires_at_unix: Some(1_777_756_000),
                now_unix: 1_777_755_000,
            },
            &passing_checks(),
        );

        assert_eq!(report.status, "ready_for_one_order_canary");
        assert!(report.ready_for_final_submission());
        assert!(report.not_submitted);
        assert!(report.canary_submission_enabled);
        assert_eq!(report.official_signing_client, OFFICIAL_SIGNING_CLIENT);
        assert!(report.canonical_approval_text.contains("Run ID: "));
        assert!(report.canonical_approval_text.contains("Cancel plan: "));
        assert!(report.canonical_approval_text.contains("Rollback: "));
    }

    #[test]
    fn canary_fails_closed_when_approval_missing_mismatched_or_expired() {
        let plan = valid_plan();
        let context = valid_context();
        let approval_text = canonical_approval_text(&plan, &context);
        let approval_hash = approval_hash(&approval_text);

        for (approval_text, expected_hash, expires_at, expected_reason) in [
            (
                None,
                Some(approval_hash.clone()),
                Some(1_777_756_000),
                "approval_text_missing",
            ),
            (
                Some("APPROVE A DIFFERENT ORDER".to_string()),
                Some(approval_hash.clone()),
                Some(1_777_756_000),
                "approval_text_mismatch",
            ),
            (
                Some(approval_text.clone()),
                Some("sha256:wrong".to_string()),
                Some(1_777_756_000),
                "approval_hash_mismatch",
            ),
            (
                Some(approval_text),
                Some(approval_hash),
                Some(1_777_754_999),
                "approval_expired",
            ),
        ] {
            let report = evaluate_canary_readiness(
                CanaryMode::FinalGated,
                &plan,
                &context,
                &CanaryApprovalGuard {
                    approval_text,
                    expected_approval_sha256: expected_hash,
                    approval_expires_at_unix: expires_at,
                    now_unix: 1_777_755_000,
                },
                &passing_checks(),
            );

            assert_eq!(report.status, "blocked");
            assert!(report.block_reasons.contains(&expected_reason));
        }
    }

    #[test]
    fn canary_fails_closed_when_secret_handles_are_missing() {
        let report = report_with_checks(CanaryRuntimeChecks {
            canary_secret_handles_present: false,
            l2_secret_handles_present: false,
            ..passing_checks()
        });

        assert!(report
            .block_reasons
            .contains(&"canary_secret_handles_missing"));
        assert!(report.block_reasons.contains(&"l2_secret_handles_missing"));
    }

    #[test]
    fn canary_fails_closed_when_geoblock_blocked_or_unknown() {
        for (geoblock_status, expected) in [
            (CanaryGateStatus::Blocked, "geoblock_blocked"),
            (CanaryGateStatus::Unknown, "geoblock_unknown"),
        ] {
            let report = report_with_checks(CanaryRuntimeChecks {
                geoblock_status,
                ..passing_checks()
            });

            assert!(report.block_reasons.contains(&expected));
        }
    }

    #[test]
    fn canary_fails_closed_when_lb4_preflight_or_open_orders_block() {
        let report = report_with_checks(CanaryRuntimeChecks {
            lb4_account_preflight_passed: false,
            open_order_count: 1,
            ..passing_checks()
        });

        assert!(report
            .block_reasons
            .contains(&"lb4_account_preflight_not_passed"));
        assert!(report.block_reasons.contains(&"open_orders_nonzero"));
    }

    #[test]
    fn canary_fails_closed_when_order_is_not_post_only_gtd_maker() {
        let mut plan = valid_plan();
        plan.order_type = "FOK".to_string();
        plan.post_only = false;
        plan.maker_only = false;

        let report = report_for_plan(plan);

        assert!(report.block_reasons.contains(&"order_type_not_gtd"));
        assert!(report.block_reasons.contains(&"post_only_missing"));
        assert!(report.block_reasons.contains(&"maker_only_missing"));
    }

    #[test]
    fn canary_fails_closed_when_price_size_or_notional_mismatch_approval() {
        let mut plan = valid_plan();
        plan.notional = 0.06;

        let report = report_for_plan(plan);

        assert!(report.block_reasons.contains(&"notional_mismatch"));
    }

    #[test]
    fn canary_fails_closed_when_best_ask_would_make_bid_marketable() {
        let mut plan = valid_plan();
        plan.best_ask = plan.price;

        let report = report_for_plan(plan);

        assert!(report.block_reasons.contains(&"best_ask_not_above_bid"));
    }

    #[test]
    fn canary_uses_best_bid_for_sell_marketability() {
        let mut plan = valid_plan();
        plan.side = Side::Sell;
        plan.price = 0.05;
        plan.notional = 0.25;
        plan.best_bid = 0.03;
        plan.best_ask = 0.04;

        let report = report_for_plan(plan);

        assert_eq!(report.status, "ready_for_one_order_canary");
        assert!(!report.block_reasons.contains(&"best_ask_not_above_bid"));
        assert!(!report
            .block_reasons
            .contains(&"best_bid_not_below_sell_price"));
    }

    #[test]
    fn canary_fails_closed_when_best_bid_would_make_sell_marketable() {
        let mut plan = valid_plan();
        plan.side = Side::Sell;
        plan.price = 0.05;
        plan.notional = 0.25;
        plan.best_bid = plan.price;
        plan.best_ask = 0.50;

        let report = report_for_plan(plan);

        assert!(report
            .block_reasons
            .contains(&"best_bid_not_below_sell_price"));
    }

    #[test]
    fn canary_fails_closed_when_second_order_would_be_attempted() {
        let report = report_with_checks(CanaryRuntimeChecks {
            previous_canary_submission_attempted: true,
            ..passing_checks()
        });

        assert!(report
            .block_reasons
            .contains(&"one_order_cap_already_consumed"));
        assert!(!report.one_order_cap_remaining);
    }

    #[test]
    fn canary_fails_closed_when_lb5_rollback_or_official_sdk_not_ready() {
        let report = report_with_checks(CanaryRuntimeChecks {
            lb5_rollback_ready: false,
            lb5_cancel_readiness_blocks_until_canary_exists: false,
            official_sdk_available: false,
            ..passing_checks()
        });

        assert!(report.block_reasons.contains(&"lb5_rollback_not_ready"));
        assert!(report
            .block_reasons
            .contains(&"lb5_cancel_readiness_not_fail_closed_before_canary"));
        assert!(report
            .block_reasons
            .contains(&"official_signing_sdk_unavailable"));
    }

    #[test]
    fn canary_order_cap_state_round_trips_without_secret_material() {
        let state = CanaryOrderCapState {
            submission_attempted: true,
            approval_sha256: "sha256:abc".to_string(),
            reserved_at_unix: 1_777_755_000,
            venue_order_id: None,
        };
        let rendered = canary_order_cap_state_json(&state).expect("state serializes");

        assert!(rendered.contains("submission_attempted"));
        assert!(!rendered.contains("private"));
        assert!(!rendered.contains("secret"));
        assert_eq!(
            canary_order_cap_state_from_json(&rendered).expect("state parses"),
            state
        );
    }

    fn report_for_plan(plan: CanaryOrderPlan) -> CanaryReadinessReport {
        let context = valid_context();
        let approval_text = canonical_approval_text(&plan, &context);
        let expected_approval_sha256 = approval_hash(&approval_text);
        evaluate_canary_readiness(
            CanaryMode::FinalGated,
            &plan,
            &context,
            &CanaryApprovalGuard {
                approval_text: Some(approval_text),
                expected_approval_sha256: Some(expected_approval_sha256),
                approval_expires_at_unix: Some(1_777_756_000),
                now_unix: 1_777_755_000,
            },
            &passing_checks(),
        )
    }

    fn report_with_checks(checks: CanaryRuntimeChecks) -> CanaryReadinessReport {
        let plan = valid_plan();
        let context = valid_context();
        let approval_text = canonical_approval_text(&plan, &context);
        let expected_approval_sha256 = approval_hash(&approval_text);
        evaluate_canary_readiness(
            CanaryMode::FinalGated,
            &plan,
            &context,
            &CanaryApprovalGuard {
                approval_text: Some(approval_text),
                expected_approval_sha256: Some(expected_approval_sha256),
                approval_expires_at_unix: Some(1_777_756_000),
                now_unix: 1_777_755_000,
            },
            &checks,
        )
    }

    fn valid_plan() -> CanaryOrderPlan {
        CanaryOrderPlan {
            market_slug: "eth-updown-15m-1777755600".to_string(),
            condition_id: "0x0ec08b1e170fca8d967445849a3fabba49858911ab8c46dc36069aa1090718dd"
                .to_string(),
            token_id:
                "32406149813503763845643545664364177107395801695160552332908724065335543321711"
                    .to_string(),
            outcome: "Up".to_string(),
            side: Side::Buy,
            price: 0.01,
            size: 5.0,
            notional: 0.05,
            order_type: "GTD".to_string(),
            post_only: true,
            maker_only: true,
            tick_size: 0.01,
            gtd_expiry_unix: 1_777_756_200,
            market_end_unix: 1_777_756_600,
            best_bid: 0.49,
            best_ask: 0.50,
        }
    }

    fn valid_context() -> CanaryApprovalContext {
        CanaryApprovalContext {
            run_id: "lb6-test-run".to_string(),
            host: "approved-mexico-host".to_string(),
            geoblock_result: "status=passed,country=MX,region=CMX".to_string(),
            wallet_address: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_address: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            signature_type: "poly_proxy".to_string(),
            available_pusd_units: 1_614_478,
            reserved_pusd_units: 0,
            fee_estimate: "0.000000 pUSD maker-only estimate; reconcile if matched".to_string(),
            book_age_ms: 250,
            reference_age_ms: 250,
            max_book_age_ms: 1_000,
            max_reference_age_ms: 1_000,
            heartbeat: "not_started_no_open_orders".to_string(),
            cancel_plan:
                "if still open after readback, cancel only this exact order ID; no cancel-all"
                    .to_string(),
            rollback_command: "LIVE_ORDER_PLACEMENT_ENABLED=false; stop service if running"
                .to_string(),
        }
    }

    fn passing_checks() -> CanaryRuntimeChecks {
        CanaryRuntimeChecks {
            canary_submission_enabled: LB6_ONE_ORDER_CANARY_SUBMISSION_ENABLED,
            geoblock_status: CanaryGateStatus::Passed,
            lb4_account_preflight_passed: true,
            open_order_count: 0,
            canary_secret_handles_present: true,
            l2_secret_handles_present: true,
            lb5_rollback_ready: true,
            lb5_cancel_readiness_blocks_until_canary_exists: true,
            official_sdk_available: true,
            previous_canary_submission_attempted: false,
        }
    }
}
