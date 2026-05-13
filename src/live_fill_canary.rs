use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::live_alpha_preflight::{LiveAlphaApprovedOrderBounds, LiveAlphaPreflightReport};
use crate::live_beta_readback::{OpenOrderReadback, SignatureType, TradeReadback};

pub const MODULE: &str = "live_fill_canary";
pub const OFFICIAL_SIGNING_CLIENT: &str = "polymarket_client_sdk_v2";
pub const OFFICIAL_SIGNING_CLIENT_VERSION: &str = "0.6.0-canary.1";

#[derive(Debug, Clone, PartialEq)]
pub struct LiveAlphaApprovalArtifact {
    pub approval_id: String,
    pub approved_host_ids: Vec<String>,
    pub wallet_id: String,
    pub funder_id: String,
    pub signature_type: String,
    pub asset_symbol: String,
    pub market_slug: String,
    pub market_question: String,
    pub condition_id: String,
    pub outcome: String,
    pub token_id: String,
    pub side: String,
    pub order_type: String,
    pub amount_or_size: f64,
    pub max_notional: f64,
    pub max_fee_estimate: f64,
    pub worst_price: f64,
    pub max_slippage_bps: u64,
    pub max_open_orders_after_run: usize,
    pub retry_count: u64,
    pub min_order_size: f64,
    pub tick_size: f64,
    pub market_end_unix: u64,
    pub approved_best_bid: Option<f64>,
    pub approved_best_bid_size: Option<f64>,
    pub approved_best_ask: Option<f64>,
    pub approved_best_ask_size: Option<f64>,
    pub approved_book_hash: Option<String>,
    pub approved_book_timestamp_ms: Option<i64>,
}

