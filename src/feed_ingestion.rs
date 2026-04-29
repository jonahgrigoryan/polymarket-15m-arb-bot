use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ring::digest::{digest, SHA1_FOR_LEGACY_USE_ONLY};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Instant};
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use url::Url;

use crate::normalization::{
    normalize_feed_message, NormalizationError, SOURCE_BINANCE, SOURCE_COINBASE,
    SOURCE_POLYMARKET_CLOB, SOURCE_RESOLUTION,
};
use crate::storage::{RawMessage, StorageBackend, StorageError, StorageResult};

pub const MODULE: &str = "feed_ingestion";

const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_HANDSHAKE_BYTES: usize = 16 * 1024;
const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolymarketMarketSubscription {
    pub asset_ids: Vec<String>,
    pub custom_feature_enabled: bool,
}

impl PolymarketMarketSubscription {
    pub fn new(asset_ids: Vec<String>) -> Self {
        Self {
            asset_ids,
            custom_feature_enabled: true,
        }
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "assets_ids": self.asset_ids,
            "type": "market",
            "custom_feature_enabled": self.custom_feature_enabled,
        })
    }

    pub fn to_payload(&self) -> String {
        self.to_json_value().to_string()
    }
}

pub fn coinbase_ticker_subscription() -> String {
    json!({
        "type": "subscribe",
        "product_ids": ["BTC-USD", "ETH-USD", "SOL-USD"],
        "channels": ["ticker"],
    })
    .to_string()
}

pub fn binance_combined_trade_url(base_ws_url: &str) -> String {
    let base = base_ws_url.trim_end_matches('/').trim_end_matches("/ws");
    format!("{base}/stream?streams=btcusdt@trade/ethusdt@trade/solusdt@trade")
}

#[derive(Debug, Clone)]
pub struct PolymarketBookSnapshotClient {
    http: reqwest::Client,
    clob_rest_url: String,
}

