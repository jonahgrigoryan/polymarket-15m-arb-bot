use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::domain::{Asset, PaperOrderStatus, RiskHaltReason};
use crate::events::EventType;

pub const MODULE: &str = "metrics";

pub const METRIC_FEED_MESSAGE_RATE: &str = "p15m_feed_message_rate_per_second";
pub const METRIC_FEED_LATENCY_MS: &str = "p15m_feed_latency_ms";
pub const METRIC_WEBSOCKET_RECONNECTS: &str = "p15m_websocket_reconnects_total";
pub const METRIC_BOOK_STALENESS_MS: &str = "p15m_book_staleness_ms";
pub const METRIC_REFERENCE_STALENESS_MS: &str = "p15m_reference_staleness_ms";
pub const METRIC_SIGNAL_DECISIONS: &str = "p15m_signal_decisions_total";
pub const METRIC_RISK_HALTS: &str = "p15m_risk_halts_total";
pub const METRIC_PAPER_ORDERS: &str = "p15m_paper_orders_total";
pub const METRIC_PAPER_FILLS: &str = "p15m_paper_fills_total";
pub const METRIC_PAPER_PNL: &str = "p15m_paper_pnl";
pub const METRIC_STORAGE_WRITE_FAILURES: &str = "p15m_storage_write_failures_total";
pub const METRIC_REPLAY_DETERMINISM_FAILURES: &str = "p15m_replay_determinism_failures_total";
pub const METRIC_LIVE_ALPHA_RECONCILIATION_STATUS: &str = "p15m_live_alpha_reconciliation_status";
pub const METRIC_LIVE_ALPHA_RECONCILIATION_MISMATCHES: &str =
    "p15m_live_alpha_reconciliation_mismatches_total";

pub const STRUCTURED_LOG_FIELDS: &[&str] = &[
    "run_id",
    "mode",
    "market_id",
    "asset",
    "source",
    "event_type",
    "reason",
    "risk_reason",
    "replay_fingerprint",
    "shutdown_phase",
    "accepting_new_work",
    "command_status",
    "error",
];

pub const REQUIRED_M8_METRIC_FAMILIES: &[&str] = &[
    METRIC_FEED_MESSAGE_RATE,
    METRIC_FEED_LATENCY_MS,
    METRIC_WEBSOCKET_RECONNECTS,
    METRIC_BOOK_STALENESS_MS,
    METRIC_REFERENCE_STALENESS_MS,
    METRIC_SIGNAL_DECISIONS,
    METRIC_RISK_HALTS,
    METRIC_PAPER_ORDERS,
    METRIC_PAPER_FILLS,
    METRIC_PAPER_PNL,
    METRIC_STORAGE_WRITE_FAILURES,
    METRIC_REPLAY_DETERMINISM_FAILURES,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Counter,
    Gauge,
}

