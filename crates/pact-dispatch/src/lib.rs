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

/// Structured audit logging for tool calls and agent execution.
pub mod audit;
/// Response caching for API calls.
pub mod cache;
/// Low-level Anthropic HTTP client.
pub mod client;
/// Conversion between PACT AST types and API request formats.
pub mod convert;
/// Tool handler execution (HTTP, shell, builtin).
pub mod executor;
/// Federated agent dispatcher for cross-network agent dispatch.
pub mod federated;
/// MCP client for connecting to external MCP servers.
pub mod mcp_client;
/// Runtime compliance mediation and permission enforcement.
pub mod mediation;
/// Observation capture for agent memory (SQLite-backed).
pub mod observation_store;
/// Ollama local model dispatcher.
pub mod ollama;
/// OpenAI Chat Completions dispatcher.
pub mod openai;
/// Built-in capability provider registry.
pub mod providers;
/// Per-agent and per-flow rate limiting.
pub mod rate_limit;
/// TF-IDF semantic search over observations.
pub mod search;
/// Session summarization for agent memory.
pub mod summarizer;
/// Multi-turn tool-use conversation loop.
pub mod tool_loop;
/// Anthropic Messages API request and response types.
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;

use client::AnthropicClient;
pub use client::StreamEvent;
pub use federated::FederatedDispatcher;
use futures_util::future::BoxFuture;
pub use ollama::OllamaDispatcher;
pub use openai::OpenAIDispatcher;
use pact_core::ast::stmt::{AgentDecl, Program};
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Dispatcher;
pub use rate_limit::{RateLimitConfig, RateLimiter};
use tool_loop::ToolUseLoop;

/// Pluggable runner for `connector X.action` handlers.
///
/// The dispatcher itself does not know about specific external services
/// (GitHub, Slack, etc.) — it delegates execution to an implementor of
/// this trait. The host (e.g. `pact-server`) supplies a runner with the
/// user's credentials at construction time.
pub trait ConnectorRunner: Send + Sync {
    /// Execute a connector operation.
    ///
    /// # Arguments
    /// * `operation` — `<connector>.<action>` (e.g. `github.push_file`).
    /// * `params` — tool parameters as string key/value pairs.
    fn execute<'a>(
        &'a self,
        operation: &'a str,
        params: HashMap<String, String>,
    ) -> BoxFuture<'a, Result<String, String>>;
}

use thiserror::Error;
use tracing::info;

/// Errors during dispatch.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// The required API key environment variable is not set.
    #[error("required API key environment variable not set")]
    MissingApiKey,

    /// A required environment variable is not set.
    #[error("environment variable '{0}' not set")]
    MissingEnvVar(String),

    /// An HTTP transport error occurred.
    #[error("HTTP error: {0}")]
    HttpError(String),

    /// The API returned a non-success status code.
    #[error("API error (status {status}): {body}")]
    ApiError {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },

    /// The API response could not be deserialized.
    #[error("failed to parse API response: {0}")]
    ParseError(String),

    /// The response was truncated because the token limit was reached.
    #[error("response exceeded max tokens")]
    MaxTokens,

    /// A protocol-level error in the API interaction.
    #[error("protocol error: {0}")]
    ProtocolError(String),

    /// A runtime mediation check failed.
    #[error("{0}")]
    Mediation(mediation::MediationError),

    /// A tool handler failed during execution.
    #[error("tool execution error: {0}")]
    ExecutionError(String),

    /// A rate limit was exceeded.
    #[error("{0}")]
    RateLimit(rate_limit::RateLimitError),
}

/// Claude API dispatcher implementing the [`Dispatcher`] trait.
///
/// Bridges the sync interpreter with the async HTTP client by
/// creating a tokio runtime for blocking dispatch calls.
pub struct ClaudeDispatcher {
    /// The tool-use conversation loop that drives multi-turn interactions.
    tool_loop: ToolUseLoop,
    /// Tokio runtime used to block on async HTTP calls from the sync dispatcher.
    runtime: tokio::runtime::Runtime,
    /// Optional rate limiter shared across the dispatch lifecycle.
    rate_limiter: Option<Arc<RateLimiter>>,
}

impl ClaudeDispatcher {
    /// Create a dispatcher from the `ANTHROPIC_API_KEY` environment variable.
    pub fn from_env() -> Result<Self, DispatchError> {
        let client = AnthropicClient::from_env()?;
        let runtime =
            tokio::runtime::Runtime::new().map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            tool_loop: ToolUseLoop::new(client),
            runtime,
            rate_limiter: None,
        })
    }

    /// Create a dispatcher with a custom client.
    pub fn with_client(client: AnthropicClient) -> Result<Self, DispatchError> {
        let runtime =
            tokio::runtime::Runtime::new().map_err(|e| DispatchError::HttpError(e.to_string()))?;
        Ok(Self {
            tool_loop: ToolUseLoop::new(client),
            runtime,
            rate_limiter: None,
        })
    }

    /// Set the maximum number of tool-use loop iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.tool_loop = self.tool_loop.with_max_iterations(max);
        self
    }

    /// Set the project context (the user's original prompt).
    ///
    /// This context flows into tool simulations so that external integration
    /// stubs produce output relevant to the user's actual project.
    pub fn set_project_context(&mut self, context: String) {
        self.tool_loop.set_project_context(context);
    }

    /// Configure rate limiting for this dispatcher.
    pub fn with_rate_limits(mut self, config: RateLimitConfig) -> Self {
        let limiter = Arc::new(RateLimiter::new(config));
        self.tool_loop = self.tool_loop.with_rate_limiter(Arc::clone(&limiter));
        self.rate_limiter = Some(limiter);
        self
    }

    /// Attach a connector runner that handles `connector X.action` handlers.
    ///
    /// Without a runner, any tool whose handler starts with `connector ` will
    /// fail with an explicit `ExecutionError`.
    pub fn with_connector_runner(mut self, runner: Arc<dyn ConnectorRunner>) -> Self {
        self.tool_loop = self.tool_loop.with_connector_runner(runner);
        self
    }

    /// Dispatch with streaming, forwarding events through the channel.
    ///
    /// This runs the full tool-use loop with streaming enabled,
    /// allowing callers to receive text deltas and tool execution
    /// events in real-time.
    pub fn dispatch_stream(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        agent_decl: &AgentDecl,
        program: &Program,
        tx: tokio::sync::mpsc::Sender<client::StreamEvent>,
    ) -> Result<Value, String> {
        info!(agent = agent_name, tool = tool_name, "dispatching (stream)");

        self.runtime
            .block_on(
                self.tool_loop
                    .dispatch_stream(agent_decl, program, tool_name, args, tx),
            )
            .map_err(|e| e.to_string())
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
        info!(agent = agent_name, tool = tool_name, "dispatching");

        self.runtime
            .block_on(
                self.tool_loop
                    .dispatch(agent_decl, program, tool_name, args),
            )
            .map_err(|e| e.to_string())
    }
}
