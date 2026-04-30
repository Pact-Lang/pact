// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Linear connector — create, search, and manage issues via GraphQL.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, LinearConfig};

const API_URL: &str = "https://api.linear.app/graphql";

pub struct LinearConnector;

#[async_trait]
impl Connector for LinearConnector {
    fn name(&self) -> &'static str { "linear" }
    fn description(&self) -> &'static str { "Linear — modern issue tracking and project management" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "linear",
            "description": self.description(),
            "credentials": {
                "api_key": { "type": "string", "required": true, "description": "Linear API key", "ui": "password" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "linear",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "API key", "help": "Linear → Settings → API → Personal API keys" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "create_issue": { "description": "Create a new issue", "params": {
                    "title": { "type": "string", "required": true },
                    "description": { "type": "string", "required": false },
                    "team_id": { "type": "string", "required": true, "description": "Team ID (use list_teams to find)" },
                    "priority": { "type": "string", "required": false, "description": "0=none, 1=urgent, 2=high, 3=medium, 4=low", "enum": ["0", "1", "2", "3", "4"] },
                    "assignee_id": { "type": "string", "required": false },
                    "labels": { "type": "string", "required": false, "description": "Comma-separated label IDs" }
                }},
                "search_issues": { "description": "Search issues by text", "params": {
                    "query": { "type": "string", "required": true },
                    "limit": { "type": "string", "required": false, "default": "20" }
                }},
                "get_issue": { "description": "Get issue by identifier (e.g. 'ENG-123')", "params": {
                    "identifier": { "type": "string", "required": true }
                }},
                "update_issue": { "description": "Update an issue", "params": {
                    "issue_id": { "type": "string", "required": true },
                    "title": { "type": "string", "required": false },
                    "description": { "type": "string", "required": false },
                    "state_id": { "type": "string", "required": false },
                    "priority": { "type": "string", "required": false }
                }},
                "list_projects": { "description": "List all projects", "params": {} },
                "list_teams": { "description": "List all teams", "params": {} }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["create_issue", "search_issues", "get_issue", "update_issue", "list_projects", "list_teams"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.linear.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.linear.as_ref().ok_or(ConnectorError::NotConfigured("linear".into()))?;
        match action {
            "create_issue" => create_issue(params, cfg).await,
            "search_issues" => search_issues(params, cfg).await,
            "get_issue" => get_issue(params, cfg).await,
            "update_issue" => update_issue(params, cfg).await,
            "list_projects" => list_projects(cfg).await,
            "list_teams" => list_teams(cfg).await,
            _ => Err(ConnectorError::InvalidOperation(format!("linear.{action}"))),
        }
    }
}

fn require_param<'a>(params: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ConnectorError> {
    params.get(key).map(|s| s.as_str()).ok_or_else(|| ConnectorError::MissingParam(key.into()))
}

async fn graphql(config: &LinearConfig, query: &str, variables: serde_json::Value) -> Result<serde_json::Value, ConnectorError> {
    let body = serde_json::json!({ "query": query, "variables": variables });
    let resp = reqwest::Client::new()
        .post(API_URL)
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if !status.is_success() {
        return Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body.to_string() });
    }
    if let Some(errors) = resp_body.get("errors") {
        let msg = errors[0]["message"].as_str().unwrap_or("GraphQL error");
        return Err(ConnectorError::ApiError { status: 400, message: msg.to_string() });
    }
    Ok(resp_body["data"].clone())
}

async fn create_issue(params: &HashMap<String, String>, config: &LinearConfig) -> Result<String, ConnectorError> {
    let title = require_param(params, "title")?;
    let team_id = require_param(params, "team_id")?;

    let mut input = serde_json::json!({ "title": title, "teamId": team_id });
    if let Some(desc) = params.get("description") { input["description"] = serde_json::json!(desc); }
    if let Some(p) = params.get("priority") { if let Ok(n) = p.parse::<i32>() { input["priority"] = serde_json::json!(n); } }
    if let Some(a) = params.get("assignee_id") { input["assigneeId"] = serde_json::json!(a); }

    let query = r#"mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id identifier title url priority state { name } } } }"#;
    let data = graphql(config, query, serde_json::json!({ "input": input })).await?;

    let issue = &data["issueCreate"]["issue"];
    info!(identifier = issue["identifier"].as_str().unwrap_or(""), "Linear issue created");
    Ok(serde_json::json!({
        "status": "success",
        "id": issue["id"],
        "identifier": issue["identifier"],
        "title": issue["title"],
        "url": issue["url"],
    }).to_string())
}

async fn search_issues(params: &HashMap<String, String>, config: &LinearConfig) -> Result<String, ConnectorError> {
    let query_text = require_param(params, "query")?;
    let limit: i32 = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(20);

    let query = r#"query($query: String!, $first: Int) { issueSearch(query: $query, first: $first) { nodes { id identifier title url state { name } priority assignee { name } createdAt updatedAt } } }"#;
    let data = graphql(config, query, serde_json::json!({ "query": query_text, "first": limit })).await?;

    info!(query = query_text, "Linear issue search completed");
    Ok(data["issueSearch"]["nodes"].to_string())
}

async fn get_issue(params: &HashMap<String, String>, config: &LinearConfig) -> Result<String, ConnectorError> {
    let identifier = require_param(params, "identifier")?;

    let query = r#"query($id: String!) { issue(id: $id) { id identifier title description url state { name } priority assignee { name } labels { nodes { name } } createdAt updatedAt } }"#;
    let data = graphql(config, query, serde_json::json!({ "id": identifier })).await?;

    info!(identifier, "Linear issue retrieved");
    Ok(data["issue"].to_string())
}

async fn update_issue(params: &HashMap<String, String>, config: &LinearConfig) -> Result<String, ConnectorError> {
    let issue_id = require_param(params, "issue_id")?;

    let mut input = serde_json::json!({});
    if let Some(t) = params.get("title") { input["title"] = serde_json::json!(t); }
    if let Some(d) = params.get("description") { input["description"] = serde_json::json!(d); }
    if let Some(s) = params.get("state_id") { input["stateId"] = serde_json::json!(s); }
    if let Some(p) = params.get("priority") { if let Ok(n) = p.parse::<i32>() { input["priority"] = serde_json::json!(n); } }

    let query = r#"mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success issue { id identifier title url } } }"#;
    let data = graphql(config, query, serde_json::json!({ "id": issue_id, "input": input })).await?;

    info!(issue_id, "Linear issue updated");
    Ok(data["issueUpdate"]["issue"].to_string())
}

async fn list_projects(config: &LinearConfig) -> Result<String, ConnectorError> {
    let query = r#"query { projects(first: 50) { nodes { id name description state url } } }"#;
    let data = graphql(config, query, serde_json::json!({})).await?;
    info!("Listed Linear projects");
    Ok(data["projects"]["nodes"].to_string())
}

async fn list_teams(config: &LinearConfig) -> Result<String, ConnectorError> {
    let query = r#"query { teams { nodes { id name key description } } }"#;
    let data = graphql(config, query, serde_json::json!({})).await?;
    info!("Listed Linear teams");
    Ok(data["teams"]["nodes"].to_string())
}