impl MetricKind {
    fn as_str(self) -> &'static str {
        match self {
            MetricKind::Counter => "counter",
            MetricKind::Gauge => "gauge",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MetricSample {
    pub name: String,
    pub help: String,
    pub kind: MetricKind,
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

impl MetricSample {
    pub fn new(
        name: impl Into<String>,
        help: impl Into<String>,
        kind: MetricKind,
        value: f64,
    ) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            kind,
            labels: BTreeMap::new(),
            value,
        }
    }

    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricsSnapshot {
    samples: Vec<MetricSample>,
}

impl MetricsSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn samples(&self) -> &[MetricSample] {
        &self.samples
    }

    pub fn record_feed_message_rate(&mut self, source: &str, rate_per_second: f64) {
        self.push(
            MetricSample::new(
                METRIC_FEED_MESSAGE_RATE,
                "Feed message rate by source.",
                MetricKind::Gauge,
                rate_per_second,
            )
            .label("source", source),
        );
    }

    pub fn record_feed_latency_ms(&mut self, source: &str, latency_ms: f64) {
        self.push(
            MetricSample::new(
                METRIC_FEED_LATENCY_MS,
                "Feed message latency by source in milliseconds.",
                MetricKind::Gauge,
                latency_ms,
            )
            .label("source", source),
        );
    }

    pub fn record_websocket_reconnects(&mut self, source: &str, reconnects: u64) {
        self.push(
            MetricSample::new(
                METRIC_WEBSOCKET_RECONNECTS,
                "Read-only WebSocket reconnect count by source.",
                MetricKind::Counter,
                reconnects as f64,
            )
            .label("source", source),
        );
    }

    pub fn record_book_staleness_ms(&mut self, market_id: &str, token_id: &str, age_ms: f64) {
        self.push(
            MetricSample::new(
                METRIC_BOOK_STALENESS_MS,
                "Order book staleness by market and token in milliseconds.",
                MetricKind::Gauge,
                age_ms,
            )
            .label("market_id", market_id)
            .label("token_id", token_id),
        );
    }

    pub fn record_reference_staleness_ms(&mut self, asset: Asset, source: &str, age_ms: f64) {
        self.push(
            MetricSample::new(
                METRIC_REFERENCE_STALENESS_MS,
                "Reference feed staleness by asset and source in milliseconds.",
                MetricKind::Gauge,
                age_ms,
            )
            .label("asset", asset.symbol())
            .label("source", source),
        );
    }

    pub fn record_signal_decision(&mut self, market_id: &str, action: &str, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_SIGNAL_DECISIONS,
                "Signal decisions by market and action.",
                MetricKind::Counter,
                count as f64,
            )
            .label("market_id", market_id)
            .label("action", action),
        );
    }

    pub fn record_risk_halt(&mut self, reason: RiskHaltReason, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_RISK_HALTS,
                "Risk halt count by reason.",
                MetricKind::Counter,
                count as f64,
            )
            .label("reason", risk_halt_reason_label(reason)),
        );
    }

    pub fn record_paper_order(&mut self, status: PaperOrderStatus, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_PAPER_ORDERS,
                "Paper order count by status.",
                MetricKind::Counter,
                count as f64,
            )
            .label("status", paper_order_status_label(status)),
        );
    }

    pub fn record_paper_fill(&mut self, market_id: &str, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_PAPER_FILLS,
                "Paper fill count by market.",
                MetricKind::Counter,
                count as f64,
            )
            .label("market_id", market_id),
        );
    }

    pub fn record_paper_pnl(&mut self, market_id: &str, asset: Asset, kind: &str, value: f64) {
        self.push(
            MetricSample::new(
                METRIC_PAPER_PNL,
                "Paper P&L by market, asset, and kind.",
                MetricKind::Gauge,
                value,
            )
            .label("market_id", market_id)
            .label("asset", asset.symbol())
            .label("kind", kind),
        );
    }

    pub fn record_storage_write_failure(&mut self, operation: &str, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_STORAGE_WRITE_FAILURES,
                "Storage write failure count by operation.",
                MetricKind::Counter,
                count as f64,
            )
            .label("operation", operation),
        );
    }

    pub fn record_replay_determinism_failure(&mut self, replay_run_id: &str, count: u64) {
        self.push(
            MetricSample::new(
                METRIC_REPLAY_DETERMINISM_FAILURES,
                "Replay determinism failure count by replay run.",
                MetricKind::Counter,
                count as f64,
            )
            .label("replay_run_id", replay_run_id),
        );
    }

    pub fn record_live_alpha_reconciliation_status(&mut self, run_id: &str, healthy: bool) {
        self.push(
            MetricSample::new(
                METRIC_LIVE_ALPHA_RECONCILIATION_STATUS,
                "Live Alpha reconciliation health by run.",
                MetricKind::Gauge,
                if healthy { 1.0 } else { 0.0 },
            )
            .label("run_id", run_id),
        );
    }

    pub fn record_live_alpha_reconciliation_mismatch(
        &mut self,
        run_id: &str,
        mismatch: &str,
        count: u64,
    ) {
        self.push(
            MetricSample::new(
                METRIC_LIVE_ALPHA_RECONCILIATION_MISMATCHES,
                "Live Alpha reconciliation mismatch count by reason.",
                MetricKind::Counter,
                count as f64,
            )
            .label("run_id", run_id)
            .label("mismatch", mismatch),
        );
    }

    pub fn record_event_type_count(&mut self, event_type: EventType, count: u64) {
        self.push(
            MetricSample::new(
                "p15m_normalized_events_total",
                "Normalized event count by event type.",
                MetricKind::Counter,
                count as f64,
            )
            .label("event_type", event_type.as_str()),
        );
    }

    pub fn push(&mut self, sample: MetricSample) {
        self.samples.push(sample);
    }

    pub fn render_prometheus(&self) -> String {
        render_prometheus(&self.samples)
    }
}

