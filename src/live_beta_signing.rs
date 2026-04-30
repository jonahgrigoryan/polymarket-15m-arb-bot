use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::domain::Side;

pub const MODULE: &str = "live_beta_signing";

const EXCHANGE_DOMAIN_NAME: &str = "Polymarket CTF Exchange";
const EXCHANGE_DOMAIN_VERSION: &str = "2";
const POLYGON_CHAIN_ID: u64 = 137;
const CTF_EXCHANGE_V2: &str = "0xE111180000d2663C0091e4f400237545B87B996B";
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
const REDACTED_OWNER: &str = "<redacted:owner-not-loaded>";
const REDACTED_SIGNATURE: &str = "<redacted:dry-run-no-key-material>";

#[derive(Debug, Clone, PartialEq)]
pub struct LiveBetaSigningDryRunInput {
    pub clob_host: String,
    pub token_id: String,
    pub side: Side,
    pub price: f64,
    pub size: f64,
    pub tick_size: f64,
    pub market_end_ts: u64,
    pub expiration_ts: u64,
    pub timestamp_ms: u64,
    pub salt: String,
    pub maker_address: String,
    pub signer_address: String,
    pub funder_address: String,
    pub signature_type: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveBetaSigningDryRunArtifact {
    pub sdk_decision: String,
    pub clob_host: String,
    pub order_type: String,
    pub post_only: bool,
    pub not_submitted: bool,
    pub network_post_enabled: bool,
    pub dry_run_only: bool,
    pub domain: Eip712DomainDraft,
    pub order: SanitizedSignedOrderDraft,
    pub owner: String,
    pub signature_source: String,
}

impl LiveBetaSigningDryRunArtifact {
    pub fn fingerprint(&self) -> LiveBetaSigningResult<String> {
        let bytes = serde_json::to_vec(self).map_err(LiveBetaSigningError::Serialize)?;
        Ok(format!(
            "sha256:{}",
            hex_digest(digest(&SHA256, &bytes).as_ref())
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Eip712DomainDraft {
    pub name: String,
    pub version: String,
    pub chain_id: u64,
    pub verifying_contract: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SanitizedSignedOrderDraft {
    pub salt: String,
    pub maker: String,
    pub signer: String,
    pub funder_proxy: String,
    pub taker: String,
    pub token_id: String,
    pub maker_amount: String,
    pub taker_amount: String,
    pub expiration: String,
    pub nonce: String,
    pub fee_rate_bps: String,
    pub side: String,
    pub side_code: u8,
    pub signature_type: u8,
    pub timestamp: String,
    pub metadata: String,
    pub builder: String,
    pub signature: String,
}

pub fn build_live_beta_signing_dry_run(
    input: LiveBetaSigningDryRunInput,
) -> LiveBetaSigningResult<LiveBetaSigningDryRunArtifact> {
    validate_input(&input)?;

    let size_units = scaled_units(input.size, "size")?;
    let notional_units = scaled_units(input.price * input.size, "notional")?;
    let (maker_amount, taker_amount) = match input.side {
        Side::Buy => (notional_units, size_units),
        Side::Sell => (size_units, notional_units),
    };

    Ok(LiveBetaSigningDryRunArtifact {
        sdk_decision: "minimal_custom_v2_payload_builder_no_sdk_import".to_string(),
        clob_host: input.clob_host,
        order_type: "GTD".to_string(),
        post_only: true,
        not_submitted: true,
        network_post_enabled: false,
        dry_run_only: true,
        domain: Eip712DomainDraft {
            name: EXCHANGE_DOMAIN_NAME.to_string(),
            version: EXCHANGE_DOMAIN_VERSION.to_string(),
            chain_id: POLYGON_CHAIN_ID,
            verifying_contract: CTF_EXCHANGE_V2.to_string(),
        },
        order: SanitizedSignedOrderDraft {
            salt: input.salt,
            maker: input.maker_address,
            signer: input.signer_address,
            funder_proxy: input.funder_address,
            taker: ZERO_ADDRESS.to_string(),
            token_id: input.token_id,
            maker_amount: maker_amount.to_string(),
            taker_amount: taker_amount.to_string(),
            expiration: input.expiration_ts.to_string(),
            nonce: "0".to_string(),
            fee_rate_bps: "0".to_string(),
            side: side_wire_value(input.side).to_string(),
            side_code: side_code(input.side),
            signature_type: input.signature_type,
            timestamp: input.timestamp_ms.to_string(),
            metadata: zero_bytes32_hex(),
            builder: zero_bytes32_hex(),
            signature: REDACTED_SIGNATURE.to_string(),
        },
        owner: REDACTED_OWNER.to_string(),
        signature_source: "redacted_dry_run_placeholder_no_key_material_or_credentials".to_string(),
    })
}

pub fn sample_live_beta_signing_dry_run(
    clob_host: impl Into<String>,
) -> LiveBetaSigningResult<LiveBetaSigningDryRunArtifact> {
    build_live_beta_signing_dry_run(LiveBetaSigningDryRunInput {
        clob_host: clob_host.into(),
        token_id: "102936000000000000000000000000000000000000000000000000000000000000".to_string(),
        side: Side::Buy,
        price: 0.20,
        size: 1.0,
        tick_size: 0.01,
        market_end_ts: 1_777_434_900,
        expiration_ts: 1_777_434_180,
        timestamp_ms: 1_777_434_000_000,
        salt: "1777434000000".to_string(),
        maker_address: "0x1111111111111111111111111111111111111111".to_string(),
        signer_address: "0x2222222222222222222222222222222222222222".to_string(),
        funder_address: "0x1111111111111111111111111111111111111111".to_string(),
        signature_type: 0,
    })
}

fn validate_input(input: &LiveBetaSigningDryRunInput) -> LiveBetaSigningResult<()> {
    let mut errors = Vec::new();

    if !input.clob_host.starts_with("https://") {
        errors.push("clob_host must use https".to_string());
    }
    if input.token_id.is_empty() || !input.token_id.chars().all(|ch| ch.is_ascii_digit()) {
        errors.push("token_id must be a decimal CLOB token id".to_string());
    }
    if !input.price.is_finite() || input.price <= 0.0 || input.price >= 1.0 {
        errors.push("price must be finite and between 0 and 1".to_string());
    }
    if !input.size.is_finite() || input.size <= 0.0 {
        errors.push("size must be finite and greater than zero".to_string());
    }
    if !input.tick_size.is_finite() || input.tick_size <= 0.0 || input.tick_size > 1.0 {
        errors.push("tick_size must be finite and between 0 and 1".to_string());
    } else if input.price.is_finite() && !is_tick_aligned(input.price, input.tick_size) {
        errors.push("price must align with tick_size".to_string());
    }
    if input.timestamp_ms == 0 {
        errors.push("timestamp_ms must be present".to_string());
    }
    let timestamp_secs = input.timestamp_ms / 1_000;
    if input.expiration_ts <= timestamp_secs + 60 {
        errors.push("expiration_ts must include the documented GTD safety buffer".to_string());
    }
    if input.market_end_ts == 0 || input.expiration_ts >= input.market_end_ts {
        errors.push("expiration_ts must be before market_end_ts".to_string());
    }
    if input.salt.is_empty() || !input.salt.chars().all(|ch| ch.is_ascii_digit()) {
        errors.push("salt must be a decimal string".to_string());
    }
    if !is_valid_nonzero_evm_address(&input.maker_address) {
        errors.push("maker_address must be a nonzero EVM address".to_string());
    }
    if !is_valid_nonzero_evm_address(&input.signer_address) {
        errors.push("signer_address must be a nonzero EVM address".to_string());
    }
    if !is_valid_nonzero_evm_address(&input.funder_address) {
        errors.push("funder_address must be a nonzero EVM address".to_string());
    }
    if input.maker_address != input.funder_address {
        errors.push("maker_address must match the funder/proxy address for LB3".to_string());
    }
    if input.signature_type > 3 {
        errors.push("signature_type must be one of the documented V2 values".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(LiveBetaSigningError::Validation(errors))
    }
}

fn is_tick_aligned(price: f64, tick_size: f64) -> bool {
    let ticks = price / tick_size;
    (ticks - ticks.round()).abs() < 1e-9
}

fn scaled_units(value: f64, field: &'static str) -> LiveBetaSigningResult<u64> {
    if !value.is_finite() || value <= 0.0 {
        return Err(LiveBetaSigningError::Validation(vec![format!(
            "{field} must be finite and greater than zero"
        )]));
    }
    let scaled = (value * 1_000_000.0).round();
    if scaled <= 0.0 || scaled > u64::MAX as f64 {
        return Err(LiveBetaSigningError::Validation(vec![format!(
            "{field} cannot be represented in six-decimal units"
        )]));
    }
    Ok(scaled as u64)
}

fn is_valid_nonzero_evm_address(value: &str) -> bool {
    let Some(stripped) = value.strip_prefix("0x") else {
        return false;
    };
    stripped.len() == 40
        && stripped.chars().all(|ch| ch.is_ascii_hexdigit())
        && stripped.chars().any(|ch| ch != '0')
}

fn side_wire_value(side: Side) -> &'static str {
    match side {
        Side::Buy => "BUY",
        Side::Sell => "SELL",
    }
}

fn side_code(side: Side) -> u8 {
    match side {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

fn zero_bytes32_hex() -> String {
    let mut output = String::with_capacity(66);
    output.push_str("0x");
    for _ in 0..32 {
        output.push_str("00");
    }
    output
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub type LiveBetaSigningResult<T> = Result<T, LiveBetaSigningError>;

#[derive(Debug)]
pub enum LiveBetaSigningError {
    Validation(Vec<String>),
    Serialize(serde_json::Error),
}

impl Display for LiveBetaSigningError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveBetaSigningError::Validation(errors) => {
                writeln!(formatter, "live beta signing dry-run validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
            LiveBetaSigningError::Serialize(source) => {
                write!(
                    formatter,
                    "failed to serialize live beta signing dry-run: {source}"
                )
            }
        }
    }
}

impl Error for LiveBetaSigningError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_dry_run_builds_sanitized_gtd_payload() {
        let artifact = sample_live_beta_signing_dry_run("https://clob.polymarket.com")
            .expect("fixture builds");

        assert_eq!(artifact.order_type, "GTD");
        assert!(artifact.post_only);
        assert!(artifact.not_submitted);
        assert!(!artifact.network_post_enabled);
        assert!(artifact.dry_run_only);
        assert_eq!(artifact.domain.version, "2");
        assert_eq!(artifact.domain.chain_id, 137);
        assert_eq!(artifact.order.side, "BUY");
        assert_eq!(artifact.order.side_code, 0);
        assert_eq!(
            artifact.order.funder_proxy,
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(artifact.order.signature, REDACTED_SIGNATURE);
        assert_eq!(artifact.owner, REDACTED_OWNER);
        assert_eq!(artifact.order.maker_amount, "200000");
        assert_eq!(artifact.order.taker_amount, "1000000");
        assert!(artifact
            .fingerprint()
            .expect("fingerprint")
            .starts_with("sha256:"));
    }

    #[test]
    fn signing_dry_run_never_loads_credential_values() {
        let artifact = sample_live_beta_signing_dry_run("https://clob.polymarket.com")
            .expect("fixture builds");
        let rendered = serde_json::to_string(&artifact).expect("serializes");
        let forbidden = [
            ["POLY", "_API", "_KEY"].concat(),
            ["POLY", "_SECRET"].concat(),
            ["POLY", "_PASSPHRASE"].concat(),
        ];

        assert!(rendered.contains(REDACTED_SIGNATURE));
        assert!(rendered.contains(REDACTED_OWNER));
        for value in forbidden {
            assert!(!rendered.contains(&value));
        }
    }

    #[test]
    fn dry_run_rejects_invalid_expiry_and_signature_type_without_echoing_addresses() {
        let mut input = valid_input();
        input.expiration_ts = input.market_end_ts;
        input.signature_type = 9;

        let error = build_live_beta_signing_dry_run(input).expect_err("invalid input fails");
        let rendered = error.to_string();

        assert!(rendered.contains("expiration_ts must be before market_end_ts"));
        assert!(rendered.contains("signature_type must be one of the documented V2 values"));
        assert!(!rendered.contains("0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn dry_run_rejects_market_order_shapes() {
        let mut input = valid_input();
        input.price = 1.0;

        let error = build_live_beta_signing_dry_run(input).expect_err("crossing shape fails");

        assert!(error
            .to_string()
            .contains("price must be finite and between 0 and 1"));
    }

    #[test]
    fn dry_run_module_has_no_network_submit_surface() {
        let source = include_str!("live_beta_signing.rs");
        let forbidden = [
            ["req", "west"].concat(),
            ["Client", "::new"].concat(),
            [".", "post", "("].concat(),
            ["po", "st", "_or", "der"].concat(),
            ["/", "orders"].concat(),
        ];

        for forbidden in forbidden {
            assert!(
                !source.contains(&forbidden),
                "unexpected network-capable token in LB3 dry-run source: {forbidden}"
            );
        }
    }

    fn valid_input() -> LiveBetaSigningDryRunInput {
        LiveBetaSigningDryRunInput {
            clob_host: "https://clob.polymarket.com".to_string(),
            token_id: "102936000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            side: Side::Buy,
            price: 0.20,
            size: 1.0,
            tick_size: 0.01,
            market_end_ts: 1_777_434_900,
            expiration_ts: 1_777_434_180,
            timestamp_ms: 1_777_434_000_000,
            salt: "1777434000000".to_string(),
            maker_address: "0x1111111111111111111111111111111111111111".to_string(),
            signer_address: "0x2222222222222222222222222222222222222222".to_string(),
            funder_address: "0x1111111111111111111111111111111111111111".to_string(),
            signature_type: 0,
        }
    }
}
