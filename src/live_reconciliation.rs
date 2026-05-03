use std::collections::{BTreeMap, BTreeSet};

use crate::live_balance_tracker::LiveBalanceSnapshot;
use crate::live_order_journal::LiveJournalState;
use crate::live_position_book::LivePositionBook;

pub const MODULE: &str = "live_reconciliation";

#[derive(Debug, Clone, PartialEq)]
pub struct LocalLiveState {
    pub known_orders: BTreeSet<String>,
    pub canceled_orders: BTreeSet<String>,
    pub partially_filled_orders: BTreeSet<String>,
    pub known_trades: BTreeSet<String>,
    pub trade_order_ids: BTreeSet<String>,
    pub balance: Option<LiveBalanceSnapshot>,
    pub positions: LivePositionBook,
    pub rust_readback_fingerprint: Option<String>,
    pub sdk_readback_fingerprint: Option<String>,
}

impl Default for LocalLiveState {
    fn default() -> Self {
        Self {
            known_orders: BTreeSet::new(),
            canceled_orders: BTreeSet::new(),
            partially_filled_orders: BTreeSet::new(),
            known_trades: BTreeSet::new(),
            trade_order_ids: BTreeSet::new(),
            balance: None,
            positions: LivePositionBook::new(),
            rust_readback_fingerprint: None,
            sdk_readback_fingerprint: None,
        }
    }
}

