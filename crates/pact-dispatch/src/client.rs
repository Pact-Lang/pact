// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-10

//! HTTP client for the Anthropic Messages API.

use crate::types::{MessagesResponse, StopReason};
use crate::DispatchError;
use futures_util::StreamExt;
use pact_build::emit_claude::ClaudeRequest;
use tokio::sync::mpsc;
use tracing::warn;

/// The Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default base URL for the Anthropic API.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// A streaming event from the Anthropic Messages API.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of text content.
    TextDelta(String),
    /// Tool use block started.
    ToolUseStart {
        /// Unique identifier for this tool use block.
        id: String,
        /// Name of the tool being invoked.
        name: String,
    },
    /// Tool use input delta (JSON chunk).
    ToolUseInputDelta(String),
    /// Content block finished.
    ContentBlockStop,
    /// Message completed with stop reason.
    MessageDone {
        /// Why the model stopped generating.
        stop_reason: StopReason,
    },
    /// A tool is being executed (informational, for UI display).
    ToolExecuting {
        /// Name of the tool being executed.
        name: String,
    },
    /// A tool execution completed with a result.
    ToolResult {
        /// Name of the tool that was executed.
        name: String,
        /// The text result from the tool.
        result: String,
    },
    /// An error occurred during streaming.
    Error(String),
}

