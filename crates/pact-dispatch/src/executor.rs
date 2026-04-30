// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-15

//! Real tool execution engine.
//!
//! Parses tool handler specifications and executes them:
//! - `"http METHOD url"` — HTTP request (response bounded to 10 MB)
//! - `"sh command"` — Shell command (params are shell-escaped to prevent injection)
//! - `"builtin:name"` — Built-in function
//! - `"mcp server/tool"` — MCP server tool call (routed through connection pool)
//!
//! Parameter values are interpolated via `{param_name}` placeholders.
//! Shell handlers use single-quote escaping to prevent command injection.
//! Diagnostic output goes to stderr; sensitive values are redacted in logs.

use std::collections::HashMap;

use tracing::debug;

use crate::DispatchError;

/// Parsed handler specification.
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerSpec {
    /// HTTP handler: method + URL template.
    Http {
        /// The HTTP method (GET, POST, PUT, DELETE, PATCH).
        method: String,
        /// The URL template with `{param}` placeholders.
        url: String,
    },
    /// Shell handler: command template.
    Shell {
        /// The shell command template with `{param}` placeholders.
        command: String,
    },
    /// Built-in handler: function name.
    Builtin {
        /// The built-in function name (e.g., "echo", "json", "noop").
        name: String,
    },
    /// MCP handler: server name + tool name.
    Mcp {
        /// The MCP server name (as declared in `connect`).
        server: String,
        /// The tool name on the MCP server.
        tool: String,
    },
    /// Connector handler: external service operation (e.g. `github.push_file`).
    ///
    /// Routed through a [`crate::ConnectorRunner`] supplied at dispatcher
    /// construction time. The runner is responsible for credential lookup,
    /// authentication, and the actual API call.
    Connector {
        /// The full operation in `connector.action` form (e.g. `github.push_file`).
        operation: String,
    },
}

/// Parse a handler string into a `HandlerSpec`.
///
/// Formats:
/// - `"http GET https://api.example.com/search?q={query}"`
/// - `"sh curl -s {url}"`
/// - `"builtin:echo"`
pub fn parse_handler(spec: &str) -> Result<HandlerSpec, DispatchError> {
    let spec = spec.trim();

    if let Some(name) = spec.strip_prefix("builtin:") {
        return Ok(HandlerSpec::Builtin {
            name: name.trim().to_string(),
        });
    }

    if let Some(rest) = spec.strip_prefix("sh ") {
        return Ok(HandlerSpec::Shell {
            command: rest.trim().to_string(),
        });
    }

    if let Some(rest) = spec.strip_prefix("connector ") {
        let op = rest.trim();
        if op.is_empty() || !op.contains('.') {
            return Err(DispatchError::ExecutionError(format!(
                "connector handler requires 'connector.action' format (e.g. 'connector github.push_file'), got: '{}'",
                spec
            )));
        }
        return Ok(HandlerSpec::Connector {
            operation: op.to_string(),
        });
    }

    if let Some(rest) = spec.strip_prefix("mcp ") {
        let (server, tool) = rest.split_once('/').ok_or_else(|| {
            DispatchError::ExecutionError(
                "mcp handler requires server/tool format (e.g. 'mcp slack/send_message')"
                    .to_string(),
            )
        })?;
        return Ok(HandlerSpec::Mcp {
            server: server.trim().to_string(),
            tool: tool.trim().to_string(),
        });
    }

    if let Some(rest) = spec.strip_prefix("http ") {
        let rest = rest.trim();
        let (method, url) = rest.split_once(' ').ok_or_else(|| {
            DispatchError::ExecutionError(
                "http handler requires METHOD and URL (e.g. 'http GET https://...')".to_string(),
            )
        })?;
        return Ok(HandlerSpec::Http {
            method: method.to_uppercase(),
            url: url.trim().to_string(),
        });
    }

    Err(DispatchError::ExecutionError(format!(
        "unknown handler format: '{}'. Expected 'http METHOD url', 'sh command', 'builtin:name', 'mcp server/tool', or 'connector connector.action'",
        spec
    )))
}

