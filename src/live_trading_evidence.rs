use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::live_trading_gate::{LiveTradingGateDecision, LiveTradingNoWriteProof};
use crate::live_trading_journal::LiveTradingJournalState;
use crate::live_trading_reconciliation::LiveTradingReconciliationReport;
use crate::secret_handling::REDACTED_VALUE;

pub const MODULE: &str = "live_trading_evidence";
pub const LIVE_TRADING_EVIDENCE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingEvidenceBundle {
    #[serde(flatten)]
    pub body: LiveTradingEvidenceBundleBody,
    pub bundle_hash: String,
}

impl LiveTradingEvidenceBundle {
    pub fn new(body: LiveTradingEvidenceBundleBody) -> Result<Self, LiveTradingEvidenceError> {
        let bundle_hash = evidence_hash(&body)?;
        Ok(Self { body, bundle_hash })
    }

    pub fn validate(&self) -> Result<(), LiveTradingEvidenceError> {
        let expected_hash = evidence_hash(&self.body)?;
        if self.bundle_hash != expected_hash {
            return Err(LiveTradingEvidenceError::HashMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingEvidenceBundleBody {
    pub schema_version: u16,
    pub phase: String,
    pub run_id: String,
    pub baseline_id: String,
    pub created_at_ms: i64,
    pub docs_checked: Vec<String>,
    pub artifact_paths: BTreeMap<String, String>,
    pub journal_summary: LiveTradingJournalSummary,
    pub reconciliation_status: String,
    pub reconciliation_mismatches: Vec<String>,
    pub gate_status: String,
    pub gate_block_reasons: Vec<String>,
    pub no_write_proof: LiveTradingNoWriteProof,
    pub redaction_status: String,
}

impl LiveTradingEvidenceBundleBody {
    pub fn from_input(input: LiveTradingEvidenceBundleInput<'_>) -> Self {
        Self {
            schema_version: LIVE_TRADING_EVIDENCE_SCHEMA_VERSION,
            phase: "lt2_gates_journal_evidence".to_string(),
            run_id: input.run_id.to_string(),
            baseline_id: input.baseline_id.to_string(),
            created_at_ms: input.created_at_ms,
            docs_checked: input.docs_checked,
            artifact_paths: input.artifact_paths,
            journal_summary: LiveTradingJournalSummary::from(input.journal_state),
            reconciliation_status: input.reconciliation.status.clone(),
            reconciliation_mismatches: input
                .reconciliation
                .mismatches
                .iter()
                .map(|mismatch| mismatch.as_str().to_string())
                .collect(),
            gate_status: input.gate.status.clone(),
            gate_block_reasons: input
                .gate
                .block_reasons
                .iter()
                .map(|reason| reason.as_str().to_string())
                .collect(),
            no_write_proof: input.no_write_proof,
            redaction_status: "redacted".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiveTradingEvidenceBundleInput<'a> {
    pub run_id: &'a str,
    pub baseline_id: &'a str,
    pub created_at_ms: i64,
    pub docs_checked: Vec<String>,
    pub artifact_paths: BTreeMap<String, String>,
    pub journal_state: &'a LiveTradingJournalState,
    pub reconciliation: &'a LiveTradingReconciliationReport,
    pub gate: &'a LiveTradingGateDecision,
    pub no_write_proof: LiveTradingNoWriteProof,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTradingJournalSummary {
    pub event_count: usize,
    pub intended_maker_order_count: usize,
    pub accepted_order_count: usize,
    pub intended_cancel_count: usize,
    pub fill_count: usize,
    pub fee_total_units: u64,
    pub position_count: usize,
    pub settlement_count: usize,
    pub unreviewed_incident_count: usize,
    pub previous_order_reconciled_or_incident_reviewed: bool,
}

impl From<&LiveTradingJournalState> for LiveTradingJournalSummary {
    fn from(state: &LiveTradingJournalState) -> Self {
        let unreviewed_incident_count = state
            .incidents
            .keys()
            .filter(|incident_id| !state.reviewed_incidents.contains(*incident_id))
            .count();
        Self {
            event_count: state.event_count,
            intended_maker_order_count: state.intended_maker_orders.len(),
            accepted_order_count: state.accepted_orders.len(),
            intended_cancel_count: state.intended_cancels.len(),
            fill_count: state.fills.len(),
            fee_total_units: state.fee_total_units,
            position_count: state.positions.len(),
            settlement_count: state.settlements.len(),
            unreviewed_incident_count,
            previous_order_reconciled_or_incident_reviewed: state
                .previous_order_reconciled_or_incident_reviewed(),
        }
    }
}

#[derive(Debug)]
pub enum LiveTradingEvidenceError {
    Serialize(serde_json::Error),
    HashMismatch,
}

impl Display for LiveTradingEvidenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(source) => {
                write!(
                    formatter,
                    "live trading evidence serialize failed: {source}"
                )
            }
            Self::HashMismatch => write!(formatter, "live trading evidence hash mismatch"),
        }
    }
}

impl Error for LiveTradingEvidenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialize(source) => Some(source),
            Self::HashMismatch => None,
        }
    }
}

pub fn live_trading_evidence_json(
    bundle: &LiveTradingEvidenceBundle,
) -> Result<String, LiveTradingEvidenceError> {
    serde_json::to_string_pretty(bundle).map_err(LiveTradingEvidenceError::Serialize)
}

pub fn redacted_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(redacted_map(map)),
        Value::Array(values) => Value::Array(values.into_iter().map(redacted_value).collect()),
        other => other,
    }
}

