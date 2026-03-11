// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-10

use crate::parser::{MermaidGraph, MermaidNode, NodeShape};

/// Convert a `MermaidGraph` into PACT source text.
///
/// Mapping heuristics:
/// - Diamond nodes  -> `agent` declarations
/// - Rounded nodes  -> `tool` declarations
/// - Rectangle nodes -> `flow` steps
/// - Circle nodes   -> `schema` declarations
/// - Subgraphs      -> `agent_bundle` declarations
/// - Edges          -> flow body wiring
pub fn graph_to_pact(graph: &MermaidGraph) -> String {
    let mut out = String::new();

    // Header comment.
    out.push_str("-- Auto-generated from Mermaid diagram\n");
    out.push_str("-- Do not edit manually — regenerate from the .mmd source.\n\n");

    // Permission tree.
    out.push_str("permit_tree {\n");
    out.push_str("    ^llm {\n");
    out.push_str("        ^llm.query\n");
    out.push_str("    }\n");
    out.push_str("    ^net {\n");
    out.push_str("        ^net.read\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");

    // Collect nodes by role.
    let tools: Vec<&MermaidNode> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::Rounded)
        .collect();
    let agents: Vec<&MermaidNode> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::Diamond)
        .collect();
    let schemas: Vec<&MermaidNode> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::Circle)
        .collect();
    let steps: Vec<&MermaidNode> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::Rectangle)
        .collect();

    // Schema declarations.
    for schema in &schemas {
        let name = to_pascal_case(&schema.label);
        out.push_str(&format!("schema {} {{\n", name));
        out.push_str("    value :: String\n");
        out.push_str("}\n\n");
    }

    // Tool declarations.
    for tool in &tools {
        let name = to_snake_case(&tool.label);
        out.push_str(&format!("tool #{} {{\n", name));
        out.push_str(&format!(
            "    description: <<{}>>\n",
            tool.label
        ));
        out.push_str("    requires: [^llm.query]\n");
        out.push_str("    params {\n");
        out.push_str("        input :: String\n");
        out.push_str("    }\n");
        out.push_str("    returns :: String\n");
        out.push_str("}\n\n");
    }

    // Agent declarations — attach tools that are connected via edges.
    for agent in &agents {
        let name = to_snake_case(&agent.label);
        // Find tools connected to this agent (incoming or outgoing edges).
        let connected_tools: Vec<String> = graph
            .edges
            .iter()
            .filter_map(|e| {
                if e.from == agent.id {
                    find_node_by_id(&graph.nodes, &e.to)
                        .filter(|n| n.shape == NodeShape::Rounded)
                        .map(|n| to_snake_case(&n.label))
                } else if e.to == agent.id {
                    find_node_by_id(&graph.nodes, &e.from)
                        .filter(|n| n.shape == NodeShape::Rounded)
                        .map(|n| to_snake_case(&n.label))
                } else {
                    None
                }
            })
            .collect();

        out.push_str(&format!("agent @{} {{\n", name));
        out.push_str("    permits: [^llm.query]\n");
        if !connected_tools.is_empty() {
            out.push_str(&format!(
                "    tools: [#{}]\n",
                connected_tools.join(", #")
            ));
        }
        out.push_str(&format!(
            "    prompt: <<You are a {} agent.>>\n",
            agent.label
        ));
        out.push_str("}\n\n");
    }

    // Agent bundle declarations from subgraphs.
    for sg in &graph.subgraphs {
        let name = to_snake_case(&sg.name);
        let member_agents: Vec<String> = sg
            .node_ids
            .iter()
            .filter_map(|id| {
                find_node_by_id(&graph.nodes, id)
                    .filter(|n| n.shape == NodeShape::Diamond)
                    .map(|n| to_snake_case(&n.label))
            })
            .collect();

        if !member_agents.is_empty() {
            out.push_str(&format!("agent_bundle @{} {{\n", name));
            out.push_str(&format!(
                "    agents: [@{}]\n",
                member_agents.join(", @")
            ));
            out.push_str("}\n\n");
        }
    }

    // Main flow.
    out.push_str("flow main(input :: String) -> String {\n");

    // Walk edges in order to produce pipeline steps.
    if !graph.edges.is_empty() {
        for (i, edge) in graph.edges.iter().enumerate() {
            let from_node = find_node_by_id(&graph.nodes, &edge.from);
            let to_node = find_node_by_id(&graph.nodes, &edge.to);

            let step_var = format!("step_{}", i + 1);

            match (from_node, to_node) {
                (Some(from), Some(to)) => {
                    let comment = match &edge.label {
                        Some(l) => format!("  -- {}", l),
                        None => String::new(),
                    };
                    let from_name = to_snake_case(&from.label);
                    let to_name = to_snake_case(&to.label);

                    let prev_input = if i > 0 {
                        format!("step_{}", i)
                    } else {
                        "input".to_string()
                    };

                    match (&from.shape, &to.shape) {
                        // agent -> tool: agent invokes tool
                        (NodeShape::Diamond, NodeShape::Rounded) => {
                            out.push_str(&format!(
                                "    {} = @{} -> #{}({}){}\n",
                                step_var, from_name, to_name, prev_input, comment
                            ));
                        }
                        // tool -> agent: tool feeds into agent
                        (NodeShape::Rounded, NodeShape::Diamond) => {
                            out.push_str(&format!(
                                "    {} = @{} -> #{}({}){}\n",
                                step_var, to_name, from_name, prev_input, comment
                            ));
                        }
                        // generic fallback
                        _ => {
                            out.push_str(&format!(
                                "    {} = {}({}){}\n",
                                step_var, to_name, prev_input, comment
                            ));
                        }
                    }
                }
                _ => {
                    // Fallback when nodes aren't registered.
                    let input = if i > 0 {
                        format!("step_{}", i)
                    } else {
                        "input".to_string()
                    };
                    out.push_str(&format!(
                        "    {} = {}({})\n",
                        step_var, edge.to, input
                    ));
                }
            }
        }

        // Return last step.
        let last = format!("step_{}", graph.edges.len());
        out.push_str(&format!("    return {}\n", last));
    } else if !steps.is_empty() {
        // No edges — just list steps.
        for step in &steps {
            let name = to_snake_case(&step.label);
            out.push_str(&format!("    {}()\n", name));
        }
    }

    out.push_str("}\n");

    out
}

