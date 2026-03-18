// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-10

use thiserror::Error;

/// Errors produced during Mermaid/agentflow diagram parsing.
#[derive(Debug, Error)]
pub enum MermaidError {
    /// No `flowchart` or `agentflow` declaration found.
    #[error("expected 'flowchart' or 'agentflow' declaration at the beginning")]
    MissingFlowchart,

    /// An unrecognized diagram direction was specified.
    #[error("unknown direction '{0}', expected LR, TD, TB, RL, or BT")]
    UnknownDirection(String),

    /// A node definition could not be parsed.
    #[error("malformed node definition: '{0}'")]
    MalformedNode(String),

    /// An edge definition could not be parsed.
    #[error("malformed edge definition: '{0}'")]
    MalformedEdge(String),

    /// A subgraph block was never closed with `end`.
    #[error("unclosed subgraph '{0}'")]
    UnclosedSubgraph(String),

    /// Metadata block (`@{...}`) could not be parsed.
    #[error("malformed metadata: {0}")]
    MalformedMetadata(String),

    /// JSON parsing or serialization failed.
    #[error("JSON error: {0}")]
    JsonError(String),

    /// The diagram type header is missing or invalid.
    #[error("missing or invalid diagram type — expected 'agentflow'")]
    MissingDiagramType,
}

/// Flowchart layout direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
    /// Left to right.
    LR,
    /// Top down.
    TD,
    /// Top to bottom (alias for TD).
    TB,
    /// Right to left.
    RL,
    /// Bottom to top.
    BT,
}

/// Shape of a Mermaid flowchart node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeShape {
    /// Rectangle node (`[label]`).
    Rectangle,
    /// Rounded rectangle node (`(label)`).
    Rounded,
    /// Diamond/decision node (`{label}`).
    Diamond,
    /// Circle node (`((label))`).
    Circle,
}

/// A node in a Mermaid flowchart graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label for the node.
    pub label: String,
    /// Visual shape of the node.
    pub shape: NodeShape,
}

/// A directed edge between two nodes in a Mermaid flowchart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidEdge {
    /// Source node identifier.
    pub from: String,
    /// Target node identifier.
    pub to: String,
    /// Optional edge label.
    pub label: Option<String>,
}

/// A named subgraph grouping nodes together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidSubgraph {
    /// Subgraph name/label.
    pub name: String,
    /// IDs of nodes contained in this subgraph.
    pub node_ids: Vec<String>,
}

/// A parsed Mermaid flowchart graph.
#[derive(Debug, Clone, PartialEq)]
pub struct MermaidGraph {
    /// Layout direction of the flowchart.
    pub direction: Direction,
    /// All nodes in the graph.
    pub nodes: Vec<MermaidNode>,
    /// All directed edges in the graph.
    pub edges: Vec<MermaidEdge>,
    /// Named subgraph groupings.
    pub subgraphs: Vec<MermaidSubgraph>,
}

/// Try to parse a node definition from a string like `A[Label]`, `A(Label)`,
/// `A{Label}`, or `A((Label))`. Returns `None` if the string does not match
/// any known node pattern.
fn try_parse_node(s: &str) -> Option<MermaidNode> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Find the first delimiter character that starts a shape bracket.
    let delim_pos = s.find(['[', '(', '{'])?;
    let id = s[..delim_pos].trim().to_string();
    if id.is_empty() {
        return None;
    }
    let rest = &s[delim_pos..];

    // Circle: ((label))
    if let Some(label) = rest.strip_prefix("((") {
        let label = label.strip_suffix("))")?;
        return Some(MermaidNode {
            id,
            label: label.trim().to_string(),
            shape: NodeShape::Circle,
        });
    }

    // Rounded: (label)
    if let Some(label) = rest.strip_prefix('(') {
        let label = label.strip_suffix(')')?;
        return Some(MermaidNode {
            id,
            label: label.trim().to_string(),
            shape: NodeShape::Rounded,
        });
    }

    // Diamond: {label}
    if let Some(label) = rest.strip_prefix('{') {
        let label = label.strip_suffix('}')?;
        return Some(MermaidNode {
            id,
            label: label.trim().to_string(),
            shape: NodeShape::Diamond,
        });
    }

    // Rectangle: [label]
    if let Some(label) = rest.strip_prefix('[') {
        let label = label.strip_suffix(']')?;
        return Some(MermaidNode {
            id,
            label: label.trim().to_string(),
            shape: NodeShape::Rectangle,
        });
    }

    None
}

