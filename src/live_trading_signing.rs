use std::error::Error;
use std::fmt::{Display, Formatter};

use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};

use crate::secret_handling::{self, SecretInventory, SecretPresenceReport};

pub const MODULE: &str = "live_trading_signing";

const SCHEMA_VERSION: &str = "lt3.live_trading_signing_dry_run.v1";
const REDACTED_OWNER: &str = "<redacted:owner-not-loaded>";
const REDACTED_SIGNATURE: &str = "<redacted:not-generated>";

#[derive(Debug, Clone)]
pub struct LiveTradingSigningDryRunInput<'a> {
    pub approval_id: &'a str,
    pub run_id: &'a str,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: &'a str,
    pub clob_host: &'a str,
    pub chain_id: u64,
    pub final_live_config_enabled: bool,
    pub wallet_address: &'a str,
    pub funder_address: &'a str,
    pub signature_type: &'a str,
    pub secret_inventory: &'a SecretInventory,
    pub secret_report: &'a SecretPresenceReport,
    pub authenticated_readback_status: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveTradingSigningDryRunArtifact {
    pub artifact_hash: String,
    pub body: LiveTradingSigningDryRunBody,
}

impl LiveTradingSigningDryRunArtifact {
    pub fn new(body: LiveTradingSigningDryRunBody) -> LiveTradingSigningResult<Self> {
        let artifact_hash = artifact_hash(&body)?;
        Ok(Self {
            artifact_hash,
            body,
        })
    }

