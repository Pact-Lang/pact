// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Federated agent dispatcher for cross-network agent dispatch.
//!
//! The [`FederatedDispatcher`] forwards `@agent -> #tool(args)` calls to
//! remote PACT agent endpoints over HTTP. It validates that the remote
//! agent's permissions fall within the trust boundary before dispatching.
//!
//! # Protocol
//!
//! The dispatcher sends a `POST /dispatch` request to the agent's endpoint
//! with a JSON body containing the agent name, tool name, arguments, and
//! the caller's granted permissions. The remote endpoint returns a JSON
//! response with the result.
//!
//! # Trust Boundaries
//!
//! Each federation registry declares a trust boundary — the maximum set
//! of permissions that agents from that registry may use. The dispatcher
//! validates that the requested agent's permissions are a subset of the
//! caller's trust boundary before sending the request.

use std::collections::HashMap;

use pact_core::ast::stmt::{AgentDecl, Program};
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Dispatcher;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Request body for remote agent dispatch.
#[derive(Debug, Serialize)]
struct RemoteDispatchRequest {
    agent: String,
    tool: String,
    arguments: Vec<serde_json::Value>,
    caller_permissions: Vec<String>,
}

/// Response body from remote agent dispatch.
#[derive(Debug, Deserialize)]
struct RemoteDispatchResponse {
    result: serde_json::Value,
    #[serde(default)]
    observations: Vec<String>,
}

/// Federated dispatcher that forwards tool calls to remote PACT agents.
///
/// Remote agents are identified by their `endpoint` field in the agent
/// declaration. The dispatcher maintains a trust map from registry URLs
/// to their permitted permission boundaries.
pub struct FederatedDispatcher {
    /// HTTP client for making remote requests.
    http: reqwest::Client,
    /// Tokio runtime for blocking on async HTTP calls.
    runtime: tokio::runtime::Runtime,
    /// Trust boundaries: registry URL -> list of trusted permissions.
    trust_map: HashMap<String, Vec<String>>,
    /// Fallback dispatcher for local agents (without endpoints).
    fallback: Box<dyn Dispatcher>,
}

