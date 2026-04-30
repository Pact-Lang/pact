// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-28

//! Jira connector — create issues, search, and manage projects.
//!
//! ## Operations
//!
//! | Operation        | Description                              | Required Params              |
//! |-----------------|------------------------------------------|------------------------------|
//! | `create_issue`  | Create a new issue (story, bug, task)     | `project`, `summary`, `issue_type` |
//! | `search`        | Search issues using JQL                   | `jql`                        |
//! | `get_issue`     | Get issue details                         | `issue_key`                  |
//! | `add_comment`   | Add a comment to an issue                 | `issue_key`, `body`          |
//! | `list_projects` | List all accessible projects              | (none)                       |
//!
//! ## Authentication
//!
//! Requires an **API Token** and Atlassian account email:
//!
//! 1. Go to [id.atlassian.com/manage-profile/security/api-tokens](https://id.atlassian.com/manage-profile/security/api-tokens)
//! 2. Click **Create API token**, give it a label
//! 3. Copy the token
//! 4. Your `domain` is the subdomain of your Jira instance (e.g. `mycompany` for `mycompany.atlassian.net`)

use std::collections::HashMap;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, JiraConfig};
use async_trait::async_trait;

pub struct JiraConnector;

#[async_trait]
impl Connector for JiraConnector {
    fn name(&self) -> &'static str { "jira" }
    fn description(&self) -> &'static str { "Jira — create issues, search, and manage projects" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "jira",
            "description": self.description(),
            "credentials": {
                "email": { "type": "string", "required": true, "description": "Atlassian account email", "ui": "text" },
                "api_token": { "type": "string", "required": true, "description": "API token from Atlassian account", "ui": "password" },
                "domain": { "type": "string", "required": true, "description": "Jira domain (e.g. 'mycompany' for mycompany.atlassian.net)", "ui": "text" },
                "default_project": { "type": "string", "required": false, "description": "Default project key (e.g. 'PROJ')", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "jira",
            "description": self.description(),
            "auth": { "type": "basic", "description": "Email + API Token (Basic Auth)", "help": "Atlassian → Account Settings → Security → API Tokens" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "create_issue": { "description": "Create a new issue (story, bug, task)", "params": {
                    "project": { "type": "string", "required": true, "default_from": "credentials.default_project", "description": "Project key (e.g. 'PROJ')" },
                    "summary": { "type": "string", "required": true, "description": "Issue title" },
                    "issue_type": { "type": "string", "required": false, "default": "Task", "enum": ["Task", "Bug", "Story", "Epic"] },
                    "description": { "type": "string", "required": false, "description": "Issue description" },
                    "priority": { "type": "string", "required": false, "enum": ["Highest", "High", "Medium", "Low", "Lowest"] },
                    "labels": { "type": "string", "required": false, "description": "Comma-separated labels" },
                    "assignee": { "type": "string", "required": false, "description": "Assignee account ID" }
                }},
                "search": { "description": "Search issues using JQL", "params": {
                    "jql": { "type": "string", "required": true, "description": "JQL query (e.g. 'project = PROJ AND created >= -1d')" },
                    "max_results": { "type": "string", "required": false, "default": "20" }
                }},
                "search_issues": { "description": "Search issues using JQL (alias for search)", "params": {
                    "jql": { "type": "string", "required": true, "description": "JQL query (e.g. 'project = PROJ AND created >= -1d')" },
                    "max_results": { "type": "string", "required": false, "default": "20" }
                }},
                "get_issue": { "description": "Get issue details", "params": {
                    "issue_key": { "type": "string", "required": true, "description": "Issue key (e.g. 'PROJ-123')" }
                }},
                "add_comment": { "description": "Add a comment to an issue", "params": {
                    "issue_key": { "type": "string", "required": true },
                    "body": { "type": "string", "required": true, "description": "Comment text" }
                }},
                "list_projects": { "description": "List all accessible projects", "params": {} }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["create_issue", "search", "search_issues", "get_issue", "add_comment", "list_projects"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.jira.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.jira.as_ref().ok_or(ConnectorError::NotConfigured("jira".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a Jira operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &JiraConfig,
) -> Result<String, ConnectorError> {
    match action {
        "create_issue" => create_issue(params, config).await,
        "search" | "search_issues" => search(params, config).await,
        "get_issue" => get_issue(params, config).await,
        "add_comment" => add_comment(params, config).await,
        "list_projects" => list_projects(config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "jira.{action}"
        ))),
    }
}

fn base_url(config: &JiraConfig) -> String {
    format!("https://{}.atlassian.net/rest/api/3", config.domain)
}

fn auth_header(config: &JiraConfig) -> String {
    use base64::Engine;
    let credentials = format!("{}:{}", config.email, config.api_token);
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    format!("Basic {}", encoded)
}

fn friendly_error(status: reqwest::StatusCode, message: &str, operation: &str) -> ConnectorError {
    let msg = match status.as_u16() {
        401 => "Authentication failed. Check your Jira email and API token.".to_string(),
        403 => format!("Permission denied for {operation}. Check your Jira project permissions."),
        404 => format!("Not found: {message}"),
        _ => format!("Jira API error ({status}): {message}"),
    };
    ConnectorError::ApiError {
        status: status.as_u16(),
        message: msg,
    }
}

/// Create a new Jira issue.
async fn create_issue(
    params: &HashMap<String, String>,
    config: &JiraConfig,
) -> Result<String, ConnectorError> {
    let project = params
        .get("project")
        .or(config.default_project.as_ref())
        .ok_or_else(|| ConnectorError::MissingParam("project".into()))?;
    let summary = params
        .get("summary")
        .ok_or_else(|| ConnectorError::MissingParam("summary".into()))?;
    let issue_type = params
        .get("issue_type")
        .or(params.get("issuetype"))
        .map(|s| s.as_str())
        .unwrap_or("Task");
    let description = params.get("description");
    let priority = params.get("priority");
    let labels = params.get("labels");
    let assignee = params.get("assignee");

    let mut fields = serde_json::json!({
        "project": { "key": project },
        "summary": summary,
        "issuetype": { "name": issue_type },
    });

    if let Some(desc) = description {
        // Jira API v3 uses Atlassian Document Format (ADF) for description.
        fields["description"] = serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": desc
                }]
            }]
        });
    }
    if let Some(p) = priority {
        fields["priority"] = serde_json::json!({ "name": p });
    }
    if let Some(l) = labels {
        let label_list: Vec<&str> = l.split(',').map(|s| s.trim()).collect();
        fields["labels"] = serde_json::json!(label_list);
    }
    if let Some(a) = assignee {
        fields["assignee"] = serde_json::json!({ "accountId": a });
    }

    let body = serde_json::json!({ "fields": fields });

    let resp = reqwest::Client::new()
        .post(&format!("{}/issue", base_url(config)))
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
        let key = resp_body["key"].as_str().unwrap_or("");
        let id = resp_body["id"].as_str().unwrap_or("");
        info!(project, key, issue_type, "created Jira issue");
        Ok(serde_json::json!({
            "status": "success",
            "key": key,
            "id": id,
            "self": resp_body["self"],
            "url": format!("https://{}.atlassian.net/browse/{}", config.domain, key),
        })
        .to_string())
    } else {
        let msg = resp_body["errorMessages"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .or_else(|| resp_body["message"].as_str())
            .unwrap_or("failed to create issue");
        Err(friendly_error(status, msg, "create_issue"))
    }
}

