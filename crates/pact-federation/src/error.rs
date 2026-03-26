// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Error types for the federation crate.

use thiserror::Error;

/// Errors that can occur during federation operations.
#[derive(Error, Debug)]
pub enum FederationError {
    /// A federation registry is unreachable or returned a non-success status.
    #[error("registry unavailable at {url}: {message}")]
    RegistryUnavailable { url: String, message: String },

    /// The requested agent was not found in the given registry.
    #[error("agent '{name}' not found in registry '{registry}'")]
    AgentNotFound { name: String, registry: String },

    /// The caller does not have the required permission to invoke the agent.
    #[error("permission denied: agent '{agent}' requires '{permission}' (registry '{registry}')")]
    PermissionDenied {
        agent: String,
        permission: String,
        registry: String,
    },

    /// A remote dispatch call failed.
    #[error("dispatch to agent '{agent}' at {endpoint} failed: {message}")]
    DispatchFailed {
        agent: String,
        endpoint: String,
        message: String,
    },

    /// The response from a remote endpoint could not be parsed.
    #[error("invalid response: {message}")]
    InvalidResponse { message: String },

    /// An agent's permissions exceed the trust boundary configured for its registry.
    #[error(
        "trust violation: agent '{agent}' uses permission '{permission}' not covered by trusted set {trusted:?}"
    )]
    TrustViolation {
        agent: String,
        permission: String,
        trusted: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_registry_unavailable() {
        let err = FederationError::RegistryUnavailable {
            url: "https://registry.example.com".into(),
            message: "connection refused".into(),
        };
        assert!(err.to_string().contains("registry unavailable"));
        assert!(err.to_string().contains("registry.example.com"));
    }

    #[test]
    fn error_display_agent_not_found() {
        let err = FederationError::AgentNotFound {
            name: "web-builder".into(),
            registry: "https://registry.example.com".into(),
        };
        assert!(err.to_string().contains("web-builder"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn error_display_permission_denied() {
        let err = FederationError::PermissionDenied {
            agent: "web-builder".into(),
            permission: "fs.write".into(),
            registry: "https://registry.example.com".into(),
        };
        assert!(err.to_string().contains("permission denied"));
        assert!(err.to_string().contains("fs.write"));
    }

    #[test]
    fn error_display_dispatch_failed() {
        let err = FederationError::DispatchFailed {
            agent: "web-builder".into(),
            endpoint: "https://agent.example.com".into(),
            message: "timeout".into(),
        };
        assert!(err.to_string().contains("dispatch"));
        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn error_display_invalid_response() {
        let err = FederationError::InvalidResponse {
            message: "expected JSON".into(),
        };
        assert!(err.to_string().contains("invalid response"));
    }

    #[test]
    fn error_display_trust_violation() {
        let err = FederationError::TrustViolation {
            agent: "web-builder".into(),
            permission: "fs.write".into(),
            trusted: vec!["net.read".into()],
        };
        assert!(err.to_string().contains("trust violation"));
        assert!(err.to_string().contains("fs.write"));
    }
}
