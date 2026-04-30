// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-15

//! Resend connector — send transactional emails.
//!
//! ## Operations
//!
//! | Operation    | Description                        | Required Params              |
//! |-------------|------------------------------------|-----------------------------|
//! | `send`      | Send a plain text or HTML email    | `to`, `subject`             |
//! | `send_html` | Send an HTML email                 | `to`, `subject`, `html`     |
//!
//! ## Authentication
//!
//! Requires an **API key** from Resend:
//!
//! 1. Sign up at [resend.com](https://resend.com)
//! 2. Go to **API Keys** in the dashboard
//! 3. Create a new API key (starts with `re_`)
//! 4. Add and verify a sender domain under **Domains**
//!    - Or use `onboarding@resend.dev` for testing (limited to your own email)
//!
//! ## Free Tier
//!
//! - 100 emails/day, 3,000 emails/month
//! - Single sender domain
//! - No credit card required

use std::collections::HashMap;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, ResendConfig};
use async_trait::async_trait;

const API_BASE: &str = "https://api.resend.com";

pub struct ResendConnector;

#[async_trait]
impl Connector for ResendConnector {
    fn name(&self) -> &'static str { "resend" }
    fn description(&self) -> &'static str { "Resend — send transactional emails" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "resend",
            "description": self.description(),
            "credentials": {
                "api_key": { "type": "string", "required": true, "description": "API key (re_...)", "ui": "password" },
                "from": { "type": "string", "required": false, "description": "Verified sender email (defaults to onboarding@resend.dev for testing)", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "resend",
            "description": self.description(),
            "auth": { "type": "api_key", "description": "Resend API key", "help": "resend.com → API Keys" },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "send": { "description": "Send an email (alias for send_email)", "params": {
                    "to": { "type": "string", "required": true },
                    "subject": { "type": "string", "required": true },
                    "body": { "type": "string", "required": true, "description": "HTML or plain text body" }
                }},
                "send_email": { "description": "Send an email", "params": {
                    "to": { "type": "string", "required": true },
                    "subject": { "type": "string", "required": true },
                    "body": { "type": "string", "required": true, "description": "HTML or plain text body" }
                }},
                "send_html": { "description": "Send an email with attachment (alias for send_email_with_attachment)", "params": {
                    "to": { "type": "string", "required": true },
                    "subject": { "type": "string", "required": true },
                    "body": { "type": "string", "required": true },
                    "filename": { "type": "string", "required": true },
                    "content": { "type": "string", "required": true, "description": "Base64-encoded file content" }
                }},
                "send_email_with_attachment": { "description": "Send an email with an attachment", "params": {
                    "to": { "type": "string", "required": true },
                    "subject": { "type": "string", "required": true },
                    "body": { "type": "string", "required": true },
                    "filename": { "type": "string", "required": true },
                    "content": { "type": "string", "required": true, "description": "Base64-encoded file content" }
                }}
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["send", "send_email", "send_html", "send_email_with_attachment"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.resend.is_some()
    }

    async fn execute(&self, action: &str, params: &HashMap<String, String>, config: &ConnectorConfig) -> Result<String, ConnectorError> {
        let cfg = config.resend.as_ref().ok_or(ConnectorError::NotConfigured("resend".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a Resend operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &ResendConfig,
) -> Result<String, ConnectorError> {
    match action {
        "send" | "send_email" => send_email(params, config, false).await,
        "send_html" | "send_email_with_attachment" => send_email(params, config, true).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "resend.{action}"
        ))),
    }
}

/// Send an email via Resend.
async fn send_email(
    params: &HashMap<String, String>,
    config: &ResendConfig,
    is_html: bool,
) -> Result<String, ConnectorError> {
    let to = params
        .get("to")
        .ok_or_else(|| ConnectorError::MissingParam("to".into()))?;
    let subject = params
        .get("subject")
        .ok_or_else(|| ConnectorError::MissingParam("subject".into()))?;

    // Support comma-separated recipients.
    let to_addresses: Vec<&str> = to.split(',').map(|s| s.trim()).collect();

    // Allow overriding `from` via tool params (e.g. from diagram metadata),
    // falling back to the connector config value.
    let from = params.get("from").unwrap_or(&config.from);

    let mut body = serde_json::json!({
        "from": from,
        "to": to_addresses,
        "subject": subject,
    });

    if is_html {
        let html = params
            .get("html")
            .or_else(|| params.get("body"))
            .or_else(|| params.get("content"))
            .or_else(|| params.get("text"))
            .or_else(|| params.get("message"))
            .ok_or_else(|| ConnectorError::MissingParam("html".into()))?;
        body["html"] = serde_json::json!(html);
    } else {
        // The tool spec exposes this param as "body", but LLMs may use other names.
        let text = params
            .get("body")
            .or_else(|| params.get("text"))
            .or_else(|| params.get("html"))
            .or_else(|| params.get("content"))
            .or_else(|| params.get("message"))
            .ok_or_else(|| ConnectorError::MissingParam("body".into()))?
            .clone();
        // If the content looks like HTML, send as html instead of text.
        if text.contains("<html") || text.contains("<div") || text.contains("<p>") || text.contains("<br") {
            body["html"] = serde_json::json!(text);
        } else {
            body["text"] = serde_json::json!(text);
        }
    }

    // Optional fields.
    if let Some(cc) = params.get("cc") {
        let cc_addresses: Vec<&str> = cc.split(',').map(|s| s.trim()).collect();
        body["cc"] = serde_json::json!(cc_addresses);
    }
    if let Some(bcc) = params.get("bcc") {
        let bcc_addresses: Vec<&str> = bcc.split(',').map(|s| s.trim()).collect();
        body["bcc"] = serde_json::json!(bcc_addresses);
    }
    if let Some(reply_to) = params.get("reply_to") {
        body["reply_to"] = serde_json::json!(reply_to);
    }

    let resp = reqwest::Client::new()
        .post(&format!("{API_BASE}/emails"))
        .header(AUTHORIZATION, format!("Bearer {}", config.api_key))
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
        let email_id = resp_body["id"].as_str().unwrap_or("unknown");
        info!(to, subject, email_id, "email sent via Resend");
        Ok(serde_json::json!({
            "status": "success",
            "email_id": email_id,
            "to": to_addresses,
            "subject": subject,
        })
        .to_string())
    } else {
        Err(ConnectorError::ApiError {
            status: status.as_u16(),
            message: resp_body["message"]
                .as_str()
                .or_else(|| resp_body["error"].as_str())
                .unwrap_or("unknown error")
                .to_string(),
        })
    }
}
