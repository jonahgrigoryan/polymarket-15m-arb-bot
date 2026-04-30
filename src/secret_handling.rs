use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const MODULE: &str = "secret_handling";
pub const ENV_SECRET_BACKEND: &str = "env";
pub const REDACTED_VALUE: &str = "<redacted>";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretHandle {
    pub label: &'static str,
    pub handle: String,
}

impl SecretHandle {
    pub fn new(label: &'static str, handle: impl Into<String>) -> Self {
        Self {
            label,
            handle: handle.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretInventory {
    pub backend: String,
    pub handles: Vec<SecretHandle>,
}

impl SecretInventory {
    pub fn new(backend: impl Into<String>, handles: Vec<SecretHandle>) -> Self {
        Self {
            backend: backend.into(),
            handles,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretPresenceReport {
    pub backend: String,
    pub checks: Vec<SecretPresenceCheck>,
}

impl SecretPresenceReport {
    pub fn all_present(&self) -> bool {
        self.checks.iter().all(|check| check.present)
    }

    pub fn missing_handle_list(&self) -> String {
        self.checks
            .iter()
            .filter(|check| !check.present)
            .map(|check| format!("{}:{}", check.label, check.handle))
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretPresenceCheck {
    pub label: &'static str,
    pub handle: String,
    pub present: bool,
}

pub trait SecretPresenceProvider {
    fn contains_handle(&self, handle: &str) -> bool;
}

#[derive(Debug, Clone, Copy)]
pub struct EnvSecretPresenceProvider;

impl SecretPresenceProvider for EnvSecretPresenceProvider {
    fn contains_handle(&self, handle: &str) -> bool {
        env::var_os(handle).is_some()
    }
}

pub fn validate_secret_inventory(inventory: &SecretInventory) -> SecretHandlingResult<()> {
    let mut errors = Vec::new();

    if inventory.backend != ENV_SECRET_BACKEND {
        errors.push("secret backend must be env".to_string());
    }
    if inventory.handles.is_empty() {
        errors.push("secret inventory must define at least one handle".to_string());
    }

    let mut seen = BTreeSet::new();
    for item in &inventory.handles {
        if item.label.trim().is_empty() {
            errors.push("secret handle label must not be empty".to_string());
        }
        if !is_valid_env_handle(&item.handle) {
            errors.push(format!(
                "secret handle {} must be an environment-variable handle starting with P15M_ and containing only A-Z, 0-9, or _",
                item.label
            ));
        }
        if !seen.insert(item.handle.as_str()) {
            errors.push(format!(
                "secret handle {} duplicates another handle",
                item.label
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(SecretHandlingError::InvalidConfig(errors))
    }
}

pub fn validate_secret_presence(
    inventory: &SecretInventory,
    provider: &impl SecretPresenceProvider,
) -> SecretHandlingResult<SecretPresenceReport> {
    validate_secret_inventory(inventory)?;
    Ok(SecretPresenceReport {
        backend: inventory.backend.clone(),
        checks: inventory
            .handles
            .iter()
            .map(|item| SecretPresenceCheck {
                label: item.label,
                handle: item.handle.clone(),
                present: provider.contains_handle(&item.handle),
            })
            .collect(),
    })
}

pub fn redact_env_assignments(input: &str, handles: &[SecretHandle]) -> String {
    let mut output = input.to_string();
    for item in handles {
        output = redact_handle_assignment(&output, &item.handle);
    }
    output
}

fn redact_handle_assignment(input: &str, handle: &str) -> String {
    let pattern = format!("{handle}=");
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(offset) = remaining.find(&pattern) {
        let (prefix, rest) = remaining.split_at(offset);
        output.push_str(prefix);
        output.push_str(&pattern);
        output.push_str(REDACTED_VALUE);

        let value_start = pattern.len();
        let value_end = value_start + assignment_value_end(&rest[value_start..]);
        remaining = &rest[value_end..];
    }

    output.push_str(remaining);
    output
}

fn assignment_value_end(value: &str) -> usize {
    let mut chars = value.char_indices();
    if let Some((_, quote @ ('"' | '\''))) = chars.next() {
        for (index, ch) in chars {
            if ch == quote {
                return index + ch.len_utf8();
            }
        }
        return value.len();
    }

    value
        .find(|ch: char| ch.is_whitespace() || matches!(ch, ',' | ';'))
        .unwrap_or(value.len())
}

fn is_valid_env_handle(value: &str) -> bool {
    value.starts_with("P15M_")
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

pub type SecretHandlingResult<T> = Result<T, SecretHandlingError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretHandlingError {
    InvalidConfig(Vec<String>),
}

impl Display for SecretHandlingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretHandlingError::InvalidConfig(errors) => {
                writeln!(formatter, "secret handling validation failed:")?;
                for error in errors {
                    writeln!(formatter, "- {error}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for SecretHandlingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StaticPresenceProvider {
        present: BTreeSet<String>,
    }

    impl SecretPresenceProvider for StaticPresenceProvider {
        fn contains_handle(&self, handle: &str) -> bool {
            self.present.contains(handle)
        }
    }

    fn sample_inventory() -> SecretInventory {
        SecretInventory::new(
            ENV_SECRET_BACKEND,
            vec![
                SecretHandle::new("clob_l2_access", "P15M_LIVE_BETA_CLOB_L2_ACCESS"),
                SecretHandle::new("clob_l2_credential", "P15M_LIVE_BETA_CLOB_L2_CREDENTIAL"),
                SecretHandle::new("clob_l2_passphrase", "P15M_LIVE_BETA_CLOB_L2_PASSPHRASE"),
            ],
        )
    }

    #[test]
    fn secret_inventory_accepts_handle_names_only() {
        validate_secret_inventory(&sample_inventory()).expect("metadata-only handles validate");
    }

    #[test]
    fn secret_inventory_rejects_value_like_handles_without_echoing_values() {
        let inventory = SecretInventory::new(
            ENV_SECRET_BACKEND,
            vec![SecretHandle::new("clob_l2_access", "lowercase-value")],
        );

        let error = validate_secret_inventory(&inventory).expect_err("invalid handle fails");
        let rendered = error.to_string();

        assert!(rendered.contains("clob_l2_access"));
        assert!(!rendered.contains("lowercase-value"));
    }

    #[test]
    fn secret_presence_report_contains_only_handles_and_presence() {
        let provider = StaticPresenceProvider {
            present: BTreeSet::from(["P15M_LIVE_BETA_CLOB_L2_ACCESS".to_string()]),
        };

        let report =
            validate_secret_presence(&sample_inventory(), &provider).expect("presence report");

        assert!(!report.all_present());
        assert_eq!(report.checks[0].label, "clob_l2_access");
        assert_eq!(report.checks[0].handle, "P15M_LIVE_BETA_CLOB_L2_ACCESS");
        assert!(report.checks[0].present);
        assert!(!report.missing_handle_list().is_empty());
    }

    #[test]
    fn redaction_scrubs_env_assignment_values() {
        let inventory = sample_inventory();
        let input = "P15M_LIVE_BETA_CLOB_L2_ACCESS=EXAMPLE_VALUE mode=validate";

        let redacted = redact_env_assignments(input, &inventory.handles);

        assert!(redacted.contains("P15M_LIVE_BETA_CLOB_L2_ACCESS=<redacted>"));
        assert!(!redacted.contains("EXAMPLE_VALUE"));
    }

    #[test]
    fn redaction_scrubs_quoted_env_assignment_values() {
        let inventory = sample_inventory();
        let input = "P15M_LIVE_BETA_CLOB_L2_ACCESS=\"EXAMPLE VALUE\" mode=validate";

        let redacted = redact_env_assignments(input, &inventory.handles);

        assert!(redacted.contains("P15M_LIVE_BETA_CLOB_L2_ACCESS=<redacted>"));
        assert!(!redacted.contains("EXAMPLE"));
        assert!(!redacted.contains("VALUE"));
    }
}
