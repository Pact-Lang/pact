// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! JSON serialization/deserialization for `AgentFlowGraph`.
//!
//! Serde does the heavy lifting — this module provides convenience wrappers
//! and validates the `"type": "agentflow"` field.

use crate::agentflow::AgentFlowGraph;
use crate::parser::MermaidError;

/// Parse a JSON string into an `AgentFlowGraph`.
///
/// Validates that `"type"` is `"agentflow"`.
pub fn parse_agentflow_json(json: &str) -> Result<AgentFlowGraph, MermaidError> {
    let graph: AgentFlowGraph =
        serde_json::from_str(json).map_err(|e| MermaidError::JsonError(e.to_string()))?;

    if graph.diagram_type != "agentflow" {
        return Err(MermaidError::MissingDiagramType);
    }

    Ok(graph)
}

/// Serialize an `AgentFlowGraph` to a pretty-printed JSON value.
pub fn agentflow_to_json(graph: &AgentFlowGraph) -> serde_json::Value {
    serde_json::to_value(graph).expect("AgentFlowGraph should always serialize")
}

/// Serialize an `AgentFlowGraph` to a pretty-printed JSON string.
pub fn agentflow_to_json_string(graph: &AgentFlowGraph) -> String {
    serde_json::to_string_pretty(graph).expect("AgentFlowGraph should always serialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agentflow::*;
    use std::collections::BTreeMap;

    fn sample_graph() -> AgentFlowGraph {
        let mut g = AgentFlowGraph::new("LR");
        g.schemas.push(AgentFlowSchemaNode {
            id: "SiteConfig".to_string(),
            label: "SiteConfig".to_string(),
            shape: "hexagon".to_string(),
            metadata: SchemaMetadata {
                fields: BTreeMap::from([
                    ("name".to_string(), "String".to_string()),
                    ("summary".to_string(), "String".to_string()),
                ]),
            },
        });
        g.agents.push(AgentFlowAgent {
            id: "researcher".to_string(),
            label: "@researcher".to_string(),
            model: None,
            prompt: None,
            memory: vec![],
            nodes: vec![AgentFlowToolNode {
                id: "research_location".to_string(),
                label: "Research Location".to_string(),
                shape: "roundedRect".to_string(),
                metadata: ToolMetadata {
                    description: "Research a city".to_string(),
                    requires: vec!["^net.read".to_string()],
                    deny: vec![],
                    source: Some("^search.duckduckgo(query)".to_string()),
                    handler: None,
                    output: None,
                    directives: vec![],
                    params: BTreeMap::from([("query".to_string(), "String".to_string())]),
                    returns: Some("String".to_string()),
                    retry: None,
                    cache: None,
                    validate: None,
                },
            }],
            skills: vec![],
        });
        g.edges.push(AgentFlowEdge {
            from: "research_location".to_string(),
            to: "write_copy".to_string(),
            label: None,
            edge_type: EdgeType::Flow,
        });
        g
    }

    #[test]
    fn json_roundtrip() {
        let graph = sample_graph();
        let json_str = agentflow_to_json_string(&graph);
        let back = parse_agentflow_json(&json_str).unwrap();
        assert_eq!(back.diagram_type, "agentflow");
        assert_eq!(back.direction, "LR");
        assert_eq!(back.schemas.len(), 1);
        assert_eq!(back.agents.len(), 1);
        assert_eq!(back.agents[0].nodes.len(), 1);
        assert_eq!(back.edges.len(), 1);
    }

    #[test]
    fn json_value_output() {
        let graph = sample_graph();
        let val = agentflow_to_json(&graph);
        assert_eq!(val["type"], "agentflow");
        assert_eq!(val["direction"], "LR");
        assert!(val["agents"].is_array());
    }

    #[test]
    fn wrong_type_rejected() {
        let json = r#"{"type": "flowchart", "direction": "LR", "agents": [], "edges": []}"#;
        let err = parse_agentflow_json(json);
        assert!(err.is_err());
    }

    #[test]
    fn malformed_json_rejected() {
        let err = parse_agentflow_json("not json at all");
        assert!(err.is_err());
    }

    #[test]
    fn edge_types_in_json() {
        let graph = sample_graph();
        let json_str = agentflow_to_json_string(&graph);
        assert!(json_str.contains("\"type\": \"flow\""));
    }
}