    pub fn validate(&self) -> LiveTradingSigningResult<()> {
        let expected = artifact_hash(&self.body)?;
        if self.artifact_hash != expected {
            return Err(LiveTradingSigningError::HashMismatch);
        }
        if !self.body.not_submitted || self.body.network_post_enabled {
            return Err(LiveTradingSigningError::Validation(vec![
                "LT3 signing dry-run must remain not_submitted=true and network_post_enabled=false"
                    .to_string(),
            ]));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveTradingSigningDryRunBody {
    pub schema_version: String,
    pub approval_id: String,
    pub run_id: String,
    pub captured_at_ms: i64,
    pub captured_at_rfc3339: String,
    pub status: String,
    pub block_reasons: Vec<String>,
    pub final_live_config_enabled: bool,
    pub clob_host: String,
    pub chain_id: u64,
    pub secret_backend: String,
    pub secret_handles: Vec<LiveTradingSecretHandleEvidence>,
    pub wallet_binding: LiveTradingWalletBindingSummary,
    pub signing_payload_shape: SanitizedLiveTradingSigningPayloadShape,
    pub sanitized_signing_payload_hash: String,
    pub not_submitted: bool,
    pub network_post_enabled: bool,
    pub network_cancel_enabled: bool,
    pub raw_signature_generated: bool,
    pub auth_headers_generated: bool,
    pub authenticated_readback_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveTradingSecretHandleEvidence {
    pub label: String,
    pub handle: String,
    pub present: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LiveTradingWalletBindingSummary {
    pub wallet_address: String,
    pub funder_address: String,
    pub wallet_address_valid: bool,
    pub funder_address_valid: bool,
    pub signature_type_config: String,
    pub signature_type_name: Option<String>,
    pub signature_type_code: Option<u8>,
    pub eoa_funder_must_match_wallet: bool,
    pub funder_matches_wallet: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SanitizedLiveTradingSigningPayloadShape {
    pub purpose: String,
    pub non_submittable_fixture: bool,
    pub order_type: String,
    pub post_only: bool,
    pub defer_exec: bool,
    pub l1_private_key_handle_label: String,
    pub l2_credential_handle_labels: Vec<String>,
    pub required_l2_header_fields: Vec<String>,
    pub required_order_fields: Vec<String>,
    pub redacted_or_absent_fields: Vec<String>,
    pub owner: String,
    pub signature: String,
    pub signature_type_code: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveTradingSignatureType {
    Eoa,
    PolyProxy,
    GnosisSafe,
    Poly1271,
}

impl LiveTradingSignatureType {
    fn from_config(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "0" | "eoa" => Some(Self::Eoa),
            "1" | "poly_proxy" | "poly-proxy" | "polyproxy" => Some(Self::PolyProxy),
            "2" | "gnosis_safe" | "gnosis-safe" | "gnosissafe" => Some(Self::GnosisSafe),
            "3" | "poly_1271" | "poly-1271" | "poly1271" => Some(Self::Poly1271),
            _ => None,
        }
    }

    fn as_config_str(self) -> &'static str {
        match self {
            Self::Eoa => "eoa",
            Self::PolyProxy => "poly_proxy",
            Self::GnosisSafe => "gnosis_safe",
            Self::Poly1271 => "poly_1271",
        }
    }

    fn as_code(self) -> u8 {
        match self {
            Self::Eoa => 0,
            Self::PolyProxy => 1,
            Self::GnosisSafe => 2,
            Self::Poly1271 => 3,
        }
    }
}

pub fn build_live_trading_signing_dry_run(
    input: LiveTradingSigningDryRunInput<'_>,
) -> LiveTradingSigningResult<LiveTradingSigningDryRunArtifact> {
    secret_handling::validate_secret_inventory(input.secret_inventory)
        .map_err(LiveTradingSigningError::SecretHandling)?;

    let signature_type = LiveTradingSignatureType::from_config(input.signature_type);
    let wallet_binding = wallet_binding_summary(
        input.wallet_address,
        input.funder_address,
        input.signature_type,
        signature_type,
    );
    let signing_payload_shape = signing_payload_shape(signature_type);
    let sanitized_signing_payload_hash = payload_hash(&signing_payload_shape)?;
    let block_reasons = block_reasons(&input, &wallet_binding, signature_type);
    let status = if block_reasons.is_empty() {
        "passed"
    } else {
        "blocked"
    };

    LiveTradingSigningDryRunArtifact::new(LiveTradingSigningDryRunBody {
        schema_version: SCHEMA_VERSION.to_string(),
        approval_id: input.approval_id.to_string(),
        run_id: input.run_id.to_string(),
        captured_at_ms: input.captured_at_ms,
        captured_at_rfc3339: input.captured_at_rfc3339.to_string(),
        status: status.to_string(),
        block_reasons,
        final_live_config_enabled: input.final_live_config_enabled,
        clob_host: input.clob_host.to_string(),
        chain_id: input.chain_id,
        secret_backend: input.secret_report.backend.clone(),
        secret_handles: input
            .secret_report
            .checks
            .iter()
            .map(|check| LiveTradingSecretHandleEvidence {
                label: check.label.to_string(),
                handle: check.handle.clone(),
                present: check.present,
            })
            .collect(),
        wallet_binding,
        signing_payload_shape,
        sanitized_signing_payload_hash,
        not_submitted: true,
        network_post_enabled: false,
        network_cancel_enabled: false,
        raw_signature_generated: false,
        auth_headers_generated: false,
        authenticated_readback_status: input.authenticated_readback_status.to_string(),
    })
}

pub fn live_trading_signing_dry_run_json(
    artifact: &LiveTradingSigningDryRunArtifact,
) -> LiveTradingSigningResult<String> {
    serde_json::to_string_pretty(artifact).map_err(LiveTradingSigningError::Serialize)
}

pub fn live_trading_signing_payload_shape_json(
    artifact: &LiveTradingSigningDryRunArtifact,
) -> LiveTradingSigningResult<String> {
    serde_json::to_string_pretty(&artifact.body.signing_payload_shape)
        .map_err(LiveTradingSigningError::Serialize)
}

fn block_reasons(
    input: &LiveTradingSigningDryRunInput<'_>,
    wallet_binding: &LiveTradingWalletBindingSummary,
    signature_type: Option<LiveTradingSignatureType>,
) -> Vec<String> {
    let mut reasons = Vec::new();

    if !input.final_live_config_enabled {
        reasons.push("final_live_config_disabled".to_string());
    }
    if !input.approval_id.starts_with("LT3-") {
        reasons.push("approval_id_not_lt3".to_string());
    }
    if !input.secret_report.all_present() {
        reasons.push("secret_handles_missing".to_string());
    }
    if input.wallet_address.trim().is_empty() {
        reasons.push("wallet_address_missing".to_string());
    } else if !wallet_binding.wallet_address_valid {
        reasons.push("wallet_address_invalid".to_string());
    }
    if input.funder_address.trim().is_empty() {
        reasons.push("funder_address_missing".to_string());
    } else if !wallet_binding.funder_address_valid {
        reasons.push("funder_address_invalid".to_string());
    }
    if input.signature_type.trim().is_empty() {
        reasons.push("signature_type_missing".to_string());
    } else if signature_type.is_none() {
        reasons.push("signature_type_invalid".to_string());
    }
    if wallet_binding.eoa_funder_must_match_wallet && !wallet_binding.funder_matches_wallet {
        reasons.push("eoa_funder_wallet_mismatch".to_string());
    }

    reasons
}

fn wallet_binding_summary(
    wallet_address: &str,
    funder_address: &str,
    signature_type_config: &str,
    signature_type: Option<LiveTradingSignatureType>,
) -> LiveTradingWalletBindingSummary {
    let wallet = wallet_address.trim().to_string();
    let funder = funder_address.trim().to_string();
    let eoa_funder_must_match_wallet = signature_type == Some(LiveTradingSignatureType::Eoa);

    LiveTradingWalletBindingSummary {
        wallet_address: wallet,
        funder_address: funder,
        wallet_address_valid: is_valid_nonzero_evm_address(wallet_address.trim()),
        funder_address_valid: is_valid_nonzero_evm_address(funder_address.trim()),
        signature_type_config: signature_type_config.trim().to_string(),
        signature_type_name: signature_type.map(|value| value.as_config_str().to_string()),
        signature_type_code: signature_type.map(LiveTradingSignatureType::as_code),
        eoa_funder_must_match_wallet,
        funder_matches_wallet: wallet_address
            .trim()
            .eq_ignore_ascii_case(funder_address.trim()),
    }
}

fn signing_payload_shape(
    signature_type: Option<LiveTradingSignatureType>,
) -> SanitizedLiveTradingSigningPayloadShape {
    SanitizedLiveTradingSigningPayloadShape {
        purpose: "lt3_shape_hash_only_no_signature_no_submit".to_string(),
        non_submittable_fixture: true,
        order_type: "GTD".to_string(),
        post_only: true,
        defer_exec: false,
        l1_private_key_handle_label: "signer_private_key".to_string(),
        l2_credential_handle_labels: vec![
            "clob_l2_access".to_string(),
            "clob_l2_credential".to_string(),
            "clob_l2_passphrase".to_string(),
        ],
        required_l2_header_fields: vec![
            "POLY_ADDRESS".to_string(),
            "POLY_SIGNATURE".to_string(),
            "POLY_TIMESTAMP".to_string(),
            "POLY_API_KEY".to_string(),
            "POLY_PASSPHRASE".to_string(),
        ],
        required_order_fields: vec![
            "order.maker".to_string(),
            "order.signer".to_string(),
            "order.tokenId".to_string(),
            "order.makerAmount".to_string(),
            "order.takerAmount".to_string(),
            "order.side".to_string(),
            "order.expiration".to_string(),
            "order.timestamp".to_string(),
            "order.metadata".to_string(),
            "order.builder".to_string(),
            "order.signature".to_string(),
            "order.salt".to_string(),
            "order.signatureType".to_string(),
            "owner".to_string(),
            "orderType".to_string(),
            "deferExec".to_string(),
        ],
        redacted_or_absent_fields: vec![
            "order.signature".to_string(),
            "owner".to_string(),
            "POLY_SIGNATURE".to_string(),
            "POLY_API_KEY".to_string(),
            "POLY_PASSPHRASE".to_string(),
        ],
        owner: REDACTED_OWNER.to_string(),
        signature: REDACTED_SIGNATURE.to_string(),
        signature_type_code: signature_type.map(LiveTradingSignatureType::as_code),
    }
}

fn is_valid_nonzero_evm_address(value: &str) -> bool {
    let Some(stripped) = value.strip_prefix("0x") else {
        return false;
    };
    stripped.len() == 40
        && stripped.chars().all(|ch| ch.is_ascii_hexdigit())
        && stripped.chars().any(|ch| ch != '0')
}

fn payload_hash(
    payload: &SanitizedLiveTradingSigningPayloadShape,
) -> LiveTradingSigningResult<String> {
    let bytes = serde_json::to_vec(payload).map_err(LiveTradingSigningError::Serialize)?;
    Ok(format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, &bytes).as_ref())
    ))
}

fn artifact_hash(body: &LiveTradingSigningDryRunBody) -> LiveTradingSigningResult<String> {
    let bytes = serde_json::to_vec(body).map_err(LiveTradingSigningError::Serialize)?;
    Ok(format!(
        "sha256:{}",
        hex_digest(digest(&SHA256, &bytes).as_ref())
    ))
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

pub type LiveTradingSigningResult<T> = Result<T, LiveTradingSigningError>;

#[derive(Debug)]
pub enum LiveTradingSigningError {
    Validation(Vec<String>),
    SecretHandling(secret_handling::SecretHandlingError),
    Serialize(serde_json::Error),
    HashMismatch,
}

impl Display for LiveTradingSigningError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LiveTradingSigningError::Validation(errors) => {
                writeln!(formatter, "live trading signing dry-run validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
            LiveTradingSigningError::SecretHandling(source) => write!(formatter, "{source}"),
            LiveTradingSigningError::Serialize(source) => {
                write!(
                    formatter,
                    "failed to serialize live trading signing dry-run: {source}"
                )
            }
            LiveTradingSigningError::HashMismatch => {
                write!(formatter, "live trading signing dry-run hash mismatch")
            }
        }
    }
}

impl Error for LiveTradingSigningError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_handling::{SecretHandle, SecretInventory, SecretPresenceCheck};

    #[test]
    fn live_trading_signing_dry_run_blocks_default_disabled_missing_state() {
        let artifact = build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
            approval_id: "LT3-LOCAL-DRY-RUN",
            run_id: "lt3-test",
            captured_at_ms: 1,
            captured_at_rfc3339: "2026-05-13T00:00:00Z",
            clob_host: "https://clob.polymarket.com",
            chain_id: 137,
            final_live_config_enabled: false,
            wallet_address: "",
            funder_address: "",
            signature_type: "",
            secret_inventory: &sample_inventory(),
            secret_report: &sample_report(false),
            authenticated_readback_status: "not_run_local_dry_run",
        })
        .expect("blocked artifact still builds");