impl LiveAlphaApprovalArtifact {
    pub fn approved_bounds(&self) -> LiveAlphaApprovedOrderBounds {
        LiveAlphaApprovedOrderBounds {
            approval_id: self.approval_id.clone(),
            approved_host_ids: self.approved_host_ids.clone(),
            wallet_id: self.wallet_id.clone(),
            funder_id: self.funder_id.clone(),
            signature_type: self.signature_type.clone(),
            market_slug: self.market_slug.clone(),
            condition_id: self.condition_id.clone(),
            token_id: self.token_id.clone(),
            asset_symbol: self.asset_symbol.clone(),
            outcome: self.outcome.clone(),
            side: self.side.clone(),
            order_type: self.order_type.clone(),
            worst_price: self.worst_price,
            amount_or_size: self.amount_or_size,
            max_notional: self.max_notional,
            max_slippage_bps: self.max_slippage_bps,
            max_fee_estimate: self.max_fee_estimate,
            max_open_orders_after_run: self.max_open_orders_after_run,
            retry_count: self.retry_count,
            market_end_unix: self.market_end_unix,
            min_order_size: self.min_order_size,
            tick_size: self.tick_size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAlphaFillCanaryEnvelope {
    pub approval_id: String,
    pub run_id: String,
    pub host_id: String,
    pub wallet_id: String,
    pub geoblock_result: String,
    pub account_preflight_id: String,
    pub heartbeat_status: String,
    pub market_slug: String,
    pub condition_id: String,
    pub token_id: String,
    pub asset_symbol: String,
    pub outcome: String,
    pub side: String,
    pub order_type: String,
    pub price: f64,
    pub amount_or_size: f64,
    pub max_notional: f64,
    pub max_slippage_bps: u64,
    pub max_fee_estimate: f64,
    pub book_snapshot_id: String,
    pub reference_snapshot_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAlphaFillCanaryCapState {
    pub approval_id: String,
    pub submission_attempted: bool,
    pub reserved_at_unix: u64,
    pub venue_order_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAlphaFillSubmissionReport {
    pub status: &'static str,
    pub order_id: String,
    pub venue_status: String,
    pub success: bool,
    pub making_amount: String,
    pub taking_amount: String,
    pub trade_ids: Vec<String>,
    pub transaction_hashes: Vec<String>,
    pub approval_id: String,
    pub submitted_order_count: u8,
    pub not_submitted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAlphaFillReconciliationReport {
    pub status: &'static str,
    pub order_id: String,
    pub venue_status: String,
    pub success: bool,
    pub open_orders_after_run: usize,
    pub matching_trade_ids: Vec<String>,
    pub available_pusd_units_after: u64,
    pub reserved_pusd_units_after: u64,
    pub block_reasons: Vec<&'static str>,
}

pub struct LiveAlphaFillSubmitInput {
    pub clob_host: String,
    pub signer_handle: String,
    pub l2_access_handle: String,
    pub l2_secret_handle: String,
    pub l2_passphrase_handle: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: SignatureType,
    pub approval: LiveAlphaApprovalArtifact,
}

pub fn parse_la3_approval_artifact(
    markdown: &str,
) -> LiveAlphaFillCanaryResult<LiveAlphaApprovalArtifact> {
    let approval_id = required_backtick(markdown, "- Approval ID:")?;
    let host_line = required_line(markdown, "- Local hostname evidence:")?;
    let approved_host_ids = backtick_values(host_line);
    let wallet_id = required_backtick(markdown, "- Approved wallet/signer address:")?;
    let funder_id = required_backtick(markdown, "- Approved funder/proxy address:")?;
    let signature_values = backtick_values(required_line(markdown, "- Signature type:")?);
    let signature_type = signature_values
        .get(1)
        .or_else(|| signature_values.first())
        .map(|value| normalize_signature_type(value))
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval("signature type missing".to_string()))?;

    let asset_symbol = required_after_label(markdown, "- Approved asset:")?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let market_slug = required_backtick(markdown, "- Approved market slug:")?;
    let market_question = required_backtick(markdown, "- Market question:")?;
    let condition_id = required_backtick(markdown, "- Condition ID:")?;
    let outcome = required_backtick(markdown, "- Approved outcome:")?;
    let token_id = required_backtick(markdown, "- Approved token ID:")?;
    let side = required_backtick(markdown, "- Approved side:")?;
    let order_type = required_backtick(markdown, "- Approved order type:")?;
    let amount_or_size =
        parse_first_f64(&required_backtick(markdown, "- Approved amount_or_size:")?)?;
    let max_notional = parse_first_f64(&required_backtick(markdown, "- Approved max notional:")?)?;
    let max_fee_estimate = parse_first_f64(&required_backtick(
        markdown,
        "- Approved max fee estimate:",
    )?)?;
    let worst_price = parse_first_f64(&required_backtick(
        markdown,
        "- Approved worst-price limit:",
    )?)?;
    let slippage_line = required_line(markdown, "- Approved max slippage bound:")?;
    let max_slippage_bps = parse_first_u64(
        &backtick_values(slippage_line)
            .first()
            .cloned()
            .unwrap_or_default(),
    )?;
    let max_open_orders_after_run = parse_first_u64(&required_backtick(
        markdown,
        "- Approved max open orders after run:",
    )?)? as usize;
    let retry_count = parse_first_u64(&required_backtick(markdown, "- Approved retry count:")?)?;

    let market_snapshot_line = markdown
        .lines()
        .find(|line| line.contains("market snapshot used to prepare this artifact"))
        .ok_or_else(|| {
            LiveAlphaFillCanaryError::Approval("market snapshot line missing".to_string())
        })?;
    let market_snapshot_values = backtick_values(market_snapshot_line);
    let min_order_size = market_snapshot_values
        .get(3)
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval("min order size missing".to_string()))
        .and_then(|value| parse_first_f64(value))?;
    let tick_size = market_snapshot_values
        .get(4)
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval("tick size missing".to_string()))
        .and_then(|value| parse_first_f64(value))?;
    let market_end_unix = market_snapshot_values
        .get(5)
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval("market end time missing".to_string()))
        .and_then(|value| parse_rfc3339_unix(value))?;

    let book_line = markdown
        .lines()
        .find(|line| line.contains("token order book snapshot showed"))
        .unwrap_or_default();
    let book_values = backtick_values(book_line);
    let approved_best_bid = book_values
        .first()
        .and_then(|value| parse_first_f64(value).ok());
    let approved_best_bid_size = book_values
        .get(1)
        .and_then(|value| parse_first_f64(value).ok());
    let approved_best_ask = book_values
        .get(2)
        .and_then(|value| parse_first_f64(value).ok());
    let approved_best_ask_size = book_values
        .get(3)
        .and_then(|value| parse_first_f64(value).ok());
    let approved_book_hash = book_values.get(4).cloned();
    let approved_book_timestamp_ms = book_values
        .get(5)
        .and_then(|value| value.trim().parse::<i64>().ok());

    let artifact = LiveAlphaApprovalArtifact {
        approval_id,
        approved_host_ids,
        wallet_id,
        funder_id,
        signature_type,
        asset_symbol,
        market_slug,
        market_question,
        condition_id,
        outcome,
        token_id,
        side,
        order_type,
        amount_or_size,
        max_notional,
        max_fee_estimate,
        worst_price,
        max_slippage_bps,
        max_open_orders_after_run,
        retry_count,
        min_order_size,
        tick_size,
        market_end_unix,
        approved_best_bid,
        approved_best_bid_size,
        approved_best_ask,
        approved_best_ask_size,
        approved_book_hash,
        approved_book_timestamp_ms,
    };
    validate_artifact_shape(&artifact)?;
    Ok(artifact)
}

pub fn build_fill_canary_envelope(
    report: &LiveAlphaPreflightReport,
    created_at: i64,
) -> LiveAlphaFillCanaryEnvelope {
    LiveAlphaFillCanaryEnvelope {
        approval_id: report.approval_id.clone(),
        run_id: report.run_id.clone(),
        host_id: report.host_id.clone(),
        wallet_id: report.wallet_id.clone(),
        geoblock_result: report.geoblock_result.clone(),
        account_preflight_id: format!(
            "account:{}:{}:{}",
            report.wallet_id, report.funder_id, report.account_preflight_live_network_enabled
        ),
        heartbeat_status: report.heartbeat_status.clone(),
        market_slug: report.market_slug.clone(),
        condition_id: report.condition_id.clone(),
        token_id: report.token_id.clone(),
        asset_symbol: report.asset_symbol.clone(),
        outcome: report.outcome.clone(),
        side: report.side.clone(),
        order_type: report.order_type.clone(),
        price: report.price,
        amount_or_size: report.amount_or_size,
        max_notional: report.max_notional,
        max_slippage_bps: report.max_slippage_bps,
        max_fee_estimate: report.max_fee_estimate,
        book_snapshot_id: report.book_snapshot_id.clone(),
        reference_snapshot_id: report.reference_snapshot_id.clone(),
        created_at,
    }
}

pub fn canonical_fill_canary_prompt(
    envelope: &LiveAlphaFillCanaryEnvelope,
    report: &LiveAlphaPreflightReport,
) -> String {
    format!(
        concat!(
            "APPROVE LA3 FILL CANARY:\n",
            "Approval ID: {}\n",
            "Run ID: {}\n",
            "Host: {}\n",
            "Wallet address: {}\n",
            "Funder address: {}\n",
            "Geoblock: {}\n",
            "Available pUSD units: {}\n",
            "Reserved pUSD units: {}\n",
            "Open orders: {}\n",
            "Recent trades count: {}\n",
            "Market slug: {}\n",
            "Condition ID: {}\n",
            "Token ID: {}\n",
            "Outcome: {}\n",
            "Side: {}\n",
            "Order type: {}\n",
            "Worst-price limit: {}\n",
            "Amount or size: {}\n",
            "Max notional: {}\n",
            "Max fee estimate: {}\n",
            "Official taker fee estimate: {}\n",
            "Book age ms: {}\n",
            "Reference age ms: {}\n",
            "Heartbeat state: {}\n",
            "Cancel/reconciliation plan: read order/trades/open orders/balances immediately; cancel only the exact LA3 order ID if an unexpected live order exists; never cancel-all\n",
            "Rollback command: run live-alpha-fill-canary readback/reconciliation for this run ID; if needed use the approved exact single-order cancel path only"
        ),
        envelope.approval_id,
        envelope.run_id,
        envelope.host_id,
        envelope.wallet_id,
        report.funder_id,
        envelope.geoblock_result,
        report.available_pusd_units,
        report.reserved_pusd_units,
        report.open_order_count,
        report.recent_trade_count,
        envelope.market_slug,
        envelope.condition_id,
        envelope.token_id,
        envelope.outcome,
        envelope.side,
        envelope.order_type,
        decimal_label(envelope.price),
        decimal_label(envelope.amount_or_size),
        decimal_label(envelope.max_notional),
        decimal_label(envelope.max_fee_estimate),
        report
            .official_taker_fee_estimate
            .map(decimal_label)
            .unwrap_or_else(|| "missing".to_string()),
        report
            .book_age_ms
            .map(|age| age.to_string())
            .unwrap_or_else(|| "missing".to_string()),
        report
            .reference_age_ms
            .map(|age| age.to_string())
            .unwrap_or_else(|| "missing".to_string()),
        envelope.heartbeat_status
    )
}

pub fn approval_hash(text: &str) -> String {
    format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, text.as_bytes()).as_ref())
    )
}

