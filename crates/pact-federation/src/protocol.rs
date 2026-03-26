// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Federation HTTP protocol types.
//!
//! These types define the wire format for communication between federation
//! registries, discovery clients, and remote agents.

use serde::{Deserialize, Serialize};

/// Request to discover agents from a registry.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscoverRequest {
    /// Optional name filter (substring match).
    pub query: Option<String>,
    /// Optional permission filter — only return agents whose permissions are a subset.
    pub permissions: Option<Vec<String>>,
}

/// Response from agent discovery.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscoverResponse {
    /// The agent cards matching the discovery query.
    pub agents: Vec<RemoteAgentCard>,
}

/// A remote agent's advertised capabilities — mirrors the agent card format.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteAgentCard {
    /// The agent's unique name within its registry.
    pub name: String,
    /// Human-readable description of what the agent does.
    pub description: Option<String>,
    /// URL where this agent accepts dispatch requests.
    pub endpoint: String,
    /// What permissions this agent uses.
    pub permissions: Vec<String>,
    /// Tools this agent exposes.
    pub tools: Vec<RemoteToolInfo>,
    /// Compliance profile name, if any.
    pub compliance: Option<String>,
}

/// Information about a tool exposed by a remote agent.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteToolInfo {
    /// The tool's name.
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Parameters the tool accepts.
    pub parameters: Vec<RemoteParamInfo>,
}

/// A parameter on a remote tool.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteParamInfo {
    /// Parameter name.
    pub name: String,
    /// Parameter type as a string (e.g. "string", "int", "bool").
    pub param_type: String,
}

/// Request to dispatch a tool call to a remote agent.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DispatchRequest {
    /// Target agent name.
    pub agent: String,
    /// Tool to invoke on the agent.
    pub tool: String,
    /// Arguments to pass to the tool.
    pub arguments: serde_json::Value,
    /// Permissions the caller grants for this dispatch.
    pub caller_permissions: Vec<String>,
}

/// Response from a remote dispatch.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DispatchResponse {
    /// The result value from the tool invocation.
    pub result: serde_json::Value,
    /// Audit trail observations from the remote agent.
    pub observations: Vec<String>,
}

/// Request to register an agent with a registry.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterRequest {
    /// The agent card to register.
    pub card: RemoteAgentCard,
}

/// Response from agent registration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RegisterResponse {
    /// Whether registration succeeded.
    pub success: bool,
    /// Optional message (e.g. reason for failure).
    pub message: Option<String>,
}

/// Health check response from a registry.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthResponse {
    /// Registry status (e.g. "ok").
    pub status: String,
    /// Registry version.
    pub version: String,
    /// Number of agents currently registered.
    pub agent_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_card() -> RemoteAgentCard {
        RemoteAgentCard {
            name: "web-builder".into(),
            description: Some("Builds websites".into()),
            endpoint: "https://agent.example.com".into(),
            permissions: vec!["net.read".into(), "fs.write".into()],
            tools: vec![RemoteToolInfo {
                name: "build_page".into(),
                description: Some("Build an HTML page".into()),
                parameters: vec![RemoteParamInfo {
                    name: "title".into(),
                    param_type: "string".into(),
                }],
            }],
            compliance: Some("strict".into()),
        }
    }

    #[test]
    fn round_trip_remote_agent_card() {
        let card = sample_card();
        let json = serde_json::to_string(&card).unwrap();
        let parsed: RemoteAgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "web-builder");
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.tools[0].parameters[0].param_type, "string");
    }

    #[test]
    fn round_trip_discover_request() {
        let req = DiscoverRequest {
            query: Some("web".into()),
            permissions: Some(vec!["net.read".into()]),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DiscoverRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.query.unwrap(), "web");
    }

    #[test]
    fn round_trip_dispatch_request() {
        let req = DispatchRequest {
            agent: "web-builder".into(),
            tool: "build_page".into(),
            arguments: serde_json::json!({"title": "Hello"}),
            caller_permissions: vec!["net.read".into()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: DispatchRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent, "web-builder");
        assert_eq!(parsed.arguments["title"], "Hello");
    }

    #[test]
    fn round_trip_dispatch_response() {
        let resp = DispatchResponse {
            result: serde_json::json!({"html": "<h1>Hello</h1>"}),
            observations: vec!["built page in 200ms".into()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: DispatchResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.observations.len(), 1);
    }

    #[test]
    fn round_trip_register_request() {
        let req = RegisterRequest {
            card: sample_card(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: RegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.card.name, "web-builder");
    }

    #[test]
    fn round_trip_health_response() {
        let resp = HealthResponse {
            status: "ok".into(),
            version: "0.1.0".into(),
            agent_count: 42,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: HealthResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agent_count, 42);
    }
}
