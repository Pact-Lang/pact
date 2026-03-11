// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-01

//! Real API dispatch for the PACT language runtime.
//!
//! This crate provides multiple dispatcher backends, each implementing
//! [`pact_core::interpreter::Dispatcher`]:
//!
//! - [`ClaudeDispatcher`] — Anthropic Messages API (with tool-use loop)
//! - [`OpenAIDispatcher`] — OpenAI Chat Completions API
//! - [`OllamaDispatcher`] — Local Ollama instance
//!
//! ## Usage
//!
//! ```no_run
//! use pact_dispatch::{ClaudeDispatcher, OpenAIDispatcher, OllamaDispatcher};
//! use pact_core::interpreter::Interpreter;
//!
//! // Anthropic Claude
//! let dispatcher = ClaudeDispatcher::from_env().unwrap();
//! let mut interp = Interpreter::with_dispatcher(Box::new(dispatcher));
//!
//! // OpenAI
//! let dispatcher = OpenAIDispatcher::from_env().unwrap();
//! let mut interp = Interpreter::with_dispatcher(Box::new(dispatcher));
//!
//! // Ollama (local)
//! let dispatcher = OllamaDispatcher::from_env().unwrap();
//! let mut interp = Interpreter::with_dispatcher(Box::new(dispatcher));
//! ```
//!
//! ## Architecture
//!
//! ```text
//! ClaudeDispatcher (implements Dispatcher trait)
//!   └── ToolUseLoop (conversation loop)
//!         ├── AnthropicClient (HTTP)
//!         └── RuntimeMediator (compliance checks)
//!
//! OpenAIDispatcher (implements Dispatcher trait)
//!   └── reqwest::Client → OpenAI Chat Completions API
//!
//! OllamaDispatcher (implements Dispatcher trait)
//!   └── reqwest::Client → Ollama /api/generate
//! ```

pub mod cache;
pub mod client;
pub mod convert;
pub mod executor;
pub mod mediation;
pub mod ollama;
pub mod openai;
pub mod providers;
pub mod tool_loop;
pub mod types;

use client::AnthropicClient;
pub use client::StreamEvent;
pub use ollama::OllamaDispatcher;
pub use openai::OpenAIDispatcher;
use pact_core::ast::stmt::{AgentDecl, Program};
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Dispatcher;
use tool_loop::ToolUseLoop;

use thiserror::Error;

/// Errors during dispatch.
#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("required API key environment variable not set")]
    MissingApiKey,

    #[error("environment variable '{0}' not set")]
    MissingEnvVar(String),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("API error (status {status}): {body}")]
    ApiError { status: u16, body: String },

    #[error("failed to parse API response: {0}")]
    ParseError(String),

    #[error("response exceeded max tokens")]
    MaxTokens,

    #[error("protocol error: {0}")]
    ProtocolError(String),

    #[error("{0}")]
    Mediation(mediation::MediationError),

    #[error("tool execution error: {0}")]
    ExecutionError(String),
}

/// Claude API dispatcher implementing the [`Dispatcher`] trait.
///
/// Bridges the sync interpreter with the async HTTP client by
/// creating a tokio runtime for blocking dispatch calls.
pub struct ClaudeDispatcher {
    tool_loop: ToolUseLoop,
    runtime: tokio::runtime::Runtime,
}

impl ClaudeDispatcher {
    /// Create a dispatcher from the `ANTHROPIC_API_KEY` environment variable.
    pub fn from_env() -> Result<Self, DispatchError> {
        let client = AnthropicClient::from_env()?;
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            tool_loop: ToolUseLoop::new(client),
            runtime,
        })
    }

    /// Create a dispatcher with a custom client.
    pub fn with_client(client: AnthropicClient) -> Result<Self, DispatchError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            tool_loop: ToolUseLoop::new(client),
            runtime,
        })
    }

    /// Set the maximum number of tool-use loop iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.tool_loop = self.tool_loop.with_max_iterations(max);
        self
    }
}

impl Dispatcher for ClaudeDispatcher {
    fn dispatch(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        agent_decl: &AgentDecl,
        program: &Program,
    ) -> Result<Value, String> {
        println!("[CLAUDE] @{agent_name} -> #{tool_name}");

        self.runtime
            .block_on(
                self.tool_loop
                    .dispatch(agent_decl, program, tool_name, args),
            )
            .map_err(|e| e.to_string())
    }
}