pub fn m8_smoke_metrics_snapshot() -> MetricsSnapshot {
    let mut snapshot = MetricsSnapshot::new();
    snapshot.record_feed_message_rate("polymarket_clob", 0.0);
    snapshot.record_feed_latency_ms("polymarket_clob", 0.0);
    snapshot.record_websocket_reconnects("polymarket_clob", 0);
    snapshot.record_book_staleness_ms("market-smoke", "token-smoke", 0.0);
    snapshot.record_reference_staleness_ms(Asset::Btc, "chainlink", 0.0);
    snapshot.record_signal_decision("market-smoke", "skip", 0);
    snapshot.record_risk_halt(RiskHaltReason::StaleBook, 0);
    snapshot.record_paper_order(PaperOrderStatus::Open, 0);
    snapshot.record_paper_fill("market-smoke", 0);
    snapshot.record_paper_pnl("market-smoke", Asset::Btc, "realized", 0.0);
    snapshot.record_storage_write_failure("append_normalized_event", 0);
    snapshot.record_replay_determinism_failure("replay-smoke", 0);
    snapshot
}

pub fn required_structured_log_fields() -> &'static [&'static str] {
    STRUCTURED_LOG_FIELDS
}

pub fn required_m8_metric_families() -> &'static [&'static str] {
    REQUIRED_M8_METRIC_FAMILIES
}

pub async fn serve_prometheus_once(
    listener: TcpListener,
    metrics_body: String,
) -> MetricsResult<()> {
    let (mut stream, _) = listener.accept().await.map_err(MetricsError::Io)?;
    let mut buffer = [0u8; 1024];
    let bytes_read = stream.read(&mut buffer).await.map_err(MetricsError::Io)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    let (status, body) = if request.starts_with("GET /metrics ") {
        ("200 OK", metrics_body)
    } else {
        ("404 Not Found", "not found\n".to_string())
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(MetricsError::Io)?;
    stream.shutdown().await.map_err(MetricsError::Io)?;
    Ok(())
}

pub type MetricsResult<T> = Result<T, MetricsError>;

#[derive(Debug)]
pub enum MetricsError {
    Io(std::io::Error),
}

impl Display for MetricsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricsError::Io(source) => write!(formatter, "metrics endpoint I/O failed: {source}"),
        }
    }
}

impl Error for MetricsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MetricsError::Io(source) => Some(source),
        }
    }
}

fn render_prometheus(samples: &[MetricSample]) -> String {
    let mut output = String::new();
    let mut emitted_metadata = BTreeSet::new();
    let mut sorted_samples = samples.to_vec();
    sorted_samples.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.labels.cmp(&right.labels))
            .then_with(|| stable_f64(left.value).cmp(&stable_f64(right.value)))
    });

    for sample in sorted_samples {
        if emitted_metadata.insert(sample.name.clone()) {
            output.push_str("# HELP ");
            output.push_str(&sample.name);
            output.push(' ');
            output.push_str(&escape_help(&sample.help));
            output.push('\n');
            output.push_str("# TYPE ");
            output.push_str(&sample.name);
            output.push(' ');
            output.push_str(sample.kind.as_str());
            output.push('\n');
        }

        output.push_str(&sample.name);
        if !sample.labels.is_empty() {
            output.push('{');
            for (index, (key, value)) in sample.labels.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(key);
                output.push_str("=\"");
                output.push_str(&escape_label_value(value));
                output.push('"');
            }
            output.push('}');
        }
        output.push(' ');
        output.push_str(&format_metric_value(sample.value));
        output.push('\n');
    }

    output
}

