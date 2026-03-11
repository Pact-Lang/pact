// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-05

//! Anthropic Messages API request and response types.
//!
//! These types match the Anthropic Messages API format for both
//! serialization (requests) and deserialization (responses).
//! Request types reuse [`pact_build::emit_claude`] where possible.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Response Types ──────────────────────────────────────────────

/// Top-level response from the Anthropic Messages API.
#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub id: String,
    pub model: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: Usage,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
}

/// A content block in the response — either text or a tool use request.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: JsonValue,
    },
}

/// Token usage information.
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

// ── Tool Result (for feeding results back) ──────────────────────

/// A tool result to send back to Claude after executing a tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResultContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolResultContent {
    /// Create a successful tool result.
    pub fn success(tool_use_id: &str, content: &str) -> Self {
        Self {
            content_type: "tool_result".to_string(),
            tool_use_id: tool_use_id.to_string(),
            content: content.to_string(),
            is_error: None,
        }
    }

    /// Create an error tool result.
    pub fn error(tool_use_id: &str, error_msg: &str) -> Self {
        Self {
            content_type: "tool_result".to_string(),
            tool_use_id: tool_use_id.to_string(),
            content: error_msg.to_string(),
            is_error: Some(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_text_response() {
        let json = r#"{
            "id": "msg_01",
            "model": "claude-sonnet-4-20250514",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello, world!"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        }"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.stop_reason, StopReason::EndTurn);
        assert_eq!(resp.content.len(), 1);
        match &resp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello, world!"),
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn deserialize_tool_use_response() {
        let json = r#"{
            "id": "msg_02",
            "model": "claude-sonnet-4-20250514",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "I'll search for that."},
                {"type": "tool_use", "id": "tu_01", "name": "web_search", "input": {"query": "rust lang"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 15}
        }"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.stop_reason, StopReason::ToolUse);
        assert_eq!(resp.content.len(), 2);
        match &resp.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "tu_01");
                assert_eq!(name, "web_search");
                assert_eq!(input["query"], "rust lang");
            }
            _ => panic!("expected tool_use block"),
        }
    }

    #[test]
    fn tool_result_success() {
        let result = ToolResultContent::success("tu_01", "search results here");
        assert_eq!(result.content_type, "tool_result");
        assert_eq!(result.tool_use_id, "tu_01");
        assert!(result.is_error.is_none());
    }

    #[test]
    fn tool_result_error() {
        let result = ToolResultContent::error("tu_02", "permission denied");
        assert_eq!(result.is_error, Some(true));
    }
}