pub fn fill_canary_cap_state_json(
    state: &LiveAlphaFillCanaryCapState,
) -> LiveAlphaFillCanaryResult<String> {
    serde_json::to_string_pretty(state).map_err(LiveAlphaFillCanaryError::Serialize)
}

pub fn fill_canary_cap_state_from_json(
    json: &str,
) -> LiveAlphaFillCanaryResult<LiveAlphaFillCanaryCapState> {
    serde_json::from_str(json).map_err(LiveAlphaFillCanaryError::Parse)
}

pub fn validate_fill_submit_input_without_network(
    input: &LiveAlphaFillSubmitInput,
) -> LiveAlphaFillCanaryResult<()> {
    let private_key = env_required_value(&input.signer_handle, "canary_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let _l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let _l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    use polymarket_client_sdk_v2::auth::{LocalSigner, Uuid};

    LocalSigner::from_str(&private_key).map_err(|_| {
        LiveAlphaFillCanaryError::Submit(
            "official SDK rejected the LA3 private-key handle value".to_string(),
        )
    })?;
    Uuid::parse_str(&l2_key).map_err(|_| {
        LiveAlphaFillCanaryError::Submit(
            "official SDK rejected the LA3 clob_l2_access handle value".to_string(),
        )
    })?;
    parse_address(&input.wallet_address, "wallet_address")?;
    parse_address(&input.funder_address, "funder_address")?;
    parse_token_id(&input.approval.token_id)?;
    parse_decimal(input.approval.worst_price, "worst_price")?;
    parse_decimal(input.approval.amount_or_size, "amount_or_size")?;
    if input.signature_type == SignatureType::Eoa
        && !input
            .wallet_address
            .eq_ignore_ascii_case(&input.funder_address)
    {
        return Err(LiveAlphaFillCanaryError::Submit(
            "EOA LA3 signer requires wallet_address and funder_address to match".to_string(),
        ));
    }
    if !input.approval.side.eq_ignore_ascii_case("BUY") {
        return Err(LiveAlphaFillCanaryError::Submit(
            "LA3 approval artifact must bind side BUY".to_string(),
        ));
    }
    if !input.approval.order_type.eq_ignore_ascii_case("FAK") {
        return Err(LiveAlphaFillCanaryError::Submit(
            "LA3 approval artifact must bind order type FAK".to_string(),
        ));
    }
    Ok(())
}

pub async fn submit_one_fill_canary_with_official_sdk(
    input: LiveAlphaFillSubmitInput,
) -> LiveAlphaFillCanaryResult<LiveAlphaFillSubmissionReport> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::types::{Amount, OrderType, Side};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::types::{Decimal, U256};
    use polymarket_client_sdk_v2::POLYGON;

    validate_fill_submit_input_without_network(&input)?;

    let private_key = env_required_value(&input.signer_handle, "canary_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveAlphaFillCanaryError::Submit(
                "official SDK rejected the LA3 private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveAlphaFillCanaryError::Submit(
                "official SDK rejected the LA3 clob_l2_access handle value".to_string(),
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
        .map_err(|_| LiveAlphaFillCanaryError::Submit("invalid LA3 token id".to_string()))?;
    let price = Decimal::from_str(&decimal_label(input.approval.worst_price))
        .map_err(|_| LiveAlphaFillCanaryError::Submit("invalid LA3 worst price".to_string()))?;
    let amount = Decimal::from_str(&decimal_label(input.approval.amount_or_size))
        .map_err(|_| LiveAlphaFillCanaryError::Submit("invalid LA3 amount".to_string()))?;

    let signable_order = client
        .market_order()
        .token_id(token_id)
        .side(Side::Buy)
        .amount(Amount::usdc(amount).map_err(sdk_error)?)
        .price(price)
        .order_type(OrderType::FAK)
        .build()
        .await
        .map_err(sdk_error)?;
    let signed_order = client
        .sign(&signer, signable_order)
        .await
        .map_err(sdk_error)?;
    let response = client.post_order(signed_order).await.map_err(sdk_error)?;

    Ok(LiveAlphaFillSubmissionReport {
        status: "submitted",
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
        approval_id: input.approval.approval_id,
        submitted_order_count: 1,
        not_submitted: false,
    })
}

pub fn reconcile_fill_submission(
    submission: &LiveAlphaFillSubmissionReport,
    approval: &LiveAlphaApprovalArtifact,
    preflight: &LiveAlphaPreflightReport,
    open_orders: &[OpenOrderReadback],
    trades: &[TradeReadback],
) -> LiveAlphaFillReconciliationReport {
    let mut block_reasons = Vec::new();
    let matching_trade_ids = trades
        .iter()
        .filter(|trade| {
            trade
                .order_id
                .as_deref()
                .is_some_and(|order_id| order_id.eq_ignore_ascii_case(&submission.order_id))
                || submission
                    .trade_ids
                    .iter()
                    .any(|trade_id| trade_id.eq_ignore_ascii_case(&trade.id))
        })
        .map(|trade| trade.id.clone())
        .collect::<Vec<_>>();
    let open_orders_after_run = open_orders.len();

    if open_orders_after_run > approval.max_open_orders_after_run {
        block_reasons.push("unexpected_open_order_after_fill");
    }
    if preflight.reserved_pusd_units != 0 {
        block_reasons.push("reserved_pusd_after_reconciliation_nonzero");
    }
    if submission.submitted_order_count != 1 {
        block_reasons.push("submitted_order_count_not_one");
    }

    let filled_evidence = !submission.trade_ids.is_empty()
        || !matching_trade_ids.is_empty()
        || submission.venue_status.eq_ignore_ascii_case("matched");
    let status = if block_reasons.is_empty() && filled_evidence {
        "filled_and_reconciled"
    } else if block_reasons.is_empty() && open_orders_after_run == 0 && submission.success {
        "not_filled_canceled_expired_cleanly"
    } else {
        if !filled_evidence {
            block_reasons.push("fill_evidence_missing");
        }
        "ambiguous_incident_required"
    };

    LiveAlphaFillReconciliationReport {
        status,
        order_id: submission.order_id.clone(),
        venue_status: submission.venue_status.clone(),
        success: submission.success,
        open_orders_after_run,
        matching_trade_ids,
        available_pusd_units_after: preflight.available_pusd_units,
        reserved_pusd_units_after: preflight.reserved_pusd_units,
        block_reasons,
    }
}

fn validate_artifact_shape(artifact: &LiveAlphaApprovalArtifact) -> LiveAlphaFillCanaryResult<()> {
    let mut errors = Vec::new();
    if artifact.approved_host_ids.is_empty() {
        errors.push("approved host evidence missing");
    }
    if artifact.wallet_id.trim().is_empty() || artifact.funder_id.trim().is_empty() {
        errors.push("wallet/funder missing");
    }
    if !artifact.asset_symbol.eq_ignore_ascii_case("BTC") {
        errors.push("LA3 artifact must bind BTC");
    }
    if !artifact.side.eq_ignore_ascii_case("BUY") {
        errors.push("LA3 artifact must bind BUY");
    }
    if !artifact.order_type.eq_ignore_ascii_case("FAK") {
        errors.push("LA3 artifact must bind FAK");
    }
    if artifact.retry_count != 0 {
        errors.push("LA3 artifact retry count must be 0");
    }
    if artifact.max_open_orders_after_run != 0 {
        errors.push("LA3 artifact max open orders after run must be 0");
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(LiveAlphaFillCanaryError::Approval(errors.join(", ")))
    }
}

fn required_line<'a>(markdown: &'a str, prefix: &str) -> LiveAlphaFillCanaryResult<&'a str> {
    markdown
        .lines()
        .find(|line| line.trim_start().starts_with(prefix))
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval(format!("missing line: {prefix}")))
}

