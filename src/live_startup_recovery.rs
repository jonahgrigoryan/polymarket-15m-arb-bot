use std::collections::BTreeMap;

use crate::live_alpha_config::LiveAlphaMode;
use crate::live_balance_tracker::LiveBalanceSnapshot;
use crate::live_beta_readback::{
    OpenOrderReadback, OrderReadbackStatus, TradeReadback, TradeReadbackStatus,
};
use crate::live_order_journal::LiveJournalEventType;
use crate::live_position_book::LivePositionBook;
use crate::live_reconciliation::{
    reconcile_live_state, LiveReconciliationInput, LiveReconciliationMismatch,
    LiveReconciliationResult, VenueLiveState, VenueOrderState, VenueOrderStatus, VenueTradeState,
    VenueTradeStatus,
};
use crate::safety::GeoblockGateStatus;

pub const MODULE: &str = "live_startup_recovery";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupRecoveryCheckStatus {
    Passed,
    Failed,
    Unknown,
}

impl StartupRecoveryCheckStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveStartupRecoveryInput {
    pub run_id: String,
    pub checked_at_ms: i64,
    pub live_alpha_enabled: bool,
    pub live_alpha_mode: LiveAlphaMode,
    pub geoblock_status: GeoblockGateStatus,
    pub account_preflight_status: StartupRecoveryCheckStatus,
    pub balance_allowance_status: StartupRecoveryCheckStatus,
    pub open_orders_readback_status: StartupRecoveryCheckStatus,
    pub recent_trades_readback_status: StartupRecoveryCheckStatus,
    pub journal_replay_status: StartupRecoveryCheckStatus,
    pub position_reconstruction_status: StartupRecoveryCheckStatus,
    pub reconciliation_input: Option<LiveReconciliationInput>,
}

impl LiveStartupRecoveryInput {
    pub fn disabled(run_id: impl Into<String>, checked_at_ms: i64) -> Self {
        Self {
            run_id: run_id.into(),
            checked_at_ms,
            live_alpha_enabled: false,
            live_alpha_mode: LiveAlphaMode::Disabled,
            geoblock_status: GeoblockGateStatus::Unknown,
            account_preflight_status: StartupRecoveryCheckStatus::Unknown,
            balance_allowance_status: StartupRecoveryCheckStatus::Unknown,
            open_orders_readback_status: StartupRecoveryCheckStatus::Unknown,
            recent_trades_readback_status: StartupRecoveryCheckStatus::Unknown,
            journal_replay_status: StartupRecoveryCheckStatus::Unknown,
            position_reconstruction_status: StartupRecoveryCheckStatus::Unknown,
            reconciliation_input: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveStartupRecoveryReport {
    pub run_id: String,
    pub checked_at_ms: i64,
    pub status: LiveStartupRecoveryStatus,
    pub block_reasons: Vec<LiveStartupRecoveryBlockReason>,
    pub reconciliation_mismatches: Vec<LiveReconciliationMismatch>,
    pub journal_event_types: Vec<LiveJournalEventType>,
}

impl LiveStartupRecoveryReport {
    pub fn status_str(&self) -> &'static str {
        self.status.as_str()
    }

    pub fn block_reason_list(&self) -> String {
        self.block_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStartupRecoveryStatus {
    Skipped,
    Passed,
    HaltRequired,
}

impl LiveStartupRecoveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Passed => "passed",
            Self::HaltRequired => "halt_required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiveStartupRecoveryBlockReason {
    LiveAlphaDisabled,
    ModeDisabled,
    GeoblockBlocked,
    GeoblockUnknown,
    AccountPreflightFailed,
    AccountPreflightUnknown,
    BalanceAllowanceFailed,
    BalanceAllowanceUnknown,
    OpenOrdersReadbackFailed,
    OpenOrdersReadbackUnknown,
    RecentTradesReadbackFailed,
    RecentTradesReadbackUnknown,
    JournalReplayFailed,
    JournalReplayUnknown,
    PositionReconstructionFailed,
    PositionReconstructionUnknown,
    ReconciliationFailed,
    ReconciliationUnknown,
}

impl LiveStartupRecoveryBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveAlphaDisabled => "live_alpha_disabled",
            Self::ModeDisabled => "mode_disabled",
            Self::GeoblockBlocked => "geoblock_blocked",
            Self::GeoblockUnknown => "geoblock_unknown",
            Self::AccountPreflightFailed => "account_preflight_failed",
            Self::AccountPreflightUnknown => "account_preflight_unknown",
            Self::BalanceAllowanceFailed => "balance_allowance_failed",
            Self::BalanceAllowanceUnknown => "balance_allowance_unknown",
            Self::OpenOrdersReadbackFailed => "open_orders_readback_failed",
            Self::OpenOrdersReadbackUnknown => "open_orders_readback_unknown",
            Self::RecentTradesReadbackFailed => "recent_trades_readback_failed",
            Self::RecentTradesReadbackUnknown => "recent_trades_readback_unknown",
            Self::JournalReplayFailed => "journal_replay_failed",
            Self::JournalReplayUnknown => "journal_replay_unknown",
            Self::PositionReconstructionFailed => "position_reconstruction_failed",
            Self::PositionReconstructionUnknown => "position_reconstruction_unknown",
            Self::ReconciliationFailed => "reconciliation_failed",
            Self::ReconciliationUnknown => "reconciliation_unknown",
        }
    }
}