fn redacted_map(map: Map<String, Value>) -> Map<String, Value> {
    map.into_iter()
        .map(|(key, value)| {
            if is_sensitive_key(&key) {
                (key, Value::String(REDACTED_VALUE.to_string()))
            } else {
                (key, redacted_value(value))
            }
        })
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("private_key")
        || key.contains("secret")
        || key.contains("api_key")
        || key.contains("passphrase")
        || key.contains("signature")
        || key.contains("authorization")
        || key == "poly_signature"
        || key == "poly_api_key"
}

fn evidence_hash(body: &LiveTradingEvidenceBundleBody) -> Result<String, LiveTradingEvidenceError> {
    let bytes = serde_json::to_vec(body).map_err(LiveTradingEvidenceError::Serialize)?;
    let hash = digest(&SHA256, &bytes);
    Ok(format!("sha256:{}", to_hex(hash.as_ref())))
}

fn to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_trading_gate::{
        evaluate_live_trading_gate, LiveTradingGateInput, LiveTradingReadinessStatus,
    };
    use crate::live_trading_journal::{
        reduce_live_trading_journal_events, AcceptedOrder, BalanceRecord, FillRecord,
        FreshnessObservation, IntendedMakerOrder, LiveTradingJournalEvent,
        LiveTradingJournalEventKind, PositionRecord, SettlementRecord,
    };
    use crate::live_trading_reconciliation::{
        reconcile_live_trading_state, LiveTradingReadbackFixture, LiveTradingReconciliationInput,
    };
    use serde_json::json;

    #[test]
    fn live_trading_evidence_hashes_and_redacts() {
        let bundle = sample_bundle();

        bundle.validate().expect("evidence hash validates");
        assert!(bundle.bundle_hash.starts_with("sha256:"));

        let redacted = redacted_value(json!({
            "POLY_API_KEY": "key",
            "nested": {
                "private_key": "0xabc",
                "asset_id": "safe-token-id"
            }
        }));

        assert_eq!(redacted["POLY_API_KEY"], REDACTED_VALUE);
        assert_eq!(redacted["nested"]["private_key"], REDACTED_VALUE);
        assert_eq!(redacted["nested"]["asset_id"], "safe-token-id");
    }

    #[test]
    fn live_trading_evidence_bundle_captures_no_live_action_proof() {
        let bundle = sample_bundle();

        assert_eq!(bundle.body.phase, "lt2_gates_journal_evidence");
        assert_eq!(bundle.body.gate_status, "allowed");
        assert_eq!(bundle.body.reconciliation_status, "passed");
        assert!(!bundle.body.no_write_proof.any_write_observed());
        assert_eq!(bundle.body.journal_summary.intended_maker_order_count, 1);
    }

    fn sample_bundle() -> LiveTradingEvidenceBundle {
        let local = matching_local_state();
        let readback = matching_readback_fixture(&local);
        let reconciliation = reconcile_live_trading_state(LiveTradingReconciliationInput {
            run_id: "lt2-evidence".to_string(),
            checked_at_ms: 2,
            local: local.clone(),
            readback,
        });
        let gate = evaluate_live_trading_gate(LiveTradingGateInput {
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
        });
        let mut paths = BTreeMap::new();
        paths.insert(
            "journal".to_string(),
            "artifacts/live_trading/lt2/journal.redacted.jsonl".to_string(),
        );

        LiveTradingEvidenceBundle::new(LiveTradingEvidenceBundleBody::from_input(
            LiveTradingEvidenceBundleInput {
                run_id: "lt2-evidence",
                baseline_id: "LT2-LOCAL-FIXTURE",
                created_at_ms: 2,
                docs_checked: vec![
                    "https://docs.polymarket.com/api-reference/authentication".to_string()
                ],
                artifact_paths: paths,
                journal_state: &local,
                reconciliation: &reconciliation,
                gate: &gate,
                no_write_proof: LiveTradingNoWriteProof::default(),
            },
        ))
        .expect("bundle builds")
    }

    fn matching_local_state() -> crate::live_trading_journal::LiveTradingJournalState {
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

    fn matching_readback_fixture(
        local: &crate::live_trading_journal::LiveTradingJournalState,
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

    fn sample_order() -> IntendedMakerOrder {
        IntendedMakerOrder {
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