        assert_eq!(artifact.body.status, "blocked");
        assert!(artifact
            .body
            .block_reasons
            .contains(&"final_live_config_disabled".to_string()));
        assert!(artifact
            .body
            .block_reasons
            .contains(&"secret_handles_missing".to_string()));
        assert!(artifact
            .body
            .block_reasons
            .contains(&"wallet_address_missing".to_string()));
        assert!(artifact.body.not_submitted);
        assert!(!artifact.body.network_post_enabled);
        assert!(!artifact.body.network_cancel_enabled);
        artifact.validate().expect("hash validates");
    }

    #[test]
    fn live_trading_signing_dry_run_passes_with_handles_and_binding_without_submission() {
        let artifact = build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
            approval_id: "LT3-APPROVED-SIGNING-001",
            run_id: "lt3-test",
            captured_at_ms: 1,
            captured_at_rfc3339: "2026-05-13T00:00:00Z",
            clob_host: "https://clob.polymarket.com",
            chain_id: 137,
            final_live_config_enabled: true,
            wallet_address: "0x1111111111111111111111111111111111111111",
            funder_address: "0x2222222222222222222222222222222222222222",
            signature_type: "poly_1271",
            secret_inventory: &sample_inventory(),
            secret_report: &sample_report(true),
            authenticated_readback_status: "not_requested",
        })
        .expect("passing artifact builds");

        assert_eq!(artifact.body.status, "passed");
        assert!(artifact.body.block_reasons.is_empty());
        assert_eq!(artifact.body.wallet_binding.signature_type_code, Some(3));
        assert!(artifact.body.not_submitted);
        assert!(!artifact.body.network_post_enabled);
        assert!(!artifact.body.raw_signature_generated);
        assert!(!artifact.body.auth_headers_generated);
        assert!(artifact
            .body
            .sanitized_signing_payload_hash
            .starts_with("sha256:"));
    }

    #[test]
    fn live_trading_signing_rejects_live_alpha_or_live_beta_approval_ids() {
        for approval_id in ["LA7-OLD-APPROVAL", "LB6-OLD-APPROVAL"] {
            let artifact = build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
                approval_id,
                run_id: "lt3-test",
                captured_at_ms: 1,
                captured_at_rfc3339: "2026-05-13T00:00:00Z",
                clob_host: "https://clob.polymarket.com",
                chain_id: 137,
                final_live_config_enabled: true,
                wallet_address: "0x1111111111111111111111111111111111111111",
                funder_address: "0x1111111111111111111111111111111111111111",
                signature_type: "eoa",
                secret_inventory: &sample_inventory(),
                secret_report: &sample_report(true),
                authenticated_readback_status: "not_requested",
            })
            .expect("artifact builds");

            assert!(artifact
                .body
                .block_reasons
                .contains(&"approval_id_not_lt3".to_string()));
        }
    }

    #[test]
    fn live_trading_signing_detects_value_like_or_duplicate_handles() {
        let mut inventory = sample_inventory();
        inventory.handles[0].handle = "lowercase-secret".to_string();

        let error = build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
            approval_id: "LT3-LOCAL-DRY-RUN",
            run_id: "lt3-test",
            captured_at_ms: 1,
            captured_at_rfc3339: "2026-05-13T00:00:00Z",
            clob_host: "https://clob.polymarket.com",
            chain_id: 137,
            final_live_config_enabled: false,
            wallet_address: "",
            funder_address: "",
            signature_type: "",
            secret_inventory: &inventory,
            secret_report: &sample_report(false),
            authenticated_readback_status: "not_run_local_dry_run",
        })
        .expect_err("invalid handle fails");

        let rendered = error.to_string();
        assert!(rendered.contains("clob_l2_access"));
        assert!(!rendered.contains("lowercase-secret"));

        let mut duplicate = sample_inventory();
        duplicate.handles[3].handle = duplicate.handles[0].handle.clone();
        let error = build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
            approval_id: "LT3-APPROVED-SIGNING-001",
            run_id: "lt3-test",
            captured_at_ms: 1,
            captured_at_rfc3339: "2026-05-13T00:00:00Z",
            clob_host: "https://clob.polymarket.com",
            chain_id: 137,
            final_live_config_enabled: true,
            wallet_address: "0x1111111111111111111111111111111111111111",
            funder_address: "0x1111111111111111111111111111111111111111",
            signature_type: "eoa",
            secret_inventory: &duplicate,
            secret_report: &sample_report(true),
            authenticated_readback_status: "not_requested",
        })
        .expect_err("duplicate handle fails");

        assert!(error.to_string().contains("signer_private_key"));
    }

    #[test]
    fn live_trading_signing_artifact_does_not_contain_secret_values_or_submit_surface() {
        let artifact = build_valid_artifact();
        let rendered = live_trading_signing_dry_run_json(&artifact).expect("serializes");
        let forbidden_values = [
            ["PRIVATE", "_KEY", "_VALUE"].concat(),
            ["API", "_SECRET", "_VALUE"].concat(),
            ["RAW", "_SIGNATURE"].concat(),
        ];

        assert!(rendered.contains(REDACTED_SIGNATURE));
        assert!(rendered.contains(REDACTED_OWNER));
        for value in forbidden_values {
            assert!(!rendered.contains(&value));
        }

        let source = include_str!("live_trading_signing.rs");
        let forbidden_source_tokens = [
            ["req", "west"].concat(),
            ["Client", "::new"].concat(),
            [".", "post", "("].concat(),
            [".", "delete", "("].concat(),
            ["post", "_order"].concat(),
            ["cancel", "_order"].concat(),
        ];
        for token in forbidden_source_tokens {
            assert!(
                !source.contains(&token),
                "unexpected network-capable token in LT3 signing dry-run source: {token}"
            );
        }
    }

    fn build_valid_artifact() -> LiveTradingSigningDryRunArtifact {
        let inventory = sample_inventory();
        let report = sample_report(true);
        build_live_trading_signing_dry_run(LiveTradingSigningDryRunInput {
            approval_id: "LT3-APPROVED-SIGNING-001",
            run_id: "lt3-test",
            captured_at_ms: 1,
            captured_at_rfc3339: "2026-05-13T00:00:00Z",
            clob_host: "https://clob.polymarket.com",
            chain_id: 137,
            final_live_config_enabled: true,
            wallet_address: "0x1111111111111111111111111111111111111111",
            funder_address: "0x1111111111111111111111111111111111111111",
            signature_type: "eoa",
            secret_inventory: &inventory,
            secret_report: &report,
            authenticated_readback_status: "not_requested",
        })
        .expect("valid artifact builds")
    }

    fn sample_inventory() -> SecretInventory {
        SecretInventory::new(
            "env",
            vec![
                SecretHandle::new("clob_l2_access", "P15M_LIVE_TRADING_CLOB_L2_ACCESS"),
                SecretHandle::new("clob_l2_credential", "P15M_LIVE_TRADING_CLOB_L2_CREDENTIAL"),
                SecretHandle::new("clob_l2_passphrase", "P15M_LIVE_TRADING_CLOB_L2_PASSPHRASE"),
                SecretHandle::new("signer_private_key", "P15M_LIVE_TRADING_SIGNER_PRIVATE_KEY"),
            ],
        )
    }

    fn sample_report(present: bool) -> SecretPresenceReport {
        SecretPresenceReport {
            backend: "env".to_string(),
            checks: sample_inventory()
                .handles
                .iter()
                .map(|handle| SecretPresenceCheck {
                    label: handle.label,
                    handle: handle.handle.clone(),
                    present,
                })
                .collect(),
        }
    }
}
