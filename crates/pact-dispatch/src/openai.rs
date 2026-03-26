// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-12-05

//! OpenAI API dispatcher for the PACT language runtime.
//!
//! Provides [`OpenAIDispatcher`] which implements
//! [`pact_core::interpreter::Dispatcher`] to call the OpenAI Chat Completions
//! API.
//!
//! ## Usage
//!
//! ```no_run
//! use pact_dispatch::OpenAIDispatcher;
//! use pact_core::interpreter::Interpreter;
//!
//! let dispatcher = OpenAIDispatcher::from_env().unwrap();
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

/// OpenAI Chat Completions API dispatcher.
///
/// Sends prompts to the OpenAI API and returns the assistant's response
/// as a [`Value::ToolResult`].
pub struct OpenAIDispatcher {
    api_key: String,
    model: String,
    client: Client,
    runtime: tokio::runtime::Runtime,
    rate_limiter: Option<Arc<RateLimiter>>,
}

/// Request body for the OpenAI Chat Completions endpoint.
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// A single message in the OpenAI chat format.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Response from the OpenAI Chat Completions endpoint.
#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

/// A single choice in the completion response.
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

/// A streamed chunk from the OpenAI Chat Completions endpoint.
#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChunkChoice>,
}

/// A single choice in a streamed chunk.
#[derive(Debug, Deserialize)]
struct ChatChunkChoice {
    delta: ChatDelta,
}

/// Delta content in a streamed chunk.
#[derive(Debug, Deserialize)]
struct ChatDelta {
    #[serde(default)]
    content: Option<String>,
}

impl OpenAIDispatcher {
    /// Create a dispatcher from the `OPENAI_API_KEY` environment variable.
    ///
    /// Uses `"gpt-4o"` as the default model.
    pub fn from_env() -> Result<Self, DispatchError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| DispatchError::MissingEnvVar("OPENAI_API_KEY".to_string()))?;
        Self::new(api_key, "gpt-4o".to_string())
    }

    /// Create a dispatcher with an explicit API key and model.
    ///
    /// Configures a 90-second request timeout for token-heavy requests.
    pub fn new(api_key: String, model: String) -> Result<Self, DispatchError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;
        let runtime =
            tokio::runtime::Runtime::new().map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            api_key,
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

    /// Send a non-streaming request to the OpenAI API.
    async fn send_request(&self, prompt: &str) -> Result<String, DispatchError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: None,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
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

        let parsed: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| DispatchError::ParseError(e.to_string()))?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| DispatchError::ParseError("no choices in response".to_string()))
    }

    /// Send a streaming request to the OpenAI API and collect the full response.
    pub async fn send_request_streaming(&self, prompt: &str) -> Result<String, DispatchError> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: Some(true),
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
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
            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }
            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(json_str) {
                    for choice in chunk.choices {
                        if let Some(content) = choice.delta.content {
                            collected.push_str(&content);
                        }
                    }
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

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: Some(true),
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
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

                        if line.is_empty() || line == "data: [DONE]" {
                            if line == "data: [DONE]" {
                                let _ = tx
                                    .send(StreamEvent::MessageDone {
                                        stop_reason: StopReason::EndTurn,
                                    })
                                    .await;
                            }
                            continue;
                        }
                        if let Some(json_str) = line.strip_prefix("data: ") {
                            if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(json_str)
                            {
                                for choice in chunk.choices {
                                    if let Some(content) = choice.delta.content {
                                        let _ = tx.send(StreamEvent::TextDelta(content)).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

impl Dispatcher for OpenAIDispatcher {
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
            OpenAIDispatcher::build_prompt("greeter", "greet", &[Value::String("world".into())]);
        assert!(prompt.contains("@greeter"));
        assert!(prompt.contains("#greet"));
        assert!(prompt.contains("world"));
    }

    #[test]
    fn build_prompt_multiple_args() {
        let prompt = OpenAIDispatcher::build_prompt("math", "add", &[Value::Int(2), Value::Int(3)]);
        assert!(prompt.contains("@math"));
        assert!(prompt.contains("#add"));
        assert!(prompt.contains("2"));
        assert!(prompt.contains("3"));
    }

    #[test]
    fn build_prompt_no_args() {
        let prompt = OpenAIDispatcher::build_prompt("bot", "status", &[]);
        assert!(prompt.contains("@bot"));
        assert!(prompt.contains("#status"));
        assert!(prompt.contains("[]"));
    }

    #[test]
    fn from_env_missing_key() {
        // Ensure OPENAI_API_KEY is not set for this test
        std::env::remove_var("OPENAI_API_KEY");
        let result = OpenAIDispatcher::from_env();
        assert!(result.is_err());
    }

    #[test]
    fn new_creates_dispatcher() {
        let dispatcher = OpenAIDispatcher::new("test-key".into(), "gpt-4o".into());
        assert!(dispatcher.is_ok());
        let d = dispatcher.unwrap();
        assert_eq!(d.model, "gpt-4o");
        assert_eq!(d.api_key, "test-key");
    }

    #[test]
    fn parse_response_json() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello, world!"
                }
            }]
        }"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices.len(), 1);
        assert_eq!(parsed.choices[0].message.content, "Hello, world!");
    }

    #[test]
    fn parse_response_empty_choices() {
        let json = r#"{"choices": []}"#;
        let parsed: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.choices.is_empty());
    }

    #[test]
    fn parse_stream_chunk() {
        let json = r#"{"choices": [{"delta": {"content": "Hello"}}]}"#;
        let parsed: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.choices[0].delta.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn parse_stream_chunk_no_content() {
        let json = r#"{"choices": [{"delta": {}}]}"#;
        let parsed: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert!(parsed.choices[0].delta.content.is_none());
    }
}