/// Register a node in the graph if it hasn't been registered yet.
/// If the node already exists, this is a no-op.
fn register_node(nodes: &mut Vec<MermaidNode>, node: MermaidNode) {
    if !nodes.iter().any(|n| n.id == node.id) {
        nodes.push(node);
    }
}

/// Parse a Mermaid flowchart string into a `MermaidGraph`.
pub fn parse_mermaid(input: &str) -> Result<MermaidGraph, MermaidError> {
    let mut direction = None;
    let mut nodes: Vec<MermaidNode> = Vec::new();
    let mut edges: Vec<MermaidEdge> = Vec::new();
    let mut subgraphs: Vec<MermaidSubgraph> = Vec::new();

    // Subgraph tracking.
    let mut current_subgraph: Option<(String, Vec<String>)> = None;

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments.
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }

        // Flowchart declaration.
        if trimmed.starts_with("flowchart") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 2 {
                return Err(MermaidError::UnknownDirection(String::new()));
            }
            direction = Some(match parts[1] {
                "LR" => Direction::LR,
                "TD" => Direction::TD,
                "TB" => Direction::TB,
                "RL" => Direction::RL,
                "BT" => Direction::BT,
                other => return Err(MermaidError::UnknownDirection(other.to_string())),
            });
            continue;
        }

        // Subgraph start.
        if trimmed.starts_with("subgraph") {
            let name = trimmed.strip_prefix("subgraph").unwrap().trim().to_string();
            current_subgraph = Some((name, Vec::new()));
            continue;
        }

        // Subgraph end.
        if trimmed == "end" {
            if let Some((name, node_ids)) = current_subgraph.take() {
                subgraphs.push(MermaidSubgraph { name, node_ids });
            }
            continue;
        }

        // Edge line: contains `-->`
        if trimmed.contains("-->") {
            parse_edge_line(trimmed, &mut nodes, &mut edges, &mut current_subgraph)?;
            continue;
        }

        // Standalone node definition.
        if let Some(node) = try_parse_node(trimmed) {
            if let Some((_, ref mut ids)) = current_subgraph {
                if !ids.contains(&node.id) {
                    ids.push(node.id.clone());
                }
            }
            register_node(&mut nodes, node);
            continue;
        }

        // Inside a subgraph, bare identifiers count as node references.
        if current_subgraph.is_some() {
            let bare_id = trimmed.to_string();
            if !bare_id.is_empty() {
                if let Some((_, ref mut ids)) = current_subgraph {
                    if !ids.contains(&bare_id) {
                        ids.push(bare_id);
                    }
                }
            }
        }
    }

    // Check for unclosed subgraph.
    if let Some((name, _)) = current_subgraph {
        return Err(MermaidError::UnclosedSubgraph(name));
    }

    let direction = direction.ok_or(MermaidError::MissingFlowchart)?;

    Ok(MermaidGraph {
        direction,
        nodes,
        edges,
        subgraphs,
    })
}