fn required_after_label(markdown: &str, prefix: &str) -> LiveAlphaFillCanaryResult<String> {
    let line = required_line(markdown, prefix)?;
    Ok(line
        .split_once(':')
        .map(|(_, value)| value.trim().trim_end_matches('.').to_string())
        .unwrap_or_default())
}

fn required_backtick(markdown: &str, prefix: &str) -> LiveAlphaFillCanaryResult<String> {
    let line = required_line(markdown, prefix)?;
    backtick_values(line)
        .into_iter()
        .next()
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval(format!("missing backtick: {prefix}")))
}

fn backtick_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        values.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }
    values
}

fn normalize_signature_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "poly_proxy" | "poly-proxy" | "polyproxy" => "poly_proxy".to_string(),
        "2" | "gnosis_safe" | "gnosis-safe" | "gnosissafe" => "gnosis_safe".to_string(),
        "0" | "eoa" => "eoa".to_string(),
        other => other.to_string(),
    }
}

fn parse_first_f64(value: &str) -> LiveAlphaFillCanaryResult<f64> {
    let token = value
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-'))
        .find(|part| !part.is_empty())
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval(format!("number missing: {value}")))?;
    token
        .parse::<f64>()
        .map_err(|_| LiveAlphaFillCanaryError::Approval(format!("invalid number: {value}")))
}

