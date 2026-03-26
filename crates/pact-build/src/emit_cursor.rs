// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! Cursor IDE integration generation.
//!
//! Generates `.cursorrules` markdown files and `.cursor/mcp.json` configs
//! from a PACT program. Cursor uses markdown instruction files for agent
//! rules and a JSON config for MCP server integration.
//!
//! # `.cursorrules` Format
//!
//! ```markdown
//! # Agent: agent_name
//! ## Role
//! Agent prompt text
//!
//! ## Tools
//! - tool_name: Tool description
//!
//! ## Permissions
//! - permission.path
//!
//! ## Compliance
//! - Risk: high
//! - Frameworks: pci_dss, gdpr
//! ```
//!
//! # `.cursor/mcp.json` Format
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "server_name": {
//!       "command": "command",
//!       "args": ["arg1"]
//!     }
//!   }
//! }
//! ```

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};
use serde_json::json;

/// Generate `.cursorrules` markdown content from a PACT program.
///
/// Produces one section per agent, each containing the agent's role (prompt),
/// available tools with descriptions, required permissions, and compliance
/// metadata when present.
pub fn generate_cursor_rules(program: &Program) -> String {
    let mut output = String::new();

    // Collect tools for lookup by name
    let tools: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) => Some(t),
            _ => None,
        })
        .collect();

    // Collect compliance profiles for lookup by name
    let compliance_profiles: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Compliance(c) => Some(c),
            _ => None,
        })
        .collect();

    let mut first = true;
    for decl in &program.decls {
        if let DeclKind::Agent(agent) = &decl.kind {
            if !first {
                output.push('\n');
            }
            first = false;

            // Agent header
            output.push_str(&format!("# Agent: {}\n", agent.name));

            // Role section
            output.push_str("## Role\n");
            if let Some(prompt) = &agent.prompt {
                match &prompt.kind {
                    ExprKind::PromptLit(s) | ExprKind::StringLit(s) => {
                        output.push_str(s.trim());
                        output.push('\n');
                    }
                    _ => {}
                }
            }
            output.push('\n');

            // Tools section
            let tool_refs: Vec<&str> = agent
                .tools
                .iter()
                .filter_map(|e| match &e.kind {
                    ExprKind::ToolRef(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();

            if !tool_refs.is_empty() {
                output.push_str("## Tools\n");
                for tool_name in &tool_refs {
                    let description = tools
                        .iter()
                        .find(|t| t.name == *tool_name)
                        .map(|t| match &t.description.kind {
                            ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    output.push_str(&format!("- {}: {}\n", tool_name, description));
                }
                output.push('\n');
            }

            // Permissions section
            let permissions: Vec<String> = agent
                .permits
                .iter()
                .filter_map(|e| match &e.kind {
                    ExprKind::PermissionRef(segments) => Some(segments.join(".")),
                    _ => None,
                })
                .collect();

            if !permissions.is_empty() {
                output.push_str("## Permissions\n");
                for perm in &permissions {
                    output.push_str(&format!("- {}\n", perm));
                }
                output.push('\n');
            }

            // Compliance section
            if let Some(compliance_name) = &agent.compliance {
                if let Some(profile) = compliance_profiles
                    .iter()
                    .find(|c| c.name == *compliance_name)
                {
                    output.push_str("## Compliance\n");
                    if let Some(risk) = &profile.risk {
                        output.push_str(&format!("- Risk: {}\n", risk));
                    }
                    if !profile.frameworks.is_empty() {
                        output.push_str(&format!(
                            "- Frameworks: {}\n",
                            profile.frameworks.join(", ")
                        ));
                    }
                    if let Some(audit) = &profile.audit {
                        output.push_str(&format!("- Audit: {}\n", audit));
                    }
                    if let Some(retention) = &profile.retention {
                        output.push_str(&format!("- Retention: {}\n", retention));
                    }
                    output.push('\n');
                }
            }
        }
    }

    output
}

/// Generate `.cursor/mcp.json` content from a PACT program.
///
/// Converts `connect` blocks into Cursor's MCP server configuration format.
/// Only `stdio` transports are emitted since Cursor expects command-based servers.
pub fn generate_cursor_mcp_json(program: &Program) -> String {
    let mut servers = serde_json::Map::new();

    for decl in &program.decls {
        if let DeclKind::Connect(connect) = &decl.kind {
            for entry in &connect.servers {
                let transport = &entry.transport;
                if let Some(rest) = transport.strip_prefix("stdio ") {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    if let Some((command, args)) = parts.split_first() {
                        let args_json: Vec<serde_json::Value> =
                            args.iter().map(|a| json!(a)).collect();
                        servers.insert(
                            entry.name.clone(),
                            json!({
                                "command": command,
                                "args": args_json,
                            }),
                        );
                    }
                } else if let Some(url) = transport.strip_prefix("sse ") {
                    servers.insert(
                        entry.name.clone(),
                        json!({
                            "url": url.trim(),
                        }),
                    );
                }
            }
        }
    }

    let config = json!({
        "mcpServers": servers,
    });

    serde_json::to_string_pretty(&config).expect("JSON serialization should not fail")
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
    fn cursor_rules_basic_agent() {
        let src = r#"
            tool #greet {
                description: <<Generate a greeting message.>>
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
        let rules = generate_cursor_rules(&program);

        assert!(rules.contains("# Agent: greeter"));
        assert!(rules.contains("## Role"));
        assert!(rules.contains("You are a friendly greeter."));
        assert!(rules.contains("## Tools"));
        assert!(rules.contains("- greet: Generate a greeting message."));
        assert!(rules.contains("## Permissions"));
        assert!(rules.contains("- llm.query"));
    }

    #[test]
    fn cursor_rules_with_compliance() {
        let src = r#"
            compliance "payment_processing" {
                risk: high
                frameworks: [pci_dss, gdpr]
                audit: full
            }
            tool #process_payment {
                description: <<Process a payment.>>
                requires: [^net.write]
                params { amount :: Float }
                returns :: String
            }
            agent @payment_agent {
                permits: [^net.write]
                tools: [#process_payment]
                prompt: <<Handle payments securely.>>
                compliance: "payment_processing"
            }
        "#;
        let program = parse_program(src);
        let rules = generate_cursor_rules(&program);

        assert!(rules.contains("## Compliance"));
        assert!(rules.contains("- Risk: high"));
        assert!(rules.contains("- Frameworks: pci_dss, gdpr"));
        assert!(rules.contains("- Audit: full"));
    }

    #[test]
    fn cursor_mcp_json_stdio_transport() {
        let src = r#"
            connect {
                slack "stdio npx -y @anthropic/slack-mcp"
                github "stdio gh mcp serve"
            }
        "#;
        let program = parse_program(src);
        let json_str = generate_cursor_mcp_json(&program);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        let servers = &parsed["mcpServers"];
        assert_eq!(servers["slack"]["command"], "npx");
        assert_eq!(servers["slack"]["args"][0], "-y");
        assert_eq!(servers["slack"]["args"][1], "@anthropic/slack-mcp");
        assert_eq!(servers["github"]["command"], "gh");
        assert_eq!(servers["github"]["args"][0], "mcp");
        assert_eq!(servers["github"]["args"][1], "serve");
    }

    #[test]
    fn cursor_mcp_json_empty_connect() {
        let program = Program { decls: vec![] };
        let json_str = generate_cursor_mcp_json(&program);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["mcpServers"].as_object().unwrap().is_empty());
    }

    #[test]
    fn cursor_rules_multiple_agents() {
        let src = r#"
            tool #read_file {
                description: <<Read a file.>>
                requires: [^fs.read]
                params { path :: String }
                returns :: String
            }
            tool #write_file {
                description: <<Write a file.>>
                requires: [^fs.write]
                params { path :: String content :: String }
                returns :: String
            }
            agent @reader {
                permits: [^fs.read]
                tools: [#read_file]
                prompt: <<You read files.>>
            }
            agent @writer {
                permits: [^fs.write]
                tools: [#write_file]
                prompt: <<You write files.>>
            }
        "#;
        let program = parse_program(src);
        let rules = generate_cursor_rules(&program);

        assert!(rules.contains("# Agent: reader"));
        assert!(rules.contains("# Agent: writer"));
        assert!(rules.contains("- read_file: Read a file."));
        assert!(rules.contains("- write_file: Write a file."));
    }
}
