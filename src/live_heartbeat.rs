use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::live_alpha_gate::LiveAlphaReadinessStatus;

pub const MODULE: &str = "live_heartbeat";
pub const OFFICIAL_HEARTBEAT_SDK_METHOD: &str = "postHeartbeat";
pub const HEARTBEAT_NETWORK_POST_ENABLED: bool = false;
pub const DEFAULT_EXPECTED_INTERVAL_MS: u64 = 5_000;
pub const DEFAULT_MAX_STALENESS_MS: u64 = 15_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatFailureAction {
    HaltAndReconcile,
    Unknown,
}

impl HeartbeatFailureAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HaltAndReconcile => "halt_and_reconcile",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatAction {
    HeartbeatNotStarted,
    HeartbeatHealthy,
    HeartbeatStale,
    HeartbeatRejected,
    HeartbeatUnknown,
}

impl HeartbeatAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HeartbeatNotStarted => "not_started",
            Self::HeartbeatHealthy => "healthy",
            Self::HeartbeatStale => "stale",
            Self::HeartbeatRejected => "rejected",
            Self::HeartbeatUnknown => "unknown",
        }
    }

    pub fn readiness_status(self) -> LiveAlphaReadinessStatus {
        match self {
            Self::HeartbeatHealthy => LiveAlphaReadinessStatus::Passed,
            Self::HeartbeatStale | Self::HeartbeatRejected => LiveAlphaReadinessStatus::Failed,
            Self::HeartbeatNotStarted | Self::HeartbeatUnknown => LiveAlphaReadinessStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveHeartbeatState {
    pub heartbeat_id: String,
    pub last_sent_at: Option<i64>,
    pub last_acknowledged_at: Option<i64>,
    pub expected_interval_ms: u64,
    pub max_staleness_ms: u64,
    pub associated_open_orders: BTreeSet<String>,
    pub heartbeat_enabled: bool,
    pub heartbeat_failure_action: HeartbeatFailureAction,
    #[serde(default)]
    pub last_rejected_at: Option<i64>,
}

impl Default for LiveHeartbeatState {
    fn default() -> Self {
        Self {
            heartbeat_id: String::new(),
            last_sent_at: None,
            last_acknowledged_at: None,
            expected_interval_ms: DEFAULT_EXPECTED_INTERVAL_MS,
            max_staleness_ms: DEFAULT_MAX_STALENESS_MS,
            associated_open_orders: BTreeSet::new(),
            heartbeat_enabled: false,
            heartbeat_failure_action: HeartbeatFailureAction::HaltAndReconcile,
            last_rejected_at: None,
        }
    }
}

impl LiveHeartbeatState {
    pub fn enabled() -> Self {
        Self {
            heartbeat_enabled: true,
            ..Self::default()
        }
    }

    pub fn with_associated_open_orders(
        mut self,
        order_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        self.associated_open_orders = order_ids.into_iter().collect();
        self
    }

    pub fn record_sent(&mut self, sent_at_ms: i64) {
        self.last_sent_at = Some(sent_at_ms);
    }

    pub fn record_acknowledged(
        &mut self,
        heartbeat_id: impl Into<String>,
        acknowledged_at_ms: i64,
    ) {
        self.heartbeat_id = heartbeat_id.into();
        self.last_acknowledged_at = Some(acknowledged_at_ms);
        self.last_rejected_at = None;
    }

    pub fn record_rejected(&mut self, heartbeat_id: impl Into<String>, rejected_at_ms: i64) {
        self.heartbeat_id = heartbeat_id.into();
        self.last_rejected_at = Some(rejected_at_ms);
    }

    pub fn evaluate(&self, now_ms: i64) -> HeartbeatAction {
        if !self.heartbeat_enabled {
            return HeartbeatAction::HeartbeatUnknown;
        }
        if self.last_rejected_at.is_some() {
            return HeartbeatAction::HeartbeatRejected;
        }

        match (self.last_sent_at, self.last_acknowledged_at) {
            (None, None) => HeartbeatAction::HeartbeatNotStarted,
            (Some(_), None) => HeartbeatAction::HeartbeatUnknown,
            (_, Some(acknowledged_at)) if now_ms < acknowledged_at => {
                HeartbeatAction::HeartbeatUnknown
            }
            (_, Some(acknowledged_at)) => {
                if now_ms.saturating_sub(acknowledged_at) as u64 > self.max_staleness_ms {
                    HeartbeatAction::HeartbeatStale
                } else {
                    HeartbeatAction::HeartbeatHealthy
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub accepted: bool,
    pub heartbeat_id: Option<String>,
    pub status: Option<String>,
}

pub fn parse_heartbeat_response(status_code: u16, body: &str) -> LiveHeartbeatResult<HeartbeatAck> {
    let wire: HeartbeatAckWire = serde_json::from_str(body).map_err(LiveHeartbeatError::Parse)?;
    let heartbeat_id = wire
        .heartbeat_id
        .filter(|heartbeat_id| !heartbeat_id.trim().is_empty());
    let status = wire.status.filter(|status| !status.trim().is_empty());

    match status_code {
        200..=299 if heartbeat_id.is_some() || is_ok_status(status.as_deref()) => {
            Ok(HeartbeatAck {
                accepted: true,
                heartbeat_id,
                status,
            })
        }
        200..=299 => Err(LiveHeartbeatError::MissingHeartbeatConfirmation),
        400 => {
            let heartbeat_id = heartbeat_id.ok_or(LiveHeartbeatError::MissingHeartbeatId)?;
            Ok(HeartbeatAck {
                accepted: false,
                heartbeat_id: Some(heartbeat_id),
                status,
            })
        }
        status => Err(LiveHeartbeatError::RejectedStatus(status)),
    }
}

#[derive(Debug, Deserialize)]
struct HeartbeatAckWire {
    heartbeat_id: Option<String>,
    status: Option<String>,
}

fn is_ok_status(status: Option<&str>) -> bool {
    status.is_some_and(|status| status.eq_ignore_ascii_case("ok"))
}

pub type LiveHeartbeatResult<T> = Result<T, LiveHeartbeatError>;

#[derive(Debug)]
pub enum LiveHeartbeatError {
    Parse(serde_json::Error),
    MissingHeartbeatId,
    MissingHeartbeatConfirmation,
    RejectedStatus(u16),
}

impl Display for LiveHeartbeatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(source) => write!(formatter, "heartbeat response parse failed: {source}"),
            Self::MissingHeartbeatId => {
                write!(formatter, "heartbeat response missing heartbeat_id")
            }
            Self::MissingHeartbeatConfirmation => {
                write!(
                    formatter,
                    "heartbeat response missing heartbeat_id or status=ok"
                )
            }
            Self::RejectedStatus(status) => {
                write!(formatter, "heartbeat response returned HTTP {status}")
            }
        }
    }
}

impl Error for LiveHeartbeatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parse(source) => Some(source),
            Self::MissingHeartbeatId
            | Self::MissingHeartbeatConfirmation
            | Self::RejectedStatus(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_heartbeat_tracks_healthy_ack_with_official_interval_defaults() {
        let mut state = LiveHeartbeatState::enabled();
        state.record_sent(1_000);
        state.record_acknowledged("hb-1", 1_100);

        assert_eq!(state.expected_interval_ms, 5_000);
        assert_eq!(state.max_staleness_ms, 15_000);
        assert_eq!(state.evaluate(10_000), HeartbeatAction::HeartbeatHealthy);
        assert_eq!(
            state.evaluate(10_000).readiness_status(),
            LiveAlphaReadinessStatus::Passed
        );
    }

    #[test]
    fn live_heartbeat_stale_or_not_started_is_not_gate_ready() {
        let mut state = LiveHeartbeatState::enabled();

        assert_eq!(state.evaluate(1_000), HeartbeatAction::HeartbeatNotStarted);
        assert_eq!(
            state.evaluate(1_000).readiness_status(),
            LiveAlphaReadinessStatus::Unknown
        );

        state.record_acknowledged("hb-1", 1_000);

        assert_eq!(state.evaluate(17_000), HeartbeatAction::HeartbeatStale);
        assert_eq!(
            state.evaluate(17_000).readiness_status(),
            LiveAlphaReadinessStatus::Failed
        );
    }

    #[test]
    fn live_heartbeat_rejected_response_updates_correct_heartbeat_id() {
        let ack = parse_heartbeat_response(400, r#"{"heartbeat_id":"hb-current"}"#)
            .expect("400 heartbeat with current id parses");
        let mut state = LiveHeartbeatState::enabled();
        state.record_rejected(
            ack.heartbeat_id
                .expect("rejected heartbeat ack keeps current id"),
            2_000,
        );

        assert!(!ack.accepted);
        assert_eq!(state.heartbeat_id, "hb-current");
        assert_eq!(state.evaluate(2_001), HeartbeatAction::HeartbeatRejected);
    }

    #[test]
    fn live_heartbeat_accepts_documented_rest_status_ok_response() {
        let ack = parse_heartbeat_response(200, r#"{"status":"ok"}"#)
            .expect("REST heartbeat status response parses");

        assert!(ack.accepted);
        assert_eq!(ack.heartbeat_id, None);
        assert_eq!(ack.status.as_deref(), Some("ok"));
    }

    #[test]
    fn live_heartbeat_post_network_path_is_not_enabled_in_la2() {
        assert!(!HEARTBEAT_NETWORK_POST_ENABLED);
        assert_eq!(OFFICIAL_HEARTBEAT_SDK_METHOD, "postHeartbeat");
    }
}
