// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Remote dispatch client for invoking tools on federated agents.
//!
//! The [`FederationClient`] sends tool call requests to remote PACT agents
//! and handles agent registration with federation registries.

use crate::error::FederationError;
use crate::protocol::{
    DispatchRequest, DispatchResponse, RegisterRequest, RegisterResponse, RemoteAgentCard,
};

/// Client for dispatching tool calls to remote PACT agents.
pub struct FederationClient {
    http: reqwest::Client,
}

impl FederationClient {
    /// Create a new federation client with a 90-second timeout.
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Dispatch a tool call to a remote agent.
    pub async fn dispatch(
        &self,
        endpoint: &str,
        request: &DispatchRequest,
    ) -> Result<DispatchResponse, FederationError> {
        let url = format!("{endpoint}/dispatch");

        let resp = self
            .http
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|e| FederationError::DispatchFailed {
                agent: request.agent.clone(),
                endpoint: endpoint.to_string(),
                message: e.to_string(),
            })?;

        if !resp.status().is_success() {
            return Err(FederationError::DispatchFailed {
                agent: request.agent.clone(),
                endpoint: endpoint.to_string(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        resp.json::<DispatchResponse>()
            .await
            .map_err(|e| FederationError::InvalidResponse {
                message: e.to_string(),
            })
    }

    /// Register an agent with a remote registry.
    pub async fn register(
        &self,
        registry_url: &str,
        card: &RemoteAgentCard,
    ) -> Result<RegisterResponse, FederationError> {
        let url = format!("{registry_url}/register");
        let req = RegisterRequest {
            card: card.clone(),
        };

        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| FederationError::RegistryUnavailable {
                url: registry_url.to_string(),
                message: e.to_string(),
            })?;

        resp.json::<RegisterResponse>()
            .await
            .map_err(|e| FederationError::InvalidResponse {
                message: e.to_string(),
            })
    }
}

impl Default for FederationClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{
        DispatchRequest, DispatchResponse, RegisterRequest, RegisterResponse, RemoteAgentCard,
        RemoteToolInfo,
    };

    fn sample_card() -> RemoteAgentCard {
        RemoteAgentCard {
            name: "test-agent".into(),
            description: Some("A test agent".into()),
            endpoint: "https://test.example.com".into(),
            permissions: vec!["net.read".into()],
            tools: vec![RemoteToolInfo {
                name: "ping".into(),
                description: Some("Ping a host".into()),
                parameters: vec![],
            }],
            compliance: None,
        }
    }

    #[test]
    fn client_creation() {
        let client = FederationClient::new();
        // Just verify it doesn't panic.
        let _ = client;
    }

    #[test]
    fn client_default() {
        let client = FederationClient::default();
        let _ = client;
    }

    #[test]
    fn dispatch_request_serialization() {
        let req = DispatchRequest {
            agent: "test-agent".into(),
            tool: "ping".into(),
            arguments: serde_json::json!({"host": "example.com"}),
            caller_permissions: vec!["net.read".into()],
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: DispatchRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent, "test-agent");
        assert_eq!(parsed.tool, "ping");
        assert_eq!(parsed.arguments["host"], "example.com");
    }

    #[test]
    fn dispatch_response_serialization() {
        let resp = DispatchResponse {
            result: serde_json::json!({"latency_ms": 42}),
            observations: vec!["host reachable".into(), "latency measured".into()],
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DispatchResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.result["latency_ms"], 42);
        assert_eq!(parsed.observations.len(), 2);
    }

    #[test]
    fn register_request_serialization() {
        let req = RegisterRequest {
            card: sample_card(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let parsed: RegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.card.name, "test-agent");
        assert_eq!(parsed.card.tools.len(), 1);
    }

    #[test]
    fn register_response_serialization() {
        let resp = RegisterResponse {
            success: true,
            message: Some("registered".into()),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: RegisterResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
    }
}
