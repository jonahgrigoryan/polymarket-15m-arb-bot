use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use ring::hmac;
use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_beta_readback";
pub const CLOB_HOST: &str = "https://clob.polymarket.com";
pub const BALANCE_ALLOWANCE_PATH: &str = "/balance-allowance";
pub const USER_ORDERS_PATH: &str = "/data/orders";
pub const TRADES_PATH: &str = "/trades";
pub const SAMPLING_MARKETS_PATH: &str = "/sampling-markets";
pub const SINGLE_ORDER_PATH_PREFIX: &str = "/data/order/";
const HTTP_GET: &str = "GET";
const INITIAL_CURSOR: &str = "MA==";
const END_CURSOR: &str = "LTE=";
const MAX_READBACK_PAGES: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadbackPrerequisites {
    pub lb3_hold_released: bool,
    pub legal_access_approved: bool,
    pub deployment_geoblock_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPreflight {
    pub clob_host: String,
    pub chain_id: u64,
    pub wallet_address: String,
    pub funder_address: String,
    pub signature_type: SignatureType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureType {
    Eoa,
    PolyProxy,
    GnosisSafe,
}

impl SignatureType {
    pub fn from_config(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "0" | "eoa" => Some(Self::Eoa),
            "1" | "poly_proxy" | "poly-proxy" | "polyproxy" => Some(Self::PolyProxy),
            "2" | "gnosis_safe" | "gnosis-safe" | "gnosissafe" => Some(Self::GnosisSafe),
            _ => None,
        }
    }

    pub fn as_config_str(self) -> &'static str {
        match self {
            Self::Eoa => "eoa",
            Self::PolyProxy => "poly_proxy",
            Self::GnosisSafe => "gnosis_safe",
        }
    }

    fn as_balance_allowance_param(self) -> &'static str {
        match self {
            Self::Eoa => "0",
            Self::PolyProxy => "1",
            Self::GnosisSafe => "2",
        }
    }
}

pub struct L2ReadbackCredentials {
    pub api_key: String,
    pub api_secret: String,
    pub api_passphrase: String,
}

pub struct AuthenticatedReadbackInput {
    pub prerequisites: ReadbackPrerequisites,
    pub account: AccountPreflight,
    pub credentials: L2ReadbackCredentials,
    pub required_collateral_allowance_units: u64,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    Collateral,
    Conditional,
}