impl PolymarketBookSnapshotClient {
    pub fn new(clob_rest_url: impl Into<String>, timeout_ms: u64) -> FeedResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| FeedError::Network {
                operation: "book_snapshot_client_build",
                message: source.to_string(),
            })?;

        Ok(Self {
            http,
            clob_rest_url: clob_rest_url.into(),
        })
    }

    pub async fn fetch_book(&self, token_id: &str) -> FeedResult<String> {
        let url = format!("{}/book", self.clob_rest_url.trim_end_matches('/'));
        let response = self
            .http
            .get(&url)
            .query(&[("token_id", token_id)])
            .send()
            .await
            .map_err(|source| FeedError::Network {
                operation: "book_snapshot_request",
                message: source.to_string(),
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(FeedError::Protocol(format!(
                "book snapshot request to {url} returned HTTP {status}"
            )));
        }

        response.text().await.map_err(|source| FeedError::Network {
            operation: "book_snapshot_body",
            message: source.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedConnectionConfig {
    pub source: String,
    pub ws_url: String,
    pub subscribe_payload: Option<String>,
    pub message_limit: usize,
    pub connect_timeout_ms: u64,
    pub read_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedProbeResult {
    pub source: String,
    pub connected: bool,
    pub received_text_messages: Vec<String>,
    pub close_received: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOnlyWebSocketClient;

impl ReadOnlyWebSocketClient {
    pub async fn connect_and_capture(
        &self,
        config: &FeedConnectionConfig,
    ) -> FeedResult<FeedProbeResult> {
        let url = Url::parse(&config.ws_url).map_err(|source| FeedError::InvalidUrl {
            url: config.ws_url.clone(),
            message: source.to_string(),
        })?;
        let mut stream = connect_stream(&url, config.connect_timeout_ms).await?;
        let key = websocket_key();
        send_handshake(&mut stream, &url, &key, config.connect_timeout_ms).await?;
        read_handshake(&mut stream, &key, config.connect_timeout_ms).await?;

        if let Some(payload) = &config.subscribe_payload {
            send_text_frame(&mut stream, payload.as_bytes()).await?;
        }

        let mut received_text_messages = Vec::new();
        let mut close_received = false;
        let mut idle_heartbeats = 0_u8;
        let capture_deadline = Instant::now() + websocket_capture_deadline(config);
        while received_text_messages.len() < config.message_limit {
            let remaining = capture_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let frame_timeout_ms = duration_ms_at_least_one(std::cmp::min(
                Duration::from_millis(config.read_timeout_ms),
                remaining,
            ));
            match read_frame(&mut stream, frame_timeout_ms).await {
                Ok(WebSocketFrame::Text(payload)) => {
                    idle_heartbeats = 0;
                    if !is_text_heartbeat(&payload) {
                        received_text_messages.push(payload);
                    }
                }
                Ok(WebSocketFrame::Ping(payload)) => {
                    idle_heartbeats = 0;
                    send_pong_frame(&mut stream, &payload).await?;
                }
                Ok(WebSocketFrame::Pong) => {
                    idle_heartbeats = 0;
                }
                Ok(WebSocketFrame::Close) => {
                    close_received = true;
                    break;
                }
                Ok(WebSocketFrame::Binary) => {
                    idle_heartbeats = 0;
                }
                Err(FeedError::Timeout {
                    operation: "websocket_frame_header",
                    ..
                }) if capture_deadline
                    .saturating_duration_since(Instant::now())
                    .is_zero() =>
                {
                    break;
                }
                Err(FeedError::Timeout {
                    operation: "websocket_frame_header",
                    ..
                }) if idle_heartbeats < 3 => {
                    idle_heartbeats += 1;
                    send_text_frame(&mut stream, b"PING").await?;
                }
                Err(error) => return Err(error),
            }
        }

        Ok(FeedProbeResult {
            source: config.source.clone(),
            connected: true,
            received_text_messages,
            close_received,
        })
    }
}

fn websocket_capture_deadline(config: &FeedConnectionConfig) -> Duration {
    Duration::from_millis(config.read_timeout_ms.saturating_mul(4).max(1))
}

fn duration_ms_at_least_one(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}

trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> AsyncReadWrite for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

type BoxedAsyncStream = Box<dyn AsyncReadWrite>;

async fn connect_stream(url: &Url, timeout_ms: u64) -> FeedResult<BoxedAsyncStream> {
    let host = url
        .host_str()
        .ok_or_else(|| FeedError::InvalidUrl {
            url: url.to_string(),
            message: "missing host".to_string(),
        })?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| FeedError::InvalidUrl {
            url: url.to_string(),
            message: "missing port".to_string(),
        })?;
    let tcp = timeout(
        Duration::from_millis(timeout_ms),
        TcpStream::connect((host.as_str(), port)),
    )
    .await
    .map_err(|_| FeedError::Timeout {
        operation: "tcp_connect",
        timeout_ms,
    })?
    .map_err(|source| FeedError::Network {
        operation: "tcp_connect",
        message: source.to_string(),
    })?;

    match url.scheme() {
        "ws" => Ok(Box::new(tcp)),
        "wss" => {
            let roots = native_root_store()?;
            let config = ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(config));
            let server_name =
                ServerName::try_from(host.clone()).map_err(|_| FeedError::InvalidUrl {
                    url: url.to_string(),
                    message: format!("invalid TLS server name {host}"),
                })?;
            let tls = timeout(
                Duration::from_millis(timeout_ms),
                connector.connect(server_name, tcp),
            )
            .await
            .map_err(|_| FeedError::Timeout {
                operation: "tls_connect",
                timeout_ms,
            })?
            .map_err(|source| FeedError::Network {
                operation: "tls_connect",
                message: source.to_string(),
            })?;
            Ok(Box::new(tls))
        }
        scheme => Err(FeedError::InvalidUrl {
            url: url.to_string(),
            message: format!("unsupported WebSocket scheme {scheme}"),
        }),
    }
}

fn native_root_store() -> FeedResult<RootCertStore> {
    let certs = rustls_native_certs::load_native_certs();
    let mut roots = RootCertStore::empty();
    for cert in certs.certs {
        roots.add(cert).map_err(|source| FeedError::Network {
            operation: "load_native_cert",
            message: source.to_string(),
        })?;
    }
    if roots.is_empty() {
        return Err(FeedError::Network {
            operation: "load_native_certs",
            message: "no native root certificates loaded".to_string(),
        });
    }
    Ok(roots)
}

async fn send_handshake(
    stream: &mut BoxedAsyncStream,
    url: &Url,
    key: &str,
    timeout_ms: u64,
) -> FeedResult<()> {
    let host = host_header(url)?;
    let path = path_and_query(url);
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n"
    );
    timeout(
        Duration::from_millis(timeout_ms),
        stream.write_all(request.as_bytes()),
    )
    .await
    .map_err(|_| FeedError::Timeout {
        operation: "websocket_handshake_write",
        timeout_ms,
    })?
    .map_err(|source| FeedError::Network {
        operation: "websocket_handshake_write",
        message: source.to_string(),
    })?;
    stream.flush().await.map_err(|source| FeedError::Network {
        operation: "websocket_handshake_flush",
        message: source.to_string(),
    })
}

async fn read_handshake(
    stream: &mut BoxedAsyncStream,
    key: &str,
    timeout_ms: u64,
) -> FeedResult<()> {
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while response.len() < MAX_HANDSHAKE_BYTES {
        timeout(
            Duration::from_millis(timeout_ms),
            stream.read_exact(&mut byte),
        )
        .await
        .map_err(|_| FeedError::Timeout {
            operation: "websocket_handshake_read",
            timeout_ms,
        })?
        .map_err(|source| FeedError::Network {
            operation: "websocket_handshake_read",
            message: source.to_string(),
        })?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&response);
            return validate_handshake_response(&text, key);
        }
    }

    Err(FeedError::Protocol(
        "websocket handshake exceeded maximum response size".to_string(),
    ))
}

fn validate_handshake_response(response: &str, key: &str) -> FeedResult<()> {
    if !response.starts_with("HTTP/1.1 101") && !response.starts_with("HTTP/2 101") {
        return Err(FeedError::Protocol(format!(
            "websocket handshake did not return 101: {}",
            response.lines().next().unwrap_or_default()
        )));
    }

    let expected_accept = websocket_accept(key);
    let observed_accept = response.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("sec-websocket-accept") {
            Some(value.trim().to_string())
        } else {
            None
        }
    });

    match observed_accept {
        Some(value) if value == expected_accept => Ok(()),
        Some(value) => Err(FeedError::Protocol(format!(
            "websocket accept mismatch expected={expected_accept} observed={value}"
        ))),
        None => Err(FeedError::Protocol(
            "websocket handshake missing Sec-WebSocket-Accept".to_string(),
        )),
    }
}

