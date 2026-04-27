use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::domain::{Asset, FeeParameters, OrderKind, PaperFill, Side};
use crate::state::PositionSnapshot;

const FLAT_EPSILON: f64 = 1e-12;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PositionKey {
    pub market_id: String,
    pub token_id: String,
    pub asset: Asset,
}

impl PositionKey {
    pub fn new(market_id: impl Into<String>, token_id: impl Into<String>, asset: Asset) -> Self {
        Self {
            market_id: market_id.into(),
            token_id: token_id.into(),
            asset,
        }
    }

    pub fn from_fill(fill: &PaperFill) -> Self {
        Self::new(fill.market_id.clone(), fill.token_id.clone(), fill.asset)
    }
}

impl Ord for PositionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.market_id
            .cmp(&other.market_id)
            .then_with(|| self.token_id.cmp(&other.token_id))
            .then_with(|| asset_rank(self.asset).cmp(&asset_rank(other.asset)))
    }
}

impl PartialOrd for PositionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaperPosition {
    pub key: PositionKey,
    pub net_size: f64,
    pub average_price: f64,
    pub gross_realized_pnl: f64,
    pub realized_pnl: f64,
    pub fees_paid: f64,
    pub last_mark_price: Option<f64>,
    pub unrealized_pnl: f64,
}

impl PaperPosition {
    fn new(key: PositionKey) -> Self {
        Self {
            key,
            net_size: 0.0,
            average_price: 0.0,
            gross_realized_pnl: 0.0,
            realized_pnl: 0.0,
            fees_paid: 0.0,
            last_mark_price: None,
            unrealized_pnl: 0.0,
        }
    }

