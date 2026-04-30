// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-15

//! Slack connector — post messages, upload files, and react to messages.
//!
//! ## Operations
//!
//! | Operation       | Description                          | Required Params           |
//! |----------------|--------------------------------------|---------------------------|
//! | `post_message` | Post a message to a channel          | `channel`, `text`         |
//! | `upload_file`  | Upload a file to a channel           | `channel`, `content`, `filename` |
//! | `add_reaction` | Add a reaction emoji to a message    | `channel`, `timestamp`, `emoji` |
//!
//! ## Authentication
//!
//! Requires a **Bot User OAuth Token** (starts with `xoxb-`):
//!
//! 1. Go to [api.slack.com/apps](https://api.slack.com/apps) and create a new app
//! 2. Under **OAuth & Permissions**, add these Bot Token Scopes:
//!    - `chat:write` — post messages
//!    - `files:write` — upload files
//!    - `reactions:write` — add reactions
//! 3. Install the app to your workspace
//! 4. Copy the **Bot User OAuth Token** (starts with `xoxb-`)
//! 5. Invite the bot to target channels: `/invite @YourBotName`

use std::collections::HashMap;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, SlackConfig};
use async_trait::async_trait;

const API_BASE: &str = "https://slack.com/api";

pub struct SlackConnector;

#[async_trait]
impl Connector for SlackConnector {
    fn name(&self) -> &'static str { "slack" }
    fn description(&self) -> &'static str { "Slack — post messages, upload files, and react to messages" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "slack",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Bot Token (xoxb-...)", "ui": "password" },
                "default_channel": { "type": "string", "required": false, "description": "Default channel", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "slack",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "Bot User OAuth Token (starts with xoxb-)", "help": "api.slack.com/apps → OAuth & Permissions → Bot Token Scopes: chat:write, files:write, reactions:write" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "post_message": { "description": "Post a message to a channel", "params": {
                    "channel": { "type": "string", "required": true, "default_from": "credentials.default_channel" },
                    "text": { "type": "string", "required": true },
                    "thread_ts": { "type": "string", "required": false, "description": "Thread timestamp for replies" },
                    "blocks": { "type": "string", "required": false, "description": "JSON array of Block Kit blocks for rich formatting" }
                }},
                "upload_file": { "description": "Upload a file to a channel", "params": {
                    "channel": { "type": "string", "required": true, "default_from": "credentials.default_channel" },
                    "content": { "type": "string", "required": true },
                    "filename": { "type": "string", "required": false, "default": "output.txt" },
                    "title": { "type": "string", "required": false },
                    "message": { "type": "string", "required": false, "description": "Initial comment on the file" }
                }},
                "add_reaction": { "description": "Add a reaction emoji to a message", "params": {
                    "channel": { "type": "string", "required": true, "default_from": "credentials.default_channel" },
                    "timestamp": { "type": "string", "required": true, "description": "Message timestamp" },
                    "emoji": { "type": "string", "required": true, "description": "Emoji name without colons" }
                }}
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["post_message", "upload_file", "add_reaction"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.slack.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.slack.as_ref().ok_or(ConnectorError::NotConfigured("slack".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a Slack operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &SlackConfig,
) -> Result<String, ConnectorError> {
    match action {
        "post_message" => post_message(params, config).await,
        "upload_file" => upload_file(params, config).await,
        "add_reaction" => add_reaction(params, config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "slack.{action}"
        ))),
    }
}

fn get_channel<'a>(
    params: &'a HashMap<String, String>,
    config: &'a SlackConfig,
) -> Result<&'a str, ConnectorError> {
    params
        .get("channel")
        .map(|s| s.as_str())
        .or(config.default_channel.as_deref())
        .ok_or_else(|| ConnectorError::MissingParam("channel".into()))
}

