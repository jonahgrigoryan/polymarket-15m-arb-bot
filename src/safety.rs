pub const MODULE: &str = "safety";

#[cfg(feature = "live-alpha-orders")]
pub const LIVE_ORDER_PLACEMENT_ENABLED: bool = true;

#[cfg(not(feature = "live-alpha-orders"))]
pub const LIVE_ORDER_PLACEMENT_ENABLED: bool = false;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoblockGateStatus {
    Passed,
    Blocked,
    Unknown,
}

impl GeoblockGateStatus {
    pub fn from_blocked(blocked: bool) -> Self {
        if blocked {
            GeoblockGateStatus::Blocked
        } else {
            GeoblockGateStatus::Passed
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GeoblockGateStatus::Passed => "passed",
            GeoblockGateStatus::Blocked => "blocked",
            GeoblockGateStatus::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveModeGateInput {
    pub config_intent_enabled: bool,
    pub cli_intent_enabled: bool,
    pub kill_switch_active: bool,
    pub geoblock_status: GeoblockGateStatus,
    pub later_phase_approvals_complete: bool,
}

impl LiveModeGateInput {
    pub fn lb1(
        config_intent_enabled: bool,
        cli_intent_enabled: bool,
        kill_switch_active: bool,
        geoblock_status: GeoblockGateStatus,
    ) -> Self {
        Self {
            config_intent_enabled,
            cli_intent_enabled,
            kill_switch_active,
            geoblock_status,
            later_phase_approvals_complete: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveModeGateBlockReason {
    PlacementDisabled,
    MissingConfigIntent,
    MissingCliIntent,
    KillSwitchActive,
    GeoblockBlocked,
    GeoblockUnknown,
    LaterPhaseApprovalsMissing,
}

impl LiveModeGateBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            LiveModeGateBlockReason::PlacementDisabled => "live_order_placement_disabled",
            LiveModeGateBlockReason::MissingConfigIntent => "missing_config_intent",
            LiveModeGateBlockReason::MissingCliIntent => "missing_cli_intent",
            LiveModeGateBlockReason::KillSwitchActive => "kill_switch_active",
            LiveModeGateBlockReason::GeoblockBlocked => "geoblock_blocked",
            LiveModeGateBlockReason::GeoblockUnknown => "geoblock_unknown",
            LiveModeGateBlockReason::LaterPhaseApprovalsMissing => "later_phase_approvals_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveModeGateDecision {
    pub allowed: bool,
    pub block_reasons: Vec<LiveModeGateBlockReason>,
}

impl LiveModeGateDecision {
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

pub fn evaluate_live_mode_gate(input: LiveModeGateInput) -> LiveModeGateDecision {
    let mut block_reasons = Vec::new();

    if !LIVE_ORDER_PLACEMENT_ENABLED {
        block_reasons.push(LiveModeGateBlockReason::PlacementDisabled);
    }
    if !input.config_intent_enabled {
        block_reasons.push(LiveModeGateBlockReason::MissingConfigIntent);
    }
    if !input.cli_intent_enabled {
        block_reasons.push(LiveModeGateBlockReason::MissingCliIntent);
    }
    if input.kill_switch_active {
        block_reasons.push(LiveModeGateBlockReason::KillSwitchActive);
    }
    match input.geoblock_status {
        GeoblockGateStatus::Passed => {}
        GeoblockGateStatus::Blocked => {
            block_reasons.push(LiveModeGateBlockReason::GeoblockBlocked);
        }
        GeoblockGateStatus::Unknown => {
            block_reasons.push(LiveModeGateBlockReason::GeoblockUnknown);
        }
    }
    if !input.later_phase_approvals_complete {
        block_reasons.push(LiveModeGateBlockReason::LaterPhaseApprovalsMissing);
    }

    LiveModeGateDecision {
        allowed: block_reasons.is_empty(),
        block_reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(feature = "live-alpha-orders"))]
    fn live_order_placement_is_disabled_by_default() {
        assert!(!LIVE_ORDER_PLACEMENT_ENABLED);
    }

    #[test]
    #[cfg(feature = "live-alpha-orders")]
    fn live_order_placement_is_enabled_only_by_explicit_feature() {
        assert!(LIVE_ORDER_PLACEMENT_ENABLED);
    }

    #[test]
    fn live_mode_gate_requires_config_and_cli_intent() {
        let decision = evaluate_live_mode_gate(LiveModeGateInput {
            config_intent_enabled: false,
            cli_intent_enabled: false,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            later_phase_approvals_complete: true,
        });

        assert!(!decision.allowed);
        assert!(decision
            .block_reasons
            .contains(&LiveModeGateBlockReason::MissingConfigIntent));
        assert!(decision
            .block_reasons
            .contains(&LiveModeGateBlockReason::MissingCliIntent));
        #[cfg(not(feature = "live-alpha-orders"))]
        assert!(decision
            .block_reasons
            .contains(&LiveModeGateBlockReason::PlacementDisabled));
    }

    #[test]
    fn geoblock_blocked_or_unknown_blocks_future_live_mode() {
        for geoblock_status in [GeoblockGateStatus::Blocked, GeoblockGateStatus::Unknown] {
            let decision = evaluate_live_mode_gate(LiveModeGateInput {
                config_intent_enabled: true,
                cli_intent_enabled: true,
                kill_switch_active: false,
                geoblock_status,
                later_phase_approvals_complete: true,
            });

            assert!(!decision.allowed);
            assert!(decision.block_reasons.iter().any(|reason| matches!(
                reason,
                LiveModeGateBlockReason::GeoblockBlocked | LiveModeGateBlockReason::GeoblockUnknown
            )));
        }
    }

    #[test]
    #[cfg(not(feature = "live-alpha-orders"))]
    fn lb1_gate_remains_closed_even_when_visible_inputs_pass() {
        let decision = evaluate_live_mode_gate(LiveModeGateInput {
            config_intent_enabled: true,
            cli_intent_enabled: true,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            later_phase_approvals_complete: true,
        });

        assert!(!decision.allowed);
        assert_eq!(
            decision.block_reasons,
            vec![LiveModeGateBlockReason::PlacementDisabled]
        );
    }

    #[test]
    #[cfg(feature = "live-alpha-orders")]
    fn live_mode_gate_allows_explicit_feature_build_when_visible_inputs_pass() {
        let decision = evaluate_live_mode_gate(LiveModeGateInput {
            config_intent_enabled: true,
            cli_intent_enabled: true,
            kill_switch_active: false,
            geoblock_status: GeoblockGateStatus::Passed,
            later_phase_approvals_complete: true,
        });

        assert!(decision.allowed);
        assert!(decision.block_reasons.is_empty());
    }
}