fn parse_first_u64(value: &str) -> LiveAlphaFillCanaryResult<u64> {
    let token = value
        .split(|ch: char| !ch.is_ascii_digit())
        .find(|part| !part.is_empty())
        .ok_or_else(|| LiveAlphaFillCanaryError::Approval(format!("integer missing: {value}")))?;
    token
        .parse::<u64>()
        .map_err(|_| LiveAlphaFillCanaryError::Approval(format!("invalid integer: {value}")))
}

fn parse_rfc3339_unix(value: &str) -> LiveAlphaFillCanaryResult<u64> {
    let timestamp = OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|_| LiveAlphaFillCanaryError::Approval(format!("invalid RFC3339: {value}")))?;
    u64::try_from(timestamp.unix_timestamp())
        .map_err(|_| LiveAlphaFillCanaryError::Approval(format!("invalid timestamp: {value}")))
}

fn env_required_value(handle: &str, label: &'static str) -> LiveAlphaFillCanaryResult<String> {
    let value = env::var(handle).map_err(|_| LiveAlphaFillCanaryError::MissingSecretHandle {
        label,
        handle: handle.to_string(),
    })?;
    if value.trim().is_empty() {
        return Err(LiveAlphaFillCanaryError::MissingSecretHandle {
            label,
            handle: handle.to_string(),
        });
    }
    Ok(value)
}

