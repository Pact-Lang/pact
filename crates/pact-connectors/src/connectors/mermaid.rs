// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-17

//! Mermaid Chart connector — create, update, and list diagrams.
//!
//! ## Operations
//!
//! | Operation          | Description                          | Required Params              |
//! |-------------------|--------------------------------------|------------------------------|
//! | `list_projects`   | List all projects                    | (none)                       |
//! | `list_documents`  | List documents in a project          | `project_id`                 |
//! | `get_document`    | Get a specific document/diagram      | `document_id`                |
//! | `create_document` | Create a new diagram in a project    | `project_id`, `code`         |
//! | `update_document` | Update an existing diagram           | `document_id`, `project_id`, `code` |
//!
//! ## Authentication
//!
//! Requires an **API Token** from Mermaid Chart account settings:
//!
//! 1. Go to [mermaidchart.com](https://www.mermaidchart.com) and sign in
//! 2. Navigate to Settings → API Tokens
//! 3. Generate a new token
//! 4. Pass it as the `token` field in the mermaid connector config

use std::collections::HashMap;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, MermaidConfig};
use async_trait::async_trait;

/// Mermaid syntax guidelines injected into the LLM prompt when the Mermaid
/// connector is active.  Keeps the LLM from producing common syntax errors.
pub const MERMAID_SYNTAX_GUIDE: &str = r#"## Mermaid Diagram Syntax Rules

When generating Mermaid diagram code, you MUST follow these rules strictly:

### Output format
- Return ONLY the raw Mermaid diagram code.
- Do NOT wrap the output in markdown code fences (```mermaid ... ```).
- Do NOT add any commentary, explanation, or narrative before or after the diagram.

### General rules
- Every diagram starts with a type declaration (e.g., `classDiagram`, `flowchart TD`, `erDiagram`, `sequenceDiagram`).
- The word `end` is reserved — if you need it as a label, wrap it in quotes.
- Line comments use `%%`.
- Keep diagrams simple and readable. Prefer clarity over exhaustiveness.

### classDiagram
- Class members use visibility prefixes: `+` public, `-` private, `#` protected, `~` package.
- Method return types go after a space: `+someMethod() ReturnType`.
- Static members are marked with `$`, abstract with `*`.
- Notes: `note for ClassName "text"` — do NOT use a colon after the class name.
- Relationships: `<|--` inheritance, `*--` composition, `o--` aggregation, `-->` association, `..>` dependency.
- Labels on relationships: `ClassA --> ClassB : label text`.
- Generics: use `~` instead of `<>`, e.g., `List~String~` not `List<String>`.

### flowchart
- Direction: `flowchart TD` (top-down), `LR` (left-right), etc.
- Node shapes: `id["label"]` rectangle, `id("label")` rounded, `id{"label"}` diamond, `id(["label"])` stadium.
- Arrows: `-->` solid, `-.->` dotted, `==>` thick.

### sequenceDiagram
- `participant A` or `actor A` to declare participants.
- Messages: `A->>B: message` (solid), `A-->>B: message` (dotted).
- `activate`/`deactivate` or `+`/`-` suffixes for activation boxes.
- Notes: `note over A,B: text` or `note right of A: text`.

### erDiagram
- Relationships: `CUSTOMER ||--o{ ORDER : places`.
- Attributes inside entity blocks: `string name`.
"#;

const API_BASE: &str = "https://www.mermaidchart.com/rest-api";

pub struct MermaidConnector;

#[async_trait]
impl Connector for MermaidConnector {
    fn name(&self) -> &'static str { "mermaid" }
    fn description(&self) -> &'static str { "Mermaid Chart — create, update, and list diagrams" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "mermaid",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Secure Token for Plugins", "ui": "password" },
                "default_project_id": { "type": "string", "required": false, "description": "Default project ID", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "mermaid",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "Secure Token for Plugins", "help": "Mermaid Chart → Settings → Secure Tokens for Plugins" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "list_projects": { "description": "List all projects", "params": {} },
                "list_documents": { "description": "List documents in a project", "params": {
                    "project_id": { "type": "string", "required": false, "default_from": "credentials.default_project_id", "description": "Auto-detected from first project if omitted" }
                }},
                "get_document": { "description": "Get a specific document/diagram", "params": {
                    "document_id": { "type": "string", "required": true }
                }},
                "create_document": { "description": "Create a new diagram in a project. Only 'code' is required — project_id is resolved from credentials or auto-detected from the first available project.", "params": {
                    "project_id": { "type": "string", "required": false, "default_from": "credentials.default_project_id", "description": "Auto-detected from first project if omitted" },
                    "code": { "type": "string", "required": true, "description": "Mermaid diagram markup" },
                    "title": { "type": "string", "required": false, "default": "Untitled" }
                }},
                "update_document": { "description": "Update an existing diagram", "params": {
                    "document_id": { "type": "string", "required": true },
                    "project_id": { "type": "string", "required": true, "default_from": "credentials.default_project_id" },
                    "code": { "type": "string", "required": true, "description": "Mermaid diagram markup" },
                    "title": { "type": "string", "required": false }
                }}
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["list_projects", "list_documents", "get_document", "create_document", "update_document"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.mermaid.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.mermaid.as_ref().ok_or(ConnectorError::NotConfigured("mermaid".into()))?;
        execute_action(action, params, cfg).await
    }

    fn prompt_additions(&self) -> Option<&'static str> {
        Some(MERMAID_SYNTAX_GUIDE)
    }
}

