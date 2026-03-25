// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-24

//! MCP server recommendation engine.
//!
//! Provides a curated registry mapping PACT permission categories to recommended
//! MCP servers, and functions to generate recommendations based on a program's
//! declared permissions.

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};
use std::collections::BTreeSet;

/// A recommended MCP server for a given capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRecommendation {
    /// The PACT permission capability this server satisfies (e.g. `"net.read"`).
    pub capability: &'static str,
    /// The logical server name (e.g. `"brave-search"`).
    pub server_name: &'static str,
    /// Human-readable description of what this server provides.
    pub description: &'static str,
    /// Shell command to install/run the server.
    pub install_hint: &'static str,
    /// Environment variables required by this server.
    pub requires_env: &'static [&'static str],
}

/// Curated registry of MCP servers mapped to PACT permission capabilities.
pub const MCP_REGISTRY: &[McpRecommendation] = &[
    McpRecommendation {
        capability: "net.read",
        server_name: "brave-search",
        description: "Web search via Brave Search API",
        install_hint: "npx @anthropic/mcp-server-brave",
        requires_env: &["BRAVE_API_KEY"],
    },
    McpRecommendation {
        capability: "net.read",
        server_name: "fetch",
        description: "Fetch web pages and APIs",
        install_hint: "npx @anthropic/mcp-server-fetch",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "fs.read",
        server_name: "filesystem",
        description: "Read and write files",
        install_hint: "npx @anthropic/mcp-server-filesystem",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "fs.write",
        server_name: "filesystem",
        description: "Read and write files",
        install_hint: "npx @anthropic/mcp-server-filesystem",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "db.read",
        server_name: "sqlite",
        description: "SQLite database access",
        install_hint: "npx @anthropic/mcp-server-sqlite",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "db.write",
        server_name: "sqlite",
        description: "SQLite database access",
        install_hint: "npx @anthropic/mcp-server-sqlite",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "db.read",
        server_name: "postgres",
        description: "PostgreSQL database access",
        install_hint: "npx @anthropic/mcp-server-postgres",
        requires_env: &["DATABASE_URL"],
    },
    McpRecommendation {
        capability: "db.write",
        server_name: "postgres",
        description: "PostgreSQL database access",
        install_hint: "npx @anthropic/mcp-server-postgres",
        requires_env: &["DATABASE_URL"],
    },
    McpRecommendation {
        capability: "exec.run",
        server_name: "shell",
        description: "Execute shell commands",
        install_hint: "npx @anthropic/mcp-server-shell",
        requires_env: &[],
    },
    McpRecommendation {
        capability: "email.send",
        server_name: "email",
        description: "Send emails",
        install_hint: "npx @anthropic/mcp-server-email",
        requires_env: &["EMAIL_HOST", "EMAIL_USER", "EMAIL_PASS"],
    },
    McpRecommendation {
        capability: "pay.charge",
        server_name: "stripe",
        description: "Stripe payment processing",
        install_hint: "npx @anthropic/mcp-server-stripe",
        requires_env: &["STRIPE_API_KEY"],
    },
];

