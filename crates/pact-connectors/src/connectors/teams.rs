// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Microsoft Teams connector — post messages, manage channels via Microsoft Graph.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, TeamsConfig};

const API_BASE: &str = "https://graph.microsoft.com/v1.0";

pub struct TeamsConnector;

#[async_trait]
impl Connector for TeamsConnector {
    fn name(&self) -> &'static str { "teams" }
    fn description(&self) -> &'static str { "Microsoft Teams — post messages, manage channels and teams" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "teams",
            "description": self.description(),
            "credentials": {
                "access_token": { "type": "string", "required": true, "description": "OAuth 2.0 access token (Microsoft Graph)", "ui": "password" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "teams",
            "description": self.description(),
            "auth": { "type": "oauth2", "description": "Microsoft Graph OAuth 2.0 Bearer token", "help": "Azure Portal → App registrations → API permissions → Microsoft Graph" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "post_message": { "description": "Post a message to a channel", "params": {
                    "team_id": { "type": "string", "required": true },
                    "channel_id": { "type": "string", "required": true },
                    "content": { "type": "string", "required": true, "description": "Message content (HTML supported)" },
                    "content_type": { "type": "string", "required": false, "default": "html", "enum": ["text", "html"] }
                }},
                "list_channels": { "description": "List channels in a team", "params": {
                    "team_id": { "type": "string", "required": true }
                }},
                "create_channel": { "description": "Create a channel in a team", "params": {
                    "team_id": { "type": "string", "required": true },
                    "display_name": { "type": "string", "required": true },
                    "description": { "type": "string", "required": false }
                }},
                "list_teams": { "description": "List teams the user belongs to", "params": {} }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["post_message", "list_channels", "create_channel", "list_teams"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.teams.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.teams.as_ref().ok_or(ConnectorError::NotConfigured("teams".into()))?;
        match action {
            "post_message" => post_message(params, cfg).await,
            "list_channels" => list_channels(params, cfg).await,
            "create_channel" => create_channel(params, cfg).await,
            "list_teams" => list_teams(cfg).await,
            _ => Err(ConnectorError::InvalidOperation(format!("teams.{action}"))),
        }
    }
}

fn require_param<'a>(params: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ConnectorError> {
    params.get(key).map(|s| s.as_str()).ok_or_else(|| ConnectorError::MissingParam(key.into()))
}

async fn post_message(params: &HashMap<String, String>, config: &TeamsConfig) -> Result<String, ConnectorError> {
    let team_id = require_param(params, "team_id")?;
    let channel_id = require_param(params, "channel_id")?;
    let content = require_param(params, "content")?;
    let content_type = params.get("content_type").map(|s| s.as_str()).unwrap_or("html");

    let body = serde_json::json!({
        "body": { "contentType": content_type, "content": content }
    });

    let url = format!("{API_BASE}/teams/{team_id}/channels/{channel_id}/messages");
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let msg_id = resp_body["id"].as_str().unwrap_or("");
        info!(team_id, channel_id, "Teams message posted");
        Ok(serde_json::json!({ "status": "success", "message_id": msg_id }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to post message").to_string() })
    }
}

async fn list_channels(params: &HashMap<String, String>, config: &TeamsConfig) -> Result<String, ConnectorError> {
    let team_id = require_param(params, "team_id")?;

    let url = format!("{API_BASE}/teams/{team_id}/channels");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(team_id, "Teams channels listed");
        Ok(resp_body["value"].to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to list channels").to_string() })
    }
}

async fn create_channel(params: &HashMap<String, String>, config: &TeamsConfig) -> Result<String, ConnectorError> {
    let team_id = require_param(params, "team_id")?;
    let display_name = require_param(params, "display_name")?;

    let mut body = serde_json::json!({ "displayName": display_name, "membershipType": "standard" });
    if let Some(desc) = params.get("description") {
        body["description"] = serde_json::json!(desc);
    }

    let url = format!("{API_BASE}/teams/{team_id}/channels");
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let channel_id = resp_body["id"].as_str().unwrap_or("");
        info!(team_id, channel_id, display_name, "Teams channel created");
        Ok(serde_json::json!({ "status": "success", "channel_id": channel_id, "display_name": display_name }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to create channel").to_string() })
    }
}

async fn list_teams(config: &TeamsConfig) -> Result<String, ConnectorError> {
    let url = format!("{API_BASE}/me/joinedTeams");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!("Teams listed");
        Ok(resp_body["value"].to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to list teams").to_string() })
    }
}