fn path_and_query(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => {
            if url.path().is_empty() {
                "/".to_string()
            } else {
                url.path().to_string()
            }
        }
    }
}

fn host_header(url: &Url) -> FeedResult<String> {
    let host = url.host_str().ok_or_else(|| FeedError::InvalidUrl {
        url: url.to_string(),
        message: "missing host".to_string(),
    })?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn websocket_key() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id() as u128;
    let mixed = now ^ (pid << 64) ^ 0x15_15_15_15_42_42_42_42_u128;
    BASE64.encode(mixed.to_be_bytes())
}

fn websocket_accept(key: &str) -> String {
    let mut bytes = Vec::with_capacity(key.len() + WEBSOCKET_GUID.len());
    bytes.extend_from_slice(key.as_bytes());
    bytes.extend_from_slice(WEBSOCKET_GUID.as_bytes());
    BASE64.encode(digest(&SHA1_FOR_LEGACY_USE_ONLY, &bytes).as_ref())
}

fn is_text_heartbeat(payload: &str) -> bool {
    if payload.trim().is_empty() {
        return true;
    }
    matches!(
        payload.trim().to_ascii_uppercase().as_str(),
        "PING" | "PONG"
    )
}

async fn send_text_frame(stream: &mut BoxedAsyncStream, payload: &[u8]) -> FeedResult<()> {
    send_client_frame(stream, 0x1, payload).await
}

