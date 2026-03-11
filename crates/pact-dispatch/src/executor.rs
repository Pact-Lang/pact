// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-15

//! Real tool execution engine.
//!
//! Parses tool handler specifications and executes them:
//! - `"http METHOD url"` — HTTP request
//! - `"sh command"` — Shell command
//! - `"builtin:name"` — Built-in function
//!
//! Parameter values are interpolated via `{param_name}` placeholders.

use std::collections::HashMap;

use crate::DispatchError;

/// Parsed handler specification.
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerSpec {
    /// HTTP handler: method + URL template.
    Http { method: String, url: String },
    /// Shell handler: command template.
    Shell { command: String },
    /// Built-in handler: function name.
    Builtin { name: String },
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

    if let Some(rest) = spec.strip_prefix("http ") {
        let rest = rest.trim();
        let (method, url) = rest
            .split_once(' ')
            .ok_or_else(|| DispatchError::ExecutionError(
                "http handler requires METHOD and URL (e.g. 'http GET https://...')".to_string(),
            ))?;
        return Ok(HandlerSpec::Http {
            method: method.to_uppercase(),
            url: url.trim().to_string(),
        });
    }

    Err(DispatchError::ExecutionError(format!(
        "unknown handler format: '{}'. Expected 'http METHOD url', 'sh command', or 'builtin:name'",
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
    }
}

/// Execute an HTTP handler.
async fn execute_http(
    method: &str,
    url_template: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    let url = interpolate(url_template, params);

    println!("[EXECUTOR] HTTP {method} {url}");

    let client = reqwest::Client::new();
    let request = match method {
        "GET" => client.get(&url),
        "POST" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client.post(&url).header("Content-Type", "application/json").body(body)
        }
        "PUT" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client.put(&url).header("Content-Type", "application/json").body(body)
        }
        "DELETE" => client.delete(&url),
        "PATCH" => {
            let body = serde_json::to_string(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
            client.patch(&url).header("Content-Type", "application/json").body(body)
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
    let body = response
        .text()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response body: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "HTTP {method} {url} returned {status}: {body}"
        )));
    }

    println!("[EXECUTOR] HTTP {method} => {status} ({} bytes)", body.len());
    Ok(body)
}

/// Execute a shell handler.
async fn execute_shell(
    command_template: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    let command = interpolate(command_template, params);

    println!("[EXECUTOR] SH: {command}");

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to execute shell command: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(DispatchError::ExecutionError(format!(
            "shell command failed (exit {}): stdout={stdout}, stderr={stderr}",
            output.status.code().unwrap_or(-1)
        )));
    }

    if !stderr.is_empty() {
        println!("[EXECUTOR] SH stderr: {stderr}");
    }

    println!("[EXECUTOR] SH => {} bytes", stdout.len());
    Ok(stdout)
}

/// Execute a built-in handler.
fn execute_builtin(
    name: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    println!("[EXECUTOR] BUILTIN: {name}");

    match name {
        "echo" => {
            let message = params.get("message").or_else(|| params.get("text"))
                .cloned()
                .unwrap_or_else(|| {
                    serde_json::to_string(params).unwrap_or_default()
                });
            Ok(message)
        }
        "json" => {
            serde_json::to_string_pretty(params)
                .map_err(|e| DispatchError::ExecutionError(e.to_string()))
        }
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
}