fn parse_address(
    value: &str,
    label: &'static str,
) -> LiveAlphaFillCanaryResult<polymarket_client_sdk_v2::types::Address> {
    polymarket_client_sdk_v2::types::Address::from_str(value)
        .map_err(|_| LiveAlphaFillCanaryError::Submit(format!("official SDK rejected {label}")))
}

fn parse_token_id(value: &str) -> LiveAlphaFillCanaryResult<polymarket_client_sdk_v2::types::U256> {
    polymarket_client_sdk_v2::types::U256::from_str(value)
        .map_err(|_| LiveAlphaFillCanaryError::Submit("official SDK rejected token id".to_string()))
}

fn parse_decimal(
    value: f64,
    label: &'static str,
) -> LiveAlphaFillCanaryResult<polymarket_client_sdk_v2::types::Decimal> {
    polymarket_client_sdk_v2::types::Decimal::from_str(&decimal_label(value))
        .map_err(|_| LiveAlphaFillCanaryError::Submit(format!("official SDK rejected {label}")))
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
        SignatureType::Poly1271 => polymarket_client_sdk_v2::clob::types::SignatureType::Poly1271,
    }
}

fn sdk_error(source: polymarket_client_sdk_v2::error::Error) -> LiveAlphaFillCanaryError {
    LiveAlphaFillCanaryError::Submit(format!("official SDK LA3 fill path failed: {source}"))
}

fn decimal_label(value: f64) -> String {
    let rounded = format!("{value:.6}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
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

pub type LiveAlphaFillCanaryResult<T> = Result<T, LiveAlphaFillCanaryError>;

#[derive(Debug)]
pub enum LiveAlphaFillCanaryError {
    Approval(String),
    MissingSecretHandle { label: &'static str, handle: String },
    Parse(serde_json::Error),
    Serialize(serde_json::Error),
    Submit(String),
}

impl Display for LiveAlphaFillCanaryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Approval(message) => {
                write!(formatter, "LA3 approval artifact invalid: {message}")
            }
            Self::MissingSecretHandle { label, handle } => {
                write!(formatter, "LA3 {label} handle is not present: {handle}")
            }
            Self::Parse(source) => write!(formatter, "failed to parse LA3 canary state: {source}"),
            Self::Serialize(source) => {
                write!(formatter, "failed to serialize LA3 canary state: {source}")
            }
            Self::Submit(message) => formatter.write_str(message),
        }
    }
}

impl Error for LiveAlphaFillCanaryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_la3_approval_artifact_shape() {
        let artifact = parse_la3_approval_artifact(APPROVAL).expect("artifact parses");