/// Execute a Mermaid Chart operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    match action {
        "list_projects" => list_projects(config).await,
        "list_documents" => list_documents(params, config).await,
        "get_document" => get_document(params, config).await,
        "create_document" => create_document(params, config).await,
        "update_document" => update_document(params, config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "mermaid.{action}"
        ))),
    }
}

fn auth_header(config: &MermaidConfig) -> String {
    format!("Bearer {}", config.token)
}

/// Produce a user-friendly error message from a Mermaid Chart API failure.
fn friendly_error(
    status: reqwest::StatusCode,
    api_message: &str,
    operation: &str,
    resource_id: &str,
) -> ConnectorError {
    let code = status.as_u16();
    let detail = match code {
        401 => format!(
            "Mermaid Chart authentication failed ({operation}). \
             Your API token may be invalid, expired, or revoked. \
             Generate a new token at mermaidchart.com → Settings → API Tokens."
        ),
        403 => format!(
            "Mermaid Chart denied access ({operation}). \
             Your token does not have permission to access this resource{id_hint}.",
            id_hint = if resource_id.is_empty() { String::new() } else { format!(" (ID: {resource_id})") }
        ),
        404 => format!(
            "Mermaid Chart resource not found ({operation}){id_hint}. \
             Verify the ID is correct and that your token has access to this project/document.",
            id_hint = if resource_id.is_empty() { String::new() } else { format!(" — ID '{resource_id}' does not exist or is not accessible") }
        ),
        _ => format!(
            "Mermaid Chart API error ({code}) during {operation}: {api_message}"
        ),
    };
    ConnectorError::ApiError {
        status: code,
        message: detail,
    }
}

/// List all projects.
async fn list_projects(
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    let url = format!("{API_BASE}/projects");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, auth_header(config))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!("listed Mermaid Chart projects");
        Ok(body.to_string())
    } else {
        Err(friendly_error(
            status,
            body["message"].as_str().unwrap_or("unknown error"),
            "list_projects",
            "",
        ))
    }
}

/// Resolve project_id from params, config default, or by auto-detecting
/// the first available project.
async fn resolve_project_id(
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    if let Some(pid) = params.get("project_id") {
        return Ok(pid.clone());
    }
    if let Some(pid) = &config.default_project_id {
        return Ok(pid.clone());
    }
    // Auto-detect: list projects and use the first one.
    let body = list_projects(config).await?;
    let projects: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| ConnectorError::HttpError(format!("failed to parse projects: {e}")))?;
    let pid = projects
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|p| p["id"].as_str().or(p["projectID"].as_str()))
        .ok_or_else(|| ConnectorError::HttpError("no projects found — create one in Mermaid Chart first".into()))?;
    info!(project_id = pid, "auto-detected Mermaid Chart project");
    Ok(pid.to_string())
}

/// List documents in a project.
async fn list_documents(
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    let project_id = resolve_project_id(params, config).await?;

    let url = format!("{API_BASE}/projects/{project_id}/documents");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, auth_header(config))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(project_id, "listed Mermaid Chart documents");
        Ok(body.to_string())
    } else {
        Err(friendly_error(
            status,
            body["message"].as_str().unwrap_or("unknown error"),
            "list_documents",
            &project_id,
        ))
    }
}

