use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_beta_readback";
pub const CLOB_HOST: &str = "https://clob.polymarket.com";
pub const BALANCE_ALLOWANCE_PATH: &str = "/balance-allowance";
pub const USER_ORDERS_PATH: &str = "/data/orders";
pub const TRADES_PATH: &str = "/trades";
pub const SINGLE_ORDER_PATH_PREFIX: &str = "/order/";

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
        match value {
            "ORDER_STATUS_LIVE" => Self::Live,
            "ORDER_STATUS_INVALID" => Self::Invalid,
            "ORDER_STATUS_CANCELED_MARKET_RESOLVED" => Self::CanceledMarketResolved,
            "ORDER_STATUS_CANCELED" => Self::Canceled,
            "ORDER_STATUS_MATCHED" => Self::Matched,
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

pub fn readback_path_catalog() -> Vec<&'static str> {
    vec![
        BALANCE_ALLOWANCE_PATH,
        USER_ORDERS_PATH,
        TRADES_PATH,
        SINGLE_ORDER_PATH_PREFIX,
    ]
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
        balance_units: parse_u64_units(&wire.balance, "balance")?,
        allowance_units: parse_u64_units(&wire.allowance, "allowance")?,
    })
}

pub fn parse_user_orders_page(json: &str) -> LiveBetaReadbackResult<Vec<OpenOrderReadback>> {
    let wire: OpenOrdersPageWire =
        serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    wire.data
        .into_iter()
        .map(OpenOrderReadback::try_from)
        .collect()
}

pub fn parse_trades_page(json: &str) -> LiveBetaReadbackResult<Vec<TradeReadback>> {
    let wire: TradesPageWire = serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    wire.data.into_iter().map(TradeReadback::try_from).collect()
}

pub fn parse_venue_state(json: &str) -> LiveBetaReadbackResult<VenueState> {
    let wire: VenueStateWire = serde_json::from_str(json).map_err(LiveBetaReadbackError::Parse)?;
    Ok(VenueState::from_wire(&wire.state))
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
    balance: String,
    allowance: String,
}

#[derive(Debug, Deserialize)]
struct OpenOrdersPageWire {
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
        Ok(Self {
            id: value.id,
            status: OrderReadbackStatus::from_wire(&value.status),
            maker_address: value.maker_address,
            market: value.market,
            asset_id: value.asset_id,
            side: value.side,
            original_size_units: parse_u64_units(&value.original_size, "original_size")?,
            size_matched_units: parse_u64_units(&value.size_matched, "size_matched")?,
            price: value.price,
            outcome: value.outcome,
            expiration: value.expiration,
            order_type: value.order_type,
            associate_trades: value.associate_trades,
            created_at: value.created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct TradesPageWire {
    data: Vec<TradeWire>,
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
}

#[derive(Debug, Deserialize)]
struct VenueStateWire {
    state: String,
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

impl TryFrom<TradeWire> for TradeReadback {
    type Error = LiveBetaReadbackError;

    fn try_from(value: TradeWire) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            market: value.market,
            asset_id: value.asset_id,
            status: TradeReadbackStatus::from_wire(&value.status),
            transaction_hash: value.transaction_hash,
            maker_address: value.maker_address,
        })
    }
}

fn parse_u64_units(value: &str, field: &'static str) -> LiveBetaReadbackResult<u64> {
    value.parse::<u64>().map_err(|_| {
        LiveBetaReadbackError::Validation(format!("{field} must be unsigned fixed-math units"))
    })
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
}

impl Display for LiveBetaReadbackError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(source) => write!(formatter, "failed to parse LB4 readback JSON: {source}"),
            Self::Validation(message) => {
                write!(formatter, "LB4 readback validation failed: {message}")
            }
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
    fn readback_parses_confirmed_trade_with_transaction_hash() {
        let json = format!(
            r#"{{
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
        );
        let trades = parse_trades_page(&json).expect("trades parse");

        assert_eq!(trades[0].status, TradeReadbackStatus::Confirmed);
        assert!(trades[0]
            .transaction_hash
            .as_deref()
            .is_some_and(is_valid_tx_hash));
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
        }
    }

    fn valid_tx_hash() -> String {
        format!("0x{}", "1".repeat(64))
    }
}
