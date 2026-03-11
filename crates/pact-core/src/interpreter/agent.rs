// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-07-16

//! Agent dispatch backends for the PACT interpreter.
//!
//! The [`Dispatcher`] trait defines how `@agent -> #tool(args)` is executed.
//! The default [`MockDispatcher`] prints calls and returns deterministic
//! results. The `pact-dispatch` crate provides a [`ClaudeDispatcher`] that
//! calls the real Anthropic Messages API.

use super::value::Value;
use crate::ast::stmt::{AgentDecl, Program};

/// Trait for agent dispatch backends.
///
/// Implementors handle the actual execution of `@agent -> #tool(args)`
/// expressions. The interpreter calls [`Dispatcher::dispatch`] whenever
/// an agent dispatch expression is evaluated.
pub trait Dispatcher {
    /// Execute an agent dispatch.
    ///
    /// # Arguments
    ///
    /// * `agent_name` — The agent being dispatched (without `@`).
    /// * `tool_name` — The tool being called (without `#`).
    /// * `args` — The arguments passed to the tool.
    /// * `agent_decl` — The full agent declaration for this agent.
    /// * `program` — The full program AST (for tool lookups, etc.).
    fn dispatch(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        agent_decl: &AgentDecl,
        program: &Program,
    ) -> Result<Value, String>;
}

/// Mock dispatcher that prints calls and returns deterministic results.
///
/// Used by default for `pact run` (without `--dispatch claude`) and
/// for `pact test`.
pub struct MockDispatcher;

impl Dispatcher for MockDispatcher {
    fn dispatch(
        &self,
        agent_name: &str,
        tool_name: &str,
        args: &[Value],
        _agent_decl: &AgentDecl,
        _program: &Program,
    ) -> Result<Value, String> {
        Ok(mock_dispatch(agent_name, tool_name, args))
    }
}

/// Execute a mock agent dispatch, printing the call and returning a mock result.
pub fn mock_dispatch(agent_name: &str, tool_name: &str, args: &[Value]) -> Value {
    let args_str: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
    let args_display = args_str.join(", ");
    println!("[AGENT @{agent_name}] calling #{tool_name}({args_display})");

    let result = format!("{tool_name}_result");
    println!("[AGENT @{agent_name}] -> ToolResult(\"{result}\")");

    Value::ToolResult(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_dispatch_returns_tool_result() {
        let result = mock_dispatch("greeter", "greet", &[Value::String("world".into())]);
        assert_eq!(result, Value::ToolResult("greet_result".into()));
    }

    #[test]
    fn mock_dispatch_no_args() {
        let result = mock_dispatch("bot", "status", &[]);
        assert_eq!(result, Value::ToolResult("status_result".into()));
    }
}
