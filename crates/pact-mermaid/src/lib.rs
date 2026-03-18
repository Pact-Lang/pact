// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-10

//! Mermaid and agentflow diagram parsing, conversion, and emission for PACT.

/// Agentflow graph data structures.
pub mod agentflow;
/// Agentflow graph to PACT source conversion.
pub mod agentflow_convert;
/// PACT AST to agentflow diagram emission.
pub mod agentflow_emit;
/// Agentflow JSON serialization and deserialization.
pub mod agentflow_json;
/// Agentflow text format parser.
pub mod agentflow_parse;
/// Mermaid flowchart graph to PACT source conversion.
pub mod convert;
/// Mermaid flowchart diagram parser.
pub mod parser;

// ── Flowchart inbound API (from-mermaid) ───────────────────────────────────

pub use convert::graph_to_pact;
pub use parser::{parse_mermaid, MermaidError, MermaidGraph};

/// Parse a Mermaid flowchart diagram and generate PACT source.
pub fn mermaid_to_pact(input: &str) -> Result<String, MermaidError> {
    let graph = parse_mermaid(input)?;
    Ok(graph_to_pact(&graph))
}

// ── Agentflow API ──────────────────────────────────────────────────────────

pub use agentflow::AgentFlowGraph;
pub use agentflow_convert::agentflow_graph_to_pact;

/// Parse agentflow text and generate PACT source.
pub fn agentflow_to_pact(input: &str) -> Result<String, MermaidError> {
    let graph = agentflow_parse::parse_agentflow_text(input)?;
    Ok(agentflow_graph_to_pact(&graph))
}

/// Parse agentflow JSON and generate PACT source.
pub fn agentflow_json_to_pact(json: &str) -> Result<String, MermaidError> {
    let graph = agentflow_json::parse_agentflow_json(json)?;
    Ok(agentflow_graph_to_pact(&graph))
}

/// Convert a PACT `Program` into agentflow text.
pub fn pact_to_agentflow_text(program: &pact_core::ast::stmt::Program) -> String {
    agentflow_emit::pact_to_agentflow(program)
}

/// Convert a PACT `Program` into agentflow JSON.
pub fn pact_to_agentflow_json_value(
    program: &pact_core::ast::stmt::Program,
) -> serde_json::Value {
    agentflow_emit::pact_to_agentflow_json(program)
}

// ── Unified API ────────────────────────────────────────────────────────────

/// Auto-detect diagram type and generate PACT source.
///
/// Dispatches to `agentflow_to_pact` if the input starts with `agentflow`,
/// otherwise falls back to `mermaid_to_pact` (flowchart).
pub fn diagram_to_pact(input: &str) -> Result<String, MermaidError> {
    let trimmed = input.trim_start();
    if trimmed.starts_with("agentflow") {
        agentflow_to_pact(input)
    } else if trimmed.starts_with('{') {
        // Likely JSON — try agentflow JSON.
        agentflow_json_to_pact(input)
    } else {
        mermaid_to_pact(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_mermaid_to_pact() {
        let input = r#"
flowchart LR
    A(Search Web) -->|results| B{Researcher}
    B -->|summary| C(Summarize)
    C -->|report| D{Writer}
    D --> E(Draft Report)
"#;
        let pact = mermaid_to_pact(input).unwrap();

        // Should contain tool declarations for rounded nodes.
        assert!(pact.contains("tool #search_web"));
        assert!(pact.contains("tool #summarize"));
        assert!(pact.contains("tool #draft_report"));

        // Should contain agent declarations for diamond nodes.
        assert!(pact.contains("agent @researcher"));
        assert!(pact.contains("agent @writer"));

        // Should contain a flow.
        assert!(pact.contains("flow main(input :: String)"));

        // Should contain the permit_tree block.
        assert!(pact.contains("permit_tree"));
        assert!(pact.contains("^llm.query"));

        // Should have the header comment.
        assert!(pact.contains("Auto-generated from Mermaid"));
    }

    #[test]
    fn end_to_end_agentflow_to_pact() {
        let input = r#"
agentflow LR
    subgraph researcher["@researcher"]
        direction LR

        search["Search Web"]@{
            description: "Search the web for information"
            requires: ["^net.read"]
            params:
                query: "String"
            returns: "String"
        }
    end

    subgraph writer["@writer"]
        summarize["Summarize"]@{
            description: "Summarize search results"
            requires: ["^llm.query"]
            params:
                content: "String"
            returns: "String"
        }
    end

    search --> summarize
"#;
        let pact = agentflow_to_pact(input).unwrap();

        assert!(pact.contains("tool #search"));
        assert!(pact.contains("tool #summarize"));
        assert!(pact.contains("agent @researcher"));
        assert!(pact.contains("agent @writer"));
        assert!(pact.contains("flow main"));
        assert!(pact.contains("permit_tree"));
    }

    #[test]
    fn diagram_to_pact_dispatches_flowchart() {
        let input = r#"
flowchart LR
    A(Tool) --> B{Agent}
"#;
        let pact = diagram_to_pact(input).unwrap();
        assert!(pact.contains("tool #tool"));
        assert!(pact.contains("agent @agent"));
    }

    #[test]
    fn diagram_to_pact_dispatches_agentflow() {
        let input = r#"
agentflow LR
    subgraph researcher["@researcher"]
        search["Search"]@{
            description: "Search"
            returns: "String"
        }
    end
"#;
        let pact = diagram_to_pact(input).unwrap();
        assert!(pact.contains("tool #search"));
        assert!(pact.contains("agent @researcher"));
    }

    #[test]
    fn diagram_to_pact_dispatches_json() {
        let json = r#"{
            "type": "agentflow",
            "direction": "LR",
            "agents": [{
                "id": "researcher",
                "label": "@researcher",
                "nodes": [{
                    "id": "search",
                    "label": "Search",
                    "shape": "roundedRect",
                    "metadata": {
                        "description": "Search the web"
                    }
                }]
            }],
            "edges": []
        }"#;
        let pact = diagram_to_pact(json).unwrap();
        assert!(pact.contains("tool #search"));
        assert!(pact.contains("agent @researcher"));
    }

    #[test]
    fn agentflow_json_roundtrip() {
        let input = r#"
agentflow LR
    SiteConfig{{"SiteConfig"}}@{
        fields:
            name: "String"
    }

    subgraph researcher["@researcher"]
        search["Search"]@{
            description: "Search the web"
            requires: ["^net.read"]
            params:
                query: "String"
            returns: "String"
        }
    end

    search --> done
"#;
        // Parse text → graph.
        let graph1 = agentflow_parse::parse_agentflow_text(input).unwrap();

        // Graph → JSON string → parse JSON → graph.
        let json_str = agentflow_json::agentflow_to_json_string(&graph1);
        let graph2 = agentflow_json::parse_agentflow_json(&json_str).unwrap();

        // Compare key structures.
        assert_eq!(graph1.schemas.len(), graph2.schemas.len());
        assert_eq!(graph1.agents.len(), graph2.agents.len());
        assert_eq!(graph1.edges.len(), graph2.edges.len());
        assert_eq!(
            graph1.agents[0].nodes[0].metadata.description,
            graph2.agents[0].nodes[0].metadata.description
        );
    }
}
