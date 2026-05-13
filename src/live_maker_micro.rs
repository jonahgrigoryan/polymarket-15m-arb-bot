use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::Side;
use crate::execution_intent::ExecutionIntent;
use crate::live_alpha_config::LiveAlphaMakerConfig;
use crate::live_beta_readback::SignatureType;
use crate::live_risk_engine::LiveRiskApproved;

pub const MODULE: &str = "live_maker_micro";
pub const GTD_SECURITY_BUFFER_SECONDS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveMakerOrderPlan {
    pub intent_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub post_only: bool,
    pub order_type: String,
    pub effective_quote_ttl_seconds: u64,
    pub gtd_expiration_unix: u64,
    pub cancel_after_unix: u64,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveMakerSubmissionReport {
    pub status: String,
    pub order_id: String,
    pub venue_status: String,
    pub success: bool,
    pub making_amount: String,
    pub taking_amount: String,
    pub trade_ids: Vec<String>,
    pub transaction_hashes: Vec<String>,
    pub not_submitted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveMakerOrderReadbackReport {
    pub order_id: String,
    pub venue_status: String,
    pub market: String,
    pub token_id: String,
    pub side: String,
    pub original_size: f64,
    pub size_matched: f64,
    pub remaining_size: f64,
    pub price: f64,
    pub outcome: String,
    pub order_type: String,
    pub expiration_unix: i64,
    pub associate_trades: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveMakerSubmitInput {
    pub clob_host: String,
    pub signer_handle: String,
    pub l2_access_handle: String,
    pub l2_secret_handle: String,
    pub l2_passphrase_handle: String,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: SignatureType,
    pub plan: LiveMakerOrderPlan,
}

pub fn venue_gtd_expiration_unix(now_unix: u64, effective_quote_ttl_seconds: u64) -> u64 {
    now_unix
        .saturating_add(GTD_SECURITY_BUFFER_SECONDS)
        .saturating_add(effective_quote_ttl_seconds)
}

pub fn quote_is_stale(
    submitted_at_unix: u64,
    now_unix: u64,
    effective_quote_ttl_seconds: u64,
) -> bool {
    now_unix >= submitted_at_unix.saturating_add(effective_quote_ttl_seconds)
}

pub fn build_live_maker_order_plan(
    intent: &ExecutionIntent,
    approval: &LiveRiskApproved,
    maker: &LiveAlphaMakerConfig,
    now_unix: u64,
) -> LiveMakerResult<LiveMakerOrderPlan> {
    validate_maker_config(maker)?;
    if approval.approved_ttl_seconds != maker.ttl_seconds {
        return Err(LiveMakerError::Validation(vec![
            "risk approval TTL must match maker effective quote TTL".to_string(),
        ]));
    }
    if intent.edge_bps < maker.min_edge_bps as f64 {
        return Err(LiveMakerError::Validation(vec![format!(
            "intent edge_bps {:.2} below live_alpha.maker.min_edge_bps {}",
            intent.edge_bps, maker.min_edge_bps
        )]));
    }
    let gtd_expiration_unix = venue_gtd_expiration_unix(now_unix, maker.ttl_seconds);
    Ok(LiveMakerOrderPlan {
        intent_id: intent.intent_id.clone(),
        token_id: approval.approved_token_id.clone(),
        outcome: approval.approved_outcome.clone(),
        side: approval.approved_side,
        price: intent.price,
        size: approval.approved_size,
        notional: approval.approved_notional,
        post_only: maker.post_only,
        order_type: maker.order_type.clone(),
        effective_quote_ttl_seconds: maker.ttl_seconds,
        gtd_expiration_unix,
        cancel_after_unix: now_unix.saturating_add(maker.ttl_seconds),
        reason_codes: approval.reason_codes.clone(),
    })
}

pub fn should_cancel_open_maker_order(
    plan: &LiveMakerOrderPlan,
    now_unix: u64,
    heartbeat_healthy: bool,
) -> bool {
    !heartbeat_healthy
        || quote_is_stale(
            plan.cancel_after_unix
                .saturating_sub(plan.effective_quote_ttl_seconds),
            now_unix,
            plan.effective_quote_ttl_seconds,
        )
}

pub fn validate_maker_submit_input_without_network(
    input: &LiveMakerSubmitInput,
) -> LiveMakerResult<()> {
    validate_maker_plan(&input.plan)?;
    let private_key = env_required_value(&input.signer_handle, "maker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let _l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let _l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    use polymarket_client_sdk_v2::auth::{LocalSigner, Uuid};

    LocalSigner::from_str(&private_key).map_err(|_| {
        LiveMakerError::Submit("official SDK rejected the LA5 private-key handle value".to_string())
    })?;
    Uuid::parse_str(&l2_key).map_err(|_| {
        LiveMakerError::Submit(
            "official SDK rejected the LA5 clob_l2_access handle value".to_string(),
        )
    })?;
    parse_address(&input.wallet_address, "wallet_address")?;
    parse_address(&input.funder_address, "funder_address")?;
    parse_token_id(&input.plan.token_id)?;
    parse_decimal(input.plan.price, "price")?;
    parse_decimal(input.plan.size, "size")?;
    if input.signature_type == SignatureType::Eoa
        && !input
            .wallet_address
            .eq_ignore_ascii_case(&input.funder_address)
    {
        return Err(LiveMakerError::Submit(
            "EOA LA5 signer requires wallet_address and funder_address to match".to_string(),
        ));
    }
    Ok(())
}

pub async fn submit_maker_order_with_official_sdk(
    input: LiveMakerSubmitInput,
) -> LiveMakerResult<LiveMakerSubmissionReport> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::types::{OrderType, Side as SdkSide};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::types::{DateTime, Decimal, Utc, U256};
    use polymarket_client_sdk_v2::POLYGON;

    validate_maker_submit_input_without_network(&input)?;

    let private_key = env_required_value(&input.signer_handle, "maker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;

    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 clob_l2_access handle value".to_string(),
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

    let expiration = DateTime::<Utc>::from_timestamp(input.plan.gtd_expiration_unix as i64, 0)
        .ok_or_else(|| {
            LiveMakerError::Submit("invalid LA5 GTD expiration timestamp".to_string())
        })?;
    let token_id = U256::from_str(&input.plan.token_id)
        .map_err(|_| LiveMakerError::Submit("invalid LA5 token id".to_string()))?;
    let price = Decimal::from_str(&decimal_label(input.plan.price))
        .map_err(|_| LiveMakerError::Submit("invalid LA5 price".to_string()))?;
    let size = Decimal::from_str(&decimal_label(input.plan.size))
        .map_err(|_| LiveMakerError::Submit("invalid LA5 size".to_string()))?;
    let side = match input.plan.side {
        Side::Buy => SdkSide::Buy,
        Side::Sell => SdkSide::Sell,
    };

    let signable_order = client
        .limit_order()
        .token_id(token_id)
        .price(price)
        .size(size)
        .side(side)
        .order_type(OrderType::GTD)
        .expiration(expiration)
        .post_only(true)
        .build()
        .await
        .map_err(sdk_error)?;
    let signed_order = client
        .sign(&signer, signable_order)
        .await
        .map_err(sdk_error)?;
    let response = client.post_order(signed_order).await.map_err(sdk_error)?;

    Ok(LiveMakerSubmissionReport {
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
        not_submitted: false,
    })
}

pub async fn cancel_exact_maker_order_with_official_sdk(
    input: &LiveMakerSubmitInput,
    order_id: &str,
) -> LiveMakerResult<Vec<String>> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::POLYGON;

    if !is_order_id(order_id) {
        return Err(LiveMakerError::Submit(
            "LA5 cancel requires one exact known order ID".to_string(),
        ));
    }
    validate_maker_submit_input_without_network(input)?;

    let private_key = env_required_value(&input.signer_handle, "maker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;
    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 clob_l2_access handle value".to_string(),
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
    let response = client.cancel_order(order_id).await.map_err(sdk_error)?;
    Ok(response.canceled)
}

pub async fn read_maker_order_with_official_sdk(
    input: &LiveMakerSubmitInput,
    order_id: &str,
) -> LiveMakerResult<LiveMakerOrderReadbackReport> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::POLYGON;

    if !is_order_id(order_id) {
        return Err(LiveMakerError::Submit(
            "LA5 order readback requires one exact known order ID".to_string(),
        ));
    }
    validate_maker_submit_input_without_network(input)?;

    let private_key = env_required_value(&input.signer_handle, "maker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;
    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 clob_l2_access handle value".to_string(),
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
    let response = client.order(order_id).await.map_err(sdk_error)?;

    let original_size = decimal_to_f64(&response.original_size.to_string(), "original_size")?;
    let size_matched = decimal_to_f64(&response.size_matched.to_string(), "size_matched")?;
    let price = decimal_to_f64(&response.price.to_string(), "price")?;
    Ok(LiveMakerOrderReadbackReport {
        order_id: response.id,
        venue_status: response.status.to_string(),
        market: response.market.to_string(),
        token_id: response.asset_id.to_string(),
        side: response.side.to_string(),
        original_size,
        size_matched,
        remaining_size: (original_size - size_matched).max(0.0),
        price,
        outcome: response.outcome,
        order_type: response.order_type.to_string(),
        expiration_unix: response.expiration.timestamp(),
        associate_trades: response.associate_trades,
    })
}

pub async fn post_maker_heartbeat_with_official_sdk(
    input: &LiveMakerSubmitInput,
    heartbeat_id: Option<&str>,
) -> LiveMakerResult<String> {
    use polymarket_client_sdk_v2::auth::{Credentials, LocalSigner, Signer as _, Uuid};
    use polymarket_client_sdk_v2::clob::{Client, Config};
    use polymarket_client_sdk_v2::POLYGON;

    validate_maker_submit_input_without_network(input)?;

    let private_key = env_required_value(&input.signer_handle, "maker_private_key")?;
    let l2_key = env_required_value(&input.l2_access_handle, "clob_l2_access")?;
    let l2_secret = env_required_value(&input.l2_secret_handle, "clob_l2_credential")?;
    let l2_passphrase = env_required_value(&input.l2_passphrase_handle, "clob_l2_passphrase")?;
    let signer = LocalSigner::from_str(&private_key)
        .map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 private-key handle value".to_string(),
            )
        })?
        .with_chain_id(Some(POLYGON));
    let credentials = Credentials::new(
        Uuid::parse_str(&l2_key).map_err(|_| {
            LiveMakerError::Submit(
                "official SDK rejected the LA5 clob_l2_access handle value".to_string(),
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
    let heartbeat_id = heartbeat_id
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| LiveMakerError::Submit("invalid LA5 heartbeat id".to_string()))?;
    let response = client
        .post_heartbeat(heartbeat_id)
        .await
        .map_err(sdk_error)?;
    Ok(response.heartbeat_id.to_string())
}

fn validate_maker_config(maker: &LiveAlphaMakerConfig) -> LiveMakerResult<()> {
    let mut errors = Vec::new();
    if !maker.enabled {
        errors.push("live_alpha.maker.enabled must be true for LA5".to_string());
    }
    if !maker.post_only {
        errors.push("live_alpha.maker.post_only must be true".to_string());
    }
    if !maker.order_type.eq_ignore_ascii_case("GTD") {
        errors.push("live_alpha.maker.order_type must be GTD".to_string());
    }
    if maker.ttl_seconds == 0 {
        errors.push("live_alpha.maker.ttl_seconds must be positive".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(LiveMakerError::Validation(errors))
    }
}

fn validate_maker_plan(plan: &LiveMakerOrderPlan) -> LiveMakerResult<()> {
    let mut errors = Vec::new();
    if !plan.post_only {
        errors.push("LA5 maker plan must be post-only".to_string());
    }
    if !plan.order_type.eq_ignore_ascii_case("GTD") {
        errors.push("LA5 maker plan must use GTD".to_string());
    }
    if plan.gtd_expiration_unix
        < plan
            .cancel_after_unix
            .saturating_add(GTD_SECURITY_BUFFER_SECONDS)
    {
        errors.push("LA5 GTD expiration must include the one-minute venue buffer".to_string());
    }
    if plan.side == Side::Sell && plan.size <= 0.0 {
        errors.push("LA5 maker sell size must be positive and inventory capped".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(LiveMakerError::Validation(errors))
    }
}

fn env_required_value(handle: &str, label: &'static str) -> LiveMakerResult<String> {
    let value = env::var(handle).map_err(|_| LiveMakerError::MissingSecretHandle {
        label,
        handle: handle.to_string(),
    })?;
    if value.trim().is_empty() {
        return Err(LiveMakerError::MissingSecretHandle {
            label,
            handle: handle.to_string(),
        });
    }
    Ok(value)
}

fn parse_address(
    value: &str,
    label: &'static str,
) -> LiveMakerResult<polymarket_client_sdk_v2::types::Address> {
    polymarket_client_sdk_v2::types::Address::from_str(value)
        .map_err(|_| LiveMakerError::Submit(format!("official SDK rejected {label}")))
}

fn parse_token_id(value: &str) -> LiveMakerResult<polymarket_client_sdk_v2::types::U256> {
    polymarket_client_sdk_v2::types::U256::from_str(value)
        .map_err(|_| LiveMakerError::Submit("official SDK rejected token id".to_string()))
}

fn parse_decimal(value: f64, label: &'static str) -> LiveMakerResult<()> {
    polymarket_client_sdk_v2::types::Decimal::from_str(&decimal_label(value))
        .map(|_| ())
        .map_err(|_| LiveMakerError::Submit(format!("official SDK rejected {label}")))
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

fn sdk_error(source: polymarket_client_sdk_v2::error::Error) -> LiveMakerError {
    LiveMakerError::Submit(format!("official SDK LA5 maker path failed: {source}"))
}

fn decimal_label(value: f64) -> String {
    let rounded = format!("{value:.6}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn decimal_to_f64(value: &str, label: &'static str) -> LiveMakerResult<f64> {
    value
        .parse::<f64>()
        .map_err(|_| LiveMakerError::Submit(format!("official SDK returned invalid {label}")))
}

fn is_order_id(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.chars().all(|ch| ch.is_ascii_hexdigit()))
}

pub type LiveMakerResult<T> = Result<T, LiveMakerError>;

#[derive(Debug)]
pub enum LiveMakerError {
    Validation(Vec<String>),
    MissingSecretHandle { label: &'static str, handle: String },
    Submit(String),
}

impl Display for LiveMakerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(errors) => {
                writeln!(formatter, "live maker validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
            Self::MissingSecretHandle { label, handle } => {
                write!(formatter, "missing LA5 {label} env handle {handle}")
            }
            Self::Submit(message) => write!(formatter, "{message}"),
        }
    }
}

impl Error for LiveMakerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Asset;

    #[test]
    fn maker_only_ttl_30_sets_venue_gtd_expiration_at_now_plus_90() {
        let now = 1_777_000_000;
        assert_eq!(venue_gtd_expiration_unix(now, 30), now + 90);

        let plan =
            build_live_maker_order_plan(&sample_intent(), &sample_approval(), &maker_config(), now)
                .expect("plan builds");

        assert_eq!(plan.effective_quote_ttl_seconds, 30);
        assert_eq!(plan.cancel_after_unix, now + 30);
        assert_eq!(plan.gtd_expiration_unix, now + 90);
        assert!(plan.gtd_expiration_unix >= now + 90);
    }

    #[test]
    fn maker_only_quote_is_cancel_eligible_after_effective_ttl_not_venue_expiration() {
        let now = 1_777_000_000;
        let plan =
            build_live_maker_order_plan(&sample_intent(), &sample_approval(), &maker_config(), now)
                .expect("plan builds");

        assert!(!should_cancel_open_maker_order(&plan, now + 29, true));
        assert!(should_cancel_open_maker_order(&plan, now + 30, true));
        assert!(
            now + 30 < plan.gtd_expiration_unix,
            "cancel eligibility must use effective TTL before venue expiration"
        );
    }

    #[test]
    fn maker_only_post_only_gtd_plan_rejects_invalid_shapes() {
        let mut maker = maker_config();
        maker.ttl_seconds = 0;

        let error = build_live_maker_order_plan(
            &sample_intent(),
            &sample_approval(),
            &maker,
            1_777_000_000,
        )
        .expect_err("zero ttl fails");

        assert!(error.to_string().contains("live_alpha.maker.ttl_seconds"));
    }

    #[test]
    fn maker_only_plan_rejects_edge_below_configured_minimum() {
        let mut maker = maker_config();
        maker.min_edge_bps = 100;
        let mut intent = sample_intent();
        intent.edge_bps = 99.0;

        let error = build_live_maker_order_plan(&intent, &sample_approval(), &maker, 1_777_000_000)
            .expect_err("edge below configured threshold fails closed");

        assert!(error.to_string().contains("intent edge_bps"));
        assert!(error.to_string().contains("live_alpha.maker.min_edge_bps"));
    }

    #[test]
    fn maker_only_exact_cancel_requires_single_known_order_id() {
        assert!(is_order_id(
            "0x1111111111111111111111111111111111111111111111111111111111111111"
        ));
        assert!(!is_order_id("order-1"));
    }

    fn maker_config() -> LiveAlphaMakerConfig {
        LiveAlphaMakerConfig {
            enabled: true,
            post_only: true,
            order_type: "GTD".to_string(),
            ttl_seconds: 30,
            min_edge_bps: 0,
            replace_tolerance_bps: 0,
            min_quote_lifetime_ms: 0,
        }
    }

    fn sample_approval() -> LiveRiskApproved {
        LiveRiskApproved {
            intent_id: "intent-1".to_string(),
            approved_token_id: "token-up".to_string(),
            approved_outcome: "Up".to_string(),
            approved_notional: 0.2,
            approved_size: 1.0,
            approved_ttl_seconds: 30,
            approved_side: Side::Buy,
            reason_codes: Vec::new(),
        }
    }

    fn sample_intent() -> ExecutionIntent {
        ExecutionIntent {
            intent_id: "intent-1".to_string(),
            strategy_snapshot_id: "snapshot-1".to_string(),
            market_slug: "btc-updown-15m-test".to_string(),
            condition_id: "condition-1".to_string(),
            token_id: "token-up".to_string(),
            asset_symbol: "BTC".to_string(),
            asset: Asset::Btc,
            outcome: "Up".to_string(),
            side: Side::Buy,
            price: 0.20,
            size: 1.0,
            notional: 0.20,
            order_type: "GTD".to_string(),
            time_in_force: "GTD".to_string(),
            post_only: true,
            expiry: None,
            fair_probability: 0.23,
            edge_bps: 300.0,
            reference_price: 100_000.0,
            reference_source_timestamp: Some(1_777_000_000_000),
            book_snapshot_id: "book-1".to_string(),
            best_bid: Some(0.19),
            best_ask: Some(0.21),
            spread: Some(0.02),
            created_at: 1_777_000_000_000,
        }
    }
}
