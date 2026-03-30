// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! In-memory agent registry for federation.
//!
//! Stores [`RemoteAgentCard`] entries and supports discovery queries with
//! name and permission filtering.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tracing::{debug, warn};

use crate::protocol::{
    DiscoverRequest, DiscoverResponse, HealthResponse, RegisterResponse, RemoteAgentCard,
};

/// In-memory registry of remote agent cards.
pub struct AgentRegistry {
    agents: Arc<RwLock<HashMap<String, RemoteAgentCard>>>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an agent card. Overwrites any existing entry with the same name.
    pub fn register(&self, card: RemoteAgentCard) -> RegisterResponse {
        let name = card.name.clone();
        let mut agents = self.agents.write().expect("registry lock poisoned");
        let overwritten = agents.insert(name.clone(), card).is_some();

        if overwritten {
            debug!(agent = %name, "updated existing agent registration");
        } else {
            debug!(agent = %name, "registered new agent");
        }

        RegisterResponse {
            success: true,
            message: Some(if overwritten {
                format!("updated registration for '{name}'")
            } else {
                format!("registered '{name}'")
            }),
        }
    }

    /// Remove an agent by name. Returns `true` if the agent was found and removed.
    pub fn unregister(&self, name: &str) -> bool {
        let mut agents = self.agents.write().expect("registry lock poisoned");
        let removed = agents.remove(name).is_some();
        if removed {
            debug!(agent = %name, "unregistered agent");
        } else {
            warn!(agent = %name, "attempted to unregister unknown agent");
        }
        removed
    }

    /// Discover agents matching the query.
    ///
    /// - If `query` is set, only agents whose name contains the query substring are returned.
    /// - If `permissions` is set, only agents whose permissions are a subset of the
    ///   requested permissions are returned.
    pub fn discover(&self, req: &DiscoverRequest) -> DiscoverResponse {
        let agents = self.agents.read().expect("registry lock poisoned");

        let matching: Vec<RemoteAgentCard> = agents
            .values()
            .filter(|card| {
                // Name substring filter.
                if let Some(ref q) = req.query {
                    if !card.name.to_lowercase().contains(&q.to_lowercase()) {
                        return false;
                    }
                }
                // Permission subset filter.
                if let Some(ref required) = req.permissions {
                    let all_covered = card.permissions.iter().all(|perm| {
                        required
                            .iter()
                            .any(|r| perm == r || perm.starts_with(&format!("{r}.")))
                    });
                    if !all_covered {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        debug!(count = matching.len(), "discovery returned agents");
        DiscoverResponse { agents: matching }
    }

    /// Health check returning the registry status.
    pub fn health(&self) -> HealthResponse {
        let count = self.agents.read().expect("registry lock poisoned").len();
        HealthResponse {
            status: "ok".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            agent_count: count,
        }
    }

    /// Get all registered agents.
    pub fn list_all(&self) -> Vec<RemoteAgentCard> {
        self.agents
            .read()
            .expect("registry lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    /// Get a specific agent by name.
    pub fn get(&self, name: &str) -> Option<RemoteAgentCard> {
        self.agents
            .read()
            .expect("registry lock poisoned")
            .get(name)
            .cloned()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RemoteAgentCard;

    fn card(name: &str, perms: &[&str]) -> RemoteAgentCard {
        RemoteAgentCard {
            name: name.into(),
            description: Some(format!("{name} agent")),
            endpoint: format!("https://{name}.example.com"),
            permissions: perms.iter().map(|s| (*s).into()).collect(),
            tools: vec![],
            compliance: None,
        }
    }

    #[test]
    fn register_and_get() {
        let reg = AgentRegistry::new();
        let resp = reg.register(card("alpha", &["net.read"]));
        assert!(resp.success);
        assert!(resp.message.unwrap().contains("registered"));

        let agent = reg.get("alpha").expect("agent should exist");
        assert_eq!(agent.name, "alpha");
    }

    #[test]
    fn register_overwrites() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &["net.read"]));
        let resp = reg.register(card("alpha", &["net.read", "fs.write"]));
        assert!(resp.success);
        assert!(resp.message.unwrap().contains("updated"));

        let agent = reg.get("alpha").unwrap();
        assert_eq!(agent.permissions.len(), 2);
    }

    #[test]
    fn unregister_existing() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &[]));
        assert!(reg.unregister("alpha"));
        assert!(reg.get("alpha").is_none());
    }

    #[test]
    fn unregister_missing() {
        let reg = AgentRegistry::new();
        assert!(!reg.unregister("nonexistent"));
    }

    #[test]
    fn discover_no_filter() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &["net.read"]));
        reg.register(card("beta", &["fs.write"]));

        let resp = reg.discover(&DiscoverRequest {
            query: None,
            permissions: None,
        });
        assert_eq!(resp.agents.len(), 2);
    }

    #[test]
    fn discover_with_name_query() {
        let reg = AgentRegistry::new();
        reg.register(card("web-builder", &["net.read"]));
        reg.register(card("data-processor", &["fs.read"]));

        let resp = reg.discover(&DiscoverRequest {
            query: Some("web".into()),
            permissions: None,
        });
        assert_eq!(resp.agents.len(), 1);
        assert_eq!(resp.agents[0].name, "web-builder");
    }

    #[test]
    fn discover_case_insensitive() {
        let reg = AgentRegistry::new();
        reg.register(card("WebBuilder", &["net.read"]));

        let resp = reg.discover(&DiscoverRequest {
            query: Some("webbuilder".into()),
            permissions: None,
        });
        assert_eq!(resp.agents.len(), 1);
    }

    #[test]
    fn discover_with_permission_filter() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &["net.read"]));
        reg.register(card("beta", &["net.read", "fs.write"]));

        // Only agents whose perms are subset of ["net.read"]
        let resp = reg.discover(&DiscoverRequest {
            query: None,
            permissions: Some(vec!["net.read".into()]),
        });
        assert_eq!(resp.agents.len(), 1);
        assert_eq!(resp.agents[0].name, "alpha");
    }

    #[test]
    fn discover_permission_parent_coverage() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &["net.read", "net.write"]));

        // Parent "net" should cover "net.read" and "net.write"
        let resp = reg.discover(&DiscoverRequest {
            query: None,
            permissions: Some(vec!["net".into()]),
        });
        assert_eq!(resp.agents.len(), 1);
    }

    #[test]
    fn health_reports_count() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &[]));
        reg.register(card("beta", &[]));

        let health = reg.health();
        assert_eq!(health.status, "ok");
        assert_eq!(health.agent_count, 2);
    }

    #[test]
    fn list_all_returns_all() {
        let reg = AgentRegistry::new();
        reg.register(card("alpha", &[]));
        reg.register(card("beta", &[]));
        reg.register(card("gamma", &[]));

        let all = reg.list_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn get_missing_returns_none() {
        let reg = AgentRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }
}
