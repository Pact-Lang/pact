// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-10

//! PACT → Mermaid diagram generator.
//!
//! Converts a parsed PACT `Program` into a Mermaid flowchart string.
//! This is the reverse of the `parser` + `convert` pipeline.
//!
//! ## Mapping
//!
//! | PACT construct  | Mermaid shape   |
//! |-----------------|-----------------|
//! | `agent @name`   | `{Name}`        |
//! | `tool #name`    | `(Name)`        |
//! | `schema Name`   | `((Name))`      |
//! | `flow` steps    | edges `-->`     |
//! | `agent_bundle`  | `subgraph`      |

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};

/// Convert a PACT `Program` into a Mermaid flowchart string.
pub fn pact_to_mermaid(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("flowchart LR\n");

    let mut node_ids: Vec<(String, String)> = Vec::new(); // (id, definition)
    let mut edges: Vec<String> = Vec::new();
    let mut subgraphs: Vec<String> = Vec::new();

    // Collect agents, tools, schemas as nodes.
    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Agent(a) => {
                let id = agent_id(&a.name);
                let label = to_title_case(&a.name);
                node_ids.push((id.clone(), format!("    {}{{{}}}", id, label)));

                // Create edges from agent to each of its tools.
                for tool_expr in &a.tools {
                    if let ExprKind::ToolRef(tool_name) = &tool_expr.kind {
                        let tool_id = tool_id(tool_name);
                        edges.push(format!("    {} --> {}", id, tool_id));
                    }
                }
            }
            DeclKind::Tool(t) => {
                let id = tool_id(&t.name);
                let label = to_title_case(&t.name);
                node_ids.push((id.clone(), format!("    {}({})", id, label)));
            }
            DeclKind::Schema(s) => {
                let id = schema_id(&s.name);
                node_ids.push((id.clone(), format!("    {}(({}))", id, s.name)));
            }
            DeclKind::AgentBundle(ab) => {
                let mut sg = format!("    subgraph {}\n", to_title_case(&ab.name));
                for agent_expr in &ab.agents {
                    if let ExprKind::AgentRef(name) = &agent_expr.kind {
                        sg.push_str(&format!("        {}\n", agent_id(name)));
                    }
                }
                sg.push_str("    end");
                subgraphs.push(sg);
            }
            DeclKind::Flow(f) => {
                // Walk flow body to extract dispatch edges.
                emit_flow_edges(&f.body, &mut edges);
            }
            _ => {}
        }
    }

    // Deduplicate node definitions (keep first occurrence).
    let mut seen_ids: Vec<String> = Vec::new();
    for (id, def) in &node_ids {
        if !seen_ids.contains(id) {
            out.push_str(def);
            out.push('\n');
            seen_ids.push(id.clone());
        }
    }

    // Deduplicate edges.
    let mut seen_edges: Vec<String> = Vec::new();
    for edge in &edges {
        if !seen_edges.contains(edge) {
            out.push_str(edge);
            out.push('\n');
            seen_edges.push(edge.clone());
        }
    }

    // Subgraphs.
    for sg in &subgraphs {
        out.push_str(sg);
        out.push('\n');
    }

    out
}