/// Search Jira issues using JQL.
async fn search(
    params: &HashMap<String, String>,
    config: &JiraConfig,
) -> Result<String, ConnectorError> {
    let jql = params
        .get("jql")
        .ok_or_else(|| ConnectorError::MissingParam("jql".into()))?;
    let max_results = params
        .get("max_results")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20);

    let body = serde_json::json!({
        "jql": jql,
        "maxResults": max_results,
        "fields": ["summary", "status", "issuetype", "priority", "assignee", "created", "updated", "labels"],
    });

    let resp = reqwest::Client::new()
        .post(&format!("{}/search", base_url(config)))
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
        let total = resp_body["total"].as_u64().unwrap_or(0);
        info!(jql, total, "Jira search completed");
        Ok(resp_body.to_string())
    } else {
        let msg = resp_body["errorMessages"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("search failed");
        Err(friendly_error(status, msg, "search"))
    }
}

/// Get details for a specific issue.
async fn get_issue(
    params: &HashMap<String, String>,
    config: &JiraConfig,
) -> Result<String, ConnectorError> {
    let issue_key = params
        .get("issue_key")
        .or(params.get("key"))
        .ok_or_else(|| ConnectorError::MissingParam("issue_key".into()))?;

    let resp = reqwest::Client::new()
        .get(&format!("{}/issue/{}", base_url(config), issue_key))
        .header(AUTHORIZATION, auth_header(config))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(issue_key, "fetched Jira issue");
        Ok(resp_body.to_string())
    } else {
        Err(friendly_error(
            status,
            &format!("Issue '{}' not found", issue_key),
            "get_issue",
        ))
    }
}

/// Add a comment to an issue.
async fn add_comment(
    params: &HashMap<String, String>,
    config: &JiraConfig,
) -> Result<String, ConnectorError> {
    let issue_key = params
        .get("issue_key")
        .or(params.get("key"))
        .ok_or_else(|| ConnectorError::MissingParam("issue_key".into()))?;
    let body_text = params
        .get("body")
        .or(params.get("comment"))
        .ok_or_else(|| ConnectorError::MissingParam("body".into()))?;

    // ADF format for the comment body.
    let comment_body = serde_json::json!({
        "body": {
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": body_text
                }]
            }]
        }
    });

    let resp = reqwest::Client::new()
        .post(&format!(
            "{}/issue/{}/comment",
            base_url(config),
            issue_key
        ))
        .header(AUTHORIZATION, auth_header(config))
        .header(CONTENT_TYPE, "application/json")
        .json(&comment_body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let comment_id = resp_body["id"].as_str().unwrap_or("");
        info!(issue_key, comment_id, "added comment to Jira issue");
        Ok(serde_json::json!({
            "status": "success",
            "issue_key": issue_key,
            "comment_id": comment_id,
        })
        .to_string())
    } else {
        let msg = resp_body["errorMessages"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("failed to add comment");
        Err(friendly_error(status, msg, "add_comment"))
    }
}

/// List all accessible Jira projects.
async fn list_projects(config: &JiraConfig) -> Result<String, ConnectorError> {
    let resp = reqwest::Client::new()
        .get(&format!("{}/project", base_url(config)))
        .header(AUTHORIZATION, auth_header(config))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!("listed Jira projects");
        Ok(resp_body.to_string())
    } else {
        Err(friendly_error(
            status,
            "failed to list projects",
            "list_projects",
        ))
    }
}