/// Normalize a server name by replacing hyphens with underscores.
///
/// PACT identifiers use underscores (e.g. `brave_search`), while the MCP
/// registry uses hyphens (e.g. `brave-search`). This function maps both
/// forms to a single canonical representation for comparison.
fn normalize_server_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Recommend MCP servers based on the permissions declared in a PACT program.
///
/// This function:
/// 1. Collects all permission paths from all agents' `permits` lists
/// 2. Matches them against the [`MCP_REGISTRY`]
/// 3. Deduplicates by server name
/// 4. Excludes servers already declared in `connect` blocks
pub fn recommend_servers(program: &Program) -> Vec<&'static McpRecommendation> {
    // Collect all permission capability strings from agents.
    let mut capabilities: BTreeSet<String> = BTreeSet::new();
    for decl in &program.decls {
        if let DeclKind::Agent(agent) = &decl.kind {
            for permit in &agent.permits {
                if let ExprKind::PermissionRef(segments) = &permit.kind {
                    capabilities.insert(segments.join("."));
                }
            }
        }
    }

    // Collect server names already declared in connect blocks.
    // Normalize to a canonical form (hyphens replaced with underscores) so
    // that `brave_search` in a connect block matches `brave-search` in the registry.
    let mut connected: BTreeSet<String> = BTreeSet::new();
    for decl in &program.decls {
        if let DeclKind::Connect(connect) = &decl.kind {
            for entry in &connect.servers {
                connected.insert(normalize_server_name(&entry.name));
            }
        }
    }

    // Match capabilities against registry, deduplicate by server_name.
    let mut seen_servers: BTreeSet<&str> = BTreeSet::new();
    let mut recommendations: Vec<&'static McpRecommendation> = Vec::new();

    for rec in MCP_REGISTRY {
        let normalized = normalize_server_name(rec.server_name);
        if capabilities.contains(rec.capability)
            && !connected.contains(&normalized)
            && !seen_servers.contains(rec.server_name)
        {
            seen_servers.insert(rec.server_name);
            recommendations.push(rec);
        }
    }

    recommendations
}

