// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-12-12

//! Ollama API dispatcher for the PACT language runtime.
//!
//! Provides [`OllamaDispatcher`] which implements
//! [`pact_core::interpreter::Dispatcher`] to call a local Ollama instance.
//!
//! ## Usage
//!
//! ```no_run
//! use pact_dispatch::OllamaDispatcher;
//! use pact_core::interpreter::Interpreter;
//!
//! let dispatcher = OllamaDispatcher::from_env().unwrap();
//! let mut interp = Interpreter::with_dispatcher(Box::new(dispatcher));
//! ```

use std::sync::Arc;

use pact_core::ast::stmt::{AgentDecl, Program};
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Dispatcher;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::rate_limit::{RateLimitConfig, RateLimiter};
use crate::DispatchError;

/// Default base URL for the Ollama API.
const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Default model for Ollama.
const DEFAULT_MODEL: &str = "llama3";

/// Ollama API dispatcher.
///
/// Sends prompts to a local (or remote) Ollama instance and returns the
/// model's response as a [`Value::ToolResult`].
pub struct OllamaDispatcher {
    base_url: String,
    model: String,
    client: Client,
    runtime: tokio::runtime::Runtime,
    rate_limiter: Option<Arc<RateLimiter>>,
}

/// Request body for the Ollama `/api/generate` endpoint.
#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

/// Response from the Ollama `/api/generate` endpoint (non-streaming).
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

/// A single streamed token from the Ollama `/api/generate` endpoint.
#[derive(Debug, Deserialize)]
struct GenerateStreamChunk {
    response: String,
    #[serde(default)]
    done: bool,
}

