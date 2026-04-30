// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Airtable connector — list, create, update, and search records.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{AirtableConfig, Connector, ConnectorConfig, ConnectorError};

const API_BASE: &str = "https://api.airtable.com/v0";

pub struct AirtableConnector;

#[async_trait]
impl Connector for AirtableConnector {
    fn name(&self) -> &'static str { "airtable" }
    fn description(&self) -> &'static str { "Airtable — manage records, tables, and bases" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "airtable",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Personal access token", "ui": "password" },
                "base_id": { "type": "string", "required": false, "description": "Default base ID (appXXXXXXXXXXXXXX)", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "airtable",
            "description": self.description(),
            "auth": { "type": "bearer", "description": "Personal access token", "help": "airtable.com/create/tokens" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "list_records": { "description": "List records in a table", "params": {
                    "base_id": { "type": "string", "required": true, "default_from": "credentials.base_id" },
                    "table": { "type": "string", "required": true, "description": "Table name or ID" },
                    "max_records": { "type": "string", "required": false, "default": "100" },
                    "view": { "type": "string", "required": false, "description": "View name or ID" }
                }},
                "get_record": { "description": "Get a single record", "params": {
                    "base_id": { "type": "string", "required": true, "default_from": "credentials.base_id" },
                    "table": { "type": "string", "required": true },
                    "record_id": { "type": "string", "required": true }
                }},
                "create_record": { "description": "Create a new record", "params": {
                    "base_id": { "type": "string", "required": true, "default_from": "credentials.base_id" },
                    "table": { "type": "string", "required": true },
                    "fields": { "type": "string", "required": true, "description": "JSON object of field values" }
                }},
                "update_record": { "description": "Update an existing record", "params": {
                    "base_id": { "type": "string", "required": true, "default_from": "credentials.base_id" },
                    "table": { "type": "string", "required": true },
                    "record_id": { "type": "string", "required": true },
                    "fields": { "type": "string", "required": true, "description": "JSON object of field values to update" }
                }},
                "search": { "description": "Search records using a formula", "params": {
                    "base_id": { "type": "string", "required": true, "default_from": "credentials.base_id" },
                    "table": { "type": "string", "required": true },
                    "formula": { "type": "string", "required": true, "description": "Airtable formula (e.g. {Name} = 'Alice')" }
                }},
                "list_bases": { "description": "List all accessible bases", "params": {} }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["list_records", "get_record", "create_record", "update_record", "search", "list_bases"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.airtable.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.airtable.as_ref().ok_or(ConnectorError::NotConfigured("airtable".into()))?;
        match action {
            "list_records" => list_records(params, cfg).await,
            "get_record" => get_record(params, cfg).await,
            "create_record" => create_record(params, cfg).await,
            "update_record" => update_record(params, cfg).await,
            "search" => search(params, cfg).await,
            "list_bases" => list_bases(cfg).await,
            _ => Err(ConnectorError::InvalidOperation(format!("airtable.{action}"))),
        }
    }
}

fn require_param<'a>(params: &'a HashMap<String, String>, key: &str) -> Result<&'a str, ConnectorError> {
    params.get(key).map(|s| s.as_str()).ok_or_else(|| ConnectorError::MissingParam(key.into()))
}

fn resolve_base_id<'a>(params: &'a HashMap<String, String>, config: &'a AirtableConfig) -> Result<&'a str, ConnectorError> {
    params.get("base_id").map(|s| s.as_str())
        .or(config.base_id.as_deref())
        .ok_or_else(|| ConnectorError::MissingParam("base_id".into()))
}

async fn list_records(params: &HashMap<String, String>, config: &AirtableConfig) -> Result<String, ConnectorError> {
    let base_id = resolve_base_id(params, config)?;
    let table = require_param(params, "table")?;
    let max_records = params.get("max_records").map(|s| s.as_str()).unwrap_or("100");

    let url = format!("{API_BASE}/{base_id}/{}", urlencoding::encode(table));
    let mut req = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .query(&[("maxRecords", max_records)]);
    if let Some(view) = params.get("view") {
        req = req.query(&[("view", view.as_str())]);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(base_id, table, "Airtable records listed");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to list records").to_string() })
    }
}

async fn get_record(params: &HashMap<String, String>, config: &AirtableConfig) -> Result<String, ConnectorError> {
    let base_id = resolve_base_id(params, config)?;
    let table = require_param(params, "table")?;
    let record_id = require_param(params, "record_id")?;

    let url = format!("{API_BASE}/{base_id}/{}/{record_id}", urlencoding::encode(table));
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(base_id, table, record_id, "Airtable record retrieved");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("not found").to_string() })
    }
}

async fn create_record(params: &HashMap<String, String>, config: &AirtableConfig) -> Result<String, ConnectorError> {
    let base_id = resolve_base_id(params, config)?;
    let table = require_param(params, "table")?;
    let fields_str = require_param(params, "fields")?;
    let fields: serde_json::Value = serde_json::from_str(fields_str)
        .map_err(|e| ConnectorError::HttpError(format!("invalid fields JSON: {e}")))?;

    let body = serde_json::json!({ "fields": fields });
    let url = format!("{API_BASE}/{base_id}/{}", urlencoding::encode(table));
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let record_id = resp_body["id"].as_str().unwrap_or("");
        info!(base_id, table, record_id, "Airtable record created");
        Ok(serde_json::json!({ "status": "success", "id": record_id, "fields": resp_body["fields"] }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to create record").to_string() })
    }
}

async fn update_record(params: &HashMap<String, String>, config: &AirtableConfig) -> Result<String, ConnectorError> {
    let base_id = resolve_base_id(params, config)?;
    let table = require_param(params, "table")?;
    let record_id = require_param(params, "record_id")?;
    let fields_str = require_param(params, "fields")?;
    let fields: serde_json::Value = serde_json::from_str(fields_str)
        .map_err(|e| ConnectorError::HttpError(format!("invalid fields JSON: {e}")))?;

    let body = serde_json::json!({ "fields": fields });
    let url = format!("{API_BASE}/{base_id}/{}/{record_id}", urlencoding::encode(table));
    let resp = reqwest::Client::new()
        .patch(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(base_id, table, record_id, "Airtable record updated");
        Ok(serde_json::json!({ "status": "success", "id": record_id, "fields": resp_body["fields"] }).to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to update").to_string() })
    }
}

async fn search(params: &HashMap<String, String>, config: &AirtableConfig) -> Result<String, ConnectorError> {
    let base_id = resolve_base_id(params, config)?;
    let table = require_param(params, "table")?;
    let formula = require_param(params, "formula")?;

    let url = format!("{API_BASE}/{base_id}/{}", urlencoding::encode(table));
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .query(&[("filterByFormula", formula)])
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(base_id, table, "Airtable search completed");
        Ok(resp_body.to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("search failed").to_string() })
    }
}

async fn list_bases(config: &AirtableConfig) -> Result<String, ConnectorError> {
    let url = "https://api.airtable.com/v0/meta/bases";
    let resp = reqwest::Client::new()
        .get(url)
        .header(AUTHORIZATION, format!("Bearer {}", config.token))
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!("Airtable bases listed");
        Ok(resp_body["bases"].to_string())
    } else {
        Err(ConnectorError::ApiError { status: status.as_u16(), message: resp_body["error"]["message"].as_str().unwrap_or("failed to list bases").to_string() })
    }
}