/// HTTP client wrapper for the Anthropic Messages API.
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicClient {
    /// Create a client from the `ANTHROPIC_API_KEY` environment variable.
    ///
    /// Also loads `.env` files from the current directory and parent
    /// directories so users can store their key in a `.env` file.
    pub fn from_env() -> Result<Self, DispatchError> {
        // Load .env file if present (silently ignore if missing)
        dotenvy::dotenv().ok();

        let api_key =
            std::env::var("ANTHROPIC_API_KEY").map_err(|_| DispatchError::MissingApiKey)?;
        Ok(Self::new(api_key))
    }

    /// Create a client with the given API key.
    ///
    /// Configures a 90-second request timeout for token-heavy requests.
    pub fn new(api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|e| {
                warn!(error = %e, "TLS client build failed, falling back to default");
                reqwest::Client::new()
            });
        Self {
            http,
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Override the base URL (useful for testing).
    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    /// Maximum number of retries for transient API errors (429, 5xx).
    const MAX_RETRIES: u32 = 3;

    /// Send a Messages API request and return the parsed response.
    ///
    /// Automatically retries on transient errors (429 Too Many Requests, 5xx)
    /// with exponential backoff.
    pub async fn send_message(
        &self,
        request: &ClaudeRequest,
    ) -> Result<MessagesResponse, DispatchError> {
        let url = format!("{}/v1/messages", self.base_url);

        let mut last_err = None;
        for attempt in 0..=Self::MAX_RETRIES {
            if attempt > 0 {
                let delay_ms = 1000 * 2u64.pow(attempt - 1); // 1s, 2s, 4s
                warn!(attempt, delay_ms, max = Self::MAX_RETRIES, "API retry");
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }

            let response = match self
                .http
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(request)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    last_err = Some(DispatchError::HttpError(e.to_string()));
                    continue;
                }
            };

            let status = response.status();
            if status.is_success() {
                let resp: MessagesResponse = response
                    .json()
                    .await
                    .map_err(|e| DispatchError::ParseError(e.to_string()))?;
                return Ok(resp);
            }

            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());

            // Retry on 429 (rate limited) or 5xx (server error)
            if status.as_u16() == 429 || status.is_server_error() {
                last_err = Some(DispatchError::ApiError {
                    status: status.as_u16(),
                    body,
                });
                continue;
            }

            // Non-retryable error
            return Err(DispatchError::ApiError {
                status: status.as_u16(),
                body,
            });
        }

        Err(last_err.unwrap_or_else(|| {
            DispatchError::HttpError("all retry attempts exhausted".to_string())
        }))
    }

    /// Send a message with streaming enabled.
    ///
    /// Returns a receiver that yields [`StreamEvent`]s as they arrive
    /// from the Anthropic SSE stream.
    pub async fn send_message_stream(
        &self,
        request: &ClaudeRequest,
    ) -> Result<mpsc::Receiver<StreamEvent>, DispatchError> {
        let (tx, rx) = mpsc::channel(100);

        let mut body = serde_json::to_value(request)
            .map_err(|e| DispatchError::ExecutionError(e.to_string()))?;
        body["stream"] = serde_json::Value::Bool(true);

        let url = format!("{}/v1/messages", self.base_url);

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| DispatchError::HttpError(format!("stream request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DispatchError::ApiError {
                status: status.as_u16(),
                body,
            });
        }

        // Spawn a task to read the SSE stream and forward parsed events
        let mut stream = response.bytes_stream();
        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Some(chunk) = stream.next().await {
                if let Ok(bytes) = chunk {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));

                    // Process complete SSE events (separated by double newlines)
                    while let Some(pos) = buffer.find("\n\n") {
                        let event_text = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        if let Some(event) = parse_sse_event(&event_text) {
                            if tx.send(event).await.is_err() {
                                return; // Receiver dropped
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

/// Parse a Server-Sent Events text block into a [`StreamEvent`].
///
/// Each SSE block contains `event:` and `data:` lines. This function
/// maps Anthropic event types to our [`StreamEvent`] variants.
pub fn parse_sse_event(text: &str) -> Option<StreamEvent> {
    let mut event_type = None;
    let mut data = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event_type = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data: ") {
            data = Some(rest.trim().to_string());
        }
    }

    let event_type = event_type?;
    let data_str = data?;
    let data: serde_json::Value = serde_json::from_str(&data_str).ok()?;

    match event_type.as_str() {
        "content_block_delta" => {
            let delta = &data["delta"];
            match delta["type"].as_str()? {
                "text_delta" => Some(StreamEvent::TextDelta(delta["text"].as_str()?.to_string())),
                "input_json_delta" => Some(StreamEvent::ToolUseInputDelta(
                    delta["partial_json"].as_str()?.to_string(),
                )),
                _ => None,
            }
        }
        "content_block_start" => {
            let content_block = &data["content_block"];
            if content_block["type"].as_str()? == "tool_use" {
                Some(StreamEvent::ToolUseStart {
                    id: content_block["id"].as_str()?.to_string(),
                    name: content_block["name"].as_str()?.to_string(),
                })
            } else {
                None
            }
        }
        "content_block_stop" => Some(StreamEvent::ContentBlockStop),
        "message_delta" => {
            let stop_reason_str = data["delta"]["stop_reason"].as_str()?;
            let stop_reason = match stop_reason_str {
                "end_turn" => StopReason::EndTurn,
                "tool_use" => StopReason::ToolUse,
                "max_tokens" => StopReason::MaxTokens,
                "stop_sequence" => StopReason::StopSequence,
                _ => return None,
            };
            Some(StreamEvent::MessageDone { stop_reason })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_requires_api_key() {
        // Test that a client can be created with a key
        let client = AnthropicClient::new("test-key".into());
        assert_eq!(client.api_key, "test-key");
    }

    #[test]
    fn client_with_custom_base_url() {
        let client = AnthropicClient::new("test-key".into()).with_base_url("http://localhost:8080");
        assert_eq!(client.base_url, "http://localhost:8080");
    }

    #[test]
    fn parse_sse_text_delta() {
        let text = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";
        let event = parse_sse_event(text).unwrap();
        match event {
            StreamEvent::TextDelta(t) => assert_eq!(t, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_sse_tool_use_start() {
        let text = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_01\",\"name\":\"search\",\"input\":{}}}";
        let event = parse_sse_event(text).unwrap();
        match event {
            StreamEvent::ToolUseStart { id, name } => {
                assert_eq!(id, "tu_01");
                assert_eq!(name, "search");
            }
            other => panic!("expected ToolUseStart, got {:?}", other),
        }
    }

    #[test]
    fn parse_sse_input_json_delta() {
        let text = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"query\\\"\"}}";
        let event = parse_sse_event(text).unwrap();
        match event {
            StreamEvent::ToolUseInputDelta(json) => assert_eq!(json, "{\"query\""),
            other => panic!("expected ToolUseInputDelta, got {:?}", other),
        }
    }

    #[test]
    fn parse_sse_content_block_stop() {
        let text = "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}";
        let event = parse_sse_event(text).unwrap();
        assert!(matches!(event, StreamEvent::ContentBlockStop));
    }

    #[test]
    fn parse_sse_message_done() {
        let text = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":15}}";
        let event = parse_sse_event(text).unwrap();
        match event {
            StreamEvent::MessageDone { stop_reason } => {
                assert_eq!(stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected MessageDone, got {:?}", other),
        }
    }

    #[test]
    fn parse_sse_message_done_tool_use() {
        let text = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":10}}";
        let event = parse_sse_event(text).unwrap();
        match event {
            StreamEvent::MessageDone { stop_reason } => {
                assert_eq!(stop_reason, StopReason::ToolUse);
            }
            other => panic!("expected MessageDone, got {:?}", other),
        }
    }

    #[test]
    fn parse_sse_unknown_event_returns_none() {
        let text = "event: ping\ndata: {}";
        assert!(parse_sse_event(text).is_none());
    }

    #[test]
    fn parse_sse_missing_data_returns_none() {
        let text = "event: content_block_delta";
        assert!(parse_sse_event(text).is_none());
    }

    #[test]
    fn parse_sse_text_block_start_returns_none() {
        // Text content_block_start should return None (only tool_use is mapped)
        let text = "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}";
        assert!(parse_sse_event(text).is_none());
    }

    #[test]
    fn stream_event_tool_executing() {
        let event = StreamEvent::ToolExecuting {
            name: "search".to_string(),
        };
        assert!(matches!(event, StreamEvent::ToolExecuting { name } if name == "search"));
    }

    #[test]
    fn stream_event_tool_result() {
        let event = StreamEvent::ToolResult {
            name: "search".to_string(),
            result: "found 5 results".to_string(),
        };
        assert!(
            matches!(event, StreamEvent::ToolResult { name, result } if name == "search" && result == "found 5 results")
        );
    }

    #[test]
    fn stream_event_error() {
        let event = StreamEvent::Error("rate limited".to_string());
        assert!(matches!(event, StreamEvent::Error(msg) if msg == "rate limited"));
    }
}