/// Get a specific document/diagram.
async fn get_document(
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    let document_id = params
        .get("document_id")
        .ok_or_else(|| ConnectorError::MissingParam("document_id".into()))?;

    let url = format!("{API_BASE}/documents/{document_id}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, auth_header(config))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(document_id, "fetched Mermaid Chart document");
        Ok(body.to_string())
    } else {
        Err(friendly_error(
            status,
            body["message"].as_str().unwrap_or("unknown error"),
            "get_document",
            document_id,
        ))
    }
}

/// Create a new document in a project.
///
/// Two-step process: (1) POST to create the document shell, (2) PUT to
/// set the diagram code. The Mermaid API doesn't accept `code` on create.
async fn create_document(
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    let project_id = resolve_project_id(params, config).await?;
    let raw_code = params
        .get("code")
        .ok_or_else(|| ConnectorError::MissingParam("code".into()))?;
    // Strip markdown code fences (```mermaid ... ```) that LLMs often add.
    let code = strip_code_fences(raw_code);
    let title = params.get("title").map(|s| s.as_str()).unwrap_or("Untitled");

    // Step 1: Create the document shell.
    let create_url = format!("{API_BASE}/projects/{project_id}/documents");
    let create_body = serde_json::json!({ "title": title });

    let resp = reqwest::Client::new()
        .post(&create_url)
        .header(AUTHORIZATION, auth_header(config))
        .header(CONTENT_TYPE, "application/json")
        .json(&create_body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if !status.is_success() {
        return Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("failed to create document"),
            "create_document",
            &project_id,
        ));
    }

    let document_id = resp_body["documentID"]
        .as_str()
        .ok_or_else(|| ConnectorError::HttpError("no documentID in create response".into()))?;

    // Step 2: Update with the diagram code.
    let update_url = format!("{API_BASE}/documents/{document_id}");
    let update_body = serde_json::json!({
        "documentID": document_id,
        "projectID": project_id,
        "code": code,
    });

    let resp = reqwest::Client::new()
        .put(&update_url)
        .header(AUTHORIZATION, auth_header(config))
        .header(CONTENT_TYPE, "application/json")
        .json(&update_body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let err_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        return Err(friendly_error(
            status,
            err_body["message"].as_str().unwrap_or("failed to set diagram code"),
            "create_document (set code)",
            document_id,
        ));
    }

    info!(project_id, document_id, title, "created Mermaid Chart document");
    Ok(serde_json::json!({
        "status": "success",
        "documentID": document_id,
        "projectID": project_id,
        "title": title,
    })
    .to_string())
}

/// Update an existing document/diagram.
///
/// Requires `document_id`, `project_id`, and `code`. The Mermaid API
/// requires both `documentID` and `projectID` in the PUT body.
async fn update_document(
    params: &HashMap<String, String>,
    config: &MermaidConfig,
) -> Result<String, ConnectorError> {
    let document_id = params
        .get("document_id")
        .ok_or_else(|| ConnectorError::MissingParam("document_id".into()))?;
    let project_id = resolve_project_id(params, config).await?;
    let raw_code = params
        .get("code")
        .ok_or_else(|| ConnectorError::MissingParam("code".into()))?;
    let code = strip_code_fences(raw_code);

    let url = format!("{API_BASE}/documents/{document_id}");
    let mut body = serde_json::json!({
        "documentID": document_id,
        "projectID": project_id,
        "code": code,
    });
    if let Some(title) = params.get("title") {
        body["title"] = serde_json::json!(title);
    }

    let resp = reqwest::Client::new()
        .put(&url)
        .header(AUTHORIZATION, auth_header(config))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(document_id, "updated Mermaid Chart document");
        Ok(resp_body.to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "update_document",
            document_id,
        ))
    }
}

/// Strip markdown code fences (` ```lang ... ``` `) that LLMs often wrap around output.
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    let without_fences = if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let after_open = if let Some(nl) = trimmed.find('\n') {
            &trimmed[nl + 1..]
        } else {
            return sanitize_mermaid(trimmed);
        };
        let content = after_open.strip_suffix("```").unwrap_or(after_open);
        content.trim().to_string()
    } else {
        s.to_string()
    };
    sanitize_mermaid(&without_fences)
}

/// Fix common Mermaid syntax errors that LLMs produce.
fn sanitize_mermaid(code: &str) -> String {
    code.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            // Fix `note for ClassName : "text"` → `note for ClassName "text"`
            // In classDiagram, the note syntax does not use a colon.
            if trimmed.starts_with("note for ") || trimmed.starts_with("note \"") {
                let indent = &line[..line.len() - trimmed.len()];
                let fixed = trimmed.replacen(" : ", " ", 1);
                format!("{}{}", indent, fixed)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