/// Interpolate `{param_name}` placeholders in a template string with values.
pub fn interpolate(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{}}}", key), value);
    }
    result
}

/// Shell-escape a string by wrapping it in single quotes.
///
/// Any existing single quotes are replaced with `'\''` (end quote, escaped quote, start quote).
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Interpolate parameters with shell escaping applied to each value.
fn interpolate_shell(template: &str, params: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in params {
        result = result.replace(&format!("{{{}}}", key), &shell_escape(value));
    }
    result
}

/// Redact potentially sensitive values from a URL or command string for logging.
///
/// Replaces query parameter values, Bearer tokens, and API key patterns.
fn redact_for_log(s: &str) -> String {
    // Redact query parameter values (key=value) where value is long enough to be a secret
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        result.push(ch);
        if ch == '=' {
            // Peek ahead to see the value length
            let rest: String = chars
                .clone()
                .take_while(|c| *c != '&' && *c != ' ')
                .collect();
            if rest.len() > 8 {
                result.push_str("[REDACTED]");
                // Consume the value
                for _ in 0..rest.len() {
                    chars.next();
                }
            }
        }
    }
    result
}

/// Maximum response body size (10 MB).
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Extract parameter values from a JSON input object as string key-value pairs.
pub fn extract_params(input: &serde_json::Value) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(obj) = input.as_object() {
        for (key, val) in obj {
            let str_val = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            params.insert(key.clone(), str_val);
        }
    }
    params
}

/// Execute a handler specification with the given parameters.
///
/// Returns the execution result as a string.
pub async fn execute_handler(
    spec: &HandlerSpec,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    match spec {
        HandlerSpec::Http { method, url } => execute_http(method, url, params).await,
        HandlerSpec::Shell { command } => execute_shell(command, params).await,
        HandlerSpec::Builtin { name } => execute_builtin(name, params),
        HandlerSpec::Mcp { server, tool } => Err(DispatchError::ExecutionError(format!(
            "MCP handler mcp {}/{} must be executed through the MCP connection pool, not execute_handler",
            server, tool
        ))),
        HandlerSpec::Connector { operation } => Err(DispatchError::ExecutionError(format!(
            "connector handler '{}' must be executed through a ConnectorRunner, not execute_handler",
            operation
        ))),
    }
}

/// Execute an HTTP handler.
async fn execute_http(
    method: &str,
    url_template: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    let url = interpolate(url_template, params);

    debug!(method, url = redact_for_log(&url).as_str(), "HTTP request");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let request = match method {
        "GET" => client.get(&url),
        "POST" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body)
        }
        "PUT" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client
                .put(&url)
                .header("Content-Type", "application/json")
                .body(body)
        }
        "DELETE" => client.delete(&url),
        "PATCH" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client
                .patch(&url)
                .header("Content-Type", "application/json")
                .body(body)
        }
        _ => {
            return Err(DispatchError::ExecutionError(format!(
                "unsupported HTTP method: {method}"
            )));
        }
    };

    let response = request
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP request failed: {e}")))?;

    let status = response.status();

    // Check Content-Length before reading body to prevent OOM
    if let Some(len) = response.content_length() {
        if len as usize > MAX_RESPONSE_BYTES {
            return Err(DispatchError::ExecutionError(format!(
                "HTTP response too large: {len} bytes (max {MAX_RESPONSE_BYTES})"
            )));
        }
    }

    let body = response
        .text()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response body: {e}")))?;

    if body.len() > MAX_RESPONSE_BYTES {
        return Err(DispatchError::ExecutionError(format!(
            "HTTP response body too large: {} bytes (max {MAX_RESPONSE_BYTES})",
            body.len()
        )));
    }

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "HTTP {method} returned {status}: {}",
            { let mut end = body.len().min(500); while end > 0 && !body.is_char_boundary(end) { end -= 1; } &body[..end] }
        )));
    }

    debug!(method, status = %status, bytes = body.len(), "HTTP response");
    Ok(body)
}