impl From<&LiveJournalState> for LocalLiveState {
    fn from(state: &LiveJournalState) -> Self {
        Self {
            known_orders: state.orders.keys().cloned().collect(),
            canceled_orders: state.canceled_orders.clone(),
            partially_filled_orders: state.partially_filled_orders.clone(),
            known_trades: state.trades.clone(),
            trade_order_ids: state.trade_order_ids.clone(),
            balance: state.balance_tracker.latest().cloned(),
            positions: state.position_book.clone(),
            rust_readback_fingerprint: None,
            sdk_readback_fingerprint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VenueLiveState {
    pub orders: BTreeMap<String, VenueOrderState>,
    pub trades: BTreeMap<String, VenueTradeState>,
    pub balance: Option<LiveBalanceSnapshot>,
    pub positions: LivePositionBook,
    pub rust_readback_fingerprint: Option<String>,
    pub sdk_readback_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VenueOrderState {
    pub order_id: String,
    pub status: VenueOrderStatus,
    pub matched_size: f64,
    pub remaining_size: f64,
}

impl VenueOrderState {
    pub fn is_open(&self) -> bool {
        matches!(
            self.status,
            VenueOrderStatus::Live | VenueOrderStatus::PartiallyFilled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueOrderStatus {
    Live,
    Canceled,
    Filled,
    PartiallyFilled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VenueTradeState {
    pub trade_id: String,
    pub order_id: String,
    pub status: VenueTradeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueTradeStatus {
    Matched,
    Mined,
    Confirmed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveReconciliationInput {
    pub run_id: String,
    pub checked_at_ms: i64,
    pub local: LocalLiveState,
    pub venue: VenueLiveState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiveReconciliationMismatch {
    UnknownOpenOrder,
    MissingVenueOrder,
    UnknownVenueOrderStatus,
    UnexpectedFill,
    UnexpectedPartialFill,
    CancelNotConfirmed,
    ReservedBalanceMismatch,
    BalanceDeltaMismatch,
    PositionMismatch,
    UnknownVenueTradeStatus,
    TradeStatusFailed,
    SdkRustDisagreement,
}

impl LiveReconciliationMismatch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownOpenOrder => "unknown_open_order",
            Self::MissingVenueOrder => "missing_venue_order",
            Self::UnknownVenueOrderStatus => "unknown_venue_order_status",
            Self::UnexpectedFill => "unexpected_fill",
            Self::UnexpectedPartialFill => "unexpected_partial_fill",
            Self::CancelNotConfirmed => "cancel_not_confirmed",
            Self::ReservedBalanceMismatch => "reserved_balance_mismatch",
            Self::BalanceDeltaMismatch => "balance_delta_mismatch",
            Self::PositionMismatch => "position_mismatch",
            Self::UnknownVenueTradeStatus => "unknown_venue_trade_status",
            Self::TradeStatusFailed => "trade_status_failed",
            Self::SdkRustDisagreement => "sdk_rust_disagreement",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveReconciliationResult {
    Passed {
        run_id: String,
        checked_at_ms: i64,
    },
    HaltRequired {
        run_id: String,
        checked_at_ms: i64,
        mismatches: Vec<LiveReconciliationMismatch>,
    },
}

impl LiveReconciliationResult {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Passed { .. } => "passed",
            Self::HaltRequired { .. } => "halt_required",
        }
    }

    pub fn mismatches(&self) -> &[LiveReconciliationMismatch] {
        match self {
            Self::Passed { .. } => &[],
            Self::HaltRequired { mismatches, .. } => mismatches,
        }
    }

    pub fn mismatch_list(&self) -> String {
        self.mismatches()
            .iter()
            .map(|mismatch| mismatch.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn reconcile_live_state(input: LiveReconciliationInput) -> LiveReconciliationResult {
    let mut mismatches = BTreeSet::new();

    for order in input.venue.orders.values().filter(|order| order.is_open()) {
        if !input.local.known_orders.contains(&order.order_id) {
            mismatches.insert(LiveReconciliationMismatch::UnknownOpenOrder);
        }
    }
    for order_id in &input.local.known_orders {
        if !input.venue.orders.contains_key(order_id) {
            mismatches.insert(LiveReconciliationMismatch::MissingVenueOrder);
        }
    }
    for order_id in &input.local.canceled_orders {
        if input
            .venue
            .orders
            .get(order_id)
            .is_none_or(|order| order.status != VenueOrderStatus::Canceled)
        {
            mismatches.insert(LiveReconciliationMismatch::CancelNotConfirmed);
        }
    }
    for order in input.venue.orders.values() {
        if order.status == VenueOrderStatus::Unknown {
            mismatches.insert(LiveReconciliationMismatch::UnknownVenueOrderStatus);
        }
        if order.status == VenueOrderStatus::PartiallyFilled
            && !input
                .local
                .partially_filled_orders
                .contains(&order.order_id)
        {
            mismatches.insert(LiveReconciliationMismatch::UnexpectedPartialFill);
        }
        if order.status == VenueOrderStatus::Filled
            && !input.local.trade_order_ids.contains(&order.order_id)
        {
            mismatches.insert(LiveReconciliationMismatch::UnexpectedFill);
        }
    }
    for trade in input.venue.trades.values() {
        if !input.local.known_trades.contains(&trade.trade_id) {
            mismatches.insert(LiveReconciliationMismatch::UnexpectedFill);
        }
        if trade.status == VenueTradeStatus::Unknown {
            mismatches.insert(LiveReconciliationMismatch::UnknownVenueTradeStatus);
        }
        if trade.status == VenueTradeStatus::Failed {
            mismatches.insert(LiveReconciliationMismatch::TradeStatusFailed);
        }
    }
    match (&input.local.balance, &input.venue.balance) {
        (Some(local), Some(venue)) => {
            if (local.p_usd_reserved - venue.p_usd_reserved).abs()
                > crate::live_balance_tracker::BALANCE_EPSILON
            {
                mismatches.insert(LiveReconciliationMismatch::ReservedBalanceMismatch);
            }
            if (local.p_usd_available - venue.p_usd_available).abs()
                > crate::live_balance_tracker::BALANCE_EPSILON
                || (local.p_usd_total - venue.p_usd_total).abs()
                    > crate::live_balance_tracker::BALANCE_EPSILON
            {
                mismatches.insert(LiveReconciliationMismatch::BalanceDeltaMismatch);
            }
        }
        (Some(_), None) | (None, Some(_)) => {
            mismatches.insert(LiveReconciliationMismatch::BalanceDeltaMismatch);
        }
        (None, None) => {}
    }
    if !input.local.positions.matches(&input.venue.positions) {
        mismatches.insert(LiveReconciliationMismatch::PositionMismatch);
    }
    if readback_fingerprints_disagree(&input.local, &input.venue) {
        mismatches.insert(LiveReconciliationMismatch::SdkRustDisagreement);
    }

    if mismatches.is_empty() {
        LiveReconciliationResult::Passed {
            run_id: input.run_id,
            checked_at_ms: input.checked_at_ms,
        }
    } else {
        LiveReconciliationResult::HaltRequired {
            run_id: input.run_id,
            checked_at_ms: input.checked_at_ms,
            mismatches: mismatches.into_iter().collect(),
        }
    }
}

fn readback_fingerprints_disagree(local: &LocalLiveState, venue: &VenueLiveState) -> bool {
    readback_fingerprint_pair_disagrees(
        local.rust_readback_fingerprint.as_deref(),
        local.sdk_readback_fingerprint.as_deref(),
    ) || readback_fingerprint_pair_disagrees(
        venue.rust_readback_fingerprint.as_deref(),
        venue.sdk_readback_fingerprint.as_deref(),
    )
}

fn readback_fingerprint_pair_disagrees(rust: Option<&str>, sdk: Option<&str>) -> bool {
    match (rust, sdk) {
        (Some(rust), Some(sdk)) => rust != sdk,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Asset, Side};
    use crate::live_balance_tracker::LiveBalanceSnapshot;
    use crate::live_position_book::LivePositionKey;

    #[test]
    fn live_reconciliation_passes_when_local_and_venue_state_match() {
        let result = reconcile_live_state(matching_input());

        assert_eq!(result.status(), "passed");
        assert!(result.mismatches().is_empty());
    }

    #[test]
    fn live_reconciliation_unknown_open_order_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.orders.insert(
            "unknown-order".to_string(),
            VenueOrderState {
                order_id: "unknown-order".to_string(),
                status: VenueOrderStatus::Live,
                matched_size: 0.0,
                remaining_size: 5.0,
            },
        );

        assert_mismatch(input, LiveReconciliationMismatch::UnknownOpenOrder);
    }

    #[test]
    fn live_reconciliation_missing_venue_order_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.orders.clear();

        assert_mismatch(input, LiveReconciliationMismatch::MissingVenueOrder);
    }

    #[test]
    fn live_reconciliation_unknown_venue_order_status_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.orders.get_mut("order-1").expect("order").status = VenueOrderStatus::Unknown;

        assert_mismatch(input, LiveReconciliationMismatch::UnknownVenueOrderStatus);
    }

    #[test]
    fn live_reconciliation_unexpected_fill_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.trades.insert(
            "trade-2".to_string(),
            VenueTradeState {
                trade_id: "trade-2".to_string(),
                order_id: "order-1".to_string(),
                status: VenueTradeStatus::Confirmed,
            },
        );

        assert_mismatch(input, LiveReconciliationMismatch::UnexpectedFill);
    }

    #[test]
    fn live_reconciliation_filled_order_requires_matching_local_trade_order() {
        let mut input = matching_input();
        input.local.known_orders.insert("order-2".to_string());
        input.venue.orders.insert(
            "order-2".to_string(),
            VenueOrderState {
                order_id: "order-2".to_string(),
                status: VenueOrderStatus::Filled,
                matched_size: 5.0,
                remaining_size: 0.0,
            },
        );

        assert_mismatch(input, LiveReconciliationMismatch::UnexpectedFill);
    }

    #[test]
    fn live_reconciliation_unexpected_partial_fill_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.orders.get_mut("order-1").expect("order").status =
            VenueOrderStatus::PartiallyFilled;

        assert_mismatch(input, LiveReconciliationMismatch::UnexpectedPartialFill);
    }

    #[test]
    fn live_reconciliation_cancel_not_confirmed_halts_fail_closed() {
        let mut input = matching_input();
        input.local.canceled_orders.insert("order-1".to_string());
        input.venue.orders.get_mut("order-1").expect("order").status = VenueOrderStatus::Live;

        assert_mismatch(input, LiveReconciliationMismatch::CancelNotConfirmed);
    }

    #[test]
    fn live_reconciliation_reserved_balance_mismatch_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.balance = Some(balance(10.0, 1.0, 11.0));

        assert_mismatch(input, LiveReconciliationMismatch::ReservedBalanceMismatch);
    }

    #[test]
    fn live_reconciliation_balance_delta_mismatch_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.balance = Some(balance(9.0, 0.0, 9.0));

        assert_mismatch(input, LiveReconciliationMismatch::BalanceDeltaMismatch);
    }

    #[test]
    fn live_reconciliation_position_mismatch_halts_fail_closed() {
        let mut input = matching_input();
        input
            .venue
            .positions
            .apply_fill(position_key(), Side::Buy, 0.42, 1.0, 0.0, 1)
            .expect("venue position applies");

        assert_mismatch(input, LiveReconciliationMismatch::PositionMismatch);
    }

    #[test]
    fn live_reconciliation_trade_status_failed_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.trades.insert(
            "trade-1".to_string(),
            VenueTradeState {
                trade_id: "trade-1".to_string(),
                order_id: "order-1".to_string(),
                status: VenueTradeStatus::Failed,
            },
        );

        assert_mismatch(input, LiveReconciliationMismatch::TradeStatusFailed);
    }

    #[test]
    fn live_reconciliation_unknown_venue_trade_status_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.trades.insert(
            "trade-1".to_string(),
            VenueTradeState {
                trade_id: "trade-1".to_string(),
                order_id: "order-1".to_string(),
                status: VenueTradeStatus::Unknown,
            },
        );

        assert_mismatch(input, LiveReconciliationMismatch::UnknownVenueTradeStatus);
    }

    #[test]
    fn live_reconciliation_sdk_rust_disagreement_halts_fail_closed() {
        let mut input = matching_input();
        input.venue.rust_readback_fingerprint = Some("rust-a".to_string());
        input.venue.sdk_readback_fingerprint = Some("sdk-b".to_string());

        assert_mismatch(input, LiveReconciliationMismatch::SdkRustDisagreement);
    }

    #[test]
    fn live_reconciliation_readback_fingerprints_do_not_mix_sources() {
        let mut input = matching_input();
        input.local.rust_readback_fingerprint = Some("rust-a".to_string());
        input.venue.sdk_readback_fingerprint = Some("sdk-b".to_string());

        let result = reconcile_live_state(input);

        assert_eq!(result.status(), "passed");
        assert!(result.mismatches().is_empty());
    }

    fn assert_mismatch(input: LiveReconciliationInput, expected: LiveReconciliationMismatch) {
        let result = reconcile_live_state(input);
        assert_eq!(result.status(), "halt_required");
        assert!(
            result.mismatches().contains(&expected),
            "missing {expected:?}; got {}",
            result.mismatch_list()
        );
    }

    fn matching_input() -> LiveReconciliationInput {
        let mut local = LocalLiveState::default();
        local.known_orders.insert("order-1".to_string());
        local.known_trades.insert("trade-1".to_string());
        local.trade_order_ids.insert("order-1".to_string());
        local.balance = Some(balance(10.0, 0.0, 10.0));
        local
            .positions
            .apply_fill(position_key(), Side::Buy, 0.42, 5.0, 0.0, 1)
            .expect("local position applies");

        let mut venue = VenueLiveState {
            balance: Some(balance(10.0, 0.0, 10.0)),
            ..VenueLiveState::default()
        };
        venue.orders.insert(
            "order-1".to_string(),
            VenueOrderState {
                order_id: "order-1".to_string(),
                status: VenueOrderStatus::Live,
                matched_size: 0.0,
                remaining_size: 5.0,
            },
        );
        venue.trades.insert(
            "trade-1".to_string(),
            VenueTradeState {
                trade_id: "trade-1".to_string(),
                order_id: "order-1".to_string(),
                status: VenueTradeStatus::Confirmed,
            },
        );
        venue
            .positions
            .apply_fill(position_key(), Side::Buy, 0.42, 5.0, 0.0, 1)
            .expect("venue position applies");

        LiveReconciliationInput {
            run_id: "run-1".to_string(),
            checked_at_ms: 1,
            local,
            venue,
        }
    }

    fn balance(available: f64, reserved: f64, total: f64) -> LiveBalanceSnapshot {
        LiveBalanceSnapshot {
            p_usd_available: available,
            p_usd_reserved: reserved,
            p_usd_total: total,
            conditional_token_positions: BTreeMap::new(),
            balance_snapshot_at: 1,
            source: "fixture".to_string(),
        }
    }

    fn position_key() -> crate::live_position_book::LivePositionKey {
        LivePositionKey {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            outcome: "Up".to_string(),
        }
    }
}