pub fn evaluate_startup_recovery(input: LiveStartupRecoveryInput) -> LiveStartupRecoveryReport {
    if !input.live_alpha_enabled || input.live_alpha_mode == LiveAlphaMode::Disabled {
        return LiveStartupRecoveryReport {
            run_id: input.run_id,
            checked_at_ms: input.checked_at_ms,
            status: LiveStartupRecoveryStatus::Skipped,
            block_reasons: vec![if !input.live_alpha_enabled {
                LiveStartupRecoveryBlockReason::LiveAlphaDisabled
            } else {
                LiveStartupRecoveryBlockReason::ModeDisabled
            }],
            reconciliation_mismatches: Vec::new(),
            journal_event_types: Vec::new(),
        };
    }

    let mut block_reasons = Vec::new();
    push_geoblock(input.geoblock_status, &mut block_reasons);
    push_check_status(
        input.account_preflight_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::AccountPreflightFailed,
        LiveStartupRecoveryBlockReason::AccountPreflightUnknown,
    );
    push_check_status(
        input.balance_allowance_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::BalanceAllowanceFailed,
        LiveStartupRecoveryBlockReason::BalanceAllowanceUnknown,
    );
    push_check_status(
        input.open_orders_readback_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::OpenOrdersReadbackFailed,
        LiveStartupRecoveryBlockReason::OpenOrdersReadbackUnknown,
    );
    push_check_status(
        input.recent_trades_readback_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::RecentTradesReadbackFailed,
        LiveStartupRecoveryBlockReason::RecentTradesReadbackUnknown,
    );
    push_check_status(
        input.journal_replay_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::JournalReplayFailed,
        LiveStartupRecoveryBlockReason::JournalReplayUnknown,
    );
    push_check_status(
        input.position_reconstruction_status,
        &mut block_reasons,
        LiveStartupRecoveryBlockReason::PositionReconstructionFailed,
        LiveStartupRecoveryBlockReason::PositionReconstructionUnknown,
    );

    let reconciliation = input.reconciliation_input.map(reconcile_live_state);
    let reconciliation_mismatches = match &reconciliation {
        Some(LiveReconciliationResult::Passed { .. }) => Vec::new(),
        Some(LiveReconciliationResult::HaltRequired { mismatches, .. }) => {
            block_reasons.push(LiveStartupRecoveryBlockReason::ReconciliationFailed);
            mismatches.clone()
        }
        None => {
            block_reasons.push(LiveStartupRecoveryBlockReason::ReconciliationUnknown);
            Vec::new()
        }
    };

    block_reasons.sort_unstable();
    block_reasons.dedup();

    let status = if block_reasons.is_empty() {
        LiveStartupRecoveryStatus::Passed
    } else {
        LiveStartupRecoveryStatus::HaltRequired
    };
    let journal_event_types = match status {
        LiveStartupRecoveryStatus::Skipped => Vec::new(),
        LiveStartupRecoveryStatus::Passed => vec![
            LiveJournalEventType::LiveStartupRecoveryStarted,
            LiveJournalEventType::LiveStartupRecoveryPassed,
        ],
        LiveStartupRecoveryStatus::HaltRequired => vec![
            LiveJournalEventType::LiveStartupRecoveryStarted,
            LiveJournalEventType::LiveStartupRecoveryFailed,
            LiveJournalEventType::LiveRiskHalt,
        ],
    };

    LiveStartupRecoveryReport {
        run_id: input.run_id,
        checked_at_ms: input.checked_at_ms,
        status,
        block_reasons,
        reconciliation_mismatches,
        journal_event_types,
    }
}