fn format_metric_value(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn stable_f64(value: f64) -> u64 {
    if value.is_finite() {
        value.to_bits()
    } else {
        0
    }
}

fn escape_help(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

fn risk_halt_reason_label(reason: RiskHaltReason) -> &'static str {
    match reason {
        RiskHaltReason::Geoblocked => "geoblocked",
        RiskHaltReason::StaleReference => "stale_reference",
        RiskHaltReason::StaleBook => "stale_book",
        RiskHaltReason::MaxLossPerMarket => "max_loss_per_market",
        RiskHaltReason::MaxNotionalPerMarket => "max_notional_per_market",
        RiskHaltReason::MaxNotionalPerAsset => "max_notional_per_asset",
        RiskHaltReason::MaxTotalNotional => "max_total_notional",
        RiskHaltReason::MaxCorrelatedNotional => "max_correlated_notional",
        RiskHaltReason::OrderRateExceeded => "order_rate_exceeded",
        RiskHaltReason::DailyDrawdown => "daily_drawdown",
        RiskHaltReason::StorageUnavailable => "storage_unavailable",
        RiskHaltReason::IneligibleMarket => "ineligible_market",
        RiskHaltReason::Unknown => "unknown",
    }
}

fn paper_order_status_label(status: PaperOrderStatus) -> &'static str {
    match status {
        PaperOrderStatus::Created => "created",
        PaperOrderStatus::Open => "open",
        PaperOrderStatus::PartiallyFilled => "partially_filled",
        PaperOrderStatus::Filled => "filled",
        PaperOrderStatus::Canceled => "canceled",
        PaperOrderStatus::Expired => "expired",
        PaperOrderStatus::Rejected => "rejected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    #[test]
    fn prometheus_rendering_uses_stable_metric_names_and_labels() {
        let mut snapshot = MetricsSnapshot::new();
        snapshot.record_signal_decision("market-1", "candidate", 2);
        snapshot.record_feed_latency_ms("polymarket_clob", 12.5);
        snapshot.record_risk_halt(RiskHaltReason::StaleBook, 1);
        snapshot.record_paper_order(PaperOrderStatus::Filled, 3);

        let rendered = snapshot.render_prometheus();

        assert!(rendered.contains("# TYPE p15m_feed_latency_ms gauge\n"));
        assert!(rendered.contains("p15m_feed_latency_ms{source=\"polymarket_clob\"} 12.5\n"));
        assert!(rendered.contains(
            "p15m_signal_decisions_total{action=\"candidate\",market_id=\"market-1\"} 2\n"
        ));
        assert!(rendered.contains("p15m_risk_halts_total{reason=\"stale_book\"} 1\n"));
        assert!(rendered.contains("p15m_paper_orders_total{status=\"filled\"} 3\n"));
    }

    #[tokio::test]
    async fn one_shot_metrics_endpoint_serves_metrics_path_only() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback metrics listener");
        let address = listener.local_addr().expect("listener has local address");
        let body = m8_smoke_metrics_snapshot().render_prometheus();
        let server = tokio::spawn(serve_prometheus_once(listener, body));

        let mut stream = TcpStream::connect(address)
            .await
            .expect("connect loopback metrics endpoint");
        stream
            .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("request writes");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("response reads");
        server
            .await
            .expect("server joins")
            .expect("server succeeds");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains(METRIC_REPLAY_DETERMINISM_FAILURES));
        assert!(response.contains(METRIC_STORAGE_WRITE_FAILURES));
    }

    #[test]
    fn structured_log_field_contract_covers_m8_operational_fields() {
        let fields = required_structured_log_fields();

        for expected in [
            "run_id",
            "mode",
            "market_id",
            "asset",
            "source",
            "event_type",
            "reason",
            "risk_reason",
            "replay_fingerprint",
            "shutdown_phase",
            "accepting_new_work",
            "command_status",
            "error",
        ] {
            assert!(fields.contains(&expected), "missing field {expected}");
        }
    }

    #[test]
    fn m8_smoke_snapshot_renders_every_required_metric_family() {
        let rendered = m8_smoke_metrics_snapshot().render_prometheus();

        for metric_family in required_m8_metric_families() {
            let help_line = format!("# HELP {metric_family} ");
            let type_line = format!("# TYPE {metric_family} ");
            assert!(
                rendered.contains(&help_line),
                "missing HELP for {metric_family}"
            );
            assert!(
                rendered.contains(&type_line),
                "missing TYPE for {metric_family}"
            );
        }
    }

    #[test]
    fn live_alpha_reconciliation_metrics_render_health_and_mismatch() {
        let mut snapshot = MetricsSnapshot::new();
        snapshot.record_live_alpha_reconciliation_status("la1-run", false);
        snapshot.record_live_alpha_reconciliation_mismatch("la1-run", "unknown_open_order", 1);

        let rendered = snapshot.render_prometheus();

        assert!(rendered.contains(METRIC_LIVE_ALPHA_RECONCILIATION_STATUS));
        assert!(rendered.contains(METRIC_LIVE_ALPHA_RECONCILIATION_MISMATCHES));
        assert!(rendered.contains("mismatch=\"unknown_open_order\""));
        assert!(rendered.contains("run_id=\"la1-run\""));
    }

    #[test]
    fn observability_runbook_covers_live_beta_handoff_signals() {
        let runbook = include_str!("../docs/m8-observability-runbook.md");
        for required in [
            "LIVE_ORDER_PLACEMENT_ENABLED",
            "geoblock status",
            "kill-switch state",
            "heartbeat state",
            "attempted order count",
            "accepted order count",
            "rejected order count",
            "DELETE /order",
            "/data/order/{orderID}",
            "readback mismatch",
            "reserved pUSD",
            "open notional",
            "realized P&L",
            "settlement P&L",
        ] {
            assert!(
                runbook.contains(required),
                "observability runbook missing live-beta signal {required}"
            );
        }
    }
}
