// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-15

use std::io::{self, BufRead, Write};

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};
use crate::tools::{handle_tool_call, tool_definitions};

pub fn run_server() {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {e}"));
                write_response(&stdout, &resp);
                continue;
            }
        };

        let response = handle_request(&request);
        write_response(&stdout, &response);
    }
}

fn handle_request(req: &JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            req.id.clone(),
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "pact-mcp",
                    "version": "0.4.0"
                }
            }),
        ),
        "notifications/initialized" => {
            // No response needed for notifications
            JsonRpcResponse::success(req.id.clone(), serde_json::json!({}))
        }
        "tools/list" => {
            let tools: Vec<serde_json::Value> = tool_definitions()
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();
            JsonRpcResponse::success(req.id.clone(), serde_json::json!({ "tools": tools }))
        }
        "tools/call" => {
            let tool_name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            match handle_tool_call(tool_name, &args) {
                Ok(result) => JsonRpcResponse::success(
                    req.id.clone(),
                    serde_json::json!({
                        "content": [{ "type": "text", "text": result }]
                    }),
                ),
                Err(e) => JsonRpcResponse::success(
                    req.id.clone(),
                    serde_json::json!({
                        "content": [{ "type": "text", "text": e }],
                        "isError": true
                    }),
                ),
            }
        }
        _ => JsonRpcResponse::error(
            req.id.clone(),
            -32601,
            format!("Method not found: {}", req.method),
        ),
    }
}

fn write_response(stdout: &io::Stdout, resp: &JsonRpcResponse) {
    let mut out = stdout.lock();
    let json = match serde_json::to_string(resp) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[pact-mcp] failed to serialize response: {e}");
            return;
        }
    };
    if let Err(e) = writeln!(out, "{json}") {
        eprintln!("[pact-mcp] failed to write response: {e}");
        return;
    }
    if let Err(e) = out.flush() {
        eprintln!("[pact-mcp] failed to flush stdout: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(1)),
            method: "initialize".to_string(),
            params: serde_json::json!({}),
        };
        let resp = handle_request(&req);
        let result = resp.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "pact-mcp");
        assert_eq!(result["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn handle_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(2)),
            method: "tools/list".to_string(),
            params: serde_json::json!({}),
        };
        let resp = handle_request(&req);
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"pact_check"));
        assert!(names.contains(&"pact_list"));
        assert!(names.contains(&"pact_run"));
        assert!(names.contains(&"pact_scaffold"));
        assert!(names.contains(&"pact_validate_permissions"));
    }

    #[test]
    fn handle_unknown_method() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(3)),
            method: "unknown/method".to_string(),
            params: serde_json::json!({}),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn handle_tools_call_check() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::json!(4)),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "pact_check",
                "arguments": {
                    "source": "agent @g { permits: [^llm.query] tools: [#greet] }"
                }
            }),
        };
        let resp = handle_request(&req);
        let result = resp.result.unwrap();
        assert!(result["isError"].is_null());
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("OK"));
    }
}
