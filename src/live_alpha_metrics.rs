use std::collections::BTreeMap;

pub const MODULE: &str = "live_alpha_metrics";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveAlphaMetrics {
    counters: BTreeMap<&'static str, u64>,
    gauges: BTreeMap<&'static str, f64>,
}

impl LiveAlphaMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment(&mut self, name: &'static str) {
        *self.counters.entry(name).or_insert(0) += 1;
    }

    pub fn set_gauge(&mut self, name: &'static str, value: f64) {
        self.gauges.insert(name, value);
    }

    pub fn render_prometheus(&self) -> String {
        let mut output = String::new();
        for name in LIVE_ALPHA_COUNTERS {
            output.push_str(name);
            output.push(' ');
            output.push_str(
                &self
                    .counters
                    .get(name)
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
            );
            output.push('\n');
        }
        for name in LIVE_ALPHA_GAUGES {
            output.push_str(name);
            output.push(' ');
            output.push_str(&format!(
                "{:.6}",
                self.gauges.get(name).copied().unwrap_or_default()
            ));
            output.push('\n');
        }
        output
    }
}

pub const LIVE_ALPHA_COUNTERS: &[&str] = &[
    "live_orders_submitted_total",
    "live_orders_accepted_total",
    "live_orders_rejected_total",
    "live_orders_filled_total",
    "live_orders_canceled_total",
    "live_unknown_open_orders_total",
    "live_reconciliation_mismatches_total",
    "live_risk_halts_total",
    "live_balance_mismatch_total",
    "live_position_mismatch_total",
    "live_reserved_balance_mismatch_total",
];

pub const LIVE_ALPHA_GAUGES: &[&str] = &[
    "live_submit_latency_ms",
    "live_cancel_latency_ms",
    "live_readback_latency_ms",
    "live_edge_at_submit_bps",
    "live_edge_at_fill_bps",
    "live_realized_pnl",
    "live_unrealized_pnl",
    "live_fee_spend",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_alpha_metrics_render_all_la5_families() {
        let mut metrics = LiveAlphaMetrics::new();
        metrics.increment("live_orders_submitted_total");
        metrics.set_gauge("live_edge_at_submit_bps", 12.5);

        let rendered = metrics.render_prometheus();

        for family in LIVE_ALPHA_COUNTERS.iter().chain(LIVE_ALPHA_GAUGES.iter()) {
            assert!(rendered.contains(family), "missing {family}");
        }
        assert!(rendered.contains("live_orders_submitted_total 1\n"));
        assert!(rendered.contains("live_edge_at_submit_bps 12.500000\n"));
    }
}