impl AssetType {
    pub fn as_str(self) -> &'static str {
        match self {
            AssetType::Collateral => "COLLATERAL",
            AssetType::Conditional => "CONDITIONAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceAllowanceReadback {
    pub asset_type: AssetType,
    pub token_id: Option<String>,
    pub balance_units: u64,
    pub allowance_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenOrderReadback {
    pub id: String,
    pub status: OrderReadbackStatus,
    pub maker_address: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub original_size_units: u64,
    pub size_matched_units: u64,
    pub price: String,
    pub outcome: String,
    pub expiration: String,
    pub order_type: String,
    pub associate_trades: Vec<String>,
    pub created_at: i64,
}

impl OpenOrderReadback {
    pub fn remaining_size_units(&self) -> u64 {
        self.original_size_units
            .saturating_sub(self.size_matched_units)
    }

    pub fn reserved_pusd_units(&self) -> LiveBetaReadbackResult<u64> {
        if self.side != "BUY" {
            return Ok(0);
        }
        let price_units = parse_decimal_to_fixed6(&self.price, "price")?;
        Ok(self.remaining_size_units().saturating_mul(price_units) / 1_000_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderReadbackStatus {
    Live,
    Invalid,
    CanceledMarketResolved,
    Canceled,
    Matched,
    Unknown,
}

impl OrderReadbackStatus {
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "ORDER_STATUS_LIVE" | "LIVE" => Self::Live,
            "ORDER_STATUS_INVALID" | "INVALID" => Self::Invalid,
            "ORDER_STATUS_CANCELED_MARKET_RESOLVED" | "CANCELED_MARKET_RESOLVED" => {
                Self::CanceledMarketResolved
            }
            "ORDER_STATUS_CANCELED" | "CANCELED" => Self::Canceled,
            "ORDER_STATUS_MATCHED" | "MATCHED" => Self::Matched,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Invalid => "invalid",
            Self::CanceledMarketResolved => "canceled_market_resolved",
            Self::Canceled => "canceled",
            Self::Matched => "matched",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeReadback {
    pub id: String,
    pub market: String,
    pub asset_id: String,
    pub status: TradeReadbackStatus,
    pub transaction_hash: Option<String>,
    pub maker_address: String,
    pub order_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeReadbackStatus {
    Matched,
    Mined,
    Confirmed,
    Retrying,
    Failed,
    Unknown,
}

impl TradeReadbackStatus {
    pub fn from_wire(value: &str) -> Self {
        match value {
            "MATCHED" | "TRADE_STATUS_MATCHED" => Self::Matched,
            "MINED" | "TRADE_STATUS_MINED" => Self::Mined,
            "CONFIRMED" | "TRADE_STATUS_CONFIRMED" => Self::Confirmed,
            "RETRYING" | "TRADE_STATUS_RETRYING" => Self::Retrying,
            "FAILED" | "TRADE_STATUS_FAILED" => Self::Failed,
            _ => Self::Unknown,
        }
    }

    pub fn is_terminal_success(self) -> bool {
        self == Self::Confirmed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueState {
    TradingEnabled,
    TradingDisabled,
    CancelOnly,
    ClosedOnly,
    Delayed,
    Unmatched,
    Error,
    Unknown,
}

impl VenueState {
    pub fn from_wire(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "trading_enabled" | "enabled" | "open" => Self::TradingEnabled,
            "trading_disabled" | "disabled" => Self::TradingDisabled,
            "cancel_only" => Self::CancelOnly,
            "closed_only" | "closed" => Self::ClosedOnly,
            "delayed" => Self::Delayed,
            "unmatched" => Self::Unmatched,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TradingEnabled => "trading_enabled",
            Self::TradingDisabled => "trading_disabled",
            Self::CancelOnly => "cancel_only",
            Self::ClosedOnly => "closed_only",
            Self::Delayed => "delayed",
            Self::Unmatched => "unmatched",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatReadiness {
    NotStartedNoOpenOrders,
    Healthy,
    Unhealthy,
    Unknown,
}

impl HeartbeatReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStartedNoOpenOrders => "not_started_no_open_orders",
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadbackPreflightInput {
    pub prerequisites: ReadbackPrerequisites,
    pub account: AccountPreflight,
    pub venue_state: VenueState,
    pub collateral: BalanceAllowanceReadback,
    pub open_orders: Vec<OpenOrderReadback>,
    pub trades: Vec<TradeReadback>,
    pub heartbeat: HeartbeatReadiness,
    pub required_collateral_allowance_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReadbackPreflightReport {
    pub status: &'static str,
    pub block_reasons: Vec<&'static str>,
    pub open_order_count: usize,
    pub trade_count: usize,
    pub reserved_pusd_units: u64,
    pub required_collateral_allowance_units: u64,
    pub available_pusd_units: u64,
    pub venue_state: &'static str,
    pub heartbeat: &'static str,
    pub live_network_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedReadbackPreflightEvidence {
    pub report: ReadbackPreflightReport,
    pub collateral: BalanceAllowanceReadback,
    pub open_orders: Vec<OpenOrderReadback>,
    pub trades: Vec<TradeReadback>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadbackEndpointError {
    pub status_code: u16,
    pub code: String,
    pub message_redacted: bool,
}

impl ReadbackPreflightReport {
    pub fn passed(&self) -> bool {
        self.block_reasons.is_empty()
    }
}

pub fn evaluate_readback_preflight(
    input: &ReadbackPreflightInput,
) -> LiveBetaReadbackResult<ReadbackPreflightReport> {
    let mut block_reasons = Vec::new();

    if !input.prerequisites.lb3_hold_released {
        block_reasons.push("lb3_hold_not_released");
    }
    if !input.prerequisites.legal_access_approved {
        block_reasons.push("legal_access_not_recorded");
    }
    if !input.prerequisites.deployment_geoblock_passed {
        block_reasons.push("deployment_geoblock_not_recorded");
    }
    if input.account.clob_host != CLOB_HOST {
        block_reasons.push("clob_host_mismatch");
    }
    if input.account.chain_id != 137 {
        block_reasons.push("chain_id_mismatch");
    }
    let account_addresses_valid = is_valid_evm_address(&input.account.wallet_address)
        && is_valid_evm_address(&input.account.funder_address);
    if !account_addresses_valid {
        block_reasons.push("account_address_invalid");
    }
    if account_addresses_valid
        && input.account.signature_type == SignatureType::Eoa
        && !evm_addresses_equal(&input.account.wallet_address, &input.account.funder_address)
    {
        block_reasons.push("eoa_wallet_funder_mismatch");
    }
    if input.venue_state != VenueState::TradingEnabled {
        block_reasons.push("venue_state_not_open");
    }
    if !input.open_orders.is_empty() {
        block_reasons.push("unexpected_open_orders");
    }
    if input
        .open_orders
        .iter()
        .any(|order| order.status == OrderReadbackStatus::Unknown)
    {
        block_reasons.push("unknown_order_status");
    }
    if input
        .open_orders
        .iter()
        .any(|order| !evm_addresses_equal(&order.maker_address, &input.account.funder_address))
    {
        block_reasons.push("funder_mismatch");
    }

    let reserved_pusd_units = reserved_pusd_units(&input.open_orders)?;
    if input.collateral.asset_type != AssetType::Collateral {
        block_reasons.push("collateral_asset_type_mismatch");
    }
    if input.collateral.balance_units < reserved_pusd_units {
        block_reasons.push("balance_below_reserved");
    }
    if input
        .collateral
        .balance_units
        .saturating_sub(reserved_pusd_units)
        < input.required_collateral_allowance_units
    {
        block_reasons.push("balance_below_required");
    }
    if input.collateral.allowance_units < input.required_collateral_allowance_units {
        block_reasons.push("allowance_below_required");
    }

    for trade in &input.trades {
        match trade.status {
            TradeReadbackStatus::Matched
            | TradeReadbackStatus::Mined
            | TradeReadbackStatus::Retrying => {
                block_reasons.push("nonterminal_trade_status");
            }
            TradeReadbackStatus::Failed => block_reasons.push("failed_trade_status"),
            TradeReadbackStatus::Unknown => block_reasons.push("unknown_trade_status"),
            TradeReadbackStatus::Confirmed => {
                if !trade
                    .transaction_hash
                    .as_deref()
                    .is_some_and(is_valid_tx_hash)
                {
                    block_reasons.push("missing_confirmed_trade_transaction_hash");
                }
            }
        }
        if !evm_addresses_equal(&trade.maker_address, &input.account.funder_address) {
            block_reasons.push("trade_funder_mismatch");
        }
    }

    match input.heartbeat {
        HeartbeatReadiness::NotStartedNoOpenOrders if input.open_orders.is_empty() => {}
        HeartbeatReadiness::Healthy if input.open_orders.is_empty() => {
            block_reasons.push("heartbeat_active_without_approved_order");
        }
        HeartbeatReadiness::Healthy => {}
        HeartbeatReadiness::Unhealthy => block_reasons.push("heartbeat_unhealthy"),
        HeartbeatReadiness::Unknown | HeartbeatReadiness::NotStartedNoOpenOrders => {
            block_reasons.push("heartbeat_unknown")
        }
    }

    block_reasons.sort_unstable();
    block_reasons.dedup();

    let available_pusd_units = input
        .collateral
        .balance_units
        .saturating_sub(reserved_pusd_units);

    Ok(ReadbackPreflightReport {
        status: if block_reasons.is_empty() {
            "passed"
        } else {
            "blocked"
        },
        block_reasons,
        open_order_count: input.open_orders.len(),
        trade_count: input.trades.len(),
        reserved_pusd_units,
        required_collateral_allowance_units: input.required_collateral_allowance_units,
        available_pusd_units,
        venue_state: input.venue_state.as_str(),
        heartbeat: input.heartbeat.as_str(),
        live_network_enabled: false,
    })
}

pub fn sample_readback_preflight(
    prerequisites: ReadbackPrerequisites,
) -> LiveBetaReadbackResult<ReadbackPreflightReport> {
    evaluate_readback_preflight(&ReadbackPreflightInput {
        prerequisites,
        account: AccountPreflight {
            clob_host: CLOB_HOST.to_string(),
            chain_id: 137,
            wallet_address: "0x1111111111111111111111111111111111111111".to_string(),
            funder_address: "0x1111111111111111111111111111111111111111".to_string(),
            signature_type: SignatureType::Eoa,
        },
        venue_state: VenueState::TradingEnabled,
        collateral: BalanceAllowanceReadback {
            asset_type: AssetType::Collateral,
            token_id: None,
            balance_units: 25_000_000,
            allowance_units: 25_000_000,
        },
        open_orders: Vec::new(),
        trades: Vec::new(),
        heartbeat: HeartbeatReadiness::NotStartedNoOpenOrders,
        required_collateral_allowance_units: 1_000_000,
    })
}

pub async fn authenticated_readback_preflight(
    input: AuthenticatedReadbackInput,
) -> LiveBetaReadbackResult<ReadbackPreflightReport> {
    Ok(authenticated_readback_preflight_evidence(input)
        .await?
        .report)
}

pub async fn authenticated_readback_preflight_evidence(
    input: AuthenticatedReadbackInput,
) -> LiveBetaReadbackResult<AuthenticatedReadbackPreflightEvidence> {
    validate_authenticated_readback_input(&input)?;

    let client = ReadOnlyClobReadbackClient::new(
        input.account.clob_host.clone(),
        input.account.wallet_address.clone(),
        input.account.funder_address.clone(),
        input.credentials,
        input.request_timeout_ms,
    )?;
    let collateral = client
        .get_balance_allowance(input.account.signature_type)
        .await?;
    let open_orders = client.get_user_orders().await?;
    let trades = client.get_trades().await?;
    let venue_state = client.get_venue_state().await?;
    let heartbeat = if open_orders.is_empty() {
        HeartbeatReadiness::NotStartedNoOpenOrders
    } else {
        HeartbeatReadiness::Unknown
    };

    let mut report = evaluate_readback_preflight(&ReadbackPreflightInput {
        prerequisites: input.prerequisites,
        account: input.account,
        venue_state,
        collateral: collateral.clone(),
        open_orders: open_orders.clone(),
        trades: trades.clone(),
        heartbeat,
        required_collateral_allowance_units: input.required_collateral_allowance_units,
    })?;
    report.live_network_enabled = true;
    Ok(AuthenticatedReadbackPreflightEvidence {
        report,
        collateral,
        open_orders,
        trades,
    })
}

pub fn readback_path_catalog() -> Vec<&'static str> {
    vec![
        BALANCE_ALLOWANCE_PATH,
        USER_ORDERS_PATH,
        TRADES_PATH,
        SAMPLING_MARKETS_PATH,
        SINGLE_ORDER_PATH_PREFIX,
    ]
}

struct ReadOnlyClobReadbackClient {
    http: reqwest::Client,
    host: String,
    address: String,
    maker_address: String,
    credentials: L2ReadbackCredentials,
}

impl ReadOnlyClobReadbackClient {
    fn new(
        host: String,
        address: String,
        maker_address: String,
        credentials: L2ReadbackCredentials,
        timeout_ms: u64,
    ) -> LiveBetaReadbackResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| {
                LiveBetaReadbackError::Network(format!(
                    "failed to build LB4 read-only HTTP client: {source}"
                ))
            })?;
        Ok(Self {
            http,
            host: host.trim_end_matches('/').to_string(),
            address,
            maker_address,
            credentials,
        })
    }

    async fn get_balance_allowance(
        &self,
        signature_type: SignatureType,
    ) -> LiveBetaReadbackResult<BalanceAllowanceReadback> {
        let body = self
            .get_text(
                BALANCE_ALLOWANCE_PATH,
                &[
                    ("asset_type", AssetType::Collateral.as_str().to_string()),
                    (
                        "signature_type",
                        signature_type.as_balance_allowance_param().to_string(),
                    ),
                ],
            )
            .await?;
        parse_balance_allowance(&body, AssetType::Collateral, None)
    }

    async fn get_user_orders(&self) -> LiveBetaReadbackResult<Vec<OpenOrderReadback>> {
        let mut cursor = INITIAL_CURSOR.to_string();
        let mut orders = Vec::new();
        for _ in 0..MAX_READBACK_PAGES {
            let body = self
                .get_text(USER_ORDERS_PATH, &[("next_cursor", cursor.clone())])
                .await?;
            let page = parse_user_orders_page_with_cursor(&body)?;
            orders.extend(page.data);
            let Some(next_cursor) = next_readback_cursor(&cursor, &page.next_cursor)? else {
                return Ok(orders);
            };
            cursor = next_cursor;
        }
        Err(LiveBetaReadbackError::Validation(format!(
            "{USER_ORDERS_PATH} pagination exceeded {MAX_READBACK_PAGES} pages"
        )))
    }

    async fn get_trades(&self) -> LiveBetaReadbackResult<Vec<TradeReadback>> {
        let mut cursor = INITIAL_CURSOR.to_string();
        let mut trades = Vec::new();
        for _ in 0..MAX_READBACK_PAGES {
            let body = self
                .get_text(TRADES_PATH, &trades_query(&cursor, &self.maker_address))
                .await?;
            let page = parse_trades_page_with_cursor_for_account(&body, &self.maker_address)?;
            trades.extend(page.data);
            let Some(next_cursor) = next_readback_cursor(&cursor, &page.next_cursor)? else {
                return Ok(trades);
            };
            cursor = next_cursor;
        }
        Err(LiveBetaReadbackError::Validation(format!(
            "{TRADES_PATH} pagination exceeded {MAX_READBACK_PAGES} pages"
        )))
    }

    async fn get_venue_state(&self) -> LiveBetaReadbackResult<VenueState> {
        let mut cursor = INITIAL_CURSOR.to_string();
        let mut markets = Vec::new();
        for _ in 0..MAX_READBACK_PAGES {
            let body = self
                .get_text(SAMPLING_MARKETS_PATH, &[("next_cursor", cursor.clone())])
                .await?;
            let page = parse_sampling_markets_page_with_cursor(&body)?;
            if derive_venue_state_from_sampling_markets(&page.data)? == VenueState::TradingEnabled {
                return Ok(VenueState::TradingEnabled);
            }
            markets.extend(page.data);
            let Some(next_cursor) = next_readback_cursor(&cursor, &page.next_cursor)? else {
                return derive_venue_state_from_sampling_markets(&markets);
            };
            cursor = next_cursor;
        }
        Err(LiveBetaReadbackError::Validation(format!(
            "{SAMPLING_MARKETS_PATH} pagination exceeded {MAX_READBACK_PAGES} pages"
        )))
    }

    async fn get_text(
        &self,
        path: &'static str,
        query: &[(&'static str, String)],
    ) -> LiveBetaReadbackResult<String> {
        let timestamp = current_unix_timestamp()?;
        let signature = build_l2_hmac_signature(
            &self.credentials.api_secret,
            timestamp,
            HTTP_GET,
            path,
            None,
        )?;
        let response = self
            .http
            .get(format!("{}{}", self.host, path))
            .query(query)
            .header("POLY_ADDRESS", &self.address)
            .header("POLY_SIGNATURE", signature)
            .header("POLY_TIMESTAMP", timestamp.to_string())
            .header("POLY_API_KEY", &self.credentials.api_key)
            .header("POLY_PASSPHRASE", &self.credentials.api_passphrase)
            .send()
            .await
            .map_err(|source| {
                LiveBetaReadbackError::Network(format!(
                    "LB4 authenticated read-only GET failed for {path}: {source}"
                ))
            })?;
        let status = response.status();
        let status_code = status.as_u16();
        let body = response.text().await.map_err(|source| {
            LiveBetaReadbackError::Network(format!(
                "LB4 authenticated read-only response body failed for {path}: {source}"
            ))
        })?;
        if status.is_success() {
            if body.trim().is_empty() {
                return Err(LiveBetaReadbackError::Validation(format!(
                    "{path} returned an empty body"
                )));
            }
            return Ok(body);
        }

        let endpoint_error =
            parse_readback_error_response(status_code, &body).unwrap_or_else(|_| {
                ReadbackEndpointError {
                    status_code,
                    code: format!("http_{status_code}"),
                    message_redacted: true,
                }
            });
        Err(LiveBetaReadbackError::Endpoint(endpoint_error))
    }
}

pub fn build_l2_hmac_signature(
    secret: &str,
    timestamp: u64,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> LiveBetaReadbackResult<String> {
    let decoded_secret = URL_SAFE.decode(secret).map_err(|_| {
        LiveBetaReadbackError::Credential(
            "l2 credential handle value is not valid base64".to_string(),
        )
    })?;
    let mut message = format!("{timestamp}{method}{path}");
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        message.push_str(body);
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, &decoded_secret);
    let tag = hmac::sign(&key, message.as_bytes());
    Ok(URL_SAFE.encode(tag.as_ref()))
}

fn current_unix_timestamp() -> LiveBetaReadbackResult<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            LiveBetaReadbackError::Validation("system clock is before Unix epoch".to_string())
        })?
        .as_secs())
}

fn validate_authenticated_readback_input(
    input: &AuthenticatedReadbackInput,
) -> LiveBetaReadbackResult<()> {
    let mut errors = Vec::new();
    if input.request_timeout_ms == 0 {
        errors.push("request_timeout_ms must be positive".to_string());
    }
    if input.required_collateral_allowance_units == 0 {
        errors.push("required_collateral_allowance_units must be positive".to_string());
    }
    if input.credentials.api_key.trim().is_empty() {
        errors.push("clob_l2_access handle is missing".to_string());
    }
    if input.credentials.api_secret.trim().is_empty() {
        errors.push("clob_l2_credential handle is missing".to_string());
    }
    if input.credentials.api_passphrase.trim().is_empty() {
        errors.push("clob_l2_passphrase handle is missing".to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(LiveBetaReadbackError::Validation(errors.join(", ")))
    }
}

pub fn parse_balance_allowance(
    json: &str,
    asset_type: AssetType,
    token_id: Option<String>,
) -> LiveBetaReadbackResult<BalanceAllowanceReadback> {
    let wire: BalanceAllowanceWire =
        serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    Ok(BalanceAllowanceReadback {
        asset_type,
        token_id,
        balance_units: parse_u64_units_value(&wire.balance, "balance")?,
        allowance_units: parse_allowance_units(&wire)?,
    })
}

pub fn parse_user_orders_page(json: &str) -> LiveBetaReadbackResult<Vec<OpenOrderReadback>> {
    Ok(parse_user_orders_page_with_cursor(json)?.data)
}

pub fn parse_single_order(json: &str) -> LiveBetaReadbackResult<OpenOrderReadback> {
    let wire: OpenOrderWire = serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    wire.into_readback(parse_decimal_to_fixed6)
}

fn parse_user_orders_page_with_cursor(
    json: &str,
) -> LiveBetaReadbackResult<ReadbackPage<OpenOrderReadback>> {
    let wire: OpenOrdersPageWire =
        serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    let data = wire
        .data
        .into_iter()
        .map(OpenOrderReadback::try_from)
        .collect::<LiveBetaReadbackResult<Vec<_>>>()?;
    Ok(ReadbackPage {
        data,
        next_cursor: wire.next_cursor,
    })
}

pub fn parse_trades_page(json: &str) -> LiveBetaReadbackResult<Vec<TradeReadback>> {
    Ok(parse_trades_page_with_cursor(json)?.data)
}

fn parse_trades_page_with_cursor(
    json: &str,
) -> LiveBetaReadbackResult<ReadbackPage<TradeReadback>> {
    parse_trades_page_with_cursor_with_account(json, None)
}

fn parse_trades_page_with_cursor_for_account(
    json: &str,
    account_address: &str,
) -> LiveBetaReadbackResult<ReadbackPage<TradeReadback>> {
    parse_trades_page_with_cursor_with_account(json, Some(account_address))
}

fn parse_trades_page_with_cursor_with_account(
    json: &str,
    account_address: Option<&str>,
) -> LiveBetaReadbackResult<ReadbackPage<TradeReadback>> {
    let wire: TradesPageWire = serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    let data = wire
        .data
        .into_iter()
        .map(|trade| trade.into_readback(account_address))
        .collect::<LiveBetaReadbackResult<Vec<_>>>()?;
    Ok(ReadbackPage {
        data,
        next_cursor: wire.next_cursor,
    })
}

pub fn parse_venue_state(json: &str) -> LiveBetaReadbackResult<VenueState> {
    let wire: VenueStateWire = serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    Ok(VenueState::from_wire(&wire.state))
}

pub fn parse_sampling_markets_venue_state(json: &str) -> LiveBetaReadbackResult<VenueState> {
    derive_venue_state_from_sampling_markets(&parse_sampling_markets_page_with_cursor(json)?.data)
}

fn parse_sampling_markets_page_with_cursor(
    json: &str,
) -> LiveBetaReadbackResult<ReadbackPage<SamplingMarketWire>> {
    let wire: SamplingMarketsPageWire =
        serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    Ok(ReadbackPage {
        data: wire.data,
        next_cursor: wire.next_cursor,
    })
}

pub fn parse_readback_error_response(
    status_code: u16,
    json: &str,
) -> LiveBetaReadbackResult<ReadbackEndpointError> {
    let wire: ReadbackErrorWire =
        serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    let code =
        classify_readback_error_code(status_code, wire.code.as_deref(), wire.error.as_deref());
    Ok(ReadbackEndpointError {
        status_code,
        code,
        message_redacted: wire.error.is_some() || wire.message.is_some(),
    })
}

pub fn reserved_pusd_units(orders: &[OpenOrderReadback]) -> LiveBetaReadbackResult<u64> {
    let mut total = 0_u64;
    for order in orders {
        total = total.saturating_add(order.reserved_pusd_units()?);
    }
    Ok(total)
}

#[derive(Debug, Deserialize)]
struct BalanceAllowanceWire {
    balance: serde_json::Value,
    #[serde(default)]
    allowance: Option<serde_json::Value>,
    #[serde(default)]
    allowances: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenOrdersPageWire {
    next_cursor: String,
    data: Vec<OpenOrderWire>,
}

#[derive(Debug, Deserialize)]
struct OpenOrderWire {
    id: String,
    status: String,
    maker_address: String,
    market: String,
    asset_id: String,
    side: String,
    original_size: String,
    size_matched: String,
    price: String,
    outcome: String,
    expiration: String,
    order_type: String,
    #[serde(default)]
    associate_trades: Vec<String>,
    created_at: i64,
}

impl TryFrom<OpenOrderWire> for OpenOrderReadback {
    type Error = LiveBetaReadbackError;

    fn try_from(value: OpenOrderWire) -> Result<Self, Self::Error> {
        value.into_readback(parse_u64_units)
    }
}

impl OpenOrderWire {
    fn into_readback(
        self,
        parse_size_units: fn(&str, &'static str) -> LiveBetaReadbackResult<u64>,
    ) -> LiveBetaReadbackResult<OpenOrderReadback> {
        Ok(OpenOrderReadback {
            id: self.id,
            status: OrderReadbackStatus::from_wire(&self.status),
            maker_address: self.maker_address,
            market: self.market,
            asset_id: self.asset_id,
            side: self.side,
            original_size_units: parse_size_units(&self.original_size, "original_size")?,
            size_matched_units: parse_size_units(&self.size_matched, "size_matched")?,
            price: self.price,
            outcome: self.outcome,
            expiration: self.expiration,
            order_type: self.order_type,
            associate_trades: self.associate_trades,
            created_at: self.created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct TradesPageWire {
    next_cursor: String,
    data: Vec<TradeWire>,
}

struct ReadbackPage<T> {
    data: Vec<T>,
    next_cursor: String,
}

#[derive(Debug, Deserialize)]
struct TradeWire {
    id: String,
    market: String,
    asset_id: String,
    status: String,
    #[serde(default)]
    transaction_hash: Option<String>,
    maker_address: String,
    #[serde(default)]
    trader_side: Option<String>,
    #[serde(default)]
    taker_order_id: Option<String>,
    #[serde(default)]
    maker_orders: Vec<TradeMakerOrderWire>,
}

#[derive(Debug, Deserialize)]
struct TradeMakerOrderWire {
    order_id: String,
    #[serde(default)]
    maker_address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VenueStateWire {
    state: String,
}

#[derive(Debug, Deserialize)]
struct SamplingMarketsPageWire {
    next_cursor: String,
    data: Vec<SamplingMarketWire>,
}

#[derive(Debug, Deserialize)]
struct SamplingMarketWire {
    enable_order_book: bool,
    active: bool,
    closed: bool,
    archived: bool,
    accepting_orders: bool,
}

#[derive(Debug, Deserialize)]
struct ReadbackErrorWire {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl TradeWire {
    fn into_readback(self, account_address: Option<&str>) -> LiveBetaReadbackResult<TradeReadback> {
        let order_id = self.order_id_for_account(account_address);
        Ok(TradeReadback {
            id: self.id,
            market: self.market,
            asset_id: self.asset_id,
            status: TradeReadbackStatus::from_wire(&self.status),
            transaction_hash: self.transaction_hash,
            maker_address: self.maker_address,
            order_id,
        })
    }

    fn order_id_for_account(&self, account_address: Option<&str>) -> Option<String> {
        if let Some(trader_side) = self.trader_side.as_deref().map(str::trim) {
            if trader_side.eq_ignore_ascii_case("TAKER") {
                return self.taker_order_id.as_deref().and_then(non_empty_order_id);
            }

            if trader_side.eq_ignore_ascii_case("MAKER") {
                return self.maker_order_id_for_account(account_address);
            }
        }

        self.order_id_for_account_without_trader_side(account_address)
    }

    fn order_id_for_account_without_trader_side(
        &self,
        account_address: Option<&str>,
    ) -> Option<String> {
        let normalized_account = account_address
            .map(str::trim)
            .filter(|account_address| !account_address.is_empty());

        if let Some(account_address) = normalized_account {
            if let Some(order_id) = self.maker_order_id_for_account(Some(account_address)) {
                return Some(order_id);
            }

            return self.taker_order_id.as_deref().and_then(non_empty_order_id);
        }

        self.taker_order_id
            .as_deref()
            .and_then(non_empty_order_id)
            .or_else(|| {
                self.maker_orders
                    .iter()
                    .find_map(|order| non_empty_order_id(&order.order_id))
            })
    }

    fn maker_order_id_for_account(&self, account_address: Option<&str>) -> Option<String> {
        let normalized_account = account_address
            .map(str::trim)
            .filter(|account_address| !account_address.is_empty());

        if let Some(account_address) = normalized_account {
            if let Some(order_id) = self
                .maker_orders
                .iter()
                .filter(|order| {
                    order.maker_address.as_deref().is_some_and(|maker_address| {
                        evm_addresses_equal(maker_address, account_address)
                    })
                })
                .find_map(|order| non_empty_order_id(&order.order_id))
            {
                return Some(order_id);
            }

            if !evm_addresses_equal(&self.maker_address, account_address) {
                return None;
            }
        }

        self.maker_orders
            .iter()
            .find_map(|order| non_empty_order_id(&order.order_id))
    }
}

fn non_empty_order_id(order_id: &str) -> Option<String> {
    let trimmed = order_id.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_u64_units(value: &str, field: &'static str) -> LiveBetaReadbackResult<u64> {
    value.parse::<u64>().map_err(|_| {
        LiveBetaReadbackError::Validation(format!("{field} must be unsigned fixed-math units"))
    })
}

fn parse_u64_units_value(
    value: &serde_json::Value,
    field: &'static str,
) -> LiveBetaReadbackResult<u64> {
    match value {
        serde_json::Value::String(text) => parse_u64_units(text, field),
        serde_json::Value::Number(number) => number.as_u64().ok_or_else(|| {
            LiveBetaReadbackError::Validation(format!("{field} must be unsigned fixed-math units"))
        }),
        _ => Err(LiveBetaReadbackError::Validation(format!(
            "{field} must be unsigned fixed-math units"
        ))),
    }
}

fn parse_allowance_units(wire: &BalanceAllowanceWire) -> LiveBetaReadbackResult<u64> {
    if let Some(allowance) = &wire.allowance {
        return parse_saturating_u64_units_value(allowance, "allowance");
    }

    let Some(allowances) = &wire.allowances else {
        return Err(LiveBetaReadbackError::Validation(
            "allowance or allowances must be present".to_string(),
        ));
    };
    let serde_json::Value::Object(entries) = allowances else {
        return Err(LiveBetaReadbackError::Validation(
            "allowances must be an object of unsigned fixed-math unit values".to_string(),
        ));
    };
    if entries.is_empty() {
        return Err(LiveBetaReadbackError::Validation(
            "allowances must not be empty".to_string(),
        ));
    }

    entries
        .values()
        .map(|value| parse_saturating_u64_units_value(value, "allowance"))
        .try_fold(u64::MAX, |lowest, units| {
            units.map(|units| lowest.min(units))
        })
}

fn parse_saturating_u64_units_value(
    value: &serde_json::Value,
    field: &'static str,
) -> LiveBetaReadbackResult<u64> {
    match value {
        serde_json::Value::String(text) => parse_saturating_u64_units(text, field),
        serde_json::Value::Number(number) => parse_saturating_u64_units(&number.to_string(), field),
        _ => Err(LiveBetaReadbackError::Validation(format!(
            "{field} must be unsigned fixed-math units"
        ))),
    }
}

fn parse_saturating_u64_units(value: &str, field: &'static str) -> LiveBetaReadbackResult<u64> {
    if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(LiveBetaReadbackError::Validation(format!(
            "{field} must be unsigned fixed-math units"
        )));
    }
    Ok(value.parse::<u64>().unwrap_or(u64::MAX))
}

fn parse_decimal_to_fixed6(value: &str, field: &'static str) -> LiveBetaReadbackResult<u64> {
    let trimmed = value.trim();
    let (whole, fractional) = trimmed
        .split_once('.')
        .map_or((trimmed, ""), |(whole, fractional)| (whole, fractional));
    if whole.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fractional.chars().all(|ch| ch.is_ascii_digit())
        || fractional.len() > 6
    {
        return Err(LiveBetaReadbackError::Validation(format!(
            "{field} must be a nonnegative decimal with at most 6 places"
        )));
    }
    let whole_units = whole.parse::<u64>().map_err(|_| {
        LiveBetaReadbackError::Validation(format!("{field} whole units are too large"))
    })?;
    let mut fractional_padded = fractional.to_string();
    while fractional_padded.len() < 6 {
        fractional_padded.push('0');
    }
    let fractional_units = if fractional_padded.is_empty() {
        0
    } else {
        fractional_padded.parse::<u64>().map_err(|_| {
            LiveBetaReadbackError::Validation(format!("{field} fractional units are invalid"))
        })?
    };
    whole_units
        .checked_mul(1_000_000)
        .and_then(|units| units.checked_add(fractional_units))
        .ok_or_else(|| LiveBetaReadbackError::Validation(format!("{field} units overflow")))
}

fn is_valid_evm_address(value: &str) -> bool {
    let Some(stripped) = value.strip_prefix("0x") else {
        return false;
    };
    stripped.len() == 40
        && stripped.chars().all(|ch| ch.is_ascii_hexdigit())
        && stripped.chars().any(|ch| ch != '0')
}

fn evm_addresses_equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn is_valid_tx_hash(value: &str) -> bool {
    let Some(stripped) = value.strip_prefix("0x") else {
        return false;
    };
    stripped.len() == 64 && stripped.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn next_readback_cursor(
    current_cursor: &str,
    next_cursor: &str,
) -> LiveBetaReadbackResult<Option<String>> {
    let trimmed = next_cursor.trim();
    if trimmed.is_empty() || trimmed == END_CURSOR {
        return Ok(None);
    }
    if trimmed == current_cursor {
        return Err(LiveBetaReadbackError::Validation(
            "readback pagination cursor did not advance".to_string(),
        ));
    }
    Ok(Some(trimmed.to_string()))
}

fn trades_query(cursor: &str, maker_address: &str) -> Vec<(&'static str, String)> {
    vec![
        ("next_cursor", cursor.to_string()),
        ("maker_address", maker_address.to_string()),
    ]
}

fn derive_venue_state_from_sampling_markets(
    markets: &[SamplingMarketWire],
) -> LiveBetaReadbackResult<VenueState> {
    if markets.is_empty() {
        return Ok(VenueState::Unknown);
    }
    if markets.iter().any(|market| {
        market.enable_order_book && market.active && !market.closed && !market.archived
    }) {
        if markets.iter().any(|market| {
            market.enable_order_book
                && market.active
                && !market.closed
                && !market.archived
                && market.accepting_orders
        }) {
            return Ok(VenueState::TradingEnabled);
        }
        return Ok(VenueState::CancelOnly);
    }
    if markets
        .iter()
        .all(|market| market.closed || market.archived)
    {
        return Ok(VenueState::ClosedOnly);
    }
    Ok(VenueState::TradingDisabled)
}

fn classify_readback_error_code(
    status_code: u16,
    explicit_code: Option<&str>,
    error_message: Option<&str>,
) -> String {
    if let Some(code) = explicit_code.filter(|code| {
        !code.is_empty()
            && code
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    }) {
        return code.to_string();
    }

    let normalized = error_message.unwrap_or_default().to_ascii_lowercase();
    if normalized.contains("rate") || normalized.contains("too many") {
        "rate_limited".to_string()
    } else if normalized.contains("unauthorized") || normalized.contains("api key") {
        "unauthorized".to_string()
    } else if normalized.contains("not found") {
        "not_found".to_string()
    } else if normalized.contains("invalid") {
        "invalid_request".to_string()
    } else if normalized.contains("disabled")
        || normalized.contains("paused")
        || normalized.contains("cancel-only")
    {
        "venue_unavailable".to_string()
    } else {
        format!("http_{status_code}")
    }
}

pub type LiveBetaReadbackResult<T> = Result<T, LiveBetaReadbackError>;

#[derive(Debug)]
pub enum LiveBetaReadbackError {
    Parse(serde_json::Error),
    Validation(String),
    Credential(String),
    Network(String),
    Endpoint(ReadbackEndpointError),
}

impl Display for LiveBetaReadbackError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(source) => write!(formatter, "failed to parse LB4 readback JSON: {source}"),
            Self::Validation(message) => {
                write!(formatter, "LB4 readback validation failed: {message}")
            }
            Self::Credential(message) => {
                write!(
                    formatter,
                    "LB4 readback credential validation failed: {message}"
                )
            }
            Self::Network(message) => write!(formatter, "LB4 readback network failed: {message}"),
            Self::Endpoint(error) => write!(
                formatter,
                "LB4 readback endpoint returned status={} code={} message_redacted={}",
                error.status_code, error.code, error.message_redacted
            ),
        }
    }
}

impl Error for LiveBetaReadbackError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readback_preflight_fails_closed_without_legal_access_or_geoblock() {
        let report = sample_readback_preflight(ReadbackPrerequisites {
            lb3_hold_released: true,
            legal_access_approved: false,
            deployment_geoblock_passed: false,
        })
        .expect("sample report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"legal_access_not_recorded"));
        assert!(report
            .block_reasons
            .contains(&"deployment_geoblock_not_recorded"));
        assert!(!report.live_network_enabled);
    }

    #[test]
    fn account_preflight_rejects_zero_wallet_or_funder_address() {
        let mut input = passing_input();
        input.account.wallet_address = "0x0000000000000000000000000000000000000000".to_string();

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"account_address_invalid"));

        let mut input = passing_input();
        input.account.funder_address = "0x0000000000000000000000000000000000000000".to_string();

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"account_address_invalid"));
    }

    #[test]
    fn eoa_signature_requires_wallet_and_funder_to_match() {
        let same_lower = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let same_mixed = "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD";
        let mut input = passing_input();
        input.account.wallet_address = same_mixed.to_string();
        input.account.funder_address = same_lower.to_string();

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "passed");
        assert!(!report.block_reasons.contains(&"eoa_wallet_funder_mismatch"));

        input.account.funder_address = "0x2222222222222222222222222222222222222222".to_string();

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"eoa_wallet_funder_mismatch"));
        assert!(!report.block_reasons.contains(&"funder_mismatch"));
        assert!(!report.block_reasons.contains(&"trade_funder_mismatch"));
    }

    #[test]
    fn balance_preflight_accounts_for_reserved_open_orders() {
        let orders = vec![sample_order("BUY", "0.50", "2000000", "500000")];

        let reserved = reserved_pusd_units(&orders).expect("reserved balance calculates");

        assert_eq!(reserved, 750_000);
    }

    #[test]
    fn allowance_preflight_blocks_low_collateral_allowance() {
        let mut input = passing_input();
        input.collateral.allowance_units = 500_000;

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"allowance_below_required"));
    }

    #[test]
    fn balance_preflight_blocks_low_available_collateral() {
        let mut input = passing_input();
        input.collateral.balance_units = 500_000;
        input.collateral.allowance_units = 1_000_000;

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"balance_below_required"));
    }

    #[test]
    fn balance_allowance_signature_type_params_match_official_v2_client() {
        assert_eq!(SignatureType::Eoa.as_balance_allowance_param(), "0");
        assert_eq!(SignatureType::PolyProxy.as_balance_allowance_param(), "1");
        assert_eq!(SignatureType::GnosisSafe.as_balance_allowance_param(), "2");
    }

    #[test]
    fn heartbeat_unknown_blocks_maker_readiness() {
        let mut input = passing_input();
        input.heartbeat = HeartbeatReadiness::Unknown;

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"heartbeat_unknown"));
    }

    #[test]
    fn venue_states_fail_closed_unless_trading_enabled() {
        for state in [
            VenueState::TradingDisabled,
            VenueState::CancelOnly,
            VenueState::ClosedOnly,
            VenueState::Delayed,
            VenueState::Unmatched,
            VenueState::Error,
            VenueState::Unknown,
        ] {
            let mut input = passing_input();
            input.venue_state = state;

            let report = evaluate_readback_preflight(&input).expect("report builds");

            assert_eq!(report.status, "blocked");
            assert!(report.block_reasons.contains(&"venue_state_not_open"));
        }
    }

    #[test]
    fn readback_parses_open_orders_and_blocks_existing_orders() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "order-1",
                    "status": "ORDER_STATUS_LIVE",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "side": "BUY",
                    "original_size": "2000000",
                    "size_matched": "0",
                    "price": "0.25",
                    "outcome": "YES",
                    "expiration": "1777434180",
                    "order_type": "GTD",
                    "associate_trades": ["{}"],
                    "created_at": 1777434000
                }}]
            }}"#,
            "trade-1"
        );
        let orders = parse_user_orders_page(&json).expect("orders parse");
        let mut input = passing_input();
        input.open_orders = orders;

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.open_order_count, 1);
        assert!(report.block_reasons.contains(&"unexpected_open_orders"));
    }

    #[test]
    fn readback_parses_open_order_sizes_as_fixed_units() {
        let orders = parse_user_orders_page(
            r#"{
                "next_cursor": "",
                "data": [{
                    "id": "order-1",
                    "status": "ORDER_STATUS_LIVE",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "side": "BUY",
                    "original_size": "100000000",
                    "size_matched": "25000000",
                    "price": "0.50",
                    "outcome": "YES",
                    "expiration": "1777434180",
                    "order_type": "GTD",
                    "associate_trades": [],
                    "created_at": 1777434000
                }]
            }"#,
        )
        .expect("orders parse");

        assert_eq!(orders[0].original_size_units, 100_000_000);
        assert_eq!(orders[0].size_matched_units, 25_000_000);
        assert_eq!(
            reserved_pusd_units(&orders).expect("reserved balance calculates"),
            37_500_000
        );
    }

    #[test]
    fn readback_parses_single_order_live_sdk_shape() {
        let order = parse_single_order(
            r#"{
                "id": "order-1",
                "status": "CANCELED",
                "maker_address": "0x1111111111111111111111111111111111111111",
                "market": "condition-1",
                "asset_id": "token-1",
                "side": "BUY",
                "original_size": "5",
                "size_matched": "0",
                "price": "0.01",
                "outcome": "Up",
                "expiration": "1777768020",
                "order_type": "GTD",
                "associate_trades": [],
                "created_at": 1777767400
            }"#,
        )
        .expect("single order parses");

        assert_eq!(order.status, OrderReadbackStatus::Canceled);
        assert_eq!(order.original_size_units, 5_000_000);
        assert_eq!(order.size_matched_units, 0);
        assert_eq!(
            order
                .reserved_pusd_units()
                .expect("reserved balance calculates"),
            50_000
        );
    }

    #[test]
    fn funder_consistency_compares_evm_addresses_case_insensitively() {
        let lower = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        let mixed = "0xABCDEFabcdefABCDEFabcdefABCDEFabcdefABCD";
        let mut input = passing_input();
        input.account.wallet_address = mixed.to_string();
        input.account.funder_address = mixed.to_string();
        let mut order = sample_order("BUY", "0.50", "1000000", "0");
        order.maker_address = lower.to_string();
        input.open_orders = vec![order];
        let mut trade = sample_trade("trade-confirmed", "CONFIRMED", Some(valid_tx_hash()));
        trade.maker_address = lower.to_string();
        input.trades = vec![trade];

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert!(!report.block_reasons.contains(&"funder_mismatch"));
        assert!(!report.block_reasons.contains(&"trade_funder_mismatch"));
        assert!(report.block_reasons.contains(&"unexpected_open_orders"));
    }

    #[test]
    fn readback_trade_lifecycle_blocks_nonterminal_failed_and_missing_hash() {
        let mut input = passing_input();
        input.trades = vec![
            sample_trade("trade-matched", "MATCHED", None),
            sample_trade("trade-failed", "FAILED", Some(valid_tx_hash())),
            sample_trade("trade-confirmed-missing-hash", "CONFIRMED", None),
        ];

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert!(report.block_reasons.contains(&"nonterminal_trade_status"));
        assert!(report.block_reasons.contains(&"failed_trade_status"));
        assert!(report
            .block_reasons
            .contains(&"missing_confirmed_trade_transaction_hash"));
    }

    #[test]
    fn readback_parses_balance_allowance_response() {
        let parsed = parse_balance_allowance(
            r#"{"balance":"25000000","allowance":"1000000"}"#,
            AssetType::Collateral,
            None,
        )
        .expect("balance parses");

        assert_eq!(parsed.balance_units, 25_000_000);
        assert_eq!(parsed.allowance_units, 1_000_000);
    }

    #[test]
    fn readback_parses_plural_allowances_map_fail_closed_to_lowest_allowance() {
        let parsed = parse_balance_allowance(
            r#"{
                "balance":"25000000",
                "allowances":{
                    "0x1111111111111111111111111111111111111111":"3000000",
                    "0x2222222222222222222222222222222222222222":"1000000",
                    "0x3333333333333333333333333333333333333333":"115792089237316195423570985008687907853269984665640564039457584007913129639935"
                }
            }"#,
            AssetType::Collateral,
            None,
        )
        .expect("plural allowances parse");

        assert_eq!(parsed.balance_units, 25_000_000);
        assert_eq!(parsed.allowance_units, 1_000_000);
    }

    #[test]
    fn readback_rejects_missing_allowance_fields() {
        let parsed = parse_balance_allowance(
            r#"{"balance":"25000000","allowances":{}}"#,
            AssetType::Collateral,
            None,
        );

        assert!(matches!(
            parsed,
            Err(LiveBetaReadbackError::Validation(message)) if message == "allowances must not be empty"
        ));
    }

    #[test]
    fn readback_parses_confirmed_trade_with_transaction_hash() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "trade-confirmed",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "status": "TRADE_STATUS_CONFIRMED",
                    "transaction_hash": "{}",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "taker_order_id": "order-taker",
                    "maker_orders": [{{"order_id": "order-maker"}}]
                }}]
            }}"#,
            valid_tx_hash()
        );
        let trades = parse_trades_page(&json).expect("trades parse");

        assert_eq!(trades[0].status, TradeReadbackStatus::Confirmed);
        assert!(trades[0]
            .transaction_hash
            .as_deref()
            .is_some_and(is_valid_tx_hash));
        assert_eq!(trades[0].order_id.as_deref(), Some("order-taker"));
    }

    #[test]
    fn readback_derives_trade_order_id_from_account_maker_order() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "trade-confirmed",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "status": "TRADE_STATUS_CONFIRMED",
                    "transaction_hash": "{}",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "taker_order_id": "counterparty-taker-order",
                    "maker_orders": [
                        {{
                            "order_id": "local-maker-order",
                            "maker_address": "0x1111111111111111111111111111111111111111"
                        }}
                    ]
                }}]
            }}"#,
            valid_tx_hash()
        );
        let page = parse_trades_page_with_cursor_for_account(
            &json,
            "0x1111111111111111111111111111111111111111",
        )
        .expect("trades parse");

        assert_eq!(page.data[0].order_id.as_deref(), Some("local-maker-order"));
    }

    #[test]
    fn readback_uses_taker_order_id_when_account_is_not_maker() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "trade-confirmed",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "status": "TRADE_STATUS_CONFIRMED",
                    "transaction_hash": "{}",
                    "maker_address": "0x2222222222222222222222222222222222222222",
                    "taker_order_id": "local-taker-order",
                    "maker_orders": [
                        {{
                            "order_id": "counterparty-maker-order",
                            "maker_address": "0x2222222222222222222222222222222222222222"
                        }}
                    ]
                }}]
            }}"#,
            valid_tx_hash()
        );
        let page = parse_trades_page_with_cursor_for_account(
            &json,
            "0x1111111111111111111111111111111111111111",
        )
        .expect("trades parse");

        assert_eq!(page.data[0].order_id.as_deref(), Some("local-taker-order"));
    }

    #[test]
    fn readback_trader_side_taker_uses_taker_order_even_when_maker_address_matches_account() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "trade-confirmed",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "status": "TRADE_STATUS_CONFIRMED",
                    "transaction_hash": "{}",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "trader_side": "TAKER",
                    "taker_order_id": "local-taker-order",
                    "maker_orders": [
                        {{
                            "order_id": "counterparty-maker-order",
                            "maker_address": "0x1111111111111111111111111111111111111111"
                        }}
                    ]
                }}]
            }}"#,
            valid_tx_hash()
        );
        let page = parse_trades_page_with_cursor_for_account(
            &json,
            "0x1111111111111111111111111111111111111111",
        )
        .expect("trades parse");

        assert_eq!(page.data[0].order_id.as_deref(), Some("local-taker-order"));
    }

    #[test]
    fn readback_trader_side_maker_does_not_use_counterparty_taker_order() {
        let json = format!(
            r#"{{
                "next_cursor": "",
                "data": [{{
                    "id": "trade-confirmed",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "status": "TRADE_STATUS_CONFIRMED",
                    "transaction_hash": "{}",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "trader_side": "MAKER",
                    "taker_order_id": "counterparty-taker-order",
                    "maker_orders": [
                        {{
                            "order_id": "local-maker-order",
                            "maker_address": "0x1111111111111111111111111111111111111111"
                        }}
                    ]
                }}]
            }}"#,
            valid_tx_hash()
        );
        let page = parse_trades_page_with_cursor_for_account(
            &json,
            "0x1111111111111111111111111111111111111111",
        )
        .expect("trades parse");

        assert_eq!(page.data[0].order_id.as_deref(), Some("local-maker-order"));
    }

    #[test]
    fn readback_order_pages_preserve_next_cursor_for_complete_fetches() {
        let page = parse_user_orders_page_with_cursor(
            r#"{
                "next_cursor": "MTAw",
                "data": [{
                    "id": "order-1",
                    "status": "ORDER_STATUS_LIVE",
                    "maker_address": "0x1111111111111111111111111111111111111111",
                    "market": "condition-1",
                    "asset_id": "token-1",
                    "side": "BUY",
                    "original_size": "1000000",
                    "size_matched": "0",
                    "price": "0.50",
                    "outcome": "YES",
                    "expiration": "1777434180",
                    "order_type": "GTD",
                    "associate_trades": [],
                    "created_at": 1777434000
                }]
            }"#,
        )
        .expect("orders page parses");

        assert_eq!(page.data.len(), 1);
        assert_eq!(
            next_readback_cursor(INITIAL_CURSOR, &page.next_cursor).expect("cursor advances"),
            Some("MTAw".to_string())
        );
        assert_eq!(
            next_readback_cursor("MTAw", "").expect("empty cursor ends pagination"),
            None
        );
        assert_eq!(
            next_readback_cursor("MTAw", END_CURSOR).expect("end cursor ends pagination"),
            None
        );
    }

    #[test]
    fn readback_trade_pagination_can_surface_later_page_blocker() {
        let first_page = parse_trades_page_with_cursor(&format!(
            r#"{{
                    "next_cursor": "MTAw",
                    "data": [{{
                        "id": "trade-confirmed",
                        "market": "condition-1",
                        "asset_id": "token-1",
                        "status": "TRADE_STATUS_CONFIRMED",
                        "transaction_hash": "{}",
                        "maker_address": "0x1111111111111111111111111111111111111111"
                    }}]
                }}"#,
            valid_tx_hash()
        ))
        .expect("first trades page parses");
        let second_page = parse_trades_page_with_cursor(&format!(
            r#"{{
                    "next_cursor": "",
                    "data": [{{
                        "id": "trade-failed",
                        "market": "condition-1",
                        "asset_id": "token-1",
                        "status": "TRADE_STATUS_FAILED",
                        "transaction_hash": "{}",
                        "maker_address": "0x1111111111111111111111111111111111111111"
                    }}]
                }}"#,
            valid_tx_hash()
        ))
        .expect("second trades page parses");
        let mut input = passing_input();
        input.trades = first_page
            .data
            .into_iter()
            .chain(second_page.data)
            .collect();

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.trade_count, 2);
        assert!(report.block_reasons.contains(&"failed_trade_status"));
    }

    #[test]
    fn trade_readback_query_requires_maker_address_filter() {
        let query = trades_query("MTAw", "0x1111111111111111111111111111111111111111");

        assert_eq!(
            query,
            vec![
                ("next_cursor", "MTAw".to_string()),
                (
                    "maker_address",
                    "0x1111111111111111111111111111111111111111".to_string()
                )
            ]
        );
    }

    #[test]
    fn sampling_markets_derive_trading_enabled_from_accepting_orderbook_market() {
        let state = parse_sampling_markets_venue_state(
            r#"{
                "next_cursor": "",
                "data": [{
                    "enable_order_book": true,
                    "active": true,
                    "closed": false,
                    "archived": false,
                    "accepting_orders": true
                }]
            }"#,
        )
        .expect("sampling markets parse");

        assert_eq!(state, VenueState::TradingEnabled);
    }

    #[test]
    fn sampling_markets_fail_closed_when_no_market_accepts_orders() {
        let state = parse_sampling_markets_venue_state(
            r#"{
                "next_cursor": "",
                "data": [{
                    "enable_order_book": true,
                    "active": true,
                    "closed": false,
                    "archived": false,
                    "accepting_orders": false
                }]
            }"#,
        )
        .expect("sampling markets parse");
        let mut input = passing_input();
        input.venue_state = state;

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(state, VenueState::CancelOnly);
        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"venue_state_not_open"));
    }

    #[test]
    fn sampling_markets_pages_can_surface_later_accepting_market() {
        let first_page = parse_sampling_markets_page_with_cursor(
            r#"{
                "next_cursor": "MTAw",
                "data": [{
                    "enable_order_book": true,
                    "active": true,
                    "closed": false,
                    "archived": false,
                    "accepting_orders": false
                }]
            }"#,
        )
        .expect("first sampling page parses");
        let second_page = parse_sampling_markets_page_with_cursor(
            r#"{
                "next_cursor": "",
                "data": [{
                    "enable_order_book": true,
                    "active": true,
                    "closed": false,
                    "archived": false,
                    "accepting_orders": true
                }]
            }"#,
        )
        .expect("second sampling page parses");
        let mut markets = first_page.data;
        markets.extend(second_page.data);

        assert_eq!(
            next_readback_cursor(INITIAL_CURSOR, &first_page.next_cursor)
                .expect("sampling cursor advances"),
            Some("MTAw".to_string())
        );
        assert_eq!(
            derive_venue_state_from_sampling_markets(&markets).expect("venue state derives"),
            VenueState::TradingEnabled
        );
    }

    #[test]
    fn readback_parses_venue_state_and_unknown_states_fail_closed() {
        assert_eq!(
            parse_venue_state(r#"{"state":"trading_enabled"}"#).expect("venue state parses"),
            VenueState::TradingEnabled
        );

        let mut input = passing_input();
        input.venue_state =
            parse_venue_state(r#"{"state":"delayed"}"#).expect("venue state parses");

        let report = evaluate_readback_preflight(&input).expect("report builds");

        assert_eq!(report.status, "blocked");
        assert!(report.block_reasons.contains(&"venue_state_not_open"));
        assert_eq!(
            parse_venue_state(r#"{"state":"mystery"}"#).expect("venue state parses"),
            VenueState::Unknown
        );
    }

    #[test]
    fn readback_error_response_parser_keeps_status_and_redacts_message() {
        let parsed = parse_readback_error_response(
            429,
            r#"{"code":"rate_limited","message":"operator-specific detail"}"#,
        )
        .expect("error parses");

        assert_eq!(parsed.status_code, 429);
        assert_eq!(parsed.code, "rate_limited");
        assert!(parsed.message_redacted);
    }

    #[test]
    fn official_error_field_is_classified_without_preserving_message() {
        let parsed = parse_readback_error_response(
            401,
            r#"{"error":"Unauthorized/Invalid api key for operator account"}"#,
        )
        .expect("error parses");

        assert_eq!(parsed.status_code, 401);
        assert_eq!(parsed.code, "unauthorized");
        assert!(parsed.message_redacted);
        assert!(!parsed.code.contains("api key"));
        assert!(!parsed.code.contains("operator"));
    }

    #[test]
    fn l2_hmac_signature_matches_official_v2_client_fixture() {
        let signature = build_l2_hmac_signature(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            1_000_000,
            "test-sign",
            "/orders",
            Some(r#"{"hash": "0x123"}"#),
        )
        .expect("signature builds");

        assert_eq!(signature, "ZwAdJKvoYRlEKDkNMwd5BuwNNtg93kNaR_oU2HrfVvc=");
    }

    #[test]
    fn readback_path_catalog_has_no_write_paths() {
        let joined = readback_path_catalog().join(",");

        assert!(joined.contains(BALANCE_ALLOWANCE_PATH));
        assert!(joined.contains(USER_ORDERS_PATH));
        assert!(joined.contains(TRADES_PATH));
        assert!(!joined.contains("POST"));
        assert!(!joined.contains("DELETE"));
        assert!(!joined.contains("cancel"));
    }

    fn passing_input() -> ReadbackPreflightInput {
        ReadbackPreflightInput {
            prerequisites: ReadbackPrerequisites {
                lb3_hold_released: true,
                legal_access_approved: true,
                deployment_geoblock_passed: true,
            },
            account: AccountPreflight {
                clob_host: CLOB_HOST.to_string(),
                chain_id: 137,
                wallet_address: "0x1111111111111111111111111111111111111111".to_string(),
                funder_address: "0x1111111111111111111111111111111111111111".to_string(),
                signature_type: SignatureType::Eoa,
            },
            venue_state: VenueState::TradingEnabled,
            collateral: BalanceAllowanceReadback {
                asset_type: AssetType::Collateral,
                token_id: None,
                balance_units: 25_000_000,
                allowance_units: 25_000_000,
            },
            open_orders: Vec::new(),
            trades: Vec::new(),
            heartbeat: HeartbeatReadiness::NotStartedNoOpenOrders,
            required_collateral_allowance_units: 1_000_000,
        }
    }

    fn sample_order(
        side: &str,
        price: &str,
        original_size: &str,
        size_matched: &str,
    ) -> OpenOrderReadback {
        OpenOrderReadback {
            id: "order-1".to_string(),
            status: OrderReadbackStatus::Live,
            maker_address: "0x1111111111111111111111111111111111111111".to_string(),
            market: "condition-1".to_string(),
            asset_id: "token-1".to_string(),
            side: side.to_string(),
            original_size_units: parse_u64_units(original_size, "original_size")
                .expect("original size"),
            size_matched_units: parse_u64_units(size_matched, "size_matched")
                .expect("matched size"),
            price: price.to_string(),
            outcome: "YES".to_string(),
            expiration: "1777434180".to_string(),
            order_type: "GTD".to_string(),
            associate_trades: Vec::new(),
            created_at: 1_777_434_000,
        }
    }

    fn sample_trade(id: &str, status: &str, transaction_hash: Option<String>) -> TradeReadback {
        TradeReadback {
            id: id.to_string(),
            market: "condition-1".to_string(),
            asset_id: "token-1".to_string(),
            status: TradeReadbackStatus::from_wire(status),
            transaction_hash,
            maker_address: "0x1111111111111111111111111111111111111111".to_string(),
            order_id: Some("order-1".to_string()),
        }
    }

    fn valid_tx_hash() -> String {
        format!("0x{}", "1".repeat(64))
    }
}
