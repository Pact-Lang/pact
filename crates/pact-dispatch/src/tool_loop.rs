// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-28

//! Tool-use conversation loop.
//!
//! Implements the Anthropic tool-use protocol:
//! 1. Send user message with tool definitions → Claude responds
//! 2. If `stop_reason: tool_use` → validate, execute, feed results back
//! 3. Repeat until `stop_reason: end_turn` or max iterations reached

use pact_build::emit_claude::{build_agent_request, ClaudeMessage};
use pact_build::emit_markdown::generate_agent_prompt;
use pact_core::ast::stmt::{AgentDecl, Program};
use pact_core::interpreter::value::Value;
use serde_json::json;

use pact_core::ast::stmt::{DeclKind, TemplateEntry};

use crate::cache::{global_cache, parse_duration};
use crate::client::AnthropicClient;
use crate::convert::format_tool_call_message;
use crate::executor::{execute_handler, extract_params, parse_handler};
use crate::mediation::{find_tool_decl, MediationError, RuntimeMediator};
use crate::types::{ContentBlock, StopReason, ToolResultContent};
use crate::DispatchError;

/// Default maximum number of tool-use loop iterations.
const DEFAULT_MAX_ITERATIONS: usize = 10;

/// The tool-use conversation loop runner.
pub struct ToolUseLoop {
    client: AnthropicClient,
    max_iterations: usize,
}

impl ToolUseLoop {
    /// Create a new tool-use loop with the given client.
    pub fn new(client: AnthropicClient) -> Self {
        Self {
            client,
            max_iterations: DEFAULT_MAX_ITERATIONS,
        }
    }

    /// Set the maximum number of iterations.
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Execute a full agent dispatch through the tool-use loop.
    ///
    /// Sends the initial tool call message, handles Claude's responses,
    /// mediates compliance, and returns the final text result.
    pub async fn dispatch(
        &self,
        agent: &AgentDecl,
        program: &Program,
        tool_name: &str,
        args: &[Value],
    ) -> Result<Value, DispatchError> {
        let mediator = RuntimeMediator::new(agent, program);

        // Build the initial request using the build pipeline
        let user_message = format_tool_call_message(tool_name, args);
        let mut request = build_agent_request(agent, program, &user_message);

        // Override the system prompt with the full guardrails-enhanced version
        request.system = Some(generate_agent_prompt(agent, program));

        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > self.max_iterations {
                return Err(DispatchError::Mediation(
                    MediationError::MaxIterationsExceeded {
                        count: self.max_iterations,
                    },
                ));
            }

            println!(
                "[DISPATCH @{}] iteration {}/{}",
                agent.name, iteration, self.max_iterations
            );

            // Send request to Claude
            let response = self.client.send_message(&request).await?;

            println!(
                "[DISPATCH @{}] stop_reason: {:?}, tokens: {}+{}",
                agent.name, response.stop_reason, response.usage.input_tokens, response.usage.output_tokens
            );

            match response.stop_reason {
                StopReason::EndTurn => {
                    // Extract final text response
                    let text = response
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Validate the output through mediation
                    mediator
                        .validate_output(&text, tool_name, program)
                        .map_err(DispatchError::Mediation)?;

                    // Strict template validation if configured
                    if let Some(tool_decl) = find_tool_decl(program, tool_name) {
                        if tool_decl.validate.as_deref() == Some("strict") {
                            if let Some(template_name) = &tool_decl.output {
                                validate_output_against_template(&text, template_name, program)?;
                            }
                        }
                    }

                    println!("[DISPATCH @{}] => completed (output validated)", agent.name);
                    return Ok(Value::ToolResult(text));
                }

                StopReason::ToolUse => {
                    // Collect tool use blocks
                    let tool_uses: Vec<_> = response
                        .content
                        .iter()
                        .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                        .collect();

                    if tool_uses.is_empty() {
                        return Err(DispatchError::ProtocolError(
                            "stop_reason is tool_use but no tool_use blocks found".to_string(),
                        ));
                    }

                    // Validate each tool use through mediation
                    for tool_use in &tool_uses {
                        mediator
                            .validate_tool_use(tool_use, program)
                            .map_err(DispatchError::Mediation)?;
                    }

                    // Validate handler permissions for each tool use
                    for tool_use in &tool_uses {
                        if let ContentBlock::ToolUse { name, .. } = tool_use {
                            mediator
                                .validate_handler_permissions(name, program)
                                .map_err(DispatchError::Mediation)?;
                        }
                    }

                    // Build the assistant message with Claude's response
                    let assistant_content: Vec<serde_json::Value> = response
                        .content
                        .iter()
                        .map(|block| match block {
                            ContentBlock::Text { text } => {
                                json!({"type": "text", "text": text})
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                json!({"type": "tool_use", "id": id, "name": name, "input": input})
                            }
                        })
                        .collect();

                    request.messages.push(ClaudeMessage {
                        role: "assistant".to_string(),
                        content: json!(assistant_content),
                    });

                    // Execute tools and build result message
                    let mut tool_results: Vec<serde_json::Value> = Vec::new();

                    for tool_use in &tool_uses {
                        if let ContentBlock::ToolUse { id, name, input } = tool_use {
                            println!("[DISPATCH @{}] executing #{}({})", agent.name, name, input);

                            let result = execute_tool(name, input, program).await?;

                            let tool_result = ToolResultContent::success(id, &result);
                            tool_results.push(serde_json::to_value(&tool_result).unwrap());
                        }
                    }

                    // Add tool results as a user message
                    request.messages.push(ClaudeMessage {
                        role: "user".to_string(),
                        content: json!(tool_results),
                    });
                }

                StopReason::MaxTokens => {
                    // Extract partial text rather than failing
                    let text = response
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        eprintln!("[DISPATCH {}] warning: response truncated at max_tokens, using partial output", agent.name);
                        return Ok(Value::ToolResult(text));
                    }
                    return Err(DispatchError::MaxTokens);
                }

                StopReason::StopSequence => {
                    // Treat as end of turn
                    let text = response
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Validate the output through mediation
                    mediator
                        .validate_output(&text, tool_name, program)
                        .map_err(DispatchError::Mediation)?;

                    return Ok(Value::ToolResult(text));
                }
            }
        }
    }
}