        assert_eq!(artifact.approval_id, "LA3-2026-05-04-001");
        assert_eq!(artifact.asset_symbol, "BTC");
        assert_eq!(artifact.order_type, "FAK");
        assert_eq!(artifact.signature_type, "poly_proxy");
        assert_eq!(artifact.market_end_unix, 1_777_910_400);
        assert_eq!(artifact.min_order_size, 5.0);
        assert_eq!(artifact.approved_best_ask, Some(0.50));
        assert!(artifact
            .approved_host_ids
            .contains(&"Jonahs-MacBook-Pro.local".to_string()));
    }

    #[test]
    fn envelope_contains_required_plan_fields_and_prompt_hashes() {
        let report = LiveAlphaPreflightReport {
            status: "passed",
            mode: "dry_run",
            block_reasons: Vec::new(),
            run_id: "run-1".to_string(),
            approval_id: "LA3-2026-05-04-001".to_string(),
            host_id: "Jonahs-MacBook-Pro.local".to_string(),
            wallet_id: "0x280ca8b14386Fe4203670538CCdE636C295d74E9".to_string(),
            funder_id: "0xB06867f742290D25B7430fD35D7A8cE7bc3a1159".to_string(),
            geoblock_result: "status=passed,country=MX,region=CHP".to_string(),
            account_preflight_passed: true,
            account_preflight_live_network_enabled: true,
            available_pusd_units: 1_610_000,
            allowance_pusd_units: 1_610_000,
            reserved_pusd_units: 0,
            open_order_count: 0,
            recent_trade_count: 0,
            heartbeat_status: "not_started_no_open_orders".to_string(),
            market_slug: "btc-updown-15m-1777909500".to_string(),
            condition_id: "0x371c52ca5f8dbe256978e6d27f6a6d8cf64f3722b15e44ba3128685ccfbeee0c"
                .to_string(),
            token_id:
                "91899612655270438973839203540142703788805338252926995927363610489118446263952"
                    .to_string(),
            asset_symbol: "BTC".to_string(),
            outcome: "Up".to_string(),
            side: "BUY".to_string(),
            order_type: "FAK".to_string(),
            price: 0.55,
            amount_or_size: 1.0,
            max_notional: 1.0,
            max_slippage_bps: 1_000,
            max_fee_estimate: 0.05,
            official_taker_fee_estimate: Some(0.036),
            book_snapshot_id: "book".to_string(),
            book_age_ms: Some(100),
            reference_snapshot_id: "reference".to_string(),
            reference_age_ms: Some(100),
            compile_time_orders_enabled: true,
            prior_attempt_consumed: false,
        };

        let envelope = build_fill_canary_envelope(&report, 1_777_907_600_000);
        let prompt = canonical_fill_canary_prompt(&envelope, &report);
        let hash = approval_hash(&prompt);

        assert_eq!(envelope.order_type, "FAK");
        assert!(prompt.contains("APPROVE LA3 FILL CANARY"));
        assert!(prompt.contains("Worst-price limit: 0.55"));
        assert!(prompt.contains("Official taker fee estimate: 0.036"));
        assert!(hash.starts_with("sha256:"));
    }

    #[test]
    fn reconciliation_distinguishes_fill_from_clean_no_fill() {
        let approval = parse_la3_approval_artifact(APPROVAL).expect("artifact parses");
        let preflight = preflight_report();
        let mut submission = LiveAlphaFillSubmissionReport {
            status: "submitted",
            order_id: "order-1".to_string(),
            venue_status: "matched".to_string(),
            success: true,
            making_amount: "1".to_string(),
            taking_amount: "2".to_string(),
            trade_ids: vec!["trade-1".to_string()],
            transaction_hashes: Vec::new(),
            approval_id: approval.approval_id.clone(),
            submitted_order_count: 1,
            not_submitted: false,
        };
        let filled = reconcile_fill_submission(&submission, &approval, &preflight, &[], &[]);
        assert_eq!(filled.status, "filled_and_reconciled");

        submission.venue_status = "unmatched".to_string();
        submission.trade_ids.clear();
        let clean = reconcile_fill_submission(&submission, &approval, &preflight, &[], &[]);
        assert_eq!(clean.status, "not_filled_canceled_expired_cleanly");
    }

    #[test]
    fn reconciliation_counts_unrelated_open_orders_as_incidents() {
        let approval = parse_la3_approval_artifact(APPROVAL).expect("artifact parses");
        let preflight = preflight_report();
        let submission = LiveAlphaFillSubmissionReport {
            status: "submitted",
            order_id: "order-1".to_string(),
            venue_status: "matched".to_string(),
            success: true,
            making_amount: "1".to_string(),
            taking_amount: "2".to_string(),
            trade_ids: vec!["trade-1".to_string()],
            transaction_hashes: Vec::new(),
            approval_id: approval.approval_id.clone(),
            submitted_order_count: 1,
            not_submitted: false,
        };
        let open_order = OpenOrderReadback {
            id: "stale-unrelated-order".to_string(),
            status: crate::live_beta_readback::OrderReadbackStatus::Live,
            maker_address: "0x1111111111111111111111111111111111111111".to_string(),
            market: approval.market_slug.clone(),
            asset_id: approval.token_id.clone(),
            side: "BUY".to_string(),
            original_size_units: 1_000_000,
            size_matched_units: 0,
            price: "0.50".to_string(),
            outcome: approval.outcome.clone(),
            expiration: "0".to_string(),
            order_type: "GTC".to_string(),
            associate_trades: Vec::new(),
            created_at: 1_777_907_600,
        };

        let report =
            reconcile_fill_submission(&submission, &approval, &preflight, &[open_order], &[]);

        assert_eq!(report.open_orders_after_run, 1);
        assert_eq!(report.status, "ambiguous_incident_required");
        assert!(report
            .block_reasons
            .contains(&"unexpected_open_order_after_fill"));
    }

    fn preflight_report() -> LiveAlphaPreflightReport {
        LiveAlphaPreflightReport {
            status: "passed",
            mode: "final_submit",
            block_reasons: Vec::new(),
            run_id: "run-1".to_string(),
            approval_id: "LA3-2026-05-04-001".to_string(),
            host_id: "host".to_string(),
            wallet_id: "wallet".to_string(),
            funder_id: "funder".to_string(),
            geoblock_result: "status=passed".to_string(),
            account_preflight_passed: true,
            account_preflight_live_network_enabled: true,
            available_pusd_units: 1_000_000,
            allowance_pusd_units: 1_000_000,
            reserved_pusd_units: 0,
            open_order_count: 0,
            recent_trade_count: 0,
            heartbeat_status: "not_started_no_open_orders".to_string(),
            market_slug: "btc-updown-15m-1777909500".to_string(),
            condition_id: "condition".to_string(),
            token_id: "token".to_string(),
            asset_symbol: "BTC".to_string(),
            outcome: "Up".to_string(),
            side: "BUY".to_string(),
            order_type: "FAK".to_string(),
            price: 0.55,
            amount_or_size: 1.0,
            max_notional: 1.0,
            max_slippage_bps: 1_000,
            max_fee_estimate: 0.05,
            official_taker_fee_estimate: Some(0.036),
            book_snapshot_id: "book".to_string(),
            book_age_ms: Some(100),
            reference_snapshot_id: "reference".to_string(),
            reference_age_ms: Some(100),
            compile_time_orders_enabled: true,
            prior_attempt_consumed: false,
        }
    }

    const APPROVAL: &str = r#"