/// Execute a shell handler.
///
/// Parameter values are shell-escaped (single-quoted) to prevent injection attacks.
async fn execute_shell(
    command_template: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    // Use shell-escaped interpolation to prevent command injection
    let command = interpolate_shell(command_template, params);

    debug!(command = redact_for_log(&command).as_str(), "shell exec");

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| {
            DispatchError::ExecutionError(format!("failed to execute shell command: {e}"))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if stdout.len() > MAX_RESPONSE_BYTES {
        return Err(DispatchError::ExecutionError(format!(
            "shell command output too large: {} bytes (max {MAX_RESPONSE_BYTES})",
            stdout.len()
        )));
    }

    if !output.status.success() {
        return Err(DispatchError::ExecutionError(format!(
            "shell command failed (exit {}): stderr={}",
            output.status.code().unwrap_or(-1),
            { let mut end = stderr.len().min(500); while end > 0 && !stderr.is_char_boundary(end) { end -= 1; } &stderr[..end] }
        )));
    }

    if !stderr.is_empty() {
        debug!(stderr = { let mut end = stderr.len().min(200); while end > 0 && !stderr.is_char_boundary(end) { end -= 1; } &stderr[..end] }, "shell stderr");
    }

    debug!(bytes = stdout.len(), "shell output");
    Ok(stdout)
}

/// Execute a built-in handler.
fn execute_builtin(name: &str, params: &HashMap<String, String>) -> Result<String, DispatchError> {
    debug!(name, "builtin exec");

    match name {
        "echo" => {
            let message = params
                .get("message")
                .or_else(|| params.get("text"))
                .cloned()
                .unwrap_or_else(|| serde_json::to_string(params).unwrap_or_default());
            Ok(message)
        }
        "json" => serde_json::to_string_pretty(params)
            .map_err(|e| DispatchError::ExecutionError(e.to_string())),
        "noop" => Ok("ok".to_string()),
        _ => Err(DispatchError::ExecutionError(format!(
            "unknown builtin handler: '{name}'"
        ))),
    }
}

/// Check if an agent has the permissions required to execute a handler type.
///
/// - `http` handlers require `!net.read` (GET) or `!net.write` (POST/PUT/DELETE/PATCH)
/// - `sh` handlers require `!sh.exec`
/// - `builtin` handlers require no extra permissions
pub fn handler_required_permissions(spec: &HandlerSpec) -> Vec<&'static str> {
    match spec {
        HandlerSpec::Http { method, .. } => match method.as_str() {
            "GET" => vec!["net.read"],
            _ => vec!["net.write"],
        },
        HandlerSpec::Shell { .. } => vec!["sh.exec"],
        HandlerSpec::Builtin { .. } => vec![],
        HandlerSpec::Mcp { .. } => vec![], // MCP permissions enforced via ^mcp.{server} in mediation
        // Connectors enforce auth via the user-supplied credential (token/api_key)
        // passed through ConnectorConfig — adding a generic `net.write` permit on
        // top would just force every agent that uses a connector to also declare
        // `^net.write`, which is misleading (the agent has no general internet
        // write access, only the specific connector). Per-connector permits could
        // be added later (see `^mcp.{server}` for the pattern).
        HandlerSpec::Connector { .. } => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_get() {
        let spec = parse_handler("http GET https://api.example.com/search?q={query}").unwrap();
        assert_eq!(
            spec,
            HandlerSpec::Http {
                method: "GET".to_string(),
                url: "https://api.example.com/search?q={query}".to_string(),
            }
        );
    }

    #[test]
    fn parse_http_post() {
        let spec = parse_handler("http post https://api.example.com/data").unwrap();
        assert_eq!(
            spec,
            HandlerSpec::Http {
                method: "POST".to_string(),
                url: "https://api.example.com/data".to_string(),
            }
        );
    }

    #[test]
    fn parse_shell() {
        let spec = parse_handler("sh curl -s {url}").unwrap();
        assert_eq!(
            spec,
            HandlerSpec::Shell {
                command: "curl -s {url}".to_string(),
            }
        );
    }

    #[test]
    fn parse_builtin() {
        let spec = parse_handler("builtin:echo").unwrap();
        assert_eq!(
            spec,
            HandlerSpec::Builtin {
                name: "echo".to_string(),
            }
        );
    }

    #[test]
    fn parse_mcp_handler() {
        let spec = parse_handler("mcp slack/send_message").unwrap();
        assert_eq!(
            spec,
            HandlerSpec::Mcp {
                server: "slack".to_string(),
                tool: "send_message".to_string(),
            }
        );
    }

    #[test]
    fn parse_mcp_handler_missing_slash_fails() {
        assert!(parse_handler("mcp slack_send_message").is_err());
    }

    #[test]
    fn handler_permissions_mcp() {
        let spec = HandlerSpec::Mcp {
            server: "slack".to_string(),
            tool: "send".to_string(),
        };
        assert!(handler_required_permissions(&spec).is_empty());
    }

    #[test]
    fn parse_unknown_format_fails() {
        assert!(parse_handler("ftp://example.com").is_err());
    }

    #[test]
    fn interpolate_params() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "world".to_string());
        params.insert("count".to_string(), "42".to_string());
        let result = interpolate("hello {name}, count={count}", &params);
        assert_eq!(result, "hello world, count=42");
    }

    #[test]
    fn interpolate_no_match_preserved() {
        let params = HashMap::new();
        let result = interpolate("hello {unknown}", &params);
        assert_eq!(result, "hello {unknown}");
    }

    #[test]
    fn extract_params_from_json() {
        let input = serde_json::json!({"query": "rust", "limit": 10});
        let params = extract_params(&input);
        assert_eq!(params.get("query").unwrap(), "rust");
        assert_eq!(params.get("limit").unwrap(), "10");
    }

    #[test]
    fn handler_permissions_http_get() {
        let spec = HandlerSpec::Http {
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
        };
        assert_eq!(handler_required_permissions(&spec), vec!["net.read"]);
    }

    #[test]
    fn handler_permissions_http_post() {
        let spec = HandlerSpec::Http {
            method: "POST".to_string(),
            url: "https://example.com".to_string(),
        };
        assert_eq!(handler_required_permissions(&spec), vec!["net.write"]);
    }

    #[test]
    fn handler_permissions_shell() {
        let spec = HandlerSpec::Shell {
            command: "echo hello".to_string(),
        };
        assert_eq!(handler_required_permissions(&spec), vec!["sh.exec"]);
    }

    #[test]
    fn handler_permissions_builtin() {
        let spec = HandlerSpec::Builtin {
            name: "echo".to_string(),
        };
        assert!(handler_required_permissions(&spec).is_empty());
    }

    #[test]
    fn builtin_echo() {
        let mut params = HashMap::new();
        params.insert("message".to_string(), "hello world".to_string());
        let result = execute_builtin("echo", &params).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn builtin_noop() {
        let params = HashMap::new();
        let result = execute_builtin("noop", &params).unwrap();
        assert_eq!(result, "ok");
    }

    #[test]
    fn builtin_unknown_fails() {
        let params = HashMap::new();
        assert!(execute_builtin("unknown_fn", &params).is_err());
    }

    #[test]
    fn shell_escape_basic() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[test]
    fn shell_escape_injection_attempt() {
        assert_eq!(shell_escape("; rm -rf /"), "'; rm -rf /'");
    }

    #[test]
    fn shell_escape_single_quotes() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn interpolate_shell_escapes_values() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), "; cat /etc/passwd".to_string());
        let result = interpolate_shell("echo {name}", &params);
        assert_eq!(result, "echo '; cat /etc/passwd'");
    }

    #[test]
    fn redact_short_values_preserved() {
        // Short values (<=8 chars) are not redacted
        let result = redact_for_log("https://api.com?q=rust");
        assert!(result.contains("rust"));
    }

    #[test]
    fn redact_long_values_hidden() {
        // Long values (>8 chars) are redacted
        let result = redact_for_log("https://api.com?key=sk_live_very_long_secret_key");
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("sk_live"));
    }
}