/// Parse a line that contains `-->` into one or more edges, also collecting
/// any inline node definitions.
fn parse_edge_line(
    line: &str,
    nodes: &mut Vec<MermaidNode>,
    edges: &mut Vec<MermaidEdge>,
    current_subgraph: &mut Option<(String, Vec<String>)>,
) -> Result<(), MermaidError> {
    // Split on `-->` but also handle `-->|label|` and `-- label -->`.
    // We use a simple approach: split on `-->` tokens.
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() < 2 {
        return Err(MermaidError::MalformedEdge(line.to_string()));
    }

    for i in 0..parts.len() - 1 {
        let left_raw = parts[i].trim();
        let right_raw = parts[i + 1].trim();

        // Extract label and source node from the left side.
        // Pattern: `A -- label ` or just `A`
        let (from_str, label_from_left) = if let Some(dash_pos) = left_raw.rfind(" -- ") {
            // `A -- label` pattern (label is between `--` and end).
            let node_part = &left_raw[..dash_pos];
            let label_part = left_raw[dash_pos + 4..].trim();
            (node_part.trim(), Some(label_part.to_string()))
        } else {
            (left_raw, None)
        };

        // Extract label and target node from the right side.
        // Pattern: `|label| B` or just `B`
        let (to_str, label_from_right) = if let Some(rest) = right_raw.strip_prefix('|') {
            if let Some(pipe_end) = rest.find('|') {
                let label = rest[..pipe_end].trim().to_string();
                let node_part = rest[pipe_end + 1..].trim();
                (node_part, Some(label))
            } else {
                (right_raw, None)
            }
        } else {
            (right_raw, None)
        };

        let label = label_from_right.or(label_from_left);

        // Parse or extract node IDs from left and right.
        let from_id = if let Some(node) = try_parse_node(from_str) {
            let id = node.id.clone();
            if let Some((_, ref mut ids)) = current_subgraph {
                if !ids.contains(&id) {
                    ids.push(id.clone());
                }
            }
            register_node(nodes, node);
            id
        } else {
            from_str.trim().to_string()
        };

        let to_id = if let Some(node) = try_parse_node(to_str) {
            let id = node.id.clone();
            if let Some((_, ref mut ids)) = current_subgraph {
                if !ids.contains(&id) {
                    ids.push(id.clone());
                }
            }
            register_node(nodes, node);
            id
        } else {
            to_str.trim().to_string()
        };

        edges.push(MermaidEdge {
            from: from_id,
            to: to_id,
            label,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_flowchart() {
        let input = r#"
flowchart LR
    A[Start] --> B[Process] --> C[End]
"#;
        let graph = parse_mermaid(input).unwrap();
        assert_eq!(graph.direction, Direction::LR);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn parse_with_labels() {
        let input = r#"
flowchart LR
    A[Start] -->|go| B[End]
"#;
        let graph = parse_mermaid(input).unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].label.as_deref(), Some("go"));
    }

    #[test]
    fn parse_subgraph() {
        let input = r#"
flowchart LR
    subgraph Backend
        A[API]
        B[DB]
    end
    A --> B
"#;
        let graph = parse_mermaid(input).unwrap();
        assert_eq!(graph.subgraphs.len(), 1);
        assert_eq!(graph.subgraphs[0].name, "Backend");
        assert_eq!(graph.subgraphs[0].node_ids.len(), 2);
        assert!(graph.subgraphs[0].node_ids.contains(&"A".to_string()));
        assert!(graph.subgraphs[0].node_ids.contains(&"B".to_string()));
    }

    #[test]
    fn parse_comments_skipped() {
        let input = r#"
flowchart LR
    %% This is a comment
    A[Start] --> B[End]
    %% Another comment
"#;
        let graph = parse_mermaid(input).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn parse_different_shapes() {
        let input = r#"
flowchart TD
    A[Rect]
    B(Rounded)
    C{Diamond}
    D((Circle))
"#;
        let graph = parse_mermaid(input).unwrap();
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.nodes[0].shape, NodeShape::Rectangle);
        assert_eq!(graph.nodes[0].label, "Rect");
        assert_eq!(graph.nodes[1].shape, NodeShape::Rounded);
        assert_eq!(graph.nodes[1].label, "Rounded");
        assert_eq!(graph.nodes[2].shape, NodeShape::Diamond);
        assert_eq!(graph.nodes[2].label, "Diamond");
        assert_eq!(graph.nodes[3].shape, NodeShape::Circle);
        assert_eq!(graph.nodes[3].label, "Circle");
    }
}
