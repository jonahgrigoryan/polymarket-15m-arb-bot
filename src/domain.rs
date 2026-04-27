use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MODULE: &str = "domain";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Asset {
    #[serde(rename = "BTC")]
    Btc,
    #[serde(rename = "ETH")]
    Eth,
    #[serde(rename = "SOL")]
    Sol,
}

impl Asset {
    pub fn symbol(self) -> &'static str {
        match self {
            Asset::Btc => "BTC",
            Asset::Eth => "ETH",
            Asset::Sol => "SOL",
        }
    }

    pub fn chainlink_resolution_source(self) -> &'static str {
        match self {
            Asset::Btc => "https://data.chain.link/streams/btc-usd",
            Asset::Eth => "https://data.chain.link/streams/eth-usd",
            Asset::Sol => "https://data.chain.link/streams/sol-usd",
        }
    }

    pub fn chainlink_symbol(self) -> &'static str {
        match self {
            Asset::Btc => "btc/usd",
            Asset::Eth => "eth/usd",
            Asset::Sol => "sol/usd",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Asset::Btc => "bitcoin",
            Asset::Eth => "ethereum",
            Asset::Sol => "solana",
        }
    }
}

pub fn is_asset_matched_chainlink_resolution_source(asset: Asset, source: &str) -> bool {
    source
        .trim()
        .trim_end_matches('/')
        .eq_ignore_ascii_case(asset.chainlink_resolution_source())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketLifecycleState {
    Discovered,
    Active,
    Ineligible,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Market {
    pub market_id: String,
    pub slug: String,
    pub title: String,
    pub asset: Asset,
    pub condition_id: String,
    pub outcomes: Vec<OutcomeToken>,
    pub start_ts: i64,
    pub end_ts: i64,
    pub resolution_source: Option<String>,
    pub tick_size: f64,
    pub min_order_size: f64,
    pub fee_parameters: FeeParameters,
    pub lifecycle_state: MarketLifecycleState,
    pub ineligibility_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OutcomeToken {
    pub token_id: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FeeParameters {
    pub fees_enabled: bool,
    pub maker_fee_bps: f64,
    pub taker_fee_bps: f64,
    pub raw_fee_config: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OrderBookSnapshot {
    pub market_id: String,
    pub token_id: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub hash: Option<String>,
    pub source_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ReferencePrice {
    pub asset: Asset,
    pub source: String,
    pub price: f64,
    pub source_ts: Option<i64>,
    pub recv_wall_ts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderKind {
    Maker,
    Taker,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SignalDecision {
    pub asset: Asset,
    pub market_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub order_kind: OrderKind,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub fair_probability: f64,
    pub market_probability: f64,
    pub expected_value_bps: f64,
    pub reason: String,
    pub required_inputs: Vec<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PaperOrderIntent {
    pub asset: Asset,
    pub market_id: String,
    pub token_id: String,
    pub outcome: String,
    pub side: Side,
    pub order_kind: OrderKind,
    pub price: f64,
    pub size: f64,
    pub notional: f64,
    pub fair_probability: f64,
    pub market_probability: f64,
    pub expected_value_bps: f64,
    pub reason: String,
    pub required_inputs: Vec<String>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperOrderStatus {
    Created,
    Open,
    PartiallyFilled,
    Filled,
    Canceled,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PaperOrder {
    pub order_id: String,
    pub market_id: String,
    pub token_id: String,
    pub asset: Asset,
    pub side: Side,
    pub order_kind: OrderKind,
    pub price: f64,
    pub size: f64,
    pub filled_size: f64,
    pub status: PaperOrderStatus,
    pub reason: String,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PaperFill {
    pub fill_id: String,
    pub order_id: String,
    pub market_id: String,
    pub token_id: String,
    pub asset: Asset,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub fee_paid: f64,
    pub liquidity: OrderKind,
    pub filled_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskHaltReason {
    Geoblocked,
    StaleReference,
    StaleBook,
    MaxLossPerMarket,
    MaxNotionalPerMarket,
    MaxNotionalPerAsset,
    MaxTotalNotional,
    MaxCorrelatedNotional,
    OrderRateExceeded,
    DailyDrawdown,
    StorageUnavailable,
    IneligibleMarket,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RiskState {
    pub halted: bool,
    pub active_halts: Vec<RiskHaltReason>,
    pub reason: Option<String>,
    pub updated_ts: i64,
}
