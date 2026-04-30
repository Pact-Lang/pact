// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-15

//! Google Drive connector — upload files, create folders, and share documents.
//!
//! ## Operations
//!
//! | Operation       | Description                              | Required Params              |
//! |----------------|------------------------------------------|------------------------------|
//! | `upload`       | Upload a file to Google Drive             | `content`, `name`            |
//! | `create_folder`| Create a folder                           | `name`                       |
//! | `list`         | List files in a folder or root            | (none)                       |
//! | `share`        | Share a file with an email address        | `file_id`, `email`           |
//! | `get`          | Get file metadata                         | `file_id`                    |
//!
//! ## Authentication
//!
//! Requires an **OAuth 2.0 Access Token** with Drive scopes.
//!
//! ### Option A: OAuth 2.0 (recommended for end users)
//!
//! 1. Create a project in [Google Cloud Console](https://console.cloud.google.com)
//! 2. Enable the **Google Drive API**
//! 3. Create **OAuth 2.0 Client ID** (Web application type)
//! 4. Use the OAuth flow to obtain an access token with scope:
//!    `https://www.googleapis.com/auth/drive.file`
//! 5. Pass the access token in the connector config
//!
//! ### Option B: Service Account (for server-to-server)
//!
//! 1. Create a **Service Account** in Google Cloud Console
//! 2. Download the JSON key file
//! 3. Share target folders with the service account email
//! 4. Use the service account to generate access tokens
//!
//! ### Scopes
//!
//! | Scope | Access |
//! |-------|--------|
//! | `drive.file` | Files created or opened by the app only (recommended) |
//! | `drive` | Full access to all Drive files (use with caution) |

use std::collections::HashMap;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, GDriveConfig};
use async_trait::async_trait;

const API_BASE: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_BASE: &str = "https://www.googleapis.com/upload/drive/v3";

pub struct GDriveConnector;