impl OllamaDispatcher {
    /// Create a dispatcher from environment variables.
    ///
    /// Reads `OLLAMA_URL` (default `http://localhost:11434`) and
    /// `OLLAMA_MODEL` (default `llama3`).
    pub fn from_env() -> Result<Self, DispatchError> {
        let base_url = std::env::var("OLLAMA_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Self::new(base_url, model)
    }

    /// Create a dispatcher with an explicit base URL and model.
    ///
    /// Configures a 120-second request timeout (local models can be slow).
    pub fn new(base_url: String, model: String) -> Result<Self, DispatchError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;
        let runtime =
            tokio::runtime::Runtime::new().map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            base_url,
            model,
            client,
            runtime,
            rate_limiter: None,
        })
    }

    /// Configure rate limiting for this dispatcher.
    pub fn with_rate_limits(mut self, config: RateLimitConfig) -> Self {
        self.rate_limiter = Some(Arc::new(RateLimiter::new(config)));
        self
    }

    /// Build the prompt string from dispatch arguments.
    fn build_prompt(agent_name: &str, tool_name: &str, args: &[Value]) -> String {
        let args_str: Vec<String> = args.iter().map(|a| format!("{a}")).collect();
        format!(
            "You are agent @{agent_name}. Execute tool #{tool_name} with arguments: [{}]",
            args_str.join(", ")
        )
    }

    /// Send a non-streaming request to the Ollama API.
    async fn send_request(&self, prompt: &str) -> Result<String, DispatchError> {
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
        };

        let url = format!("{}/api/generate", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(DispatchError::ApiError { status, body });
        }

        let parsed: GenerateResponse = response
            .json()
            .await
            .map_err(|e| DispatchError::ParseError(e.to_string()))?;

        Ok(parsed.response)
    }

    /// Send a streaming request to the Ollama API and collect the full response.
    pub async fn send_request_streaming(&self, prompt: &str) -> Result<String, DispatchError> {
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: true,
        };

        let url = format!("{}/api/generate", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(DispatchError::ApiError { status, body });
        }

        let full_text = response
            .text()
            .await
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;

        let mut collected = String::new();
        for line in full_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<GenerateStreamChunk>(line) {
                collected.push_str(&chunk.response);
                if chunk.done {
                    break;
                }
            }
        }

        if collected.is_empty() {
            Err(DispatchError::ParseError(
                "no content in streamed response".to_string(),
            ))
        } else {
            Ok(collected)
        }
    }

    /// Send a streaming request and return a receiver of stream events.
    pub async fn send_stream_events(
        &self,
        prompt: &str,
    ) -> Result<tokio::sync::mpsc::Receiver<crate::client::StreamEvent>, DispatchError> {
        use crate::client::StreamEvent;
        use crate::types::StopReason;
        use futures_util::StreamExt;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: true,
        };

        let url = format!("{}/api/generate", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(DispatchError::ApiError { status, body });
        }

        let mut stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Some(chunk) = stream.next().await {
                if let Ok(bytes) = chunk {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].trim().to_string();
                        buffer = buffer[pos + 1..].to_string();

                        if line.is_empty() {
                            continue;
                        }
                        if let Ok(chunk) = serde_json::from_str::<GenerateStreamChunk>(&line) {
                            if !chunk.response.is_empty() {
                                let _ = tx.send(StreamEvent::TextDelta(chunk.response)).await;
                            }
                            if chunk.done {
                                let _ = tx
                                    .send(StreamEvent::MessageDone {
                                        stop_reason: StopReason::EndTurn,
                                    })
                                    .await;
                                return;
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

impl Dispatcher for OllamaDispatcher {
    fn dispatch(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        _agent_decl: &AgentDecl,
        _program: &Program,
    ) -> Result<Value, String> {
        info!(agent = agent_name, tool = tool_name, "dispatching");

        if let Some(limiter) = &self.rate_limiter {
            limiter
                .check_agent_limit(agent_name)
                .map_err(|e| e.to_string())?;
            limiter.check_global_limit().map_err(|e| e.to_string())?;
        }

        let prompt = Self::build_prompt(agent_name, tool_name, args);

        let result = self
            .runtime
            .block_on(self.send_request(&prompt))
            .map_err(|e| e.to_string())?;

        if let Some(limiter) = &self.rate_limiter {
            limiter.record_agent_call(agent_name);
        }

        Ok(Value::ToolResult(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_formats_correctly() {
        let prompt =
            OllamaDispatcher::build_prompt("greeter", "greet", &[Value::String("world".into())]);
        assert!(prompt.contains("@greeter"));
        assert!(prompt.contains("#greet"));
        assert!(prompt.contains("world"));
    }

    #[test]
    fn build_prompt_multiple_args() {
        let prompt = OllamaDispatcher::build_prompt("math", "add", &[Value::Int(2), Value::Int(3)]);
        assert!(prompt.contains("@math"));
        assert!(prompt.contains("#add"));
        assert!(prompt.contains("2"));
        assert!(prompt.contains("3"));
    }

    #[test]
    fn build_prompt_no_args() {
        let prompt = OllamaDispatcher::build_prompt("bot", "status", &[]);
        assert!(prompt.contains("@bot"));
        assert!(prompt.contains("#status"));
        assert!(prompt.contains("[]"));
    }

    #[test]
    fn from_env_uses_defaults() {
        // Clear env vars to test defaults
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("OLLAMA_MODEL");
        let dispatcher = OllamaDispatcher::from_env().unwrap();
        assert_eq!(dispatcher.base_url, DEFAULT_BASE_URL);
        assert_eq!(dispatcher.model, DEFAULT_MODEL);
    }

    #[test]
    fn from_env_reads_custom_values() {
        std::env::set_var("OLLAMA_URL", "http://myhost:9999");
        std::env::set_var("OLLAMA_MODEL", "mistral");
        let dispatcher = OllamaDispatcher::from_env().unwrap();
        assert_eq!(dispatcher.base_url, "http://myhost:9999");
        assert_eq!(dispatcher.model, "mistral");
        // Clean up
        std::env::remove_var("OLLAMA_URL");
        std::env::remove_var("OLLAMA_MODEL");
    }

    #[test]
    fn new_creates_dispatcher() {
        let dispatcher = OllamaDispatcher::new("http://localhost:11434".into(), "llama3".into());
        assert!(dispatcher.is_ok());
        let d = dispatcher.unwrap();
        assert_eq!(d.model, "llama3");
        assert_eq!(d.base_url, "http://localhost:11434");
    }

    #[test]
    fn parse_response_json() {
        let json = r#"{"response": "Hello, world!"}"#;
        let parsed: GenerateResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.response, "Hello, world!");
    }

    #[test]
    fn parse_stream_chunk() {
        let json = r#"{"response": "Hello", "done": false}"#;
        let parsed: GenerateStreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.response, "Hello");
        assert!(!parsed.done);
    }

    #[test]
    fn parse_stream_chunk_done() {
        let json = r#"{"response": "", "done": true}"#;
        let parsed: GenerateStreamChunk = serde_json::from_str(json).unwrap();
        assert!(parsed.done);
    }

    #[test]
    fn generate_url_construction() {
        let d = OllamaDispatcher::new("http://localhost:11434".into(), "llama3".into()).unwrap();
        let url = format!("{}/api/generate", d.base_url);
        assert_eq!(url, "http://localhost:11434/api/generate");
    }

    #[test]
    fn generate_url_custom_base() {
        let d = OllamaDispatcher::new("http://remote:8080".into(), "phi3".into()).unwrap();
        let url = format!("{}/api/generate", d.base_url);
        assert_eq!(url, "http://remote:8080/api/generate");
    }
}
