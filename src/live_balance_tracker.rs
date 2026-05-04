use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "live_balance_tracker";
pub const BALANCE_EPSILON: f64 = 0.000_001;

fn default_conditional_token_positions_evidence_complete() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveBalanceSnapshot {
    pub p_usd_available: f64,
    pub p_usd_reserved: f64,
    pub p_usd_total: f64,
    pub conditional_token_positions: BTreeMap<String, f64>,
    /// When false, `conditional_token_positions` on this snapshot is not authoritative venue
    /// readback (e.g. LA2 preflight only populates collateral). Reconciliation must not treat an
    /// empty map as proof of zero conditional-token exposure at the venue.
    #[serde(default = "default_conditional_token_positions_evidence_complete")]
    pub conditional_token_positions_evidence_complete: bool,
    pub balance_snapshot_at: i64,
    pub source: String,
}

impl LiveBalanceSnapshot {
    pub fn matches(&self, other: &Self) -> bool {
        nearly_equal(self.p_usd_available, other.p_usd_available)
            && nearly_equal(self.p_usd_reserved, other.p_usd_reserved)
            && nearly_equal(self.p_usd_total, other.p_usd_total)
            && maps_match(
                &self.conditional_token_positions,
                &other.conditional_token_positions,
            )
    }

    pub fn matches_p_usd_fields(&self, other: &Self) -> bool {
        nearly_equal(self.p_usd_available, other.p_usd_available)
            && nearly_equal(self.p_usd_reserved, other.p_usd_reserved)
            && nearly_equal(self.p_usd_total, other.p_usd_total)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveBalanceTracker {
    snapshots: Vec<LiveBalanceSnapshot>,
}

impl LiveBalanceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_snapshot(&mut self, snapshot: LiveBalanceSnapshot) {
        self.snapshots.push(snapshot);
    }

    pub fn latest(&self) -> Option<&LiveBalanceSnapshot> {
        self.snapshots.last()
    }

    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }
}

pub fn nearly_equal(left: f64, right: f64) -> bool {
    (left - right).abs() <= BALANCE_EPSILON
}

fn maps_match(left: &BTreeMap<String, f64>, right: &BTreeMap<String, f64>) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter().all(|(key, value)| {
        right
            .get(key)
            .is_some_and(|other| nearly_equal(*value, *other))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_balance_tracker_records_latest_snapshot() {
        let mut tracker = LiveBalanceTracker::new();
        tracker.apply_snapshot(sample_balance(10.0, 0.0, 10.0));
        tracker.apply_snapshot(sample_balance(8.0, 2.0, 10.0));

        assert_eq!(tracker.snapshot_count(), 2);
        assert_eq!(tracker.latest().expect("latest").p_usd_reserved, 2.0);
    }

    #[test]
    fn live_balance_tracker_detects_balance_mismatch() {
        let expected = sample_balance(10.0, 0.0, 10.0);
        let observed = sample_balance(9.0, 1.0, 10.0);

        assert!(!expected.matches(&observed));
    }

    pub fn sample_balance(available: f64, reserved: f64, total: f64) -> LiveBalanceSnapshot {
        LiveBalanceSnapshot {
            p_usd_available: available,
            p_usd_reserved: reserved,
            p_usd_total: total,
            conditional_token_positions: BTreeMap::new(),
            conditional_token_positions_evidence_complete: true,
            balance_snapshot_at: 1_777_000_000_000,
            source: "fixture".to_string(),
        }
    }
}
