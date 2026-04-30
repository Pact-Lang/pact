// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! External service connectors for PACT flows.
//!
//! Each connector implements the [`Connector`] trait and is registered in the
//! [`ConnectorRegistry`]. Adding a new connector = 1 new file + 1 registration
//! line in [`ConnectorRegistry::new`].

#[cfg(test)]
mod tests;

pub mod airtable;
pub mod figma;
pub mod gdrive;
pub mod github;
pub mod jira;
pub mod linear;
pub mod mermaid;
pub mod notion;
pub mod resend;
pub mod slack;
pub mod teams;

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

// ── Connector trait ─────────────────────────────────────────────────────────

/// Trait that every connector must implement.
///
/// Adding a new connector requires:
/// 1. Create `connectors/<name>.rs` implementing this trait
/// 2. Register it in [`ConnectorRegistry::new`]
#[async_trait]
pub trait Connector: Send + Sync {
    /// Unique connector name (e.g. "github", "slack").
    fn name(&self) -> &'static str;

    /// Human-readable description.
    fn description(&self) -> &'static str;

    /// Credential schema for /inspect responses.
    fn credential_schema(&self) -> serde_json::Value;

    /// Full spec for GET /connectors (auth, credentials, operations).
    fn spec(&self) -> serde_json::Value;

    /// List of supported operations (e.g. ["push_file", "create_pr"]).
    fn operations(&self) -> Vec<&'static str>;

    /// Check if this connector is configured in the given config.
    fn is_configured(&self, config: &ConnectorConfig) -> bool;

    /// Execute an operation.
    async fn execute(
        &self,
        action: &str,
        params: &HashMap<String, String>,
        config: &ConnectorConfig,
    ) -> Result<String, ConnectorError>;

    /// Optional: prompt enrichment (e.g. Mermaid syntax guide).
    fn prompt_additions(&self) -> Option<&'static str> {
        None
    }
}

// ── Connector registry ──────────────────────────────────────────────────────

/// Registry of all available connectors.
pub struct ConnectorRegistry {
    connectors: Vec<Box<dyn Connector>>,
}

impl ConnectorRegistry {
    /// Create a new registry with all built-in connectors.
    pub fn new() -> Self {
        let connectors: Vec<Box<dyn Connector>> = vec![
            Box::new(github::GitHubConnector),
            Box::new(figma::FigmaConnector),
            Box::new(slack::SlackConnector),
            Box::new(resend::ResendConnector),
            Box::new(gdrive::GDriveConnector),
            Box::new(mermaid::MermaidConnector),
            Box::new(jira::JiraConnector),
            Box::new(notion::NotionConnector),
            Box::new(linear::LinearConnector),
            Box::new(teams::TeamsConnector),
            Box::new(airtable::AirtableConnector),
        ];
        Self { connectors }
    }

