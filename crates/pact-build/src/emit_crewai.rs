// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! CrewAI YAML configuration generation.
//!
//! Converts PACT agent and flow declarations into CrewAI-compatible YAML
//! configuration. This emitter produces two top-level sections:
//!
//! - `agents:` — one entry per PACT agent, with role, goal, backstory, tools,
//!   and delegation settings.
//! - `tasks:` — one entry per PACT flow, mapping to a CrewAI task with
//!   description, assigned agent, and expected output.
//!
//! # CrewAI YAML Format
//!
//! ```yaml
//! agents:
//!   agent_name:
//!     role: "Agent Role"
//!     goal: "Agent goal description"
//!     backstory: "Agent backstory and capabilities"
//!     tools:
//!       - tool_name
//!     allow_delegation: false
//!
//! tasks:
//!   flow_name:
//!     description: "Flow description"
//!     agent: agent_name
//!     expected_output: "Return type"
//! ```

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};

/// Generate a complete CrewAI YAML configuration from a PACT program.
///
/// Iterates over all agent and flow declarations in the program, emitting
/// agents as CrewAI agent entries and flows as CrewAI task entries. Flows
/// are assigned to agents by scanning their body for agent dispatch
/// expressions (`@agent -> #tool`).
pub fn generate_crewai_config(program: &Program) -> String {
    let mut out = String::new();

    // --- Agents section ---
    let agents: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Agent(a) => Some(a),
            _ => None,
        })
        .collect();

    if !agents.is_empty() {
        out.push_str("agents:\n");
        for agent in &agents {
            out.push_str(&format!("  {}:\n", agent.name));

            // Role: derive from agent name, title-cased
            let role = title_case(&agent.name);
            out.push_str(&format!("    role: \"{}\"\n", role));

            // Goal: extract from model or use a sensible default
            let goal = agent
                .model
                .as_ref()
                .and_then(|e| match &e.kind {
                    ExprKind::StringLit(s) => Some(format!("Operate using {s}")),
                    _ => None,
                })
                .unwrap_or_else(|| format!("Fulfill the {} role effectively", agent.name));
            out.push_str(&format!("    goal: \"{}\"\n", yaml_escape(&goal)));

            // Backstory: use the agent prompt
            let backstory = agent
                .prompt
                .as_ref()
                .and_then(|e| match &e.kind {
                    ExprKind::PromptLit(s) | ExprKind::StringLit(s) => Some(s.trim().to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            out.push_str(&format!("    backstory: \"{}\"\n", yaml_escape(&backstory)));

            // Tools list
            let tool_names: Vec<String> = agent
                .tools
                .iter()
                .filter_map(|e| match &e.kind {
                    ExprKind::ToolRef(name) => Some(name.clone()),
                    _ => None,
                })
                .collect();

            if !tool_names.is_empty() {
                out.push_str("    tools:\n");
                for name in &tool_names {
                    out.push_str(&format!("      - {name}\n"));
                }
            }

            out.push_str("    allow_delegation: false\n");
        }
    }

    // --- Tasks section ---
    let flows: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f),
            _ => None,
        })
        .collect();

    if !flows.is_empty() {
        out.push_str("tasks:\n");
        for flow in &flows {
            out.push_str(&format!("  {}:\n", flow.name));

            // Description: derive from the flow's body dispatches
            let description = build_flow_description(flow);
            out.push_str(&format!(
                "    description: \"{}\"\n",
                yaml_escape(&description)
            ));

            // Agent: find the first agent dispatch in the flow body
            let agent_name = flow.body.iter().find_map(|expr| match &expr.kind {
                ExprKind::AgentDispatch { agent, .. } => match &agent.kind {
                    ExprKind::AgentRef(name) => Some(name.clone()),
                    _ => None,
                },
                _ => None,
            });

            if let Some(name) = &agent_name {
                out.push_str(&format!("    agent: {name}\n"));
            }

            // Expected output: use return type if present
            let expected = flow
                .return_type
                .as_ref()
                .map(format_type)
                .unwrap_or_else(|| "String".to_string());
            out.push_str(&format!("    expected_output: \"{expected}\"\n"));
        }
    }

    out
}