impl FederatedDispatcher {
    /// Create a new federated dispatcher with a fallback for local agents.
    ///
    /// The `trust_map` maps registry URLs to the permissions trusted from
    /// that registry. The `fallback` dispatcher handles agents without
    /// an `endpoint` field.
    pub fn new(
        trust_map: HashMap<String, Vec<String>>,
        fallback: Box<dyn Dispatcher>,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| format!("failed to create HTTP client: {e}"))?;
        let runtime =
            tokio::runtime::Runtime::new().map_err(|e| format!("failed to create runtime: {e}"))?;
        Ok(Self {
            http,
            runtime,
            trust_map,
            fallback,
        })
    }

    /// Validate that the agent's permissions are within the trust boundary.
    fn validate_trust(&self, agent_name: &str, agent_perms: &[String]) -> Result<(), String> {
        // Collect all trusted permissions from all registries.
        let all_trusted: Vec<&String> = self.trust_map.values().flatten().collect();

        for perm in agent_perms {
            let is_trusted = all_trusted
                .iter()
                .any(|trusted| *trusted == perm || perm.starts_with(&format!("{trusted}.")));
            if !is_trusted {
                return Err(format!(
                    "remote agent '@{agent_name}' requires permission '^{perm}' \
                     which is not in any federation trust boundary"
                ));
            }
        }
        Ok(())
    }

    /// Extract permission strings from an agent declaration.
    fn extract_permissions(agent_decl: &AgentDecl) -> Vec<String> {
        agent_decl
            .permits
            .iter()
            .filter_map(|e| match &e.kind {
                pact_core::ast::expr::ExprKind::PermissionRef(segs) => Some(segs.join(".")),
                _ => None,
            })
            .collect()
    }

    /// Convert PACT values to JSON for the remote dispatch request.
    fn values_to_json(args: &[Value]) -> Vec<serde_json::Value> {
        args.iter().map(Self::value_to_json).collect()
    }

    fn value_to_json(val: &Value) -> serde_json::Value {
        match val {
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Int(i) => serde_json::json!(i),
            Value::Float(f) => serde_json::json!(f),
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Null => serde_json::Value::Null,
            Value::List(items) => {
                serde_json::Value::Array(items.iter().map(Self::value_to_json).collect())
            }
            Value::Record(fields) => {
                let map: serde_json::Map<String, serde_json::Value> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::value_to_json(v)))
                    .collect();
                serde_json::Value::Object(map)
            }
            Value::ToolResult(s) => serde_json::Value::String(s.clone()),
            Value::AgentRef(s) => serde_json::Value::String(format!("@{s}")),
        }
    }

    /// Convert a JSON response value back to a PACT Value.
    fn json_to_value(json: &serde_json::Value) -> Value {
        match json {
            serde_json::Value::String(s) => Value::String(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    Value::String(n.to_string())
                }
            }
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Array(arr) => {
                Value::List(arr.iter().map(Self::json_to_value).collect())
            }
            serde_json::Value::Object(map) => {
                let fields: HashMap<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::json_to_value(v)))
                    .collect();
                Value::Record(fields)
            }
        }
    }

    /// Send the remote dispatch request.
    async fn dispatch_remote(
        &self,
        endpoint: &str,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        caller_permissions: Vec<String>,
    ) -> Result<Value, String> {
        let url = format!("{endpoint}/dispatch");
        let request = RemoteDispatchRequest {
            agent: agent_name.to_string(),
            tool: tool_name.to_string(),
            arguments: Self::values_to_json(args),
            caller_permissions,
        };

        debug!(
            agent = agent_name,
            tool = tool_name,
            endpoint = endpoint,
            "sending remote dispatch"
        );

        let resp = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                format!("failed to reach remote agent '@{agent_name}' at {endpoint}: {e}")
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "remote agent '@{agent_name}' returned HTTP {status}: {body}"
            ));
        }

        let response: RemoteDispatchResponse = resp.json().await.map_err(|e| {
            format!("failed to parse response from remote agent '@{agent_name}': {e}")
        })?;

        for obs in &response.observations {
            info!(
                agent = agent_name,
                observation = obs.as_str(),
                "remote observation"
            );
        }

        Ok(Self::json_to_value(&response.result))
    }
}