- Approval ID: `LA3-2026-05-04-001`
- Local hostname evidence: `Jonahs-MacBook-Pro.local`; local host name `Jonahs-MacBook-Pro`.
- Approved wallet/signer address: `0x280ca8b14386Fe4203670538CCdE636C295d74E9`.
- Approved funder/proxy address: `0xB06867f742290D25B7430fD35D7A8cE7bc3a1159`.
- Signature type: `1` / `POLY_PROXY`, matching the local approved readback config precedent.
- Approved asset: BTC only for this LA3 canary.
- Approved market slug: `btc-updown-15m-1777909500`.
- Market question: `Bitcoin Up or Down - May 4, 11:45AM-12:00PM ET`.
- Condition ID: `0x371c52ca5f8dbe256978e6d27f6a6d8cf64f3722b15e44ba3128685ccfbeee0c`.
- Approved outcome: `Up`.
- Approved token ID: `91899612655270438973839203540142703788805338252926995927363610489118446263952`.
- Approved side: `BUY`.
- Approved order type: `FAK` only.
- Approved amount_or_size: `1.00 pUSD` BUY spend amount.
- Approved max notional: `1.00 pUSD`.
- Approved max fee estimate: `0.05 pUSD`.
- Approved worst-price limit: `0.55`.
- Approved max slippage bound: `1000 bps` from the observed `0.50` best ask, capped by the `0.55` worst-price limit.
- Approved max open orders after run: `0`.
- Approved retry count: `0`.
The market snapshot used to prepare this artifact showed `active=true`, `closed=false`, `acceptingOrders=true`, order min size `5`, tick size `0.01`, and end time `2026-05-04T16:00:00Z`.
The approved Up token order book snapshot showed best bid `0.49` size `65`, best ask `0.50` size `75`, hash `bf39a155badbe08ae87fcdcb6a60b75181f5459c`, and timestamp `1777907581023`.
"#;
}
