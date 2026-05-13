use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::live_trading_journal::{
    AcceptedOrder, BalanceRecord, FillRecord, FreshnessObservation, LiveTradingJournalState,
    PositionRecord, SettlementRecord,
};

pub const MODULE: &str = "live_trading_reconciliation";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingReadbackFixture {
    pub orders: BTreeMap<String, AcceptedOrder>,
    pub trades: BTreeMap<String, FillRecord>,
    pub balance: Option<BalanceRecord>,
    pub positions: BTreeMap<String, PositionRecord>,
    pub settlements: BTreeMap<String, SettlementRecord>,
    pub heartbeat: Option<FreshnessObservation>,
    pub geoblock: Option<FreshnessObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveTradingReconciliationInput {
    pub run_id: String,
    pub checked_at_ms: i64,
    pub local: LiveTradingJournalState,
    pub readback: LiveTradingReadbackFixture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveTradingReconciliationMismatch {
    UnknownVenueOrder,
    AcceptedOrderDrift,
    UnknownTrade,
    MissingAcceptedOrder,
    UnexpectedFill,
    BalanceDrift,
    PositionDrift,
    SettlementMismatch,
    StaleHeartbeat,
    StaleGeoblock,
    UnreviewedIncident,
}

impl LiveTradingReconciliationMismatch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownVenueOrder => "unknown_venue_order",
            Self::AcceptedOrderDrift => "accepted_order_drift",
            Self::UnknownTrade => "unknown_trade",
            Self::MissingAcceptedOrder => "missing_accepted_order",
            Self::UnexpectedFill => "unexpected_fill",
            Self::BalanceDrift => "balance_drift",
            Self::PositionDrift => "position_drift",
            Self::SettlementMismatch => "settlement_mismatch",
            Self::StaleHeartbeat => "stale_heartbeat",
            Self::StaleGeoblock => "stale_geoblock",
            Self::UnreviewedIncident => "unreviewed_incident",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingReconciliationReport {
    pub run_id: String,
    pub checked_at_ms: i64,
    pub status: String,
    pub mismatches: Vec<LiveTradingReconciliationMismatch>,
}

impl LiveTradingReconciliationReport {
    pub fn passed(&self) -> bool {
        self.mismatches.is_empty()
    }

    pub fn mismatch_list(&self) -> String {
        self.mismatches
            .iter()
            .map(|mismatch| mismatch.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn reconcile_live_trading_state(
    input: LiveTradingReconciliationInput,
) -> LiveTradingReconciliationReport {
    let mut mismatches = BTreeSet::new();
    let local_order_ids = input.local.accepted_order_ids();

    for (order_id, readback_order) in &input.readback.orders {
        match input.local.accepted_orders.get(order_id) {
            Some(local_order) if local_order == readback_order => {}
            Some(_) => {
                mismatches.insert(LiveTradingReconciliationMismatch::AcceptedOrderDrift);
            }
            None => {
                mismatches.insert(LiveTradingReconciliationMismatch::UnknownVenueOrder);
            }
        }
    }
    for order_id in &local_order_ids {
        if !input.readback.orders.contains_key(order_id) {
            mismatches.insert(LiveTradingReconciliationMismatch::MissingAcceptedOrder);
        }
    }
    for (trade_id, trade) in &input.readback.trades {
        if !input.local.fills.contains_key(trade_id) {
            mismatches.insert(LiveTradingReconciliationMismatch::UnknownTrade);
        }
        if !local_order_ids.contains(&trade.venue_order_id) {
            mismatches.insert(LiveTradingReconciliationMismatch::UnexpectedFill);
        }
    }
    for (trade_id, fill) in &input.local.fills {
        match input.readback.trades.get(trade_id) {
            Some(readback) if readback == fill => {}
            Some(_) => {
                mismatches.insert(LiveTradingReconciliationMismatch::UnexpectedFill);
            }
            None => {
                mismatches.insert(LiveTradingReconciliationMismatch::UnknownTrade);
            }
        }
    }
    if input.local.latest_balance != input.readback.balance {
        mismatches.insert(LiveTradingReconciliationMismatch::BalanceDrift);
    }
    if input.local.positions != input.readback.positions {
        mismatches.insert(LiveTradingReconciliationMismatch::PositionDrift);
    }
    if input.local.settlements != input.readback.settlements {
        mismatches.insert(LiveTradingReconciliationMismatch::SettlementMismatch);
    }
    if !fresh(
        input
            .readback
            .heartbeat
            .as_ref()
            .or(input.local.latest_heartbeat.as_ref()),
        input.checked_at_ms,
    ) {
        mismatches.insert(LiveTradingReconciliationMismatch::StaleHeartbeat);
    }
    if !fresh(
        input
            .readback
            .geoblock
            .as_ref()
            .or(input.local.latest_geoblock.as_ref()),
        input.checked_at_ms,
    ) {
        mismatches.insert(LiveTradingReconciliationMismatch::StaleGeoblock);
    }
    if input.local.has_unreviewed_incidents() {
        mismatches.insert(LiveTradingReconciliationMismatch::UnreviewedIncident);
    }

    let mismatches: Vec<_> = mismatches.into_iter().collect();
    LiveTradingReconciliationReport {
        run_id: input.run_id,
        checked_at_ms: input.checked_at_ms,
        status: if mismatches.is_empty() {
            "passed".to_string()
        } else {
            "halt_required".to_string()
        },
        mismatches,
    }
}

fn fresh(observation: Option<&FreshnessObservation>, checked_at_ms: i64) -> bool {
    observation.is_some_and(|observation| observation.is_fresh_at(checked_at_ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_trading_journal::{
        reduce_live_trading_journal_events, BalanceRecord, FreshnessObservation,
        LiveTradingJournalEvent, LiveTradingJournalEventKind, PositionRecord, SettlementRecord,
    };

    #[test]
    fn live_trading_reconciliation_passes_matching_fixture() {
        let local = matching_local_state();
        let readback = matching_readback_fixture(&local);

        let report = reconcile_live_trading_state(LiveTradingReconciliationInput {
            run_id: "lt2-recon-pass".to_string(),
            checked_at_ms: 2,
            local,
            readback,
        });

        assert_eq!(report.status, "passed");
        assert!(report.mismatches.is_empty());
    }

    #[test]
    fn live_trading_reconciliation_halts_on_required_fail_closed_fixture() {
        let local = matching_local_state();
        let mut readback = matching_readback_fixture(&local);
        readback.orders.insert(
            "unknown-order".to_string(),
            AcceptedOrder {
                intent_id: "unknown-intent".to_string(),
                venue_order_id: "unknown-order".to_string(),
                status: "ORDER_STATUS_LIVE".to_string(),
            },
        );
        readback.orders.remove("order-1");
        readback.trades.insert(
            "unknown-trade".to_string(),
            FillRecord {
                trade_id: "unknown-trade".to_string(),
                venue_order_id: "unknown-order".to_string(),
                market: "condition-1".to_string(),
                asset_id: "asset-yes".to_string(),
                size_units: 1,
                price_microusd: 1,
                status: "TRADE_STATUS_CONFIRMED".to_string(),
            },
        );
        readback.balance = Some(BalanceRecord {
            available_pusd_units: 1,
            reserved_pusd_units: 999,
        });
        readback.positions.clear();
        readback.settlements.insert(
            "condition-1".to_string(),
            SettlementRecord {
                market: "condition-1".to_string(),
                status: "mismatch".to_string(),
                payout_units: 0,
            },
        );
        readback.heartbeat = Some(FreshnessObservation::stale(2, 30_000));
        readback.geoblock = Some(FreshnessObservation::stale(2, 30_000));

        let report = reconcile_live_trading_state(LiveTradingReconciliationInput {
            run_id: "lt2-recon-halt".to_string(),
            checked_at_ms: 2,
            local,
            readback,
        });

        assert_eq!(report.status, "halt_required");
        for expected in [
            LiveTradingReconciliationMismatch::UnknownVenueOrder,
            LiveTradingReconciliationMismatch::UnknownTrade,
            LiveTradingReconciliationMismatch::MissingAcceptedOrder,
            LiveTradingReconciliationMismatch::UnexpectedFill,
            LiveTradingReconciliationMismatch::BalanceDrift,
            LiveTradingReconciliationMismatch::PositionDrift,
            LiveTradingReconciliationMismatch::SettlementMismatch,
            LiveTradingReconciliationMismatch::StaleHeartbeat,
            LiveTradingReconciliationMismatch::StaleGeoblock,
        ] {
            assert!(report.mismatches.contains(&expected), "{expected:?}");
        }
    }

    #[test]
    fn live_trading_reconciliation_halts_on_accepted_order_drift() {
        let local = matching_local_state();
        let mut readback = matching_readback_fixture(&local);
        readback
            .orders
            .get_mut("order-1")
            .expect("fixture order exists")
            .status = "ORDER_STATUS_CANCELED".to_string();

        let report = reconcile_live_trading_state(LiveTradingReconciliationInput {
            run_id: "lt2-recon-order-drift".to_string(),
            checked_at_ms: 2,
            local,
            readback,
        });

        assert_eq!(report.status, "halt_required");
        assert!(report
            .mismatches
            .contains(&LiveTradingReconciliationMismatch::AcceptedOrderDrift));
    }

    #[test]
    fn live_trading_reconciliation_recomputes_freshness_at_check_time() {
        let local = matching_local_state();
        let readback = matching_readback_fixture(&local);

        let report = reconcile_live_trading_state(LiveTradingReconciliationInput {
            run_id: "lt2-recon-stale-at-check".to_string(),
            checked_at_ms: 30_000,
            local,
            readback,
        });

        assert_eq!(report.status, "halt_required");
        assert!(report
            .mismatches
            .contains(&LiveTradingReconciliationMismatch::StaleHeartbeat));
        assert!(report
            .mismatches
            .contains(&LiveTradingReconciliationMismatch::StaleGeoblock));
    }

    pub(crate) fn matching_local_state() -> LiveTradingJournalState {
        let events = vec![
            event(
                "event-1",
                LiveTradingJournalEventKind::MakerOrderIntended(sample_order()),
            ),
            event(
                "event-2",
                LiveTradingJournalEventKind::AcceptedOrderObserved(sample_accepted_order()),
            ),
            event(
                "event-3",
                LiveTradingJournalEventKind::FillObserved(sample_fill()),
            ),
            event(
                "event-4",
                LiveTradingJournalEventKind::BalanceObserved(sample_balance()),
            ),
            event(
                "event-5",
                LiveTradingJournalEventKind::PositionObserved(sample_position()),
            ),
            event(
                "event-6",
                LiveTradingJournalEventKind::SettlementObserved(sample_settlement()),
            ),
            event(
                "event-7",
                LiveTradingJournalEventKind::HeartbeatObserved(FreshnessObservation::fresh(1)),
            ),
            event(
                "event-8",
                LiveTradingJournalEventKind::GeoblockObserved(FreshnessObservation::fresh(1)),
            ),
        ];
        reduce_live_trading_journal_events(&events).expect("journal reduces")
    }

    pub(crate) fn matching_readback_fixture(
        local: &LiveTradingJournalState,
    ) -> LiveTradingReadbackFixture {
        LiveTradingReadbackFixture {
            orders: local.accepted_orders.clone(),
            trades: local.fills.clone(),
            balance: local.latest_balance.clone(),
            positions: local.positions.clone(),
            settlements: local.settlements.clone(),
            heartbeat: local.latest_heartbeat.clone(),
            geoblock: local.latest_geoblock.clone(),
        }
    }

    fn sample_order() -> crate::live_trading_journal::IntendedMakerOrder {
        crate::live_trading_journal::IntendedMakerOrder {
            intent_id: "intent-1".to_string(),
            market: "condition-1".to_string(),
            asset_id: "asset-yes".to_string(),
            side: "BUY".to_string(),
            price_microusd: 490_000,
            size_units: 1_000_000,
            post_only: true,
        }
    }

    fn sample_accepted_order() -> AcceptedOrder {
        AcceptedOrder {
            intent_id: "intent-1".to_string(),
            venue_order_id: "order-1".to_string(),
            status: "ORDER_STATUS_LIVE".to_string(),
        }
    }

    fn sample_fill() -> FillRecord {
        FillRecord {
            trade_id: "trade-1".to_string(),
            venue_order_id: "order-1".to_string(),
            market: "condition-1".to_string(),
            asset_id: "asset-yes".to_string(),
            size_units: 1_000_000,
            price_microusd: 490_000,
            status: "TRADE_STATUS_CONFIRMED".to_string(),
        }
    }

    fn sample_balance() -> BalanceRecord {
        BalanceRecord {
            available_pusd_units: 10_000_000,
            reserved_pusd_units: 490_000,
        }
    }

    fn sample_position() -> PositionRecord {
        PositionRecord {
            asset_id: "asset-yes".to_string(),
            size_units: 1_000_000,
        }
    }

    fn sample_settlement() -> SettlementRecord {
        SettlementRecord {
            market: "condition-1".to_string(),
            status: "pending".to_string(),
            payout_units: 0,
        }
    }

    fn event(event_id: &str, event: LiveTradingJournalEventKind) -> LiveTradingJournalEvent {
        LiveTradingJournalEvent::new("lt2-test-run", event_id, 1, event)
    }
}