    fn exposure_snapshot(&self) -> ExposureSnapshot {
        let exposure_price = self.last_mark_price.unwrap_or(self.average_price);
        let signed_exposure = self.net_size * exposure_price;

        ExposureSnapshot {
            key: self.key.clone(),
            net_size: self.net_size,
            average_price: self.average_price,
            mark_price: self.last_mark_price,
            signed_exposure,
            gross_exposure: signed_exposure.abs(),
            realized_pnl: self.realized_pnl,
            unrealized_pnl: self.unrealized_pnl,
            fees_paid: self.fees_paid,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionUpdate {
    pub key: PositionKey,
    pub fill_id: String,
    pub previous_size: f64,
    pub current_size: f64,
    pub previous_average_price: f64,
    pub current_average_price: f64,
    pub gross_realized_pnl_delta: f64,
    pub realized_pnl_delta: f64,
    pub total_gross_realized_pnl: f64,
    pub total_realized_pnl: f64,
    pub fee_paid: f64,
    pub total_fees_paid: f64,
    pub exposure: ExposureSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExposureSnapshot {
    pub key: PositionKey,
    pub net_size: f64,
    pub average_price: f64,
    pub mark_price: Option<f64>,
    pub signed_exposure: f64,
    pub gross_exposure: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub fees_paid: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketSettlementOutcome {
    WinningToken(String),
    Split,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketSettlement {
    pub market_id: String,
    pub outcome: MarketSettlementOutcome,
    pub source: String,
    pub settled_ts: i64,
}

impl MarketSettlement {
    pub fn winning_token(
        market_id: impl Into<String>,
        winning_token_id: impl Into<String>,
        source: impl Into<String>,
        settled_ts: i64,
    ) -> Self {
        Self {
            market_id: market_id.into(),
            outcome: MarketSettlementOutcome::WinningToken(winning_token_id.into()),
            source: source.into(),
            settled_ts,
        }
    }

    pub fn split(market_id: impl Into<String>, source: impl Into<String>, settled_ts: i64) -> Self {
        Self {
            market_id: market_id.into(),
            outcome: MarketSettlementOutcome::Split,
            source: source.into(),
            settled_ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettlementUpdate {
    pub key: PositionKey,
    pub settlement_price: f64,
    pub source: String,
    pub settled_ts: i64,
    pub exposure: ExposureSnapshot,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PaperPositionBook {
    positions: BTreeMap<PositionKey, PaperPosition>,
}

impl PaperPositionBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_fill(&mut self, fill: &PaperFill) -> PositionUpdate {
        let key = PositionKey::from_fill(fill);
        let position = self
            .positions
            .entry(key.clone())
            .or_insert_with(|| PaperPosition::new(key.clone()));

        let previous_size = position.net_size;
        let previous_average_price = position.average_price;
        let signed_fill_size = signed_size(fill.side, fill.size);
        let (current_size, current_average_price, gross_realized_pnl_delta) = apply_fill_math(
            previous_size,
            previous_average_price,
            signed_fill_size,
            fill.price,
        );
        let fee_paid = fill.fee_paid;
        let realized_pnl_delta = gross_realized_pnl_delta - fee_paid;

        position.net_size = normalize_flat(current_size);
        position.average_price = if is_flat(position.net_size) {
            0.0
        } else {
            current_average_price
        };
        position.gross_realized_pnl += gross_realized_pnl_delta;
        position.realized_pnl += realized_pnl_delta;
        position.fees_paid += fee_paid;
        position.unrealized_pnl = position
            .last_mark_price
            .map(|mark_price| mark_position(position.net_size, position.average_price, mark_price))
            .unwrap_or(0.0);

        PositionUpdate {
            key,
            fill_id: fill.fill_id.clone(),
            previous_size,
            current_size: position.net_size,
            previous_average_price,
            current_average_price: position.average_price,
            gross_realized_pnl_delta,
            realized_pnl_delta,
            total_gross_realized_pnl: position.gross_realized_pnl,
            total_realized_pnl: position.realized_pnl,
            fee_paid,
            total_fees_paid: position.fees_paid,
            exposure: position.exposure_snapshot(),
        }
    }

    pub fn mark(&mut self, key: &PositionKey, mark_price: f64) -> Option<ExposureSnapshot> {
        let position = self.positions.get_mut(key)?;
        position.last_mark_price = Some(mark_price);
        position.unrealized_pnl =
            mark_position(position.net_size, position.average_price, mark_price);
        Some(position.exposure_snapshot())
    }

    pub fn settle_market(&mut self, settlement: &MarketSettlement) -> Vec<SettlementUpdate> {
        let keys = self
            .positions
            .keys()
            .filter(|key| key.market_id == settlement.market_id)
            .cloned()
            .collect::<Vec<_>>();

        keys.into_iter()
            .filter_map(|key| {
                let settlement_price = settlement_price(&key, &settlement.outcome);
                let exposure = self.mark(&key, settlement_price)?;
                Some(SettlementUpdate {
                    key,
                    settlement_price,
                    source: settlement.source.clone(),
                    settled_ts: settlement.settled_ts,
                    exposure,
                })
            })
            .collect()
    }

    pub fn position(&self, key: &PositionKey) -> Option<&PaperPosition> {
        self.positions.get(key)
    }

    pub fn exposure_snapshots(&self) -> Vec<ExposureSnapshot> {
        self.positions
            .values()
            .filter(|position| !is_flat(position.net_size))
            .map(PaperPosition::exposure_snapshot)
            .collect()
    }

    pub fn position_snapshots(&self, updated_ts: i64) -> Vec<PositionSnapshot> {
        self.positions
            .values()
            .map(|position| PositionSnapshot {
                market_id: position.key.market_id.clone(),
                token_id: position.key.token_id.clone(),
                asset: position.key.asset,
                size: position.net_size,
                average_price: position.average_price,
                realized_pnl: position.realized_pnl,
                unrealized_pnl: position.unrealized_pnl,
                updated_ts,
            })
            .collect()
    }

    pub fn total_realized_pnl(&self) -> f64 {
        self.positions
            .values()
            .map(|position| position.realized_pnl)
            .sum()
    }

    pub fn total_unrealized_pnl(&self) -> f64 {
        self.positions
            .values()
            .map(|position| position.unrealized_pnl)
            .sum()
    }

    pub fn total_fees_paid(&self) -> f64 {
        self.positions
            .values()
            .map(|position| position.fees_paid)
            .sum()
    }
}

pub fn fee_paid(
    fill_size: f64,
    fill_price: f64,
    liquidity: OrderKind,
    fee_parameters: &FeeParameters,
) -> f64 {
    if !fee_parameters.fees_enabled || fill_size <= 0.0 || !fill_size.is_finite() {
        return 0.0;
    }

    match liquidity {
        OrderKind::Maker => 0.0,
        OrderKind::Taker => raw_fee_rate(fee_parameters)
            .map(|fee_rate| fill_size * fee_rate * fill_price * (1.0 - fill_price))
            .unwrap_or_else(|| fill_size * fee_parameters.taker_fee_bps / 10_000.0),
    }
}

pub fn mark_position(net_size: f64, average_price: f64, mark_price: f64) -> f64 {
    net_size * (mark_price - average_price)
}

fn apply_fill_math(
    previous_size: f64,
    previous_average_price: f64,
    signed_fill_size: f64,
    fill_price: f64,
) -> (f64, f64, f64) {
    if is_flat(previous_size) || previous_size.signum() == signed_fill_size.signum() {
        let current_size = previous_size + signed_fill_size;
        let current_average_price = weighted_average_price(
            previous_size.abs(),
            previous_average_price,
            signed_fill_size.abs(),
            fill_price,
        );
        return (current_size, current_average_price, 0.0);
    }

    let closed_size = previous_size.abs().min(signed_fill_size.abs());
    let gross_realized_pnl_delta =
        previous_size.signum() * closed_size * (fill_price - previous_average_price);
    let current_size = previous_size + signed_fill_size;
    let current_average_price = if is_flat(current_size) {
        0.0
    } else if previous_size.signum() == current_size.signum() {
        previous_average_price
    } else {
        fill_price
    };

    (
        current_size,
        current_average_price,
        gross_realized_pnl_delta,
    )
}

fn weighted_average_price(
    previous_abs_size: f64,
    previous_average_price: f64,
    fill_abs_size: f64,
    fill_price: f64,
) -> f64 {
    let total_size = previous_abs_size + fill_abs_size;
    if is_flat(total_size) {
        0.0
    } else {
        ((previous_abs_size * previous_average_price) + (fill_abs_size * fill_price)) / total_size
    }
}

fn raw_fee_rate(fee_parameters: &FeeParameters) -> Option<f64> {
    let raw = fee_parameters.raw_fee_config.as_ref()?;
    let rate = raw
        .get("r")
        .or_else(|| raw.get("rate"))
        .and_then(|value| value.as_f64())?;
    if rate.is_finite() && rate >= 0.0 {
        Some(rate)
    } else {
        None
    }
}

fn settlement_price(key: &PositionKey, outcome: &MarketSettlementOutcome) -> f64 {
    match outcome {
        MarketSettlementOutcome::WinningToken(token_id) if token_id == &key.token_id => 1.0,
        MarketSettlementOutcome::WinningToken(_) => 0.0,
        MarketSettlementOutcome::Split => 0.5,
    }
}

fn signed_size(side: Side, size: f64) -> f64 {
    match side {
        Side::Buy => size,
        Side::Sell => -size,
    }
}

fn normalize_flat(value: f64) -> f64 {
    if is_flat(value) {
        0.0
    } else {
        value
    }
}

fn is_flat(value: f64) -> bool {
    value.abs() <= FLAT_EPSILON
}

fn asset_rank(asset: Asset) -> u8 {
    match asset {
        Asset::Btc => 0,
        Asset::Eth => 1,
        Asset::Sol => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKET_ID: &str = "btc-updown-15m";
    const TOKEN_ID: &str = "token-up";

    #[test]
    fn taker_fee_uses_raw_fee_formula_and_maker_is_zero() {
        let fee_parameters = FeeParameters {
            fees_enabled: true,
            maker_fee_bps: 1_000.0,
            taker_fee_bps: 1_000.0,
            raw_fee_config: Some(serde_json::json!({"r": 0.072, "e": 1, "to": true})),
        };

        assert_close(
            fee_paid(10.0, 0.50, OrderKind::Taker, &fee_parameters),
            0.18,
        );
        assert_close(fee_paid(10.0, 0.50, OrderKind::Maker, &fee_parameters), 0.0);
    }

    #[test]
    fn position_updates_weighted_average_price() {
        let mut book = PaperPositionBook::new();
        let first = fill("fill-1", Side::Buy, 0.40, 10.0, 0.0);
        let second = fill("fill-2", Side::Buy, 0.60, 10.0, 0.0);

        book.apply_fill(&first);
        let update = book.apply_fill(&second);

        assert_close(update.current_size, 20.0);
        assert_close(update.current_average_price, 0.50);
        assert_close(update.total_realized_pnl, 0.0);
    }

    #[test]
    fn position_updates_realized_pnl_on_reduction() {
        let mut book = PaperPositionBook::new();
        book.apply_fill(&fill("fill-1", Side::Buy, 0.40, 10.0, 0.0));
        let update = book.apply_fill(&fill("fill-2", Side::Sell, 0.70, 4.0, 0.0));

        assert_close(update.current_size, 6.0);
        assert_close(update.current_average_price, 0.40);
        assert_close(update.gross_realized_pnl_delta, 1.20);
        assert_close(update.total_realized_pnl, 1.20);
    }

    #[test]
    fn unrealized_pnl_marks_long_and_short_positions() {
        assert_close(mark_position(10.0, 0.40, 0.55), 1.50);
        assert_close(mark_position(-10.0, 0.60, 0.45), 1.50);

        let mut book = PaperPositionBook::new();
        let key = PositionKey::new(MARKET_ID, TOKEN_ID, Asset::Btc);
        book.apply_fill(&fill("fill-1", Side::Buy, 0.40, 10.0, 0.0));
        let exposure = book.mark(&key, 0.55).expect("position exists");

        assert_close(exposure.unrealized_pnl, 1.50);
        assert_close(book.total_unrealized_pnl(), 1.50);
    }

    #[test]
    fn repeated_fill_sequence_is_deterministic() {
        let fills = vec![
            fill("fill-1", Side::Buy, 0.40, 10.0, 0.01),
            fill("fill-2", Side::Buy, 0.50, 5.0, 0.02),
            fill("fill-3", Side::Sell, 0.70, 8.0, 0.03),
        ];

        let (first_updates, first_book) = replay(&fills);
        let (second_updates, second_book) = replay(&fills);

        assert_eq!(first_updates, second_updates);
        assert_eq!(
            first_book.exposure_snapshots(),
            second_book.exposure_snapshots()
        );
        assert_close(first_book.total_fees_paid(), 0.06);
        assert_close(first_book.total_realized_pnl(), 2.073_333_333_333_333_3);
    }

    #[test]
    fn settlement_marks_winning_and_losing_tokens_to_market_outcome() {
        let mut book = PaperPositionBook::new();
        let down_token = "token-down";
        book.apply_fill(&fill("fill-up", Side::Buy, 0.40, 10.0, 0.0));
        book.apply_fill(&PaperFill {
            token_id: down_token.to_string(),
            ..fill("fill-down", Side::Buy, 0.60, 5.0, 0.0)
        });

        let updates = book.settle_market(&MarketSettlement::winning_token(
            MARKET_ID,
            TOKEN_ID,
            "gamma-final-outcome",
            1_777_000_900_000,
        ));

        assert_eq!(updates.len(), 2);
        let up = book
            .position(&PositionKey::new(MARKET_ID, TOKEN_ID, Asset::Btc))
            .expect("up position exists");
        let down = book
            .position(&PositionKey::new(MARKET_ID, down_token, Asset::Btc))
            .expect("down position exists");
        assert_close(up.unrealized_pnl, 6.0);
        assert_close(down.unrealized_pnl, -3.0);
        assert_close(book.total_unrealized_pnl(), 3.0);
    }

    #[test]
    fn split_settlement_marks_each_token_to_half_value() {
        let mut book = PaperPositionBook::new();
        book.apply_fill(&fill("fill-1", Side::Buy, 0.40, 10.0, 0.0));

        let updates = book.settle_market(&MarketSettlement::split(
            MARKET_ID,
            "uma-split-resolution",
            1_777_000_900_000,
        ));

        assert_eq!(updates.len(), 1);
        assert_close(updates[0].settlement_price, 0.5);
        assert_close(updates[0].exposure.unrealized_pnl, 1.0);
    }

    #[test]
    fn position_snapshots_are_storage_ready_and_deterministic() {
        let mut book = PaperPositionBook::new();
        book.apply_fill(&fill("fill-1", Side::Buy, 0.40, 10.0, 0.0));
        book.apply_fill(&PaperFill {
            token_id: "token-down".to_string(),
            ..fill("fill-2", Side::Buy, 0.60, 5.0, 0.0)
        });

        let snapshots = book.position_snapshots(1_777_000_900_000);

        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].token_id, "token-down");
        assert_eq!(snapshots[1].token_id, TOKEN_ID);
        assert_eq!(snapshots[0].updated_ts, 1_777_000_900_000);
    }

    fn replay(fills: &[PaperFill]) -> (Vec<PositionUpdate>, PaperPositionBook) {
        let mut book = PaperPositionBook::new();
        let updates = fills.iter().map(|fill| book.apply_fill(fill)).collect();
        (updates, book)
    }

    fn fill(fill_id: &str, side: Side, price: f64, size: f64, fee_paid: f64) -> PaperFill {
        PaperFill {
            fill_id: fill_id.to_string(),
            order_id: format!("order-{fill_id}"),
            market_id: MARKET_ID.to_string(),
            token_id: TOKEN_ID.to_string(),
            asset: Asset::Btc,
            side,
            price,
            size,
            fee_paid,
            liquidity: OrderKind::Maker,
            filled_ts: 1_777_000_000_000,
        }
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-9,
            "actual={actual} expected={expected}"
        );
    }
}