async fn send_pong_frame(stream: &mut BoxedAsyncStream, payload: &[u8]) -> FeedResult<()> {
    send_client_frame(stream, 0xA, payload).await
}

async fn send_client_frame(
    stream: &mut BoxedAsyncStream,
    opcode: u8,
    payload: &[u8],
) -> FeedResult<()> {
    let mut frame = Vec::new();
    frame.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | len as u8);
    } else if u16::try_from(len).is_ok() {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    let mask = websocket_mask();
    frame.extend_from_slice(&mask);
    for (index, byte) in payload.iter().enumerate() {
        frame.push(byte ^ mask[index % 4]);
    }

    stream
        .write_all(&frame)
        .await
        .map_err(|source| FeedError::Network {
            operation: "websocket_frame_write",
            message: source.to_string(),
        })?;
    stream.flush().await.map_err(|source| FeedError::Network {
        operation: "websocket_frame_flush",
        message: source.to_string(),
    })
}

fn websocket_mask() -> [u8; 4] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    (now as u32 ^ std::process::id()).to_be_bytes()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WebSocketFrame {
    Text(String),
    Binary,
    Ping(Vec<u8>),
    Pong,
    Close,
}

async fn read_frame(
    stream: &mut BoxedAsyncStream,
    read_timeout_ms: u64,
) -> FeedResult<WebSocketFrame> {
    let mut header = [0_u8; 2];
    read_exact_with_timeout(
        stream,
        &mut header,
        read_timeout_ms,
        "websocket_frame_header",
    )
    .await?;
    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    let mut len = u64::from(header[1] & 0x7f);

    if len == 126 {
        let mut extended = [0_u8; 2];
        read_exact_with_timeout(
            stream,
            &mut extended,
            read_timeout_ms,
            "websocket_frame_len16",
        )
        .await?;
        len = u64::from(u16::from_be_bytes(extended));
    } else if len == 127 {
        let mut extended = [0_u8; 8];
        read_exact_with_timeout(
            stream,
            &mut extended,
            read_timeout_ms,
            "websocket_frame_len64",
        )
        .await?;
        len = u64::from_be_bytes(extended);
    }

    if len > MAX_FRAME_BYTES {
        return Err(FeedError::Protocol(format!(
            "websocket frame too large: {len} bytes"
        )));
    }

    let mut mask = [0_u8; 4];
    if masked {
        read_exact_with_timeout(stream, &mut mask, read_timeout_ms, "websocket_frame_mask").await?;
    }

    let mut payload = vec![0_u8; len as usize];
    if len > 0 {
        read_exact_with_timeout(
            stream,
            &mut payload,
            read_timeout_ms,
            "websocket_frame_payload",
        )
        .await?;
    }

    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }

    match opcode {
        0x1 => String::from_utf8(payload)
            .map(WebSocketFrame::Text)
            .map_err(|source| FeedError::Protocol(source.to_string())),
        0x2 => Ok(WebSocketFrame::Binary),
        0x8 => Ok(WebSocketFrame::Close),
        0x9 => Ok(WebSocketFrame::Ping(payload)),
        0xA => Ok(WebSocketFrame::Pong),
        _ => Err(FeedError::Protocol(format!(
            "unsupported websocket opcode {opcode}"
        ))),
    }
}

