// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! External service connectors for PACT agent workflows.
//!
//! This crate provides a [`Connector`] trait, a [`ConnectorRegistry`] for
//! managing connectors, and 11 built-in connector implementations:
//!
//! | Connector | Service | Auth Type |
//! |-----------|---------|-----------|
//! | `github` | GitHub API | Bearer (PAT) |
//! | `slack` | Slack API | Bearer (xoxb-) |
//! | `resend` | Resend email | API key |
//! | `jira` | Jira Cloud | Basic (email + token) |
//! | `figma` | Figma API | Bearer (figd_) |
//! | `gdrive` | Google Drive | OAuth2 |
//! | `mermaid` | Mermaid Chart | Bearer |
//! | `notion` | Notion API | Bearer (secret_) |
//! | `linear` | Linear API | Bearer (API key) |
//! | `teams` | Microsoft Teams | OAuth2 (Graph) |
//! | `airtable` | Airtable API | Bearer (PAT) |
//!
//! # Adding a new connector
//!
//! 1. Create `src/connectors/<name>.rs` implementing the [`Connector`] trait
//! 2. Add `pub mod <name>;` in `src/connectors/mod.rs`
//! 3. Register it in [`ConnectorRegistry::new`]

pub mod connectors;

pub use connectors::{
    execute_connector, AirtableConfig, Connector, ConnectorConfig, ConnectorError,
    ConnectorRegistry, FigmaConfig, GDriveConfig, GitHubConfig, JiraConfig, LinearConfig,
    MermaidConfig, NotionConfig, ResendConfig, SlackConfig, TeamsConfig,
};
