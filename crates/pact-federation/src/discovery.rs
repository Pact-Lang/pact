// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Discovery client for querying remote federation registries.
//!
//! The [`DiscoveryClient`] queries one or more registry endpoints, filters
//! results through a trust boundary (permission allowlist per registry), and
//! returns matching [`RemoteAgentCard`] entries.

use tracing::{debug, warn};

use crate::error::FederationError;
use crate::protocol::{DiscoverResponse, HealthResponse, RemoteAgentCard};

/// Client for discovering agents from remote federation registries.
pub struct DiscoveryClient {
    http: reqwest::Client,
    /// Registry URLs paired with their trust permission boundaries.
    registries: Vec<(String, Vec<String>)>,
}

impl DiscoveryClient {
    /// Create a new discovery client.
    ///
    /// `registries` is a list of `(url, trusted_permissions)` pairs. Only agents
    /// whose permissions fall within the trusted set will be returned from discovery.
    pub fn new(registries: Vec<(String, Vec<String>)>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            registries,
        }
    }

    /// Discover agents across all configured registries.
    ///
    /// Filters results to only include agents whose permissions fall within the
    /// trust boundary configured for each registry. Returns `(card, registry_url)` tuples.
    pub async fn discover_all(
        &self,
        query: Option<&str>,
    ) -> Result<Vec<(RemoteAgentCard, String)>, FederationError> {
        let mut results = Vec::new();

        for (registry_url, trusted_perms) in &self.registries {
            debug!(registry = %registry_url, "querying federation registry");

            let mut url = format!("{registry_url}/discover");
            if let Some(q) = query {
                url = format!("{url}?query={}", urlencoding::encode(q));
            }

            let resp = self.http.get(&url).send().await.map_err(|e| {
                FederationError::RegistryUnavailable {
                    url: registry_url.clone(),
                    message: e.to_string(),
                }
            })?;

            if !resp.status().is_success() {
                warn!(
                    registry = %registry_url,
                    status = %resp.status(),
                    "registry returned non-success status"
                );
                return Err(FederationError::RegistryUnavailable {
                    url: registry_url.clone(),
                    message: format!("HTTP {}", resp.status()),
                });
            }

            let discover_resp: DiscoverResponse =
                resp.json()
                    .await
                    .map_err(|e| FederationError::InvalidResponse {
                        message: e.to_string(),
                    })?;

            for card in discover_resp.agents {
                if self.validate_trust(&card, trusted_perms) {
                    results.push((card, registry_url.clone()));
                } else {
                    debug!(
                        agent = %card.name,
                        registry = %registry_url,
                        "agent filtered out by trust boundary"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Discover a specific agent by name across all registries.
    pub async fn find_agent(
        &self,
        name: &str,
    ) -> Result<(RemoteAgentCard, String), FederationError> {
        let all = self.discover_all(Some(name)).await?;

        all.into_iter()
            .find(|(card, _)| card.name == name)
            .ok_or_else(|| {
                let registries: Vec<_> = self.registries.iter().map(|(u, _)| u.as_str()).collect();
                FederationError::AgentNotFound {
                    name: name.into(),
                    registry: registries.join(", "),
                }
            })
    }

    /// Check health of a specific registry.
    pub async fn check_health(
        &self,
        registry_url: &str,
    ) -> Result<HealthResponse, FederationError> {
        let url = format!("{registry_url}/health");

        let resp =
            self.http
                .get(&url)
                .send()
                .await
                .map_err(|e| FederationError::RegistryUnavailable {
                    url: registry_url.into(),
                    message: e.to_string(),
                })?;

        if !resp.status().is_success() {
            return Err(FederationError::RegistryUnavailable {
                url: registry_url.into(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        resp.json::<HealthResponse>()
            .await
            .map_err(|e| FederationError::InvalidResponse {
                message: e.to_string(),
            })
    }

    /// Validate that an agent's permissions are within the trust boundary.
    ///
    /// Every agent permission must be covered by a trusted permission — either
    /// by exact match or by parent coverage (e.g., trusted `"net"` covers `"net.read"`).
    fn validate_trust(&self, agent: &RemoteAgentCard, trusted_perms: &[String]) -> bool {
        agent.permissions.iter().all(|perm| {
            trusted_perms
                .iter()
                .any(|trusted| perm == trusted || perm.starts_with(&format!("{trusted}.")))
        })
    }
}

/// URL-encode a string for use in query parameters.
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        for ch in s.chars() {
            match ch {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(ch),
                ' ' => result.push_str("%20"),
                _ => {
                    for byte in ch.to_string().as_bytes() {
                        result.push_str(&format!("%{byte:02X}"));
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RemoteAgentCard;

    fn card(name: &str, perms: &[&str]) -> RemoteAgentCard {
        RemoteAgentCard {
            name: name.into(),
            description: None,
            endpoint: format!("https://{name}.example.com"),
            permissions: perms.iter().map(|s| (*s).into()).collect(),
            tools: vec![],
            compliance: None,
        }
    }

    #[test]
    fn validate_trust_exact_match() {
        let client = DiscoveryClient::new(vec![]);
        let agent = card("alpha", &["net.read", "fs.read"]);
        let trusted = vec!["net.read".into(), "fs.read".into()];
        assert!(client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn validate_trust_parent_coverage() {
        let client = DiscoveryClient::new(vec![]);
        let agent = card("alpha", &["net.read", "net.write"]);
        let trusted = vec!["net".into()];
        assert!(client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn validate_trust_mixed_coverage() {
        let client = DiscoveryClient::new(vec![]);
        let agent = card("alpha", &["net.read", "fs.write"]);
        let trusted = vec!["net".into(), "fs.write".into()];
        assert!(client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn validate_trust_rejects_uncovered() {
        let client = DiscoveryClient::new(vec![]);
        let agent = card("alpha", &["net.read", "fs.write"]);
        let trusted = vec!["net.read".into()];
        assert!(!client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn validate_trust_empty_permissions_passes() {
        let client = DiscoveryClient::new(vec![]);
        let agent = card("alpha", &[]);
        let trusted = vec!["net.read".into()];
        assert!(client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn validate_trust_no_false_parent_match() {
        let client = DiscoveryClient::new(vec![]);
        // "network" should NOT cover "net.read"
        let agent = card("alpha", &["net.read"]);
        let trusted = vec!["network".into()];
        assert!(!client.validate_trust(&agent, &trusted));
    }

    #[test]
    fn urlencoding_basic() {
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("web-builder"), "web-builder");
        assert_eq!(urlencoding::encode("a&b=c"), "a%26b%3Dc");
    }
}
