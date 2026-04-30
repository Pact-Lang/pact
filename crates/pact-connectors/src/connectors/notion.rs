// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Notion connector — search, read, and manage pages and databases.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, NotionConfig};

const API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

pub struct NotionConnector;

#[async_trait]
impl Connector for NotionConnector {
    fn name(&self) -> &'static str { "notion" }
    fn description(&self) -> &'static str { "Notion — search, read, and manage pages and databases" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "notion",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Integration token (secret_...)", "ui": "password" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "notion",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "Integration token", "help": "notion.so/my-integrations → Create new integration" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "search": { "description": "Search pages and databases", "params": {
                    "query": { "type": "string", "required": true, "description": "Search query" },
                    "filter": { "type": "string", "required": false, "description": "Filter by 'page' or 'database'", "enum": ["page", "database"] }
                }},
                "get_page": { "description": "Get a page by ID", "params": {
                    "page_id": { "type": "string", "required": true }
                }},
                "create_page": { "description": "Create a new page", "params": {
                    "parent_id": { "type": "string", "required": true, "description": "Parent page or database ID" },
                    "parent_type": { "type": "string", "required": false, "default": "page_id", "enum": ["page_id", "database_id"] },
                    "title": { "type": "string", "required": true },
                    "content": { "type": "string", "required": false, "description": "Page content as plain text" }
                }},
                "update_page": { "description": "Update page properties", "params": {
                    "page_id": { "type": "string", "required": true },
                    "properties": { "type": "string", "required": true, "description": "JSON object of property updates" }
                }},
                "query_database": { "description": "Query a database", "params": {
                    "database_id": { "type": "string", "required": true },
                    "filter": { "type": "string", "required": false, "description": "JSON filter object" },
                    "sorts": { "type": "string", "required": false, "description": "JSON sorts array" }
                }},
                "get_database": { "description": "Get database schema", "params": {
                    "database_id": { "type": "string", "required": true }
                }}
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["search", "get_page", "create_page", "update_page", "query_database", "get_database"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.notion.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.notion.as_ref().ok_or(ConnectorError::NotConfigured("notion".into()))?;
        match action {
            "search" => search(params, cfg).await,
            "get_page" => get_page(params, cfg).await,
            "create_page" => create_page(params, cfg).await,
            "update_page" => update_page(params, cfg).await,
            "query_database" => query_database(params, cfg).await,
            "get_database" => get_database(params, cfg).await,
            _ => Err(ConnectorError::InvalidOperation(format!("notion.{action}"))),
        }
    }
}

fn require_param<'a>(params: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ConnectorError> {
    params.get(key).map(|s| s.as_str()).ok_or_else(|| ConnectorError::MissingParam(key.into()))
}

fn http(_config: &NotionConfig) -> reqwest::Client {
    reqwest::Client::new()
}

fn auth_headers(config: &NotionConfig) -> Vec<(reqwest::header::HeaderName, String)> {
    vec![
        (AUTHORIZATION, format!("Bearer {}", config.token)),
        (reqwest::header::HeaderName::from_static("notion-version"), NOTION_VERSION.to_string()),
    ]
}

async fn search(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let query = require_param(params, "query")?;

    let mut body = serde_json::json!({ "query": query });
    if let Some(filter) = params.get("filter") {
        body["filter"] = serde_json::json!({ "value": filter, "property": "object" });
    }

    let mut req = http(config).post(&format!("{API_BASE}/search"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(query, "Notion search completed");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("unknown error").to_string() })
    }
}

async fn get_page(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let page_id = require_param(params, "page_id")?;

    let mut req = http(config).get(&format!("{API_BASE}/pages/{page_id}"));
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(page_id, "Notion page retrieved");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("not found").to_string() })
    }
}

async fn create_page(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let parent_id = require_param(params, "parent_id")?;
    let title = require_param(params, "title")?;
    let parent_type = params.get("parent_type").map(|s| s.as_str()).unwrap_or("page_id");

    let mut body = serde_json::json!({
        "parent": { parent_type: parent_id },
        "properties": {
            "title": { "title": [{ "text": { "content": title } }] }
        }
    });

    if let Some(content) = params.get("content") {
        body["children"] = serde_json::json!([{
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": [{ "type": "text", "text": { "content": content } }]
            }
        }]);
    }

    let mut req = http(config).post(&format!("{API_BASE}/pages"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let page_id = resp_body["id"].as_str().unwrap_or("");
        info!(page_id, title, "Notion page created");
        Ok(serde_json::json!({ "status": "success", "page_id": page_id, "url": resp_body["url"] }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("failed to create page").to_string() })
    }
}

async fn update_page(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let page_id = require_param(params, "page_id")?;
    let properties_str = require_param(params, "properties")?;
    let properties: serde_json::Value = serde_json::from_str(properties_str)
        .map_err(|e| ConnectorError::HttpError(format!("invalid properties JSON: {e}")))?;

    let body = serde_json::json!({ "properties": properties });
    let mut req = http(config).patch(&format!("{API_BASE}/pages/{page_id}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(page_id, "Notion page updated");
        Ok(serde_json::json!({ "status": "success", "page_id": page_id }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("failed to update").to_string() })
    }
}

async fn query_database(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let database_id = require_param(params, "database_id")?;

    let mut body = serde_json::json!({});
    if let Some(filter) = params.get("filter") {
        if let Ok(f) = serde_json::from_str::<serde_json::Value>(filter) {
            body["filter"] = f;
        }
    }
    if let Some(sorts) = params.get("sorts") {
        if let Ok(s) = serde_json::from_str::<serde_json::Value>(sorts) {
            body["sorts"] = s;
        }
    }

    let mut req = http(config).post(&format!("{API_BASE}/databases/{database_id}/query"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(database_id, "Notion database queried");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("query failed").to_string() })
    }
}

async fn get_database(params: &HashMap<String, String>, config: &NotionConfig) -> Result<String, ConnectorError> {
    let database_id = require_param(params, "database_id")?;

    let mut req = http(config).get(&format!("{API_BASE}/databases/{database_id}"));
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(database_id, "Notion database retrieved");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["message"].as_str().unwrap_or("not found").to_string() })
    }
}
