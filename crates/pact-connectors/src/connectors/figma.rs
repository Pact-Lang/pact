// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-15

//! Figma connector — read designs, export assets, and inspect components.
//!
//! ## Operations
//!
//! | Operation        | Description                              | Required Params      |
//! |-----------------|------------------------------------------|----------------------|
//! | `get_file`      | Get a Figma file's structure and metadata | `file_key`           |
//! | `export_node`   | Export a node as PNG, SVG, or PDF         | `file_key`, `node_id`, `format` |
//! | `get_components`| List all components in a file             | `file_key`           |
//! | `get_styles`    | List all styles in a file                 | `file_key`           |
//!
//! ## Authentication
//!
//! Requires a **Personal Access Token**:
//!
//! 1. Go to Figma → Settings → Account → Personal Access Tokens
//! 2. Click "Create a new personal access token"
//! 3. Give it a name (e.g. "PACT Integration")
//! 4. Copy the token — it starts with `figd_`

use std::collections::HashMap;

use reqwest::header::AUTHORIZATION;
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, FigmaConfig};
use async_trait::async_trait;

const API_BASE: &str = "https://api.figma.com/v1";

pub struct FigmaConnector;

#[async_trait]
impl Connector for FigmaConnector {
    fn name(&self) -> &'static str { "figma" }
    fn description(&self) -> &'static str { "Figma — read designs, export assets, get components and styles" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "figma",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Personal Access Token (figd_...)", "ui": "password" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "figma",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "Personal Access Token", "help": "Figma → Settings → Personal Access Tokens" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "get_file": { "description": "Get a Figma file", "params": { "file_key": { "type": "string", "required": true } } },
                "export_node": { "description": "Export a node as an image", "params": {
                    "file_key": { "type": "string", "required": true },
                    "node_id": { "type": "string", "required": true },
                    "format": { "type": "string", "required": false, "default": "png", "enum": ["png", "svg", "jpg", "pdf"] },
                    "scale": { "type": "string", "required": false, "default": "1" }
                }},
                "get_components": { "description": "Get components in a file", "params": { "file_key": { "type": "string", "required": true } } },
                "get_styles": { "description": "Get styles in a file", "params": { "file_key": { "type": "string", "required": true } } }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["get_file", "export_node", "get_components", "get_styles"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.figma.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.figma.as_ref().ok_or(ConnectorError::NotConfigured("figma".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a Figma operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &FigmaConfig,
) -> Result<String, ConnectorError> {
    match action {
        "get_file" => get_file(params, config).await,
        "export_node" => export_node(params, config).await,
        "get_components" => get_components(params, config).await,
        "get_styles" => get_styles(params, config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "figma.{action}"
        ))),
    }
}

fn require_param<'a>(
    params: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str, ConnectorError> {
    params
        .get(key)
        .map(|s| s.as_str())
        .ok_or_else(|| ConnectorError::MissingParam(key.into()))
}

/// Get a Figma file's structure (pages, frames, layers).
async fn get_file(
    params: &HashMap<String, String>,
    config: &FigmaConfig,
) -> Result<String, ConnectorError> {
    let file_key = require_param(params, "file_key")?;

    let url = format!("{API_BASE}/files/{file_key}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .query(&[("depth", "2")]) // Don't fetch the entire deep tree by default
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        // Return a summary: file name, pages, and top-level frames.
        let name = body["name"].as_str().unwrap_or("Untitled");
        let pages: Vec<serde_json::Value> = body["document"]["children"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|page| {
                let frames: Vec<serde_json::Value> = page["children"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|f| {
                        serde_json::json!({
                            "id": f["id"],
                            "name": f["name"],
                            "type": f["type"],
                        })
                    })
                    .collect();
                serde_json::json!({
                    "id": page["id"],
                    "name": page["name"],
                    "frames": frames,
                })
            })
            .collect();

        info!(file_key, name, "Figma file retrieved");
        Ok(serde_json::json!({
            "name": name,
            "last_modified": body["lastModified"],
            "version": body["version"],
            "pages": pages,
        })
        .to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: body["err"].as_str().unwrap_or("unknown error").to_string(),
        })
    }
}

/// Export a specific node as an image (PNG, SVG, or PDF).
async fn export_node(
    params: &HashMap<String, String>,
    config: &FigmaConfig,
) -> Result<String, ConnectorError> {
    let file_key = require_param(params, "file_key")?;
    let node_id = require_param(params, "node_id")?;
    let format = params
        .get("format")
        .map(|s| s.as_str())
        .unwrap_or("png");
    let scale = params.get("scale").map(|s| s.as_str()).unwrap_or("2");

    let url = format!("{API_BASE}/images/{file_key}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .query(&[("ids", node_id), ("format", format), ("scale", scale)])
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let images = &body["images"];
        let image_url = images
            .as_object()
            .and_then(|m| m.values().next())
            .and_then(|v| v.as_str())
            .unwrap_or("");

        info!(file_key, node_id, format, "Figma node exported");
        Ok(serde_json::json!({
            "status": "success",
            "format": format,
            "url": image_url,
            "node_id": node_id,
        })
        .to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: body["err"].as_str().unwrap_or("unknown error").to_string(),
        })
    }
}

/// List all published components in a file.
async fn get_components(
    params: &HashMap<String, String>,
    config: &FigmaConfig,
) -> Result<String, ConnectorError> {
    let file_key = require_param(params, "file_key")?;

    let url = format!("{API_BASE}/files/{file_key}/components");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let components: Vec<serde_json::Value> = body["meta"]["components"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|c| {
                serde_json::json!({
                    "key": c["key"],
                    "name": c["name"],
                    "description": c["description"],
                    "node_id": c["node_id"],
                    "containing_frame": c["containing_frame"]["name"],
                })
            })
            .collect();

        info!(file_key, count = components.len(), "Figma components listed");
        Ok(serde_json::json!(components).to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: body["err"].as_str().unwrap_or("unknown error").to_string(),
        })
    }
}

/// List all published styles in a file.
async fn get_styles(
    params: &HashMap<String, String>,
    config: &FigmaConfig,
) -> Result<String, ConnectorError> {
    let file_key = require_param(params, "file_key")?;

    let url = format!("{API_BASE}/files/{file_key}/styles");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let styles: Vec<serde_json::Value> = body["meta"]["styles"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|s| {
                serde_json::json!({
                    "key": s["key"],
                    "name": s["name"],
                    "style_type": s["style_type"],
                    "description": s["description"],
                    "node_id": s["node_id"],
                })
            })
            .collect();

        info!(file_key, count = styles.len(), "Figma styles listed");
        Ok(serde_json::json!(styles).to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: body["err"].as_str().unwrap_or("unknown error").to_string(),
        })
    }
}