/// Build a human-readable description from a flow's body expressions.
fn build_flow_description(flow: &pact_core::ast::stmt::FlowDecl) -> String {
    let dispatches: Vec<String> = flow
        .body
        .iter()
        .filter_map(|expr| match &expr.kind {
            ExprKind::AgentDispatch { agent, tool, .. } => {
                let agent_name = match &agent.kind {
                    ExprKind::AgentRef(n) => n.clone(),
                    _ => "unknown".to_string(),
                };
                let tool_name = match &tool.kind {
                    ExprKind::ToolRef(n) => n.clone(),
                    _ => "unknown".to_string(),
                };
                Some(format!("dispatch @{agent_name} -> #{tool_name}"))
            }
            _ => None,
        })
        .collect();

    if dispatches.is_empty() {
        format!("Execute the {} flow", flow.name)
    } else {
        dispatches.join(", then ")
    }
}

/// Format a PACT type expression as a human-readable string.
fn format_type(ty: &pact_core::ast::types::TypeExpr) -> String {
    use pact_core::ast::types::TypeExprKind;
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Generic { name, args } => {
            let inner: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, inner.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
}

/// Convert a snake_case or lowercase name to Title Case.
fn title_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escape a string for safe inclusion in a double-quoted YAML value.
fn yaml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse_program(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn generates_agent_section() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                model: "claude-sonnet-4-20250514"
                prompt: <<You are a friendly greeter.>>
            }
        "#;
        let program = parse_program(src);
        let yaml = generate_crewai_config(&program);

        assert!(yaml.contains("agents:\n"));
        assert!(yaml.contains("  greeter:\n"));
        assert!(yaml.contains("    role: \"Greeter\"\n"));
        assert!(yaml.contains("    backstory: \"You are a friendly greeter.\"\n"));
        assert!(yaml.contains("      - greet\n"));
        assert!(yaml.contains("    allow_delegation: false\n"));
    }

    #[test]
    fn generates_task_from_flow() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                prompt: <<You are a friendly greeter.>>
            }
            flow greet_world() -> String {
                @greeter -> #greet("World")
            }
        "#;
        let program = parse_program(src);
        let yaml = generate_crewai_config(&program);

        assert!(yaml.contains("tasks:\n"));
        assert!(yaml.contains("  greet_world:\n"));
        assert!(yaml.contains("    agent: greeter\n"));
        assert!(yaml.contains("    expected_output: \"String\"\n"));
        assert!(yaml.contains("dispatch @greeter -> #greet"));
    }

    #[test]
    fn empty_program_produces_empty_output() {
        let src = "";
        let program = parse_program(src);
        let yaml = generate_crewai_config(&program);
        assert!(yaml.is_empty());
    }

    #[test]
    fn multiple_agents_and_tools() {
        let src = r#"
            tool #search {
                description: <<Search the web.>>
                requires: [^net.read]
                params { query :: String }
                returns :: List<String>
            }
            tool #summarize {
                description: <<Summarize text.>>
                requires: [^llm.query]
                params { text :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read, ^llm.query]
                tools: [#search, #summarize]
                prompt: <<You are a research assistant.>>
            }
            agent @writer {
                permits: [^llm.query]
                tools: [#summarize]
                model: "gpt-4"
                prompt: <<You are a skilled writer.>>
            }
        "#;
        let program = parse_program(src);
        let yaml = generate_crewai_config(&program);

        assert!(yaml.contains("  researcher:\n"));
        assert!(yaml.contains("  writer:\n"));
        assert!(yaml.contains("      - search\n"));
        assert!(yaml.contains("      - summarize\n"));
        assert!(yaml.contains("    goal: \"Operate using gpt-4\"\n"));
    }

    #[test]
    fn title_case_conversion() {
        assert_eq!(title_case("greeter"), "Greeter");
        assert_eq!(title_case("web_researcher"), "Web Researcher");
        assert_eq!(title_case("a_b_c"), "A B C");
    }

    #[test]
    fn yaml_escape_special_chars() {
        assert_eq!(yaml_escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(yaml_escape("line1\nline2"), "line1\\nline2");
        assert_eq!(yaml_escape("path\\to"), "path\\\\to");
    }

    #[test]
    fn flow_without_dispatch_gets_default_description() {
        let src = r#"
            flow empty_flow() -> String {
                "hello"
            }
        "#;
        let program = parse_program(src);
        let yaml = generate_crewai_config(&program);

        assert!(yaml.contains("    description: \"Execute the empty_flow flow\"\n"));
    }
}