/// Execute a tool with retry and caching support.
///
/// If the tool declares `retry: N`, failed executions are retried up to N times
/// with exponential backoff. If the tool declares `cache: "duration"`, results
/// are cached and reused for the specified time.
async fn execute_tool(
    tool_name: &str,
    input: &serde_json::Value,
    program: &Program,
) -> Result<String, DispatchError> {
    let tool_decl = find_tool_decl(program, tool_name);
    let max_retries = tool_decl.and_then(|t| t.retry).unwrap_or(0);

    // Check cache before executing
    if let Some(td) = tool_decl {
        if let Some(cache_str) = &td.cache {
            let cache_key = format!("{}:{}", tool_name, input);
            if let Some(cached) = global_cache().get(&cache_key) {
                println!("[DISPATCH] #{tool_name} cache hit");
                return Ok(cached);
            }

            // Execute with retry, then cache
            let result = execute_tool_with_retry(tool_name, input, program, max_retries).await?;
            if let Some(ttl) = parse_duration(cache_str) {
                global_cache().set(cache_key, result.clone(), ttl);
            }
            return Ok(result);
        }
    }

    // No cache — just execute with retry
    execute_tool_with_retry(tool_name, input, program, max_retries).await
}

/// Execute a tool with retry logic.
async fn execute_tool_with_retry(
    tool_name: &str,
    input: &serde_json::Value,
    program: &Program,
    max_retries: u32,
) -> Result<String, DispatchError> {
    let mut last_error = None;
    for attempt in 0..=max_retries {
        if attempt > 0 {
            println!("[DISPATCH] #{tool_name} retry {attempt}/{max_retries}");
            // Brief delay before retry with exponential backoff
            tokio::time::sleep(tokio::time::Duration::from_millis(
                500 * (attempt as u64 + 1),
            ))
            .await;
        }
        match execute_tool_once(tool_name, input, program).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
            }
        }
    }
    Err(last_error.unwrap())
}

/// Execute a tool once, using its handler if declared, or falling back to simulation.
///
/// When a tool has a `handler:` field, the handler is parsed and executed for real
/// (HTTP request, shell command, or builtin function). Otherwise, a simulated
/// result is returned so Claude can continue reasoning.
async fn execute_tool_once(
    tool_name: &str,
    input: &serde_json::Value,
    program: &Program,
) -> Result<String, DispatchError> {
    // Look up the tool declaration to check for a source or handler
    if let Some(tool_decl) = find_tool_decl(program, tool_name) {
        // Check for source-based execution first (built-in providers)
        if let Some(source) = &tool_decl.source {
            println!(
                "[DISPATCH] #{tool_name} using provider: !{}",
                source.capability
            );
            let params = extract_params(input);
            return crate::providers::execute_provider(&source.capability, &params).await;
        }

        // Fall back to handler-based execution
        if let Some(handler_str) = &tool_decl.handler {
            let spec = parse_handler(handler_str)?;
            let params = extract_params(input);
            println!("[DISPATCH] #{tool_name} has handler: {handler_str}");
            return execute_handler(&spec, &params).await;
        }
    }

    // No source or handler declared — simulate execution
    let result = json!({
        "tool": tool_name,
        "status": "simulated",
        "result": format!("Simulated result from #{} with input: {}", tool_name, input),
    });
    Ok(result.to_string())
}