#[async_trait]
impl Connector for GDriveConnector {
    fn name(&self) -> &'static str { "gdrive" }
    fn description(&self) -> &'static str { "Google Drive — upload files, create folders, share" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "gdrive",
            "description": self.description(),
            "credentials": {
                "access_token": { "type": "string", "required": true, "description": "OAuth 2.0 access token", "ui": "password" },
                "default_folder": { "type": "string", "required": false, "description": "Default folder ID", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "gdrive",
            "description": self.description(),
            "auth": { "type": "oauth2", "description": "OAuth 2.0 access token", "help": "Google Cloud Console → OAuth 2.0 credentials" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "upload": { "description": "Upload a file", "params": {
                    "name": { "type": "string", "required": true },
                    "content": { "type": "string", "required": true },
                    "mime_type": { "type": "string", "required": false, "default": "text/plain" },
                    "folder_id": { "type": "string", "required": false, "default_from": "credentials.default_folder" }
                }},
                "create_folder": { "description": "Create a folder", "params": {
                    "name": { "type": "string", "required": true },
                    "parent_id": { "type": "string", "required": false, "default_from": "credentials.default_folder" }
                }},
                "list": { "description": "List files in a folder", "params": {
                    "folder_id": { "type": "string", "required": false, "default_from": "credentials.default_folder" },
                    "per_page": { "type": "string", "required": false, "default": "20" }
                }},
                "share": { "description": "Share a file or folder", "params": {
                    "file_id": { "type": "string", "required": true },
                    "email": { "type": "string", "required": true },
                    "role": { "type": "string", "required": false, "default": "reader", "enum": ["reader", "writer", "commenter"] }
                }},
                "get": { "description": "Get file metadata", "params": {
                    "file_id": { "type": "string", "required": true }
                }}
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["upload", "create_folder", "list", "share", "get"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.gdrive.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.gdrive.as_ref().ok_or(ConnectorError::NotConfigured("gdrive".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a Google Drive operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    match action {
        "upload" => upload_file(params, config).await,
        "create_folder" => create_folder(params, config).await,
        "list" => list_files(params, config).await,
        "share" => share_file(params, config).await,
        "get" => get_file(params, config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "gdrive.{action}"
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

/// Infer MIME type from filename extension.
fn mime_from_name(name: &str) -> &'static str {
    if let Some(ext) = name.rsplit('.').next() {
        match ext.to_lowercase().as_str() {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "md" => "text/markdown",
            "txt" => "text/plain",
            "csv" => "text/csv",
            "xml" => "application/xml",
            "svg" => "image/svg+xml",
            "pdf" => "application/pdf",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "zip" => "application/zip",
            _ => "application/octet-stream",
        }
    } else {
        "text/plain"
    }
}

/// Upload a file to Google Drive.
async fn upload_file(
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    let content = require_param(params, "content")?;
    let name = require_param(params, "name")?;
    let mime_type = params
        .get("mime_type")
        .map(|s| s.as_str())
        .unwrap_or_else(|| mime_from_name(name));
    let folder_id = params
        .get("folder_id")
        .or(config.default_folder.as_ref());

    // Build file metadata.
    let mut metadata = serde_json::json!({
        "name": name,
        "mimeType": mime_type,
    });
    if let Some(fid) = folder_id {
        metadata["parents"] = serde_json::json!([fid]);
    }

    // Use multipart upload: metadata + content in one request.
    let boundary = "pact_boundary_2026";
    let body = format!(
        "--{boundary}\r\n\
         Content-Type: application/json; charset=UTF-8\r\n\r\n\
         {}\r\n\
         --{boundary}\r\n\
         Content-Type: {mime_type}\r\n\r\n\
         {content}\r\n\
         --{boundary}--",
        metadata
    );

    let url = format!(
        "{UPLOAD_BASE}/files?uploadType=multipart&fields=id,name,webViewLink,webContentLink,mimeType,size"
    );
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .header(
            CONTENT_TYPE,
            format!("multipart/related; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let file_id = resp_body["id"].as_str().unwrap_or("");
        let web_link = resp_body["webViewLink"].as_str().unwrap_or("");
        info!(name, file_id, "file uploaded to Google Drive");
        Ok(serde_json::json!({
            "status": "success",
            "file_id": file_id,
            "name": name,
            "web_view_link": web_link,
            "web_content_link": resp_body["webContentLink"],
            "mime_type": resp_body["mimeType"],
        })
        .to_string())
    } else {
        let message = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: message.to_string(),
        })
    }
}

/// Create a folder in Google Drive.
async fn create_folder(
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    let name = require_param(params, "name")?;
    let parent_id = params
        .get("parent_id")
        .or(config.default_folder.as_ref());

    let mut metadata = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
    });
    if let Some(pid) = parent_id {
        metadata["parents"] = serde_json::json!([pid]);
    }

    let url = format!("{API_BASE}/files?fields=id,name,webViewLink");
    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .header(CONTENT_TYPE, "application/json")
        .json(&metadata)
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let folder_id = resp_body["id"].as_str().unwrap_or("");
        info!(name, folder_id, "folder created in Google Drive");
        Ok(serde_json::json!({
            "status": "success",
            "folder_id": folder_id,
            "name": name,
            "web_view_link": resp_body["webViewLink"],
        })
        .to_string())
    } else {
        let message = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: message.to_string(),
        })
    }
}

/// List files in Google Drive (optionally filtered by folder).
async fn list_files(
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    let folder_id = params
        .get("folder_id")
        .or(config.default_folder.as_ref());
    let page_size = params
        .get("limit")
        .map(|s| s.as_str())
        .unwrap_or("20");

    let mut query_parts = vec!["trashed = false".to_string()];
    if let Some(fid) = folder_id {
        query_parts.push(format!("'{}' in parents", fid));
    }
    let q = query_parts.join(" and ");

    let url = format!("{API_BASE}/files");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .query(&[
            ("q", q.as_str()),
            ("pageSize", page_size),
            ("fields", "files(id,name,mimeType,size,modifiedTime,webViewLink)"),
            ("orderBy", "modifiedTime desc"),
        ])
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let files: Vec<serde_json::Value> = resp_body["files"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|f| {
                serde_json::json!({
                    "id": f["id"],
                    "name": f["name"],
                    "mime_type": f["mimeType"],
                    "size": f["size"],
                    "modified": f["modifiedTime"],
                    "url": f["webViewLink"],
                })
            })
            .collect();

        info!(count = files.len(), "Google Drive files listed");
        Ok(serde_json::json!(files).to_string())
    } else {
        let message = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: message.to_string(),
        })
    }
}

/// Share a file with an email address.
async fn share_file(
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    let file_id = require_param(params, "file_id")?;
    let email = require_param(params, "email")?;
    let role = params
        .get("role")
        .map(|s| s.as_str())
        .unwrap_or("reader"); // reader, writer, commenter

    let url = format!("{API_BASE}/files/{file_id}/permissions");
    let body = serde_json::json!({
        "type": "user",
        "role": role,
        "emailAddress": email,
    });

    let resp = reqwest::Client::new()
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .header(CONTENT_TYPE, "application/json")
        .query(&[("sendNotificationEmail", "true")])
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
        info!(file_id, email, role, "file shared on Google Drive");
        Ok(serde_json::json!({
            "status": "success",
            "file_id": file_id,
            "shared_with": email,
            "role": role,
        })
        .to_string())
    } else {
        let message = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: message.to_string(),
        })
    }
}

/// Get file metadata.
async fn get_file(
    params: &HashMap<String, String>,
    config: &GDriveConfig,
) -> Result<String, ConnectorError> {
    let file_id = require_param(params, "file_id")?;

    let url = format!("{API_BASE}/files/{file_id}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", config.access_token))
        .query(&[("fields", "id,name,mimeType,size,modifiedTime,webViewLink,webContentLink,owners,shared")])
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        Ok(serde_json::json!({
            "id": resp_body["id"],
            "name": resp_body["name"],
            "mime_type": resp_body["mimeType"],
            "size": resp_body["size"],
            "modified": resp_body["modifiedTime"],
            "web_view_link": resp_body["webViewLink"],
            "shared": resp_body["shared"],
        })
        .to_string())
    } else {
        let message = resp_body["error"]["message"]
            .as_str()
            .unwrap_or("unknown error");
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: message.to_string(),
        })
    }
}
