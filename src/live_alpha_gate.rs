use crate::live_alpha_config::LiveAlphaMode;
use crate::safety::{self, GeoblockGateStatus};

pub const MODULE: &str = "live_alpha_gate";
pub const LIVE_ALPHA_ORDER_FEATURE_ENABLED: bool = cfg!(feature = "live-alpha-orders");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAlphaReadinessStatus {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveAlphaGateInput {
    pub live_alpha_enabled: bool,
    pub live_alpha_mode: LiveAlphaMode,
    pub fill_canary_enabled: bool,
    pub maker_enabled: bool,
    pub taker_enabled: bool,
    pub config_intent_enabled: bool,
    pub cli_intent_enabled: bool,
    pub kill_switch_active: bool,
    pub geoblock_status: GeoblockGateStatus,
    pub account_preflight_status: LiveAlphaReadinessStatus,
    pub heartbeat_required: bool,
    pub heartbeat_status: LiveAlphaReadinessStatus,
    pub reconciliation_status: LiveAlphaReadinessStatus,
    pub approval_status: LiveAlphaReadinessStatus,
    pub phase_status: LiveAlphaReadinessStatus,
}

impl Default for LiveAlphaGateInput {
    fn default() -> Self {
        Self {
            live_alpha_enabled: false,
            live_alpha_mode: LiveAlphaMode::Disabled,
            fill_canary_enabled: false,
            maker_enabled: false,
            taker_enabled: false,
            config_intent_enabled: false,
            cli_intent_enabled: false,
            kill_switch_active: true,
            geoblock_status: GeoblockGateStatus::Unknown,
            account_preflight_status: LiveAlphaReadinessStatus::Unknown,
            heartbeat_required: true,
            heartbeat_status: LiveAlphaReadinessStatus::Unknown,
            reconciliation_status: LiveAlphaReadinessStatus::Unknown,
            approval_status: LiveAlphaReadinessStatus::Unknown,
            phase_status: LiveAlphaReadinessStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAlphaBlockReason {
    LiveOrderPlacementDisabled,
    CompileTimeLiveDisabled,
    LiveAlphaDisabled,
    ModeDisabled,
    SubmodeDisabled,
    MissingConfigIntent,
    MissingCliIntent,
    KillSwitchActive,
    GeoblockBlocked,
    GeoblockUnknown,
    AccountPreflightFailed,
    AccountPreflightUnknown,
    HeartbeatFailed,
    HeartbeatUnknown,
    ReconciliationFailed,
    ReconciliationUnknown,
    ApprovalMissing,
    PhaseNotApproved,
}

impl LiveAlphaBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LiveOrderPlacementDisabled => "live_order_placement_disabled",
            Self::CompileTimeLiveDisabled => "compile_time_live_disabled",
            Self::LiveAlphaDisabled => "live_alpha_disabled",
            Self::ModeDisabled => "mode_disabled",
            Self::SubmodeDisabled => "submode_disabled",
            Self::MissingConfigIntent => "missing_config_intent",
            Self::MissingCliIntent => "missing_cli_intent",
            Self::KillSwitchActive => "kill_switch_active",
            Self::GeoblockBlocked => "geoblock_blocked",
            Self::GeoblockUnknown => "geoblock_unknown",
            Self::AccountPreflightFailed => "account_preflight_failed",
            Self::AccountPreflightUnknown => "account_preflight_unknown",
            Self::HeartbeatFailed => "heartbeat_failed",
            Self::HeartbeatUnknown => "heartbeat_unknown",
            Self::ReconciliationFailed => "reconciliation_failed",
            Self::ReconciliationUnknown => "reconciliation_unknown",
            Self::ApprovalMissing => "approval_missing",
            Self::PhaseNotApproved => "phase_not_approved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAlphaGateDecision {
    pub allowed: bool,
    pub block_reasons: Vec<LiveAlphaBlockReason>,
}

impl LiveAlphaGateDecision {
    pub fn status(&self) -> &'static str {
        if self.allowed {
            "allowed"
        } else {
            "blocked"
        }
    }

    pub fn reason_list(&self) -> String {
        self.block_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn evaluate_live_alpha_gate(input: LiveAlphaGateInput) -> LiveAlphaGateDecision {
    let mut block_reasons = Vec::new();

    if !safety::LIVE_ORDER_PLACEMENT_ENABLED {
        block_reasons.push(LiveAlphaBlockReason::LiveOrderPlacementDisabled);
    }
    if !LIVE_ALPHA_ORDER_FEATURE_ENABLED {
        block_reasons.push(LiveAlphaBlockReason::CompileTimeLiveDisabled);
    }
    if !input.live_alpha_enabled {
        block_reasons.push(LiveAlphaBlockReason::LiveAlphaDisabled);
    }
    if !input.live_alpha_mode.can_place_live_orders() {
        block_reasons.push(LiveAlphaBlockReason::ModeDisabled);
    }
    if input.live_alpha_mode.can_place_live_orders() && !input.selected_submode_enabled() {
        block_reasons.push(LiveAlphaBlockReason::SubmodeDisabled);
    }
    if !input.config_intent_enabled {
        block_reasons.push(LiveAlphaBlockReason::MissingConfigIntent);
    }
    if !input.cli_intent_enabled {
        block_reasons.push(LiveAlphaBlockReason::MissingCliIntent);
    }
    if input.kill_switch_active {
        block_reasons.push(LiveAlphaBlockReason::KillSwitchActive);
    }
    match input.geoblock_status {
        GeoblockGateStatus::Passed => {}
        GeoblockGateStatus::Blocked => block_reasons.push(LiveAlphaBlockReason::GeoblockBlocked),
        GeoblockGateStatus::Unknown => block_reasons.push(LiveAlphaBlockReason::GeoblockUnknown),
    }
    push_readiness_block(
        input.account_preflight_status,
        &mut block_reasons,
        LiveAlphaBlockReason::AccountPreflightFailed,
        LiveAlphaBlockReason::AccountPreflightUnknown,
    );
    if input.heartbeat_required {
        push_readiness_block(
            input.heartbeat_status,
            &mut block_reasons,
            LiveAlphaBlockReason::HeartbeatFailed,
            LiveAlphaBlockReason::HeartbeatUnknown,
        );
    }
    push_readiness_block(
        input.reconciliation_status,
        &mut block_reasons,
        LiveAlphaBlockReason::ReconciliationFailed,
        LiveAlphaBlockReason::ReconciliationUnknown,
    );
    if input.approval_status != LiveAlphaReadinessStatus::Passed {
        block_reasons.push(LiveAlphaBlockReason::ApprovalMissing);
    }
    if input.phase_status != LiveAlphaReadinessStatus::Passed {
        block_reasons.push(LiveAlphaBlockReason::PhaseNotApproved);
    }

    LiveAlphaGateDecision {
        allowed: block_reasons.is_empty(),
        block_reasons,
    }
}

fn push_readiness_block(
    status: LiveAlphaReadinessStatus,
    block_reasons: &mut Vec<LiveAlphaBlockReason>,
    failed: LiveAlphaBlockReason,
    unknown: LiveAlphaBlockReason,
) {
    match status {
        LiveAlphaReadinessStatus::Passed => {}
        LiveAlphaReadinessStatus::Failed => block_reasons.push(failed),
        LiveAlphaReadinessStatus::Unknown => block_reasons.push(unknown),
    }
}

impl LiveAlphaGateInput {
    fn selected_submode_enabled(&self) -> bool {
        match self.live_alpha_mode {
            LiveAlphaMode::FillCanary => self.fill_canary_enabled,
            LiveAlphaMode::MakerMicro | LiveAlphaMode::QuoteManager => self.maker_enabled,
            LiveAlphaMode::TakerGate => self.taker_enabled,
            LiveAlphaMode::Disabled | LiveAlphaMode::Shadow | LiveAlphaMode::Scale => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_alpha_gate_blocks_by_default() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput::default());

        assert!(!decision.allowed);
        for reason in [
            LiveAlphaBlockReason::LiveOrderPlacementDisabled,
            LiveAlphaBlockReason::LiveAlphaDisabled,
            LiveAlphaBlockReason::ModeDisabled,
            LiveAlphaBlockReason::MissingConfigIntent,
            LiveAlphaBlockReason::MissingCliIntent,
            LiveAlphaBlockReason::KillSwitchActive,
            LiveAlphaBlockReason::GeoblockUnknown,
            LiveAlphaBlockReason::AccountPreflightUnknown,
            LiveAlphaBlockReason::HeartbeatUnknown,
            LiveAlphaBlockReason::ReconciliationUnknown,
            LiveAlphaBlockReason::ApprovalMissing,
            LiveAlphaBlockReason::PhaseNotApproved,
        ] {
            assert!(decision.block_reasons.contains(&reason));
        }
        if !LIVE_ALPHA_ORDER_FEATURE_ENABLED {
            assert!(decision
                .block_reasons
                .contains(&LiveAlphaBlockReason::CompileTimeLiveDisabled));
        }
    }

    #[test]
    fn live_alpha_gate_stays_blocked_without_compile_time_and_global_placement() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
            live_alpha_enabled: true,
            live_alpha_mode: LiveAlphaMode::Shadow,
            fill_canary_enabled: false,
            maker_enabled: false,
            taker_enabled: false,
            config_intent_enabled: true,
            cli_intent_enabled: true,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            account_preflight_status: LiveAlphaReadinessStatus::Passed,
            heartbeat_required: true,
            heartbeat_status: LiveAlphaReadinessStatus::Passed,
            reconciliation_status: LiveAlphaReadinessStatus::Passed,
            approval_status: LiveAlphaReadinessStatus::Passed,
            phase_status: LiveAlphaReadinessStatus::Passed,
        });

        assert!(!decision.allowed);
        assert!(decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::LiveOrderPlacementDisabled));
        assert!(decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::ModeDisabled));
    }

    #[test]
    fn live_alpha_gate_blocks_modes_that_cannot_place_live_orders() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
            live_alpha_enabled: true,
            live_alpha_mode: LiveAlphaMode::Shadow,
            fill_canary_enabled: false,
            maker_enabled: false,
            taker_enabled: false,
            config_intent_enabled: true,
            cli_intent_enabled: true,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            account_preflight_status: LiveAlphaReadinessStatus::Passed,
            heartbeat_required: true,
            heartbeat_status: LiveAlphaReadinessStatus::Passed,
            reconciliation_status: LiveAlphaReadinessStatus::Passed,
            approval_status: LiveAlphaReadinessStatus::Passed,
            phase_status: LiveAlphaReadinessStatus::Passed,
        });

        assert!(!decision.allowed);
        assert!(decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::ModeDisabled));
    }

    #[test]
    fn live_alpha_gate_blocks_live_order_mode_when_submode_disabled() {
        for mode in [
            LiveAlphaMode::FillCanary,
            LiveAlphaMode::MakerMicro,
            LiveAlphaMode::QuoteManager,
            LiveAlphaMode::TakerGate,
        ] {
            let decision = evaluate_live_alpha_gate(live_order_capable_input(mode));

            assert!(!decision.allowed);
            assert!(
                decision
                    .block_reasons
                    .contains(&LiveAlphaBlockReason::SubmodeDisabled),
                "{mode:?} missing submode disabled block"
            );
        }
    }

    #[test]
    fn live_alpha_gate_accepts_matching_submode_enablement() {
        for (mode, fill_canary_enabled, maker_enabled, taker_enabled) in [
            (LiveAlphaMode::FillCanary, true, false, false),
            (LiveAlphaMode::MakerMicro, false, true, false),
            (LiveAlphaMode::QuoteManager, false, true, false),
            (LiveAlphaMode::TakerGate, false, false, true),
        ] {
            let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
                fill_canary_enabled,
                maker_enabled,
                taker_enabled,
                ..live_order_capable_input(mode)
            });

            assert!(
                !decision
                    .block_reasons
                    .contains(&LiveAlphaBlockReason::SubmodeDisabled),
                "{mode:?} should not have submode disabled block"
            );
        }
    }

    #[test]
    fn live_alpha_gate_fails_closed_on_reconciliation_failure() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
            reconciliation_status: LiveAlphaReadinessStatus::Failed,
            ..LiveAlphaGateInput::default()
        });

        assert!(!decision.allowed);
        assert!(decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::ReconciliationFailed));
    }

    #[test]
    fn live_alpha_gate_blocks_live_capable_mode_on_required_stale_heartbeat() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
            heartbeat_required: true,
            heartbeat_status: LiveAlphaReadinessStatus::Failed,
            ..live_order_capable_input(LiveAlphaMode::FillCanary)
        });

        assert!(!decision.allowed);
        assert!(decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::HeartbeatFailed));
    }

    #[test]
    fn live_alpha_gate_can_skip_heartbeat_only_when_not_required() {
        let decision = evaluate_live_alpha_gate(LiveAlphaGateInput {
            heartbeat_required: false,
            heartbeat_status: LiveAlphaReadinessStatus::Unknown,
            ..live_order_capable_input(LiveAlphaMode::FillCanary)
        });

        assert!(!decision
            .block_reasons
            .contains(&LiveAlphaBlockReason::HeartbeatUnknown));
    }

    fn live_order_capable_input(live_alpha_mode: LiveAlphaMode) -> LiveAlphaGateInput {
        LiveAlphaGateInput {
            live_alpha_enabled: true,
            live_alpha_mode,
            fill_canary_enabled: false,
            maker_enabled: false,
            taker_enabled: false,
            config_intent_enabled: true,
            cli_intent_enabled: true,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            account_preflight_status: LiveAlphaReadinessStatus::Passed,
            heartbeat_required: true,
            heartbeat_status: LiveAlphaReadinessStatus::Passed,
            reconciliation_status: LiveAlphaReadinessStatus::Passed,
            approval_status: LiveAlphaReadinessStatus::Passed,
            phase_status: LiveAlphaReadinessStatus::Passed,
        }
    }
}