pub fn venue_state_from_readback(
    open_orders: &[OpenOrderReadback],
    trades: &[TradeReadback],
    balance: Option<LiveBalanceSnapshot>,
    positions: LivePositionBook,
) -> VenueLiveState {
    let orders = open_orders
        .iter()
        .map(|order| {
            (
                order.id.clone(),
                VenueOrderState {
                    order_id: order.id.clone(),
                    status: venue_order_status_from_readback(order.status),
                    matched_size: fixed6_to_decimal(order.size_matched_units),
                    remaining_size: fixed6_to_decimal(order.remaining_size_units()),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let trades = trades
        .iter()
        .map(|trade| {
            (
                trade.id.clone(),
                VenueTradeState {
                    trade_id: trade.id.clone(),
                    order_id: trade
                        .order_id
                        .clone()
                        .unwrap_or_else(|| "unknown_order_id".to_string()),
                    status: venue_trade_status_from_readback(trade.status),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    VenueLiveState {
        orders,
        trades,
        balance,
        positions,
        rust_readback_fingerprint: None,
        sdk_readback_fingerprint: None,
    }
}

fn push_geoblock(
    status: GeoblockGateStatus,
    block_reasons: &mut Vec<LiveStartupRecoveryBlockReason>,
) {
    match status {
        GeoblockGateStatus::Passed => {}
        GeoblockGateStatus::Blocked => {
            block_reasons.push(LiveStartupRecoveryBlockReason::GeoblockBlocked);
        }
        GeoblockGateStatus::Unknown => {
            block_reasons.push(LiveStartupRecoveryBlockReason::GeoblockUnknown);
        }
    }
}

fn push_check_status(
    status: StartupRecoveryCheckStatus,
    block_reasons: &mut Vec<LiveStartupRecoveryBlockReason>,
    failed: LiveStartupRecoveryBlockReason,
    unknown: LiveStartupRecoveryBlockReason,
) {
    match status {
        StartupRecoveryCheckStatus::Passed => {}
        StartupRecoveryCheckStatus::Failed => block_reasons.push(failed),
        StartupRecoveryCheckStatus::Unknown => block_reasons.push(unknown),
    }
}

fn venue_order_status_from_readback(status: OrderReadbackStatus) -> VenueOrderStatus {
    match status {
        OrderReadbackStatus::Live => VenueOrderStatus::Live,
        OrderReadbackStatus::Canceled | OrderReadbackStatus::CanceledMarketResolved => {
            VenueOrderStatus::Canceled
        }
        OrderReadbackStatus::Matched => VenueOrderStatus::Filled,
        OrderReadbackStatus::Invalid | OrderReadbackStatus::Unknown => VenueOrderStatus::Unknown,
    }
}

fn venue_trade_status_from_readback(status: TradeReadbackStatus) -> VenueTradeStatus {
    match status {
        TradeReadbackStatus::Matched => VenueTradeStatus::Matched,
        TradeReadbackStatus::Mined => VenueTradeStatus::Mined,
        TradeReadbackStatus::Confirmed => VenueTradeStatus::Confirmed,
        TradeReadbackStatus::Retrying => VenueTradeStatus::Retrying,
        TradeReadbackStatus::Failed => VenueTradeStatus::Failed,
        TradeReadbackStatus::Unknown => VenueTradeStatus::Unknown,
    }
}

fn fixed6_to_decimal(value: u64) -> f64 {
    value as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Asset, Side};
    use crate::live_reconciliation::{LocalLiveState, VenueOrderStatus};

    #[test]
    fn startup_recovery_skips_when_live_alpha_is_disabled() {
        let report = evaluate_startup_recovery(LiveStartupRecoveryInput::disabled("run-1", 1));

        assert_eq!(report.status, LiveStartupRecoveryStatus::Skipped);
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::LiveAlphaDisabled));
        assert!(report.journal_event_types.is_empty());
    }

    #[test]
    fn startup_recovery_passes_after_all_readback_and_reconciliation_checks_pass() {
        let report = evaluate_startup_recovery(passing_input());

        assert_eq!(report.status_str(), "passed");
        assert!(report.block_reasons.is_empty());
        assert_eq!(
            report.journal_event_types,
            vec![
                LiveJournalEventType::LiveStartupRecoveryStarted,
                LiveJournalEventType::LiveStartupRecoveryPassed
            ]
        );
    }

    #[test]
    fn startup_recovery_unknown_open_order_enters_risk_halt() {
        let mut input = passing_input();
        let reconciliation = input
            .reconciliation_input
            .as_mut()
            .expect("reconciliation input");
        reconciliation.venue.orders.insert(
            "unknown-order".to_string(),
            VenueOrderState {
                order_id: "unknown-order".to_string(),
                status: VenueOrderStatus::Live,
                matched_size: 0.0,
                remaining_size: 5.0,
            },
        );

        let report = evaluate_startup_recovery(input);

        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        assert!(report
            .block_reasons
            .contains(&LiveStartupRecoveryBlockReason::ReconciliationFailed));
        assert!(report
            .reconciliation_mismatches
            .contains(&LiveReconciliationMismatch::UnknownOpenOrder));
        assert!(report
            .journal_event_types
            .contains(&LiveJournalEventType::LiveRiskHalt));
    }

    #[test]
    fn startup_recovery_unknown_checks_fail_closed() {
        let mut input = passing_input();
        input.geoblock_status = GeoblockGateStatus::Unknown;
        input.account_preflight_status = StartupRecoveryCheckStatus::Unknown;
        input.open_orders_readback_status = StartupRecoveryCheckStatus::Failed;
        input.reconciliation_input = None;

        let report = evaluate_startup_recovery(input);

        assert_eq!(report.status, LiveStartupRecoveryStatus::HaltRequired);
        for expected in [
            LiveStartupRecoveryBlockReason::GeoblockUnknown,
            LiveStartupRecoveryBlockReason::AccountPreflightUnknown,
            LiveStartupRecoveryBlockReason::OpenOrdersReadbackFailed,
            LiveStartupRecoveryBlockReason::ReconciliationUnknown,
        ] {
            assert!(
                report.block_reasons.contains(&expected),
                "missing {expected:?}; got {}",
                report.block_reason_list()
            );
        }
    }

    #[test]
    fn startup_recovery_readback_conversion_preserves_retrying_trade_state() {
        let trade = TradeReadback {
            id: "trade-1".to_string(),
            market: "market-1".to_string(),
            asset_id: "token-up".to_string(),
            status: TradeReadbackStatus::Retrying,
            transaction_hash: None,
            maker_address: "0x1111111111111111111111111111111111111111".to_string(),
            order_id: Some("order-1".to_string()),
        };

        let venue = venue_state_from_readback(&[], &[trade], None, LivePositionBook::new());

        assert_eq!(
            venue.trades.get("trade-1").expect("trade").status,
            VenueTradeStatus::Retrying
        );
    }

    fn passing_input() -> LiveStartupRecoveryInput {
        let mut local = LocalLiveState::default();
        local.known_orders.insert("order-1".to_string());
        local.known_trades.insert("trade-1".to_string());
        local.trade_order_ids.insert("order-1".to_string());
        local
            .trade_order_ids_by_trade
            .insert("trade-1".to_string(), "order-1".to_string());
        local
            .positions
            .apply_fill(position_key(), Side::Buy, 0.42, 5.0, 0.0, 1)
            .expect("local position applies");

        let mut venue = VenueLiveState::default();
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

        LiveStartupRecoveryInput {
            run_id: "run-1".to_string(),
            checked_at_ms: 1,
            live_alpha_enabled: true,
            live_alpha_mode: LiveAlphaMode::FillCanary,
            geoblock_status: GeoblockGateStatus::Passed,
            account_preflight_status: StartupRecoveryCheckStatus::Passed,
            balance_allowance_status: StartupRecoveryCheckStatus::Passed,
            open_orders_readback_status: StartupRecoveryCheckStatus::Passed,
            recent_trades_readback_status: StartupRecoveryCheckStatus::Passed,
            journal_replay_status: StartupRecoveryCheckStatus::Passed,
            position_reconstruction_status: StartupRecoveryCheckStatus::Passed,
            reconciliation_input: Some(LiveReconciliationInput {
                run_id: "run-1".to_string(),
                checked_at_ms: 1,
                local,
                venue,
            }),
        }
    }

    fn position_key() -> crate::live_position_book::LivePositionKey {
        crate::live_position_book::LivePositionKey {
            market_id: "market-1".to_string(),
            token_id: "token-up".to_string(),
            asset: Asset::Btc,
            outcome: "Up".to_string(),
        }
    }
}
