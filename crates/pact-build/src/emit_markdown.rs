// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-09-25

//! Markdown prompt file generation for agents.
//!
//! Each agent gets a `.prompt.md` file containing its system prompt,
//! available tools, and permission summary. This file is designed to be
//! consumed directly as a system prompt by LLM APIs.

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{AgentDecl, DeclKind, DirectiveDecl, Program, SkillDecl, TemplateDecl, ToolDecl};
use pact_core::template::render_template;

use crate::guardrails;

/// Generate the Markdown prompt file for an agent.
///
/// The output includes:
/// - The agent's prompt (from the `prompt:` field)
/// - A summary of available tools with descriptions
/// - A permission summary
pub fn generate_agent_prompt(agent: &AgentDecl, program: &Program) -> String {
    let mut md = String::new();

    // Header
    md.push_str(&format!("# Agent: {}\n\n", agent.name));

    // System prompt
    if let Some(prompt_expr) = &agent.prompt {
        match &prompt_expr.kind {
            ExprKind::PromptLit(s) | ExprKind::StringLit(s) => {
                md.push_str(s.trim());
                md.push_str("\n\n");
            }
            _ => {}
        }
    }

    // Available tools section
    let tool_names: Vec<&str> = agent
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    if !tool_names.is_empty() {
        md.push_str("## Available Tools\n\n");
        for tool_name in &tool_names {
            // Look up tool declaration for description
            if let Some(tool_decl) = find_tool_decl(program, tool_name) {
                let description = extract_description(&tool_decl.description);
                md.push_str(&format!("- **{}**: {}\n", tool_name, description));

                // Show parameters
                if !tool_decl.params.is_empty() {
                    for param in &tool_decl.params {
                        let ty = param
                            .ty
                            .as_ref()
                            .map(format_type)
                            .unwrap_or_else(|| "Any".to_string());
                        md.push_str(&format!("  - `{}` ({})\n", param.name, ty));
                    }
                }

                // Append output template format if specified
                if let Some(template_name) = &tool_decl.output {
                    if let Some(template) = find_template_decl(program, template_name) {
                        md.push('\n');
                        md.push_str(&render_template(template));
                        md.push('\n');
                    }
                }

                // Append directive prompt blocks if specified
                if !tool_decl.directives.is_empty() {
                    let directive_texts: Vec<String> = tool_decl
                        .directives
                        .iter()
                        .filter_map(|name| find_directive_decl(program, name))
                        .map(pact_core::template::render_directive)
                        .collect();
                    if !directive_texts.is_empty() {
                        md.push('\n');
                        md.push_str(&directive_texts.join("\n\n"));
                        md.push('\n');
                    }
                }
            } else {
                md.push_str(&format!("- **{}**\n", tool_name));
            }
        }
        md.push('\n');
    }

    // Skills section — merge skill strategies and tool docs into the prompt
    let skill_names: Vec<&str> = agent
        .skills
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::SkillRef(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    if !skill_names.is_empty() {
        md.push_str("## Skills\n\n");
        for skill_name in &skill_names {
            if let Some(skill_decl) = find_skill_decl(program, skill_name) {
                let description = extract_description(&skill_decl.description);
                md.push_str(&format!("### ${}: {}\n\n", skill_name, description));

                // Strategy (the key value — detailed instructions)
                if let Some(strategy_expr) = &skill_decl.strategy {
                    let strategy = extract_description(strategy_expr);
                    if !strategy.is_empty() {
                        md.push_str("**Strategy:**\n\n");
                        md.push_str(strategy.trim());
                        md.push_str("\n\n");
                    }
                }

                // Skill tools
                let skill_tools: Vec<&str> = skill_decl
                    .tools
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ExprKind::ToolRef(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .collect();
                if !skill_tools.is_empty() {
                    md.push_str("**Tools for this skill:**\n\n");
                    for st in &skill_tools {
                        if let Some(tool_decl) = find_tool_decl(program, st) {
                            let desc = extract_description(&tool_decl.description);
                            md.push_str(&format!("- **#{}**: {}\n", st, desc));
                        } else {
                            md.push_str(&format!("- **#{}**\n", st));
                        }
                    }
                    md.push('\n');
                }

                // Skill parameters
                if !skill_decl.params.is_empty() {
                    md.push_str("**Parameters:**\n\n");
                    for param in &skill_decl.params {
                        let ty = param
                            .ty
                            .as_ref()
                            .map(format_type)
                            .unwrap_or_else(|| "Any".to_string());
                        md.push_str(&format!("- `{}` ({})\n", param.name, ty));
                    }
                    md.push('\n');
                }
            } else {
                md.push_str(&format!("### ${}\n\n", skill_name));
            }
        }
    }

    // Permissions section
    let permissions: Vec<String> = agent
        .permits
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(segs) => Some(segs.join(".")),
            _ => None,
        })
        .collect();

    if !permissions.is_empty() {
        md.push_str("## Permissions\n\n");
        md.push_str("This agent has the following permissions:\n\n");
        for perm in &permissions {
            md.push_str(&format!("- `{}`\n", perm));
        }
        md.push('\n');
    }

    // Auto-generated guardrails (security, compliance, boundaries)
    md.push_str(&guardrails::generate_guardrails(agent, program));

    md
}

