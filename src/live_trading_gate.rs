use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_trading_gate";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveTradingReadinessStatus {
    Passed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingNoWriteProof {
    pub submitted_orders: bool,
    pub signed_orders_for_submission: bool,
    pub submitted_cancels: bool,
    pub heartbeat_posts: bool,
    pub authenticated_write_requests: bool,
}

impl LiveTradingNoWriteProof {
    pub fn any_write_observed(&self) -> bool {
        self.submitted_orders
            || self.signed_orders_for_submission
            || self.submitted_cancels
            || self.heartbeat_posts
            || self.authenticated_write_requests
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingGateInput {
    pub final_live_config_enabled: bool,
    pub preflight_status: LiveTradingReadinessStatus,
    pub journal_replay_status: LiveTradingReadinessStatus,
    pub evidence_hash_status: LiveTradingReadinessStatus,
    pub reconciliation_status: LiveTradingReadinessStatus,
    pub heartbeat_fresh: bool,
    pub geoblock_fresh: bool,
    pub previous_order_reconciled_or_incident_reviewed: bool,
    pub write_capability_present: bool,
    pub no_write_proof: LiveTradingNoWriteProof,
}

impl Default for LiveTradingGateInput {
    fn default() -> Self {
        Self {
            final_live_config_enabled: false,
            preflight_status: LiveTradingReadinessStatus::Unknown,
            journal_replay_status: LiveTradingReadinessStatus::Unknown,
            evidence_hash_status: LiveTradingReadinessStatus::Unknown,
            reconciliation_status: LiveTradingReadinessStatus::Unknown,
            heartbeat_fresh: false,
            geoblock_fresh: false,
            previous_order_reconciled_or_incident_reviewed: false,
            write_capability_present: false,
            no_write_proof: LiveTradingNoWriteProof::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveTradingGateBlockReason {
    FinalLiveConfigDisabled,
    PreflightFailed,
    PreflightUnknown,
    JournalReplayFailed,
    JournalReplayUnknown,
    EvidenceHashFailed,
    EvidenceHashUnknown,
    ReconciliationFailed,
    ReconciliationUnknown,
    HeartbeatStale,
    GeoblockStale,
    PreviousOrderUnresolved,
    WriteCapabilityPresent,
    LiveWriteObserved,
}

impl LiveTradingGateBlockReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FinalLiveConfigDisabled => "final_live_config_disabled",
            Self::PreflightFailed => "preflight_failed",
            Self::PreflightUnknown => "preflight_unknown",
            Self::JournalReplayFailed => "journal_replay_failed",
            Self::JournalReplayUnknown => "journal_replay_unknown",
            Self::EvidenceHashFailed => "evidence_hash_failed",
            Self::EvidenceHashUnknown => "evidence_hash_unknown",
            Self::ReconciliationFailed => "reconciliation_failed",
            Self::ReconciliationUnknown => "reconciliation_unknown",
            Self::HeartbeatStale => "heartbeat_stale",
            Self::GeoblockStale => "geoblock_stale",
            Self::PreviousOrderUnresolved => "previous_order_unresolved",
            Self::WriteCapabilityPresent => "write_capability_present",
            Self::LiveWriteObserved => "live_write_observed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingGateDecision {
    pub status: String,
    pub block_reasons: Vec<LiveTradingGateBlockReason>,
}

impl LiveTradingGateDecision {
    pub fn allowed(&self) -> bool {
        self.block_reasons.is_empty()
    }

    pub fn reason_list(&self) -> String {
        self.block_reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub fn evaluate_live_trading_gate(input: LiveTradingGateInput) -> LiveTradingGateDecision {
    let mut block_reasons = Vec::new();

    if !input.final_live_config_enabled {
        block_reasons.push(LiveTradingGateBlockReason::FinalLiveConfigDisabled);
    }
    push_readiness_block(
        input.preflight_status,
        &mut block_reasons,
        LiveTradingGateBlockReason::PreflightFailed,
        LiveTradingGateBlockReason::PreflightUnknown,
    );
    push_readiness_block(
        input.journal_replay_status,
        &mut block_reasons,
        LiveTradingGateBlockReason::JournalReplayFailed,
        LiveTradingGateBlockReason::JournalReplayUnknown,
    );
    push_readiness_block(
        input.evidence_hash_status,
        &mut block_reasons,
        LiveTradingGateBlockReason::EvidenceHashFailed,
        LiveTradingGateBlockReason::EvidenceHashUnknown,
    );
    push_readiness_block(
        input.reconciliation_status,
        &mut block_reasons,
        LiveTradingGateBlockReason::ReconciliationFailed,
        LiveTradingGateBlockReason::ReconciliationUnknown,
    );
    if !input.heartbeat_fresh {
        block_reasons.push(LiveTradingGateBlockReason::HeartbeatStale);
    }
    if !input.geoblock_fresh {
        block_reasons.push(LiveTradingGateBlockReason::GeoblockStale);
    }
    if !input.previous_order_reconciled_or_incident_reviewed {
        block_reasons.push(LiveTradingGateBlockReason::PreviousOrderUnresolved);
    }
    if input.write_capability_present {
        block_reasons.push(LiveTradingGateBlockReason::WriteCapabilityPresent);
    }
    if input.no_write_proof.any_write_observed() {
        block_reasons.push(LiveTradingGateBlockReason::LiveWriteObserved);
    }

    block_reasons.sort();
    block_reasons.dedup();

    LiveTradingGateDecision {
        status: if block_reasons.is_empty() {
            "allowed".to_string()
        } else {
            "blocked".to_string()
        },
        block_reasons,
    }
}

fn push_readiness_block(
    status: LiveTradingReadinessStatus,
    block_reasons: &mut Vec<LiveTradingGateBlockReason>,
    failed: LiveTradingGateBlockReason,
    unknown: LiveTradingGateBlockReason,
) {
    match status {
        LiveTradingReadinessStatus::Passed => {}
        LiveTradingReadinessStatus::Failed => block_reasons.push(failed),
        LiveTradingReadinessStatus::Unknown => block_reasons.push(unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_trading_gate_blocks_default_and_prior_unreconciled_order() {
        let decision = evaluate_live_trading_gate(LiveTradingGateInput::default());

        assert_eq!(decision.status, "blocked");
        for expected in [
            LiveTradingGateBlockReason::FinalLiveConfigDisabled,
            LiveTradingGateBlockReason::PreflightUnknown,
            LiveTradingGateBlockReason::JournalReplayUnknown,
            LiveTradingGateBlockReason::EvidenceHashUnknown,
            LiveTradingGateBlockReason::ReconciliationUnknown,
            LiveTradingGateBlockReason::HeartbeatStale,
            LiveTradingGateBlockReason::GeoblockStale,
            LiveTradingGateBlockReason::PreviousOrderUnresolved,
        ] {
            assert!(decision.block_reasons.contains(&expected), "{expected:?}");
        }
    }

    #[test]
    fn live_trading_gate_allows_modeling_only_when_all_gates_pass() {
        let decision = evaluate_live_trading_gate(passing_input());

        assert_eq!(decision.status, "allowed");
        assert!(decision.allowed());
        assert!(decision.block_reasons.is_empty());
    }

    #[test]
    fn live_trading_gate_blocks_any_write_capability_or_observed_write() {
        let mut input = passing_input();
        input.write_capability_present = true;
        input.no_write_proof.submitted_orders = true;

        let decision = evaluate_live_trading_gate(input);

        assert_eq!(decision.status, "blocked");
        assert!(decision
            .block_reasons
            .contains(&LiveTradingGateBlockReason::WriteCapabilityPresent));
        assert!(decision
            .block_reasons
            .contains(&LiveTradingGateBlockReason::LiveWriteObserved));
    }

    fn passing_input() -> LiveTradingGateInput {
        LiveTradingGateInput {
            final_live_config_enabled: true,
            preflight_status: LiveTradingReadinessStatus::Passed,
            journal_replay_status: LiveTradingReadinessStatus::Passed,
            evidence_hash_status: LiveTradingReadinessStatus::Passed,
            reconciliation_status: LiveTradingReadinessStatus::Passed,
            heartbeat_fresh: true,
            geoblock_fresh: true,
            previous_order_reconciled_or_incident_reviewed: true,
            write_capability_present: false,
            no_write_proof: LiveTradingNoWriteProof::default(),
        }
    }
}