    /// Look up a connector by name.
    pub fn get(&self, name: &str) -> Option<&dyn Connector> {
        self.connectors
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.as_ref())
    }

    /// All registered connectors.
    pub fn all(&self) -> Vec<&dyn Connector> {
        self.connectors.iter().map(|c| c.as_ref()).collect()
    }

    /// Check if a named connector is configured.
    pub fn is_configured(&self, name: &str, config: &ConnectorConfig) -> bool {
        self.get(name).map_or(false, |c| c.is_configured(config))
    }

    /// Return the credential schema for a named connector.
    pub fn credential_schema(&self, name: &str) -> serde_json::Value {
        self.get(name)
            .map(|c| c.credential_schema())
            .unwrap_or_else(|| {
                serde_json::json!({
                    "name": name,
                    "description": format!("Unknown connector: {}", name),
                    "credentials": {}
                })
            })
    }

    /// Build the full GET /connectors response.
    pub fn full_spec(&self) -> serde_json::Value {
        let mut connectors = serde_json::Map::new();
        for c in &self.connectors {
            connectors.insert(c.name().to_string(), c.spec());
        }
        serde_json::json!({
            "version": "1.0.0",
            "connectors": connectors,
        })
    }

    /// Build the connector section for GET /schema.
    pub fn schema_connectors(&self) -> serde_json::Value {
        let mut connectors = serde_json::Map::new();
        for c in &self.connectors {
            let ops: Vec<String> = c
                .operations()
                .iter()
                .map(|op| format!("{}.{}", c.name(), op))
                .collect();
            connectors.insert(
                c.name().to_string(),
                serde_json::json!({
                    "description": c.description(),
                    "credentials": c.credential_schema()["credentials"],
                    "operations": ops,
                }),
            );
        }
        serde_json::Value::Object(connectors)
    }

    /// Build the connector enum for the tool schema's connector field.
    pub fn all_operations(&self) -> Vec<String> {
        self.connectors
            .iter()
            .flat_map(|c| {
                let name = c.name();
                c.operations()
                    .into_iter()
                    .map(move |op| format!("{name}.{op}"))
            })
            .collect()
    }

    /// Execute a connector operation by full name (e.g. "github.push_file").
    pub async fn execute_operation(
        &self,
        operation: &str,
        params: &HashMap<String, String>,
        config: &ConnectorConfig,
    ) -> Result<String, ConnectorError> {
        let (connector_name, action) = operation
            .split_once('.')
            .ok_or_else(|| ConnectorError::InvalidOperation(operation.to_string()))?;

        let connector = self
            .get(connector_name)
            .ok_or_else(|| ConnectorError::UnknownConnector(connector_name.into()))?;

        if !connector.is_configured(config) {
            return Err(ConnectorError::NotConfigured(connector_name.into()));
        }

        connector.execute(action, params, config).await
    }

    /// Collect prompt additions from all configured connectors.
    pub fn prompt_additions_for(&self, config: &ConnectorConfig) -> Vec<&'static str> {
        self.connectors
            .iter()
            .filter(|c| c.is_configured(config))
            .filter_map(|c| c.prompt_additions())
            .collect()
    }

    /// Check if a connector name is known.
    pub fn is_known(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Map a connector name to its canonical &'static str.
    pub fn canonical_name(&self, name: &str) -> &'static str {
        self.get(name)
            .map(|c| c.name())
            .unwrap_or_else(|| Box::leak(name.to_string().into_boxed_str()))
    }
}

impl Default for ConnectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── ConnectorConfig ─────────────────────────────────────────────────────────

/// Connector credentials provided by the user at flow execution time.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ConnectorConfig {
    #[serde(default)]
    pub github: Option<GitHubConfig>,
    #[serde(default)]
    pub figma: Option<FigmaConfig>,
    #[serde(default)]
    pub slack: Option<SlackConfig>,
    #[serde(default)]
    pub resend: Option<ResendConfig>,
    #[serde(default)]
    pub gdrive: Option<GDriveConfig>,
    #[serde(default)]
    pub mermaid: Option<MermaidConfig>,
    #[serde(default)]
    pub jira: Option<JiraConfig>,
    #[serde(default)]
    pub notion: Option<NotionConfig>,
    #[serde(default)]
    pub linear: Option<LinearConfig>,
    #[serde(default)]
    pub teams: Option<TeamsConfig>,
    #[serde(default)]
    pub airtable: Option<AirtableConfig>,
}

// ── Per-connector config structs ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubConfig {
    pub token: String,
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FigmaConfig {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackConfig {
    pub token: String,
    #[serde(default)]
    pub default_channel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResendConfig {
    pub api_key: String,
    #[serde(default = "default_resend_from")]
    pub from: String,
}

fn default_resend_from() -> String {
    "onboarding@resend.dev".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct GDriveConfig {
    pub access_token: String,
    #[serde(default)]
    pub default_folder: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MermaidConfig {
    pub token: String,
    #[serde(default)]
    pub default_project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JiraConfig {
    pub email: String,
    pub api_token: String,
    pub domain: String,
    #[serde(default)]
    pub default_project: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotionConfig {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinearConfig {
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamsConfig {
    pub access_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AirtableConfig {
    pub token: String,
    #[serde(default)]
    pub base_id: Option<String>,
}

// ── execute_connector ───────────────────────────────────────────────────────

/// Execute a connector operation via a temporary registry.
pub async fn execute_connector(
    operation: &str,
    params: &HashMap<String, String>,
    config: &ConnectorConfig,
) -> Result<String, ConnectorError> {
    let registry = ConnectorRegistry::new();
    registry.execute_operation(operation, params, config).await
}

// ── ConnectorError ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("unknown connector: {0}")]
    UnknownConnector(String),

    #[error("invalid operation format: {0} (expected 'connector.action')")]
    InvalidOperation(String),

    #[error("connector '{0}' is not configured — provide credentials in the connectors field")]
    NotConfigured(String),

    #[error("missing required parameter: {0}")]
    MissingParam(String),

    #[error("API error ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("HTTP error: {0}")]
    HttpError(String),
}