/// Find a template declaration by name in the program.
fn find_template_decl<'a>(program: &'a Program, name: &str) -> Option<&'a TemplateDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Template(t) if t.name == name => Some(t),
        _ => None,
    })
}

/// Find a directive declaration by name in the program.
fn find_directive_decl<'a>(program: &'a Program, name: &str) -> Option<&'a DirectiveDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Directive(dir) if dir.name == name => Some(dir),
        _ => None,
    })
}

/// Find a tool declaration by name in the program.
fn find_tool_decl<'a>(program: &'a Program, name: &str) -> Option<&'a ToolDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Tool(t) if t.name == name => Some(t),
        _ => None,
    })
}

/// Find a skill declaration by name in the program.
fn find_skill_decl<'a>(program: &'a Program, name: &str) -> Option<&'a SkillDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Skill(s) if s.name == name => Some(s),
        _ => None,
    })
}

/// Extract the description text from a description expression.
fn extract_description(expr: &pact_core::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// Format a type expression for display in Markdown.
fn format_type(ty: &pact_core::ast::types::TypeExpr) -> String {
    use pact_core::ast::types::TypeExprKind;
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
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
    fn prompt_with_tools_and_permissions() {
        let src = r#"
            tool #web_search {
                description: <<Search the web for information.>>
                requires: [^net.read]
                params { query :: String }
                returns :: List<String>
            }
            tool #summarize {
                description: <<Summarize content into a paragraph.>>
                requires: [^llm.query]
                params { content :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read, ^llm.query]
                tools: [#web_search, #summarize]
                model: "claude-sonnet-4-20250514"
                prompt: <<You are a thorough research assistant.>>
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[2].kind {
            let md = generate_agent_prompt(agent, &program);
            assert!(md.contains("# Agent: researcher"));
            assert!(md.contains("You are a thorough research assistant."));
            assert!(md.contains("**web_search**: Search the web"));
            assert!(md.contains("**summarize**: Summarize content"));
            assert!(md.contains("`query` (String)"));
            assert!(md.contains("`net.read`"));
            assert!(md.contains("`llm.query`"));
        }
    }

    #[test]
    fn prompt_without_tool_decls() {
        let src = r#"agent @simple {
            permits: [^llm.query]
            tools: [#greet]
            prompt: <<Be helpful.>>
        }"#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[0].kind {
            let md = generate_agent_prompt(agent, &program);
            assert!(md.contains("# Agent: simple"));
            assert!(md.contains("Be helpful."));
            assert!(md.contains("**greet**")); // No description available
        }
    }

    #[test]
    fn prompt_no_prompt_field() {
        let src = "agent @bare { permits: [] tools: [] }";
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[0].kind {
            let md = generate_agent_prompt(agent, &program);
            assert!(md.contains("# Agent: bare"));
            // No crash, just no prompt section
        }
    }
}