/// Generate a Markdown report of MCP server recommendations for a program.
///
/// The output includes:
/// - A table of recommended servers with capabilities and install commands
/// - An environment variables section (only if any servers need env vars)
/// - A suggested `connect` block to add to the `.pact` file
///
/// Returns an empty string if no recommendations are generated.
pub fn generate_recommendations_md(program: &Program) -> String {
    let recommendations = recommend_servers(program);
    if recommendations.is_empty() {
        return String::new();
    }

    // Build a map from server_name to all capabilities it satisfies in this program.
    let mut capabilities: BTreeSet<String> = BTreeSet::new();
    for decl in &program.decls {
        if let DeclKind::Agent(agent) = &decl.kind {
            for permit in &agent.permits {
                if let ExprKind::PermissionRef(segments) = &permit.kind {
                    capabilities.insert(segments.join("."));
                }
            }
        }
    }

    // Group capabilities by server name (preserving recommendation order).
    let mut server_capabilities: Vec<(&'static str, Vec<&'static str>, &'static str)> = Vec::new();
    for rec in &recommendations {
        // Find all capabilities this server covers from the registry.
        let caps: Vec<&'static str> = MCP_REGISTRY
            .iter()
            .filter(|r| r.server_name == rec.server_name && capabilities.contains(r.capability))
            .map(|r| r.capability)
            .collect();
        server_capabilities.push((rec.server_name, caps, rec.install_hint));
    }

    let mut md = String::new();
    md.push_str("# MCP Server Recommendations\n\n");
    md.push_str("Based on the capabilities required by your PACT agents, the following MCP servers are recommended:\n\n");

    // Server table.
    md.push_str("| Server | Capability | Install Command |\n");
    md.push_str("|--------|-----------|----------------|\n");
    for (name, caps, install) in &server_capabilities {
        md.push_str(&format!(
            "| {} | {} | `{}` |\n",
            name,
            caps.join(", "),
            install
        ));
    }

    // Environment variables section (only if any are needed).
    let env_vars: Vec<(&'static str, &'static str)> = recommendations
        .iter()
        .flat_map(|rec| {
            rec.requires_env
                .iter()
                .map(move |env| (*env, rec.server_name))
        })
        .collect();

    if !env_vars.is_empty() {
        md.push_str("\n## Environment Variables\n\n");
        md.push_str("The following environment variables need to be set:\n\n");
        for (var, server) in &env_vars {
            md.push_str(&format!("- `{}` — required by {}\n", var, server));
        }
    }

    // Suggested connect block.
    md.push_str("\n## Suggested Connect Block\n\n");
    md.push_str("Add the following to your `.pact` file:\n\n");
    md.push_str("```pact\nconnect {\n");

    // Find the longest server name for alignment.
    let max_name_len = recommendations
        .iter()
        .map(|r| r.server_name.replace('-', "_").len())
        .max()
        .unwrap_or(0);

    for rec in &recommendations {
        let safe_name = rec.server_name.replace('-', "_");
        md.push_str(&format!(
            "    {:<width$}  \"stdio {}\"\n",
            safe_name,
            rec.install_hint,
            width = max_name_len
        ));
    }
    md.push_str("}\n```\n");

    md
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::ast::stmt::Program;
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
    fn recommend_net_read() {
        let src = r#"
            permit_tree {
                ^net { ^net.read }
            }
            agent @scraper {
                permits: [^net.read]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Scrape the web.>>
            }
        "#;
        let program = parse_program(src);
        let recs = recommend_servers(&program);
        let names: Vec<&str> = recs.iter().map(|r| r.server_name).collect();
        assert_eq!(names, vec!["brave-search", "fetch"]);
    }

    #[test]
    fn recommend_deduplicates() {
        let src = r#"
            permit_tree {
                ^fs { ^fs.read, ^fs.write }
            }
            agent @filer {
                permits: [^fs.read, ^fs.write]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Manage files.>>
            }
        "#;
        let program = parse_program(src);
        let recs = recommend_servers(&program);
        let names: Vec<&str> = recs.iter().map(|r| r.server_name).collect();
        assert_eq!(names, vec!["filesystem"]);
    }

    #[test]
    fn recommend_excludes_connected() {
        let src = r#"
            permit_tree {
                ^net { ^net.read }
            }
            connect {
                brave_search  "stdio npx @anthropic/mcp-server-brave"
                fetch         "stdio npx @anthropic/mcp-server-fetch"
            }
            agent @searcher {
                permits: [^net.read]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Search.>>
            }
        "#;
        let program = parse_program(src);
        let recs = recommend_servers(&program);
        // brave_search normalizes to match brave-search, fetch matches fetch.
        assert!(recs.is_empty());
    }

    #[test]
    fn recommend_empty_for_builtin_only() {
        let src = r#"
            permit_tree {
                ^llm { ^llm.query }
            }
            agent @thinker {
                permits: [^llm.query]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Think deeply.>>
            }
        "#;
        let program = parse_program(src);
        let recs = recommend_servers(&program);
        assert!(recs.is_empty());
    }

    #[test]
    fn generate_md_includes_env_vars() {
        let src = r#"
            permit_tree {
                ^net { ^net.read }
            }
            agent @searcher {
                permits: [^net.read]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Search.>>
            }
        "#;
        let program = parse_program(src);
        let md = generate_recommendations_md(&program);
        assert!(md.contains("# MCP Server Recommendations"));
        assert!(md.contains("brave-search"));
        assert!(md.contains("fetch"));
        assert!(md.contains("BRAVE_API_KEY"));
        assert!(md.contains("```pact"));
        assert!(md.contains("brave_search"));
    }

    #[test]
    fn generate_md_empty_for_no_recommendations() {
        let src = r#"
            permit_tree {
                ^llm { ^llm.query }
            }
            agent @thinker {
                permits: [^llm.query]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Think.>>
            }
        "#;
        let program = parse_program(src);
        let md = generate_recommendations_md(&program);
        assert!(md.is_empty());
    }

    #[test]
    fn generate_md_skips_env_section_when_no_env_vars() {
        let src = r#"
            permit_tree {
                ^fs { ^fs.read }
            }
            agent @reader {
                permits: [^fs.read]
                tools: []
                model: "claude-sonnet-4-20250514"
                prompt: <<Read files.>>
            }
        "#;
        let program = parse_program(src);
        let md = generate_recommendations_md(&program);
        assert!(md.contains("filesystem"));
        assert!(!md.contains("## Environment Variables"));
    }
}