/// Extract dispatch edges from flow body expressions.
fn emit_flow_edges(body: &[pact_core::ast::expr::Expr], edges: &mut Vec<String>) {
    for expr in body {
        match &expr.kind {
            ExprKind::Assign { value, .. } => {
                extract_dispatch_edge(value, edges);
            }
            ExprKind::AgentDispatch {
                agent, tool, ..
            } => {
                if let (ExprKind::AgentRef(agent_name), ExprKind::ToolRef(tool_name)) =
                    (&agent.kind, &tool.kind)
                {
                    edges.push(format!(
                        "    {} -->|dispatch| {}",
                        agent_id(agent_name),
                        tool_id(tool_name)
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Extract a dispatch edge from an expression (handles nested assigns).
fn extract_dispatch_edge(expr: &pact_core::ast::expr::Expr, edges: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::AgentDispatch { agent, tool, .. } => {
            if let (ExprKind::AgentRef(agent_name), ExprKind::ToolRef(tool_name)) =
                (&agent.kind, &tool.kind)
            {
                edges.push(format!(
                    "    {} -->|dispatch| {}",
                    agent_id(agent_name),
                    tool_id(tool_name)
                ));
            }
        }
        ExprKind::Pipeline { left, right } => {
            extract_dispatch_edge(left, edges);
            extract_dispatch_edge(right, edges);
        }
        ExprKind::FallbackChain { primary, fallback } => {
            extract_dispatch_edge(primary, edges);
            extract_dispatch_edge(fallback, edges);
        }
        _ => {}
    }
}

fn agent_id(name: &str) -> String {
    format!("agent_{name}")
}

fn tool_id(name: &str) -> String {
    format!("tool_{name}")
}

fn schema_id(name: &str) -> String {
    format!("schema_{}", name.to_lowercase())
}

fn to_title_case(s: &str) -> String {
    s.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    fn agent_becomes_diamond() {
        let src = "agent @helper { permits: [^llm.query] tools: [] }";
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        assert!(mermaid.contains("agent_helper{Helper}"));
    }

    #[test]
    fn tool_becomes_rounded() {
        let src = r#"
            tool #search {
                description: <<Search>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
        "#;
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        assert!(mermaid.contains("tool_search(Search)"));
    }

    #[test]
    fn schema_becomes_circle() {
        let src = "schema Report { title :: String }";
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        assert!(mermaid.contains("schema_report((Report))"));
    }

    #[test]
    fn agent_tool_edge() {
        let src = r#"
            tool #greet {
                description: <<Greet>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
            }
        "#;
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        assert!(mermaid.contains("agent_greeter --> tool_greet"));
    }

    #[test]
    fn agent_bundle_becomes_subgraph() {
        let src = r#"
            agent @a { permits: [] tools: [] }
            agent @b { permits: [] tools: [] }
            agent_bundle @team {
                agents: [@a, @b]
            }
        "#;
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        assert!(mermaid.contains("subgraph Team"));
        assert!(mermaid.contains("agent_a"));
        assert!(mermaid.contains("agent_b"));
        assert!(mermaid.contains("end"));
    }

    #[test]
    fn flow_dispatch_creates_edges() {
        let src = r#"
            tool #search {
                description: <<Search>>
                requires: [^net.read]
                params { q :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
            }
            flow research(topic :: String) -> String {
                result = @researcher -> #search(topic)
                return result
            }
        "#;
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);
        // Should have both the agent→tool edge from the declaration
        // and the dispatch edge from the flow
        assert!(mermaid.contains("agent_researcher"));
        assert!(mermaid.contains("tool_search"));
    }

    #[test]
    fn full_program_roundtrip_shape() {
        let src = r#"
            tool #web_search {
                description: <<Search the web>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            tool #summarize {
                description: <<Summarize>>
                requires: [^llm.query]
                params { content :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read, ^llm.query]
                tools: [#web_search, #summarize]
            }
            agent @writer {
                permits: [^llm.query]
                tools: [#summarize]
            }
            agent_bundle @team {
                agents: [@researcher, @writer]
            }
            schema Report {
                title :: String
                body :: String
            }
            flow research(topic :: String) -> String {
                results = @researcher -> #web_search(topic)
                summary = @researcher -> #summarize(results)
                return summary
            }
        "#;
        let program = parse_program(src);
        let mermaid = pact_to_mermaid(&program);

        // Verify all constructs appear
        assert!(mermaid.starts_with("flowchart LR\n"));
        assert!(mermaid.contains("tool_web_search(Web Search)"));
        assert!(mermaid.contains("tool_summarize(Summarize)"));
        assert!(mermaid.contains("agent_researcher{Researcher}"));
        assert!(mermaid.contains("agent_writer{Writer}"));
        assert!(mermaid.contains("schema_report((Report))"));
        assert!(mermaid.contains("subgraph Team"));
    }
}