/// Post a message to a Slack channel.
async fn post_message(
    params: &HashMap<String, String>,
    config: &SlackConfig,
) -> Result<String, ConnectorError> {
    let channel = get_channel(params, config)?;
    let text = params
        .get("text")
        .ok_or_else(|| ConnectorError::MissingParam("text".into()))?;
    let thread_ts = params.get("thread_ts");

    let mut body = serde_json::json!({
        "channel": channel,
        "text": text,
    });
    if let Some(ts) = thread_ts {
        body["thread_ts"] = serde_json::json!(ts);
    }
    // Support blocks for rich formatting.
    if let Some(blocks) = params.get("blocks") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(blocks) {
            body["blocks"] = parsed;
        }
    }

    let resp = reqwest::Client::new()
        .post(&format!("{API_BASE}/chat.postMessage"))
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if resp_body["ok"].as_bool() == Some(true) {
        let ts = resp_body["ts"].as_str().unwrap_or("");
        info!(channel, "message posted to Slack");
        Ok(serde_json::json!({
            "status": "success",
            "channel": channel,
            "timestamp": ts,
        })
        .to_string())
    } else {
        let error = resp_body["error"]
            .as_str()
            .unwrap_or("unknown error")
            .to_string();
        Err(ConnectorError::ApiError {
            status: 200, // Slack returns 200 with ok:false
            message: error,
        })
    }
}

/// Upload a file to a Slack channel.
async fn upload_file(
    params: &HashMap<String, String>,
    config: &SlackConfig,
) -> Result<String, ConnectorError> {
    let channel = get_channel(params, config)?;
    let content = params
        .get("content")
        .ok_or_else(|| ConnectorError::MissingParam("content".into()))?;
    let filename = params
        .get("filename")
        .map(|s| s.as_str())
        .unwrap_or("output.txt");
    let title = params.get("title").map(|s| s.as_str()).unwrap_or(filename);
    let initial_comment = params.get("message").map(|s| s.as_str()).unwrap_or("");

    // Step 1: Get upload URL.
    let resp = reqwest::Client::new()
        .get(&format!("{API_BASE}/files.getUploadURLExternal"))
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .query(&[
            ("filename", filename),
            ("length", &content.len().to_string()),
        ])
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if resp_body["ok"].as_bool() != Some(true) {
        return Err(ConnectorError::ApiError {
            status: 200,
            message: resp_body["error"]
                .as_str()
                .unwrap_or("failed to get upload URL")
                .to_string(),
        });
    }

    let upload_url = resp_body["upload_url"]
        .as_str()
        .ok_or_else(|| ConnectorError::HttpError("no upload_url in response".into()))?;
    let file_id = resp_body["file_id"]
        .as_str()
        .ok_or_else(|| ConnectorError::HttpError("no file_id in response".into()))?;

    // Step 2: Upload file content.
    reqwest::Client::new()
        .post(upload_url)
        .body(content.clone())
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    // Step 3: Complete the upload.
    let complete_body = serde_json::json!({
        "files": [{"id": file_id, "title": title}],
        "channel_id": channel,
        "initial_comment": initial_comment,
    });

    let resp = reqwest::Client::new()
        .post(&format!("{API_BASE}/files.completeUploadExternal"))
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .json(&complete_body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if resp_body["ok"].as_bool() == Some(true) {
        info!(channel, filename, "file uploaded to Slack");
        Ok(serde_json::json!({
            "status": "success",
            "file_id": file_id,
            "filename": filename,
            "channel": channel,
        })
        .to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: 200,
            message: resp_body["error"]
                .as_str()
                .unwrap_or("upload failed")
                .to_string(),
        })
    }
}

/// Add a reaction emoji to a message.
async fn add_reaction(
    params: &HashMap<String, String>,
    config: &SlackConfig,
) -> Result<String, ConnectorError> {
    let channel = get_channel(params, config)?;
    let timestamp = params
        .get("timestamp")
        .ok_or_else(|| ConnectorError::MissingParam("timestamp".into()))?;
    let emoji = params
        .get("emoji")
        .ok_or_else(|| ConnectorError::MissingParam("emoji".into()))?;

    let body = serde_json::json!({
        "channel": channel,
        "timestamp": timestamp,
        "name": emoji,
    });

    let resp = reqwest::Client::new()
        .post(&format!("{API_BASE}/reactions.add"))
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if resp_body["ok"].as_bool() == Some(true) {
        Ok(serde_json::json!({"status": "success", "emoji": emoji}).to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: 200,
            message: resp_body["error"]
                .as_str()
                .unwrap_or("unknown error")
                .to_string(),
        })
    }
}