async fn read_exact_with_timeout(
    stream: &mut BoxedAsyncStream,
    buffer: &mut [u8],
    timeout_ms: u64,
    operation: &'static str,
) -> FeedResult<()> {
    timeout(Duration::from_millis(timeout_ms), stream.read_exact(buffer))
        .await
        .map_err(|_| FeedError::Timeout {
            operation,
            timeout_ms,
        })?
        .map(|_| ())
        .map_err(|source| FeedError::Network {
            operation,
            message: source.to_string(),
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedStatus {
    NeverConnected,
    Connected,
    Stale,
    Disconnected,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedHealth {
    pub source: String,
    pub status: FeedStatus,
    pub last_recv_wall_ts: Option<i64>,
    pub last_source_ts: Option<i64>,
    pub stale_after_ms: u64,
    pub message_count: u64,
    pub disconnect_count: u64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedHealthTracker {
    health: FeedHealth,
}

impl FeedHealthTracker {
    pub fn new(source: impl Into<String>, stale_after_ms: u64) -> Self {
        Self {
            health: FeedHealth {
                source: source.into(),
                status: FeedStatus::NeverConnected,
                last_recv_wall_ts: None,
                last_source_ts: None,
                stale_after_ms,
                message_count: 0,
                disconnect_count: 0,
                reason: None,
            },
        }
    }

    pub fn mark_connected(&mut self, now_ms: i64) {
        self.health.status = FeedStatus::Connected;
        self.health.last_recv_wall_ts = Some(now_ms);
        self.health.reason = None;
    }

    pub fn mark_message(&mut self, recv_wall_ts: i64, source_ts: Option<i64>) {
        self.health.status = FeedStatus::Connected;
        self.health.last_recv_wall_ts = Some(recv_wall_ts);
        self.health.last_source_ts = source_ts;
        self.health.message_count += 1;
        self.health.reason = None;
    }

    pub fn mark_disconnected(&mut self, reason: impl Into<String>) {
        self.health.status = FeedStatus::Disconnected;
        self.health.disconnect_count += 1;
        self.health.reason = Some(reason.into());
    }

    pub fn mark_degraded(&mut self, reason: impl Into<String>) {
        self.health.status = FeedStatus::Degraded;
        self.health.reason = Some(reason.into());
    }

    pub fn observe(&mut self, now_ms: i64) -> FeedHealth {
        if matches!(self.health.status, FeedStatus::Connected) {
            if let Some(last_recv_wall_ts) = self.health.last_recv_wall_ts {
                if now_ms.saturating_sub(last_recv_wall_ts) > self.health.stale_after_ms as i64 {
                    self.health.status = FeedStatus::Stale;
                    self.health.reason = Some(format!(
                        "no message for {}ms",
                        now_ms.saturating_sub(last_recv_wall_ts)
                    ));
                }
            }
        }

        self.health.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub max_attempts: u16,
}

impl ReconnectPolicy {
    pub fn delay_for_attempt(&self, attempt: u16) -> Option<u64> {
        if attempt >= self.max_attempts {
            return None;
        }

        let multiplier = 1_u64.checked_shl(u32::from(attempt)).unwrap_or(u64::MAX);
        Some(
            self.initial_backoff_ms
                .saturating_mul(multiplier)
                .min(self.max_backoff_ms),
        )
    }
}

pub trait ResolutionSourceAdapter {
    fn source_name(&self) -> &'static str;

    fn normalize(
        &self,
        payload: &str,
        recv_wall_ts: i64,
    ) -> Result<crate::normalization::NormalizedFeedBatch, NormalizationError>;
}

#[derive(Debug, Clone, Copy)]
pub struct GenericResolutionSourceAdapter;

impl ResolutionSourceAdapter for GenericResolutionSourceAdapter {
    fn source_name(&self) -> &'static str {
        SOURCE_RESOLUTION
    }

    fn normalize(
        &self,
        payload: &str,
        recv_wall_ts: i64,
    ) -> Result<crate::normalization::NormalizedFeedBatch, NormalizationError> {
        normalize_feed_message(SOURCE_RESOLUTION, payload, recv_wall_ts)
    }
}

pub struct FeedRecorder<'a, S: StorageBackend> {
    storage: &'a S,
    run_id: String,
    source: String,
}

impl<'a, S: StorageBackend> FeedRecorder<'a, S> {
    pub fn new(storage: &'a S, run_id: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            storage,
            run_id: run_id.into(),
            source: source.into(),
        }
    }

    pub fn record_message(
        &self,
        payload: impl Into<String>,
        recv_wall_ts: i64,
        recv_mono_ns: u64,
        ingest_seq: u64,
    ) -> FeedResult<RecordedFeedMessage> {
        let payload = payload.into();
        self.storage.append_raw_message(RawMessage {
            run_id: self.run_id.clone(),
            source: self.source.clone(),
            recv_wall_ts,
            recv_mono_ns,
            ingest_seq,
            payload: payload.clone(),
        })?;

        let batch = normalize_feed_message(&self.source, &payload, recv_wall_ts)?;
        let normalized_event_count = batch.events.len();
        for (index, event) in batch.events.into_iter().enumerate() {
            let envelope = crate::events::EventEnvelope::new(
                &self.run_id,
                format!("{}-{}-{}", self.source, ingest_seq, index),
                &self.source,
                recv_wall_ts,
                recv_mono_ns + index as u64,
                ingest_seq + index as u64,
                event,
            );
            self.storage.append_normalized_event(envelope)?;
        }

        Ok(RecordedFeedMessage {
            raw_event_type: batch.raw_event_type,
            unknown_event_type: batch.unknown_event_type,
            normalized_event_count,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedFeedMessage {
    pub raw_event_type: Option<String>,
    pub unknown_event_type: Option<String>,
    pub normalized_event_count: usize,
}

pub type FeedResult<T> = Result<T, FeedError>;

#[derive(Debug)]
pub enum FeedError {
    InvalidUrl {
        url: String,
        message: String,
    },
    Timeout {
        operation: &'static str,
        timeout_ms: u64,
    },
    Network {
        operation: &'static str,
        message: String,
    },
    Protocol(String),
    Normalize(NormalizationError),
    Storage(StorageError),
}

impl From<NormalizationError> for FeedError {
    fn from(value: NormalizationError) -> Self {
        Self::Normalize(value)
    }
}

impl From<StorageError> for FeedError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl Display for FeedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FeedError::InvalidUrl { url, message } => {
                write!(formatter, "invalid feed WebSocket URL {url}: {message}")
            }
            FeedError::Timeout {
                operation,
                timeout_ms,
            } => {
                write!(
                    formatter,
                    "feed operation {operation} timed out after {timeout_ms}ms"
                )
            }
            FeedError::Network { operation, message } => {
                write!(formatter, "feed operation {operation} failed: {message}")
            }
            FeedError::Protocol(message) => write!(formatter, "feed protocol error: {message}"),
            FeedError::Normalize(error) => write!(formatter, "{error}"),
            FeedError::Storage(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for FeedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FeedError::Normalize(error) => Some(error),
            FeedError::Storage(error) => Some(error),
            _ => None,
        }
    }
}

pub fn source_labels() -> [&'static str; 4] {
    [
        SOURCE_POLYMARKET_CLOB,
        SOURCE_BINANCE,
        SOURCE_COINBASE,
        SOURCE_RESOLUTION,
    ]
}

#[allow(dead_code)]
fn storage_result_from_feed<T>(result: FeedResult<T>) -> StorageResult<T> {
    result.map_err(|error| StorageError::backend("feed_ingestion", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NormalizedEvent;
    use crate::storage::{InMemoryStorage, StorageBackend};

    #[test]
    fn polymarket_subscription_uses_documented_assets_ids_field() {
        let subscription = PolymarketMarketSubscription::new(vec![
            "token-up".to_string(),
            "token-down".to_string(),
        ]);
        let payload = subscription.to_json_value();

        assert_eq!(payload["type"], "market");
        assert_eq!(payload["custom_feature_enabled"], true);
        assert!(payload.get("assets_ids").is_some());
        assert!(payload.get("asset_ids").is_none());
    }

    #[test]
    fn cex_subscription_builders_cover_default_assets() {
        let binance = binance_combined_trade_url("wss://stream.binance.com:9443/ws");
        assert_eq!(
            binance,
            "wss://stream.binance.com:9443/stream?streams=btcusdt@trade/ethusdt@trade/solusdt@trade"
        );
        let coinbase = coinbase_ticker_subscription();
        assert!(coinbase.contains("BTC-USD"));
        assert!(coinbase.contains("ETH-USD"));
        assert!(coinbase.contains("SOL-USD"));
    }

    #[test]
    fn feed_recorder_persists_raw_and_normalized_messages() {
        let storage = InMemoryStorage::default();
        let recorder = FeedRecorder::new(&storage, "run-m3", SOURCE_POLYMARKET_CLOB);

        let recorded = recorder
            .record_message(
                r#"{
                  "event_type": "book",
                  "asset_id": "token-up",
                  "market": "condition-1",
                  "bids": [{"price": ".48", "size": "30"}],
                  "asks": [{"price": ".52", "size": "25"}],
                  "timestamp": "1757908892351",
                  "hash": "book-hash"
                }"#,
                1_777_000_000_000,
                10,
                20,
            )
            .expect("message records");
        let events = storage.read_run_events("run-m3").expect("events read");

        assert_eq!(recorded.normalized_event_count, 1);
        assert_eq!(storage.raw_message_count().expect("raw count"), 1);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].payload,
            NormalizedEvent::BookSnapshot { .. }
        ));
    }

    #[test]
    fn unknown_feed_event_is_still_raw_persisted() {
        let storage = InMemoryStorage::default();
        let recorder = FeedRecorder::new(&storage, "run-m3", SOURCE_POLYMARKET_CLOB);

        let recorded = recorder
            .record_message(
                r#"{"event_type":"unexpected_new_type","market":"condition-1"}"#,
                1_777_000_000_000,
                10,
                20,
            )
            .expect("unknown message records");

        assert_eq!(recorded.normalized_event_count, 0);
        assert_eq!(
            recorded.unknown_event_type.as_deref(),
            Some("unexpected_new_type")
        );
        assert_eq!(storage.raw_message_count().expect("raw count"), 1);
        assert_eq!(storage.read_run_events("run-m3").expect("events").len(), 0);
    }

    #[test]
    fn feed_health_reports_stale_after_threshold() {
        let mut health = FeedHealthTracker::new(SOURCE_POLYMARKET_CLOB, 1_000);

        health.mark_connected(1_000);
        health.mark_message(1_500, Some(1_400));
        assert_eq!(health.observe(2_000).status, FeedStatus::Connected);

        let observed = health.observe(2_600);
        assert_eq!(observed.status, FeedStatus::Stale);
        assert!(observed
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("no message"));
    }

    #[test]
    fn reconnect_policy_is_bounded() {
        let policy = ReconnectPolicy {
            initial_backoff_ms: 250,
            max_backoff_ms: 1_000,
            max_attempts: 4,
        };

        assert_eq!(policy.delay_for_attempt(0), Some(250));
        assert_eq!(policy.delay_for_attempt(1), Some(500));
        assert_eq!(policy.delay_for_attempt(2), Some(1_000));
        assert_eq!(policy.delay_for_attempt(3), Some(1_000));
        assert_eq!(policy.delay_for_attempt(4), None);
    }

    #[test]
    fn websocket_accept_matches_rfc_sample() {
        assert_eq!(
            websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn text_heartbeat_messages_are_not_feed_payloads() {
        assert!(is_text_heartbeat("PONG"));
        assert!(is_text_heartbeat(" ping "));
        assert!(is_text_heartbeat(" "));
        assert!(!is_text_heartbeat(r#"{"event_type":"book"}"#));
    }

    #[test]
    fn websocket_capture_deadline_is_wall_clock_bounded() {
        let config = FeedConnectionConfig {
            source: SOURCE_POLYMARKET_CLOB.to_string(),
            ws_url: "wss://example.test/ws".to_string(),
            subscribe_payload: None,
            message_limit: 20,
            connect_timeout_ms: 8_000,
            read_timeout_ms: 5_000,
        };

        assert_eq!(
            websocket_capture_deadline(&config),
            Duration::from_millis(20_000)
        );
    }
}