impl Dispatcher for FederatedDispatcher {
    fn dispatch(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        agent_decl: &AgentDecl,
        program: &Program,
    ) -> Result<Value, String> {
        // If the agent has an endpoint, dispatch remotely.
        if let Some(ref endpoint) = agent_decl.endpoint {
            info!(
                agent = agent_name,
                tool = tool_name,
                endpoint = endpoint.as_str(),
                "federated dispatch to remote agent"
            );

            // Validate trust boundary.
            let perms = Self::extract_permissions(agent_decl);
            self.validate_trust(agent_name, &perms)?;

            // Dispatch remotely.
            self.runtime
                .block_on(self.dispatch_remote(endpoint, agent_name, tool_name, args, perms))
        } else {
            // No endpoint — delegate to local fallback dispatcher.
            debug!(
                agent = agent_name,
                tool = tool_name,
                "no endpoint, falling back to local dispatch"
            );
            self.fallback
                .dispatch(agent_name, tool_name, args, agent_decl, program)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::ast::expr::{Expr, ExprKind};
    use pact_core::interpreter::MockDispatcher;
    use pact_core::span::{SourceId, Span};

    fn make_agent(name: &str, endpoint: Option<&str>, perms: &[&str]) -> AgentDecl {
        AgentDecl {
            name: name.to_string(),
            permits: perms
                .iter()
                .map(|p| Expr {
                    kind: ExprKind::PermissionRef(p.split('.').map(String::from).collect()),
                    span: Span::new(SourceId(0), 0, 0),
                })
                .collect(),
            tools: vec![],
            skills: vec![],
            model: None,
            prompt: None,
            memory: vec![],
            compliance: None,
            endpoint: endpoint.map(String::from),
        }
    }

    #[test]
    fn local_agent_delegates_to_fallback() {
        let trust_map = HashMap::new();
        let fallback = Box::new(MockDispatcher);
        let dispatcher = FederatedDispatcher::new(trust_map, fallback).unwrap();

        let agent = make_agent("local_bot", None, &["llm.query"]);
        let program = Program { decls: vec![] };

        let result = dispatcher
            .dispatch(
                "local_bot",
                "greet",
                &[Value::String("hi".into())],
                &agent,
                &program,
            )
            .unwrap();

        assert!(matches!(result, Value::ToolResult(_)));
    }

    #[test]
    fn trust_validation_passes_for_covered_permissions() {
        let mut trust_map = HashMap::new();
        trust_map.insert(
            "https://registry.example.com".to_string(),
            vec!["llm".to_string(), "net.read".to_string()],
        );
        let fallback = Box::new(MockDispatcher);
        let dispatcher = FederatedDispatcher::new(trust_map, fallback).unwrap();

        // "llm.query" is covered by "llm" (parent), "net.read" is exact match.
        assert!(dispatcher
            .validate_trust("bot", &["llm.query".to_string(), "net.read".to_string()])
            .is_ok());
    }

    #[test]
    fn trust_validation_fails_for_uncovered_permissions() {
        let mut trust_map = HashMap::new();
        trust_map.insert(
            "https://registry.example.com".to_string(),
            vec!["llm.query".to_string()],
        );
        let fallback = Box::new(MockDispatcher);
        let dispatcher = FederatedDispatcher::new(trust_map, fallback).unwrap();

        // "net.read" is not covered by any trust boundary.
        let result = dispatcher.validate_trust("bot", &["net.read".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("net.read"));
    }

    #[test]
    fn value_json_roundtrip() {
        let val = Value::Record(HashMap::from([
            ("name".to_string(), Value::String("test".into())),
            ("count".to_string(), Value::Int(42)),
            ("active".to_string(), Value::Bool(true)),
            (
                "tags".to_string(),
                Value::List(vec![Value::String("a".into()), Value::String("b".into())]),
            ),
        ]));

        let json = FederatedDispatcher::value_to_json(&val);
        let roundtrip = FederatedDispatcher::json_to_value(&json);

        // Check structure preserved (Records don't have guaranteed order, so check fields).
        match &roundtrip {
            Value::Record(fields) => {
                assert_eq!(fields.get("name"), Some(&Value::String("test".into())));
                assert_eq!(fields.get("count"), Some(&Value::Int(42)));
                assert_eq!(fields.get("active"), Some(&Value::Bool(true)));
            }
            _ => panic!("expected Record"),
        }
    }

    #[test]
    fn extract_permissions_from_agent() {
        let agent = make_agent("bot", None, &["llm.query", "net.read"]);
        let perms = FederatedDispatcher::extract_permissions(&agent);
        assert_eq!(perms, vec!["llm.query", "net.read"]);
    }

    #[test]
    fn remote_agent_without_trust_fails() {
        // Empty trust map — no permissions trusted.
        let trust_map = HashMap::new();
        let fallback = Box::new(MockDispatcher);
        let dispatcher = FederatedDispatcher::new(trust_map, fallback).unwrap();

        let agent = make_agent(
            "remote_bot",
            Some("https://remote.example.com"),
            &["llm.query"],
        );
        let program = Program { decls: vec![] };

        let result = dispatcher.dispatch(
            "remote_bot",
            "greet",
            &[Value::String("hi".into())],
            &agent,
            &program,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("trust boundary"));
    }
}