/// Validate tool output against a template's expected structure.
///
/// When a tool has `validate: "strict"` and an `output:` template, this
/// checks that the output text contains all expected sections/fields.
fn validate_output_against_template(
    output: &str,
    template_name: &str,
    program: &Program,
) -> Result<(), DispatchError> {
    // Find the template declaration
    let template = program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Template(t) if t.name == template_name => Some(t),
        _ => None,
    });

    if let Some(template) = template {
        for entry in &template.entries {
            match entry {
                TemplateEntry::Field { name, .. } => {
                    if !output.contains(&format!("{}:", name)) {
                        return Err(DispatchError::ExecutionError(format!(
                            "output validation failed: missing section '{}'",
                            name
                        )));
                    }
                }
                TemplateEntry::Repeat { name, count, .. } => {
                    for i in 1..=*count {
                        let label = format!("{}_{}:", name, i);
                        if !output.contains(&label) {
                            return Err(DispatchError::ExecutionError(format!(
                                "output validation failed: missing section '{}'",
                                label
                            )));
                        }
                    }
                }
                TemplateEntry::Section { name, .. } => {
                    if !output.contains(&format!("==={}===", name)) {
                        return Err(DispatchError::ExecutionError(format!(
                            "output validation failed: missing section '==={}==='",
                            name
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Extract text content from a tool execution for conversion to Value.
pub fn extract_text_from_response(content: &[ContentBlock]) -> Value {
    let texts: Vec<&str> = content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();

    if texts.is_empty() {
        Value::Null
    } else {
        Value::ToolResult(texts.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_tool_without_handler_returns_simulated() {
        use pact_core::ast::stmt::Program;
        let program = Program { decls: vec![] };
        let result = execute_tool("search", &json!({"query": "rust"}), &program)
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tool"], "search");
        assert_eq!(parsed["status"], "simulated");
    }

    #[test]
    fn extract_text_from_mixed_content() {
        let content = vec![
            ContentBlock::Text {
                text: "Hello".to_string(),
            },
            ContentBlock::ToolUse {
                id: "tu_01".to_string(),
                name: "search".to_string(),
                input: json!({}),
            },
            ContentBlock::Text {
                text: "World".to_string(),
            },
        ];
        let result = extract_text_from_response(&content);
        assert_eq!(result, Value::ToolResult("Hello\nWorld".to_string()));
    }

    #[test]
    fn extract_text_empty() {
        let content = vec![ContentBlock::ToolUse {
            id: "tu_01".to_string(),
            name: "search".to_string(),
            input: json!({}),
        }];
        let result = extract_text_from_response(&content);
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn validate_output_field_present() {
        use pact_core::ast::stmt::{Decl, Program, TemplateDecl, TemplateEntry};
        use pact_core::ast::types::{TypeExpr, TypeExprKind};
        use pact_core::span::{SourceId, Span};

        let template = TemplateDecl {
            name: "report".into(),
            entries: vec![TemplateEntry::Field {
                name: "TITLE".into(),
                ty: TypeExpr {
                    kind: TypeExprKind::Named("String".into()),
                    span: Span::new(SourceId(0), 0, 0),
                },
                description: Some("a title".into()),
            }],
        };
        let program = Program {
            decls: vec![Decl {
                kind: DeclKind::Template(template),
                span: Span::new(SourceId(0), 0, 0),
            }],
        };
        // Output has the field
        assert!(validate_output_against_template("TITLE: My Report", "report", &program).is_ok());
        // Output missing the field
        assert!(
            validate_output_against_template("No title here", "report", &program).is_err()
        );
    }

    #[test]
    fn validate_output_section_present() {
        use pact_core::ast::stmt::{Decl, Program, TemplateDecl, TemplateEntry};
        use pact_core::span::{SourceId, Span};

        let template = TemplateDecl {
            name: "bilingual".into(),
            entries: vec![TemplateEntry::Section {
                name: "ENGLISH".into(),
                description: None,
            }],
        };
        let program = Program {
            decls: vec![Decl {
                kind: DeclKind::Template(template),
                span: Span::new(SourceId(0), 0, 0),
            }],
        };
        assert!(
            validate_output_against_template("===ENGLISH===\nHello", "bilingual", &program)
                .is_ok()
        );
        assert!(
            validate_output_against_template("No section", "bilingual", &program).is_err()
        );
    }

    #[test]
    fn validate_output_repeat_present() {
        use pact_core::ast::stmt::{Decl, Program, TemplateDecl, TemplateEntry};
        use pact_core::ast::types::{TypeExpr, TypeExprKind};
        use pact_core::span::{SourceId, Span};

        let template = TemplateDecl {
            name: "items".into(),
            entries: vec![TemplateEntry::Repeat {
                name: "ITEM".into(),
                ty: TypeExpr {
                    kind: TypeExprKind::Named("String".into()),
                    span: Span::new(SourceId(0), 0, 0),
                },
                count: 2,
                description: None,
            }],
        };
        let program = Program {
            decls: vec![Decl {
                kind: DeclKind::Template(template),
                span: Span::new(SourceId(0), 0, 0),
            }],
        };
        assert!(
            validate_output_against_template("ITEM_1: A\nITEM_2: B", "items", &program)
                .is_ok()
        );
        assert!(
            validate_output_against_template("ITEM_1: A", "items", &program).is_err()
        );
    }
}