fn find_node_by_id<'a>(nodes: &'a [MermaidNode], id: &str) -> Option<&'a MermaidNode> {
    nodes.iter().find(|n| n.id == id)
}

fn to_pascal_case(s: &str) -> String {
    s.split([' ', '-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        } else if (ch == ' ' || ch == '-' || ch == '_') && !result.ends_with('_') {
            result.push('_');
        }
    }
    // Trim trailing underscores.
    result.trim_end_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Direction, MermaidEdge, MermaidSubgraph};

    fn make_graph(
        nodes: Vec<MermaidNode>,
        edges: Vec<MermaidEdge>,
        subgraphs: Vec<MermaidSubgraph>,
    ) -> MermaidGraph {
        MermaidGraph {
            direction: Direction::LR,
            nodes,
            edges,
            subgraphs,
        }
    }

    #[test]
    fn simple_graph_generates_pact() {
        let graph = make_graph(
            vec![
                MermaidNode {
                    id: "A".into(),
                    label: "Fetch".into(),
                    shape: NodeShape::Rounded,
                },
                MermaidNode {
                    id: "B".into(),
                    label: "Analyzer".into(),
                    shape: NodeShape::Diamond,
                },
            ],
            vec![MermaidEdge {
                from: "A".into(),
                to: "B".into(),
                label: None,
            }],
            vec![],
        );
        let pact = graph_to_pact(&graph);
        assert!(pact.contains("tool #fetch"));
        assert!(pact.contains("agent @analyzer"));
        assert!(pact.contains("flow main(input :: String)"));
    }

    #[test]
    fn diamond_becomes_agent() {
        let graph = make_graph(
            vec![MermaidNode {
                id: "X".into(),
                label: "Decider".into(),
                shape: NodeShape::Diamond,
            }],
            vec![],
            vec![],
        );
        let pact = graph_to_pact(&graph);
        assert!(pact.contains("agent @decider"));
    }

    #[test]
    fn rounded_becomes_tool() {
        let graph = make_graph(
            vec![MermaidNode {
                id: "T".into(),
                label: "Search Web".into(),
                shape: NodeShape::Rounded,
            }],
            vec![],
            vec![],
        );
        let pact = graph_to_pact(&graph);
        assert!(pact.contains("tool #search_web"));
    }

    #[test]
    fn subgraph_becomes_bundle() {
        let graph = make_graph(
            vec![
                MermaidNode {
                    id: "A".into(),
                    label: "Agent One".into(),
                    shape: NodeShape::Diamond,
                },
                MermaidNode {
                    id: "B".into(),
                    label: "Agent Two".into(),
                    shape: NodeShape::Diamond,
                },
            ],
            vec![],
            vec![MermaidSubgraph {
                name: "Team".into(),
                node_ids: vec!["A".into(), "B".into()],
            }],
        );
        let pact = graph_to_pact(&graph);
        assert!(pact.contains("agent_bundle @team"));
    }
}
