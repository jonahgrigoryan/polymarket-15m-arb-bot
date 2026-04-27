use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const MODULE: &str = "compliance";

#[derive(Debug, Clone)]
pub struct ComplianceClient {
    http: reqwest::Client,
    geoblock_url: String,
}

impl ComplianceClient {
    pub fn new(geoblock_url: impl Into<String>, timeout_ms: u64) -> ComplianceResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| ComplianceError::ClientBuild(source.to_string()))?;

        Ok(Self {
            http,
            geoblock_url: geoblock_url.into(),
        })
    }

    pub async fn check_geoblock(&self) -> ComplianceResult<GeoblockResponse> {
        let response = self
            .http
            .get(&self.geoblock_url)
            .send()
            .await
            .map_err(|source| ComplianceError::Request {
                url: self.geoblock_url.clone(),
                message: source.to_string(),
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ComplianceError::HttpStatus {
                url: self.geoblock_url.clone(),
                status: status.as_u16(),
            });
        }

        response.json::<GeoblockResponse>().await.map_err(|source| {
            ComplianceError::ResponseDecode {
                url: self.geoblock_url.clone(),
                message: source.to_string(),
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GeoblockResponse {
    pub blocked: bool,
    pub ip: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
}

impl GeoblockResponse {
    pub fn trading_allowed(&self) -> bool {
        !self.blocked
    }

    pub fn masked_for_logs(&self) -> Self {
        let mut masked = self.clone();
        if masked.ip.is_some() {
            masked.ip = Some("<masked>".to_string());
        }
        masked
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComplianceDecision {
    Allowed,
    Blocked {
        country: Option<String>,
        region: Option<String>,
    },
}

impl From<&GeoblockResponse> for ComplianceDecision {
    fn from(value: &GeoblockResponse) -> Self {
        if value.blocked {
            ComplianceDecision::Blocked {
                country: value.country.clone(),
                region: value.region.clone(),
            }
        } else {
            ComplianceDecision::Allowed
        }
    }
}

pub type ComplianceResult<T> = Result<T, ComplianceError>;

#[derive(Debug)]
pub enum ComplianceError {
    ClientBuild(String),
    Request {
        url: String,
        message: String,
    },
    HttpStatus {
        url: String,
        status: u16,
    },
    ResponseDecode {
        url: String,
        message: String,
    },
    Blocked {
        country: Option<String>,
        region: Option<String>,
    },
}

impl ComplianceError {
    pub fn fail_if_blocked(response: &GeoblockResponse) -> ComplianceResult<()> {
        match ComplianceDecision::from(response) {
            ComplianceDecision::Allowed => Ok(()),
            ComplianceDecision::Blocked { country, region } => {
                Err(ComplianceError::Blocked { country, region })
            }
        }
    }
}

impl Display for ComplianceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceError::ClientBuild(message) => {
                write!(
                    formatter,
                    "failed to build compliance HTTP client: {message}"
                )
            }
            ComplianceError::Request { url, message } => {
                write!(formatter, "geoblock request failed for {url}: {message}")
            }
            ComplianceError::HttpStatus { url, status } => {
                write!(
                    formatter,
                    "geoblock request to {url} returned HTTP {status}"
                )
            }
            ComplianceError::ResponseDecode { url, message } => {
                write!(
                    formatter,
                    "geoblock response from {url} could not be decoded: {message}"
                )
            }
            ComplianceError::Blocked { country, region } => {
                write!(
                    formatter,
                    "trading unavailable from geoblocked location country={:?} region={:?}",
                    country, region
                )
            }
        }
    }
}

impl Error for ComplianceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_response_fails_closed() {
        let response = GeoblockResponse {
            blocked: true,
            ip: Some("203.0.113.1".to_string()),
            country: Some("US".to_string()),
            region: Some("CA".to_string()),
        };

        let error =
            ComplianceError::fail_if_blocked(&response).expect_err("blocked response fails");

        assert!(error.to_string().contains("geoblocked"));
    }

    #[test]
    fn allowed_response_passes() {
        let response = GeoblockResponse {
            blocked: false,
            ip: None,
            country: Some("IE".to_string()),
            region: None,
        };

        ComplianceError::fail_if_blocked(&response).expect("allowed response passes");
    }

    #[test]
    fn masked_response_does_not_keep_ip() {
        let response = GeoblockResponse {
            blocked: true,
            ip: Some("203.0.113.1".to_string()),
            country: Some("US".to_string()),
            region: Some("CA".to_string()),
        };

        assert_eq!(response.masked_for_logs().ip, Some("<masked>".to_string()));
    }
}
